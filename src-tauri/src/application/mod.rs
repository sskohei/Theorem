pub mod ai_service;
pub mod drive_service;
#[cfg(test)]
mod drive_tests;
pub mod science_service;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use tokio::sync::{Mutex, Notify, RwLock};
use uuid::Uuid;

use crate::agents::manager::{AgentManager, EventEmitter};
use crate::database::Database;
use crate::database::library_repo;
use crate::document;
use crate::domain::{
    AppSettings, AttachmentBinaryResponse, AttachmentInfo, BackupInfo, BootstrapState,
    DocumentDetail, DocumentFilter, DocumentSummary, Folder, ImportBatchResult, ImportFailure,
    ImportedDocumentItem, StorageStats, Subject, Tag,
};
use crate::error::{AppError, AppResult};
use crate::gdrive::DriveEndpoints;
use crate::security::DiagnosticLogger;
use crate::storage::AppStorage;

pub type DriveUpdateEmitter = Arc<dyn Fn() + Send + Sync>;

#[derive(Clone)]
pub struct AppState {
    pub(crate) db: Arc<RwLock<Database>>,
    pub(crate) storage: AppStorage,
    pub(crate) logger: DiagnosticLogger,
    pub(crate) agents: Arc<AgentManager>,
    pub(crate) http_client: reqwest::Client,
    pub(crate) drive_endpoints: Arc<RwLock<DriveEndpoints>>,
    pub(crate) drive_worker_notify: Arc<Notify>,
    pub(crate) drive_emitter: Arc<Mutex<Option<DriveUpdateEmitter>>>,
}

impl AppState {
    pub async fn initialize(data_root: PathBuf) -> AppResult<Self> {
        crate::storage::migrate_legacy_library(&data_root).await?;
        let storage = AppStorage::initialize(data_root)?;
        let logger = DiagnosticLogger::new(&storage.logs_dir());
        let db = Database::connect(&storage.database_path()).await?;
        let workspaces_dir = storage.root().join("workspaces");
        let agents = Arc::new(AgentManager::new(workspaces_dir));
        let http_client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(15))
            .timeout(Duration::from_secs(120))
            .build()
            .map_err(|e| AppError::Drive(format!("HTTP クライアント初期化失敗: {e}")))?;
        logger.log_info("app_started", "Theorem core initialized");
        Ok(Self {
            db: Arc::new(RwLock::new(db)),
            storage,
            logger,
            agents,
            http_client,
            drive_endpoints: Arc::new(RwLock::new(DriveEndpoints::default())),
            drive_worker_notify: Arc::new(Notify::new()),
            drive_emitter: Arc::new(Mutex::new(None)),
        })
    }

    pub async fn set_agent_event_emitter(&self, emitter: EventEmitter) {
        self.agents.set_emitter(emitter).await;
    }

    pub async fn set_drive_event_emitter(&self, emitter: DriveUpdateEmitter) {
        *self.drive_emitter.lock().await = Some(emitter);
    }

    /// Spawns the resilient background worker that drains due `drive_upload_jobs`
    /// whenever notified or every 15 seconds for backoff retries.
    pub fn spawn_drive_background_worker(&self) {
        let state = self.clone();
        tokio::spawn(async move {
            loop {
                let _ = state.process_drive_queue_once().await;
                tokio::select! {
                    _ = state.drive_worker_notify.notified() => {}
                    _ = tokio::time::sleep(Duration::from_secs(15)) => {}
                }
            }
        });
    }

    pub async fn bootstrap(&self) -> AppResult<BootstrapState> {
        let db = self.db.read().await;
        let subjects = library_repo::list_subjects(db.pool()).await?;
        let folders = library_repo::list_folders(db.pool()).await?;
        let tags = library_repo::list_tags(db.pool()).await?;
        let documents = library_repo::list_documents(db.pool(), &DocumentFilter::default()).await?;
        let settings = self.storage.load_settings()?;

        Ok(BootstrapState {
            app_version: env!("CARGO_PKG_VERSION").to_string(),
            data_root_display: self.storage.root().to_string_lossy().into_owned(),
            subjects,
            folders,
            tags,
            documents,
            settings,
        })
    }

    pub async fn list_subjects(&self) -> AppResult<Vec<Subject>> {
        let db = self.db.read().await;
        library_repo::list_subjects(db.pool()).await
    }

    pub async fn create_subject(
        &self,
        name: &str,
        category: &str,
        description: Option<&str>,
    ) -> AppResult<Subject> {
        let db = self.db.read().await;
        library_repo::create_subject(db.pool(), name, category, description).await
    }

    pub async fn update_subject(
        &self,
        id: &str,
        name: &str,
        category: &str,
        description: Option<&str>,
    ) -> AppResult<Subject> {
        let db = self.db.read().await;
        library_repo::update_subject(db.pool(), id, name, category, description).await
    }

    pub async fn delete_subject(&self, id: &str) -> AppResult<()> {
        let db = self.db.read().await;
        library_repo::delete_subject(db.pool(), id).await
    }

    pub async fn list_folders(&self) -> AppResult<Vec<Folder>> {
        let db = self.db.read().await;
        library_repo::list_folders(db.pool()).await
    }

    pub async fn create_folder(
        &self,
        subject_id: &str,
        parent_id: Option<&str>,
        name: &str,
    ) -> AppResult<Folder> {
        let db = self.db.read().await;
        library_repo::create_folder(db.pool(), subject_id, parent_id, name).await
    }

    pub async fn update_folder(
        &self,
        id: &str,
        name: &str,
        parent_id: Option<&str>,
    ) -> AppResult<Folder> {
        let db = self.db.read().await;
        library_repo::update_folder(db.pool(), id, name, parent_id).await
    }

    pub async fn delete_folder(&self, id: &str) -> AppResult<()> {
        let db = self.db.read().await;
        library_repo::delete_folder(db.pool(), id).await
    }

    pub async fn list_tags(&self) -> AppResult<Vec<Tag>> {
        let db = self.db.read().await;
        library_repo::list_tags(db.pool()).await
    }

    pub async fn create_tag(&self, name: &str, color: Option<&str>) -> AppResult<Tag> {
        let db = self.db.read().await;
        library_repo::create_tag(db.pool(), name, color).await
    }

    pub async fn delete_tag(&self, id: &str) -> AppResult<()> {
        let db = self.db.read().await;
        library_repo::delete_tag(db.pool(), id).await
    }

    pub async fn set_document_tags(
        &self,
        document_id: &str,
        tag_ids: &[String],
    ) -> AppResult<Vec<Tag>> {
        let db = self.db.read().await;
        library_repo::set_document_tags(db.pool(), document_id, tag_ids).await
    }

    pub async fn list_documents(&self, filter: &DocumentFilter) -> AppResult<Vec<DocumentSummary>> {
        let db = self.db.read().await;
        library_repo::list_documents(db.pool(), filter).await
    }

    pub async fn get_document_detail(&self, document_id: &str) -> AppResult<DocumentDetail> {
        let db = self.db.read().await;
        library_repo::get_document_detail(db.pool(), document_id).await
    }

    pub async fn update_document(
        &self,
        id: &str,
        title: Option<&str>,
        subject_id: Option<&str>,
        folder_id: Option<Option<&str>>,
        is_favorite: Option<bool>,
        recognition_mode: Option<&str>,
    ) -> AppResult<DocumentDetail> {
        let db = self.db.read().await;
        library_repo::update_document(
            db.pool(),
            id,
            title,
            subject_id,
            folder_id,
            is_favorite,
            recognition_mode,
        )
        .await
    }

    pub async fn touch_document_opened(&self, id: &str) -> AppResult<String> {
        let db = self.db.read().await;
        library_repo::touch_document_opened(db.pool(), id).await
    }

    pub async fn delete_document(&self, id: &str) -> AppResult<()> {
        let db = self.db.read().await;
        let orphan_paths = library_repo::delete_document(db.pool(), id).await?;
        drop(db);
        for rel in orphan_paths {
            if let Err(err) = self.storage.delete_if_exists(&rel) {
                self.logger
                    .log_warn("orphan_cleanup_failed", &format!("{rel}: {err}"));
            }
        }
        Ok(())
    }

    pub async fn import_files_from_paths(
        &self,
        subject_id: &str,
        folder_id: Option<&str>,
        paths: &[String],
    ) -> AppResult<ImportBatchResult> {
        let db = self.db.read().await;
        let mut imported = Vec::new();
        let mut failures = Vec::new();

        for path_str in paths {
            let p = Path::new(path_str);
            let filename = p
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| path_str.clone());
            match document::import_from_path(db.pool(), &self.storage, subject_id, folder_id, p)
                .await
            {
                Ok(item) => {
                    self.logger.log_info(
                        "document_imported",
                        &format!("document_id={}", item.detail.document.id),
                    );
                    imported.push(item);
                }
                Err(err) => {
                    self.logger
                        .log_warn("document_import_failed", &err.to_string());
                    failures.push(ImportFailure {
                        filename,
                        reason: err.to_string(),
                    });
                }
            }
        }

        drop(db);
        for item in &imported {
            self.maybe_auto_enqueue_drive_for_document(&item.detail.document.id, "original_only")
                .await;
        }

        Ok(ImportBatchResult { imported, failures })
    }

    pub async fn import_file_bytes(
        &self,
        subject_id: &str,
        folder_id: Option<&str>,
        filename: &str,
        base64_data: &str,
    ) -> AppResult<ImportedDocumentItem> {
        let bytes = STANDARD.decode(base64_data.trim()).map_err(|_| {
            AppError::Validation("Base64 ファイルデータのデコードに失敗しました".into())
        })?;
        let item = {
            let db = self.db.read().await;
            document::import_bytes(
                db.pool(),
                &self.storage,
                subject_id,
                folder_id,
                filename,
                &bytes,
            )
            .await?
        };
        self.logger.log_info(
            "document_imported_bytes",
            &format!("document_id={}", item.detail.document.id),
        );
        self.maybe_auto_enqueue_drive_for_document(&item.detail.document.id, "original_only")
            .await;
        Ok(item)
    }

    pub async fn read_attachment_base64(
        &self,
        attachment_id: &str,
    ) -> AppResult<AttachmentBinaryResponse> {
        let db = self.db.read().await;
        let attachment = library_repo::get_attachment(db.pool(), attachment_id).await?;
        drop(db);
        let bytes = self.storage.read_bytes(&attachment.relative_path)?;
        Ok(AttachmentBinaryResponse {
            attachment,
            base64_data: STANDARD.encode(bytes),
        })
    }

    pub async fn save_document_thumbnail(
        &self,
        document_id: &str,
        png_base64: &str,
    ) -> AppResult<AttachmentInfo> {
        let bytes = STANDARD
            .decode(png_base64.trim())
            .map_err(|_| AppError::Validation("サムネイル PNG のデコードに失敗しました".into()))?;
        let db = self.db.read().await;
        document::save_thumbnail_png(db.pool(), &self.storage, document_id, &bytes).await
    }

    pub async fn export_attachment(
        &self,
        attachment_id: &str,
        destination_path: &str,
    ) -> AppResult<()> {
        let db = self.db.read().await;
        document::export_attachment(
            db.pool(),
            &self.storage,
            attachment_id,
            Path::new(destination_path),
        )
        .await
    }

    pub fn get_settings(&self) -> AppResult<AppSettings> {
        self.storage.load_settings()
    }

    pub fn update_settings(&self, settings: &AppSettings) -> AppResult<AppSettings> {
        if !matches!(settings.theme.as_str(), "light" | "dark" | "system") {
            return Err(AppError::Validation("不正なテーマ設定です".into()));
        }
        if !matches!(settings.language.as_str(), "ja" | "en") {
            return Err(AppError::Validation("不正な言語設定です".into()));
        }
        self.storage.save_settings(settings)?;
        Ok(settings.clone())
    }

    pub async fn get_storage_stats(&self) -> AppResult<StorageStats> {
        let db = self.db.read().await;
        let (document_count, attachment_count) = library_repo::counts(db.pool()).await?;
        drop(db);

        let database_bytes = self.storage.file_size("database/theorem.db");
        let originals_bytes = self.storage.dir_size("attachments/originals");
        let thumbnails_bytes = self.storage.dir_size("thumbnails");
        let backups_bytes = self.storage.dir_size("backups");
        let logs_bytes = self.storage.dir_size("logs");
        let temp_bytes = self.storage.dir_size("tmp") + self.storage.dir_size("workspaces");
        let total_bytes = database_bytes
            + originals_bytes
            + thumbnails_bytes
            + backups_bytes
            + logs_bytes
            + temp_bytes;

        Ok(StorageStats {
            data_root_display: self.storage.root().to_string_lossy().into_owned(),
            database_bytes,
            originals_bytes,
            thumbnails_bytes,
            backups_bytes,
            logs_bytes,
            temp_bytes,
            total_bytes,
            document_count,
            attachment_count,
        })
    }

    pub fn cleanup_temp_files(&self) -> AppResult<u64> {
        let freed = self.storage.cleanup_temp()?;
        self.logger
            .log_info("temp_cleaned", &format!("freed_bytes={freed}"));
        Ok(freed)
    }

    pub async fn create_backup(&self) -> AppResult<BackupInfo> {
        let snapshot_rel = format!("tmp/db-snapshot-{}.db", Uuid::new_v4());
        let snapshot_abs = self.storage.resolve(&snapshot_rel)?;

        {
            let db = self.db.read().await;
            db.snapshot_to(&snapshot_abs).await?;
        }

        let backup_res = self.storage.create_backup_archive(&snapshot_abs);
        let _ = self.storage.delete_if_exists(&snapshot_rel);
        let info = backup_res?;
        self.logger
            .log_info("backup_created", &format!("file={}", info.filename));
        Ok(info)
    }

    pub fn list_backups(&self) -> AppResult<Vec<BackupInfo>> {
        self.storage.list_backups()
    }

    pub async fn restore_backup(&self, backup_rel_path: &str) -> AppResult<BootstrapState> {
        let staged = self.storage.stage_backup_archive(backup_rel_path)?;
        Database::connect(&staged.database_path())
            .await?
            .close()
            .await;
        {
            let mut db_guard = self.db.write().await;
            db_guard.close().await;
            let restore_res = staged.install(&self.storage);
            let reopened = Database::connect(&self.storage.database_path()).await?;
            *db_guard = reopened;
            restore_res?;
        }
        self.logger
            .log_info("backup_restored", &format!("archive={backup_rel_path}"));
        self.bootstrap().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{ImageBuffer, Rgb};
    use lopdf::dictionary;
    use std::io::Cursor;
    use tempfile::TempDir;

    fn sample_png_bytes() -> Vec<u8> {
        let img: ImageBuffer<Rgb<u8>, Vec<u8>> =
            ImageBuffer::from_pixel(64, 48, Rgb([30, 58, 138]));
        let mut out = Vec::new();
        img.write_to(&mut Cursor::new(&mut out), image::ImageFormat::Png)
            .expect("encode png");
        out
    }

    fn sample_pdf_bytes() -> Vec<u8> {
        sample_pdf_with_pages(1)
    }

    fn sample_pdf_with_pages(page_count: i64) -> Vec<u8> {
        let mut doc = lopdf::Document::with_version("1.5");
        let pages_id = doc.new_object_id();
        let page_ids = (0..page_count)
            .map(|_| {
                doc.add_object(dictionary! {
                    "Type" => "Page",
                    "Parent" => pages_id,
                    "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()],
                })
                .into()
            })
            .collect::<Vec<_>>();
        doc.objects.insert(
            pages_id,
            lopdf::Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Kids" => page_ids,
                "Count" => page_count,
            }),
        );
        let catalog_id = doc.add_object(dictionary! {
            "Type" => "Catalog",
            "Pages" => pages_id,
        });
        doc.trailer.set("Root", catalog_id);
        let mut buf = Vec::new();
        doc.save_to(&mut buf).expect("save pdf");
        buf
    }

    #[tokio::test]
    async fn test_full_phase1_lifecycle_duplicate_and_restart_persistence() {
        let dir = TempDir::new().expect("tempdir");
        let src_dir = TempDir::new().expect("src_dir");

        // 1. First boot: default subjects seeded
        let state = AppState::initialize(dir.path().to_path_buf())
            .await
            .expect("init");
        let boot = state.bootstrap().await.expect("bootstrap");
        assert_eq!(boot.subjects.len(), 2);
        let math_id = boot.subjects[0].id.clone();

        // 2. Hierarchical folders + cycle rejection
        let f_calc = state
            .create_folder(&math_id, None, "微積分")
            .await
            .expect("create root folder");
        let f_sub = state
            .create_folder(&math_id, Some(&f_calc.id), "2026中間")
            .await
            .expect("create child folder");
        assert!(
            state
                .update_folder(&f_calc.id, "微積分", Some(&f_sub.id))
                .await
                .is_err(),
            "circular folder move must be rejected"
        );

        // 3. Import PDF from disk -> original file untouched, page_count=1
        let pdf_src = src_dir.path().join("exam1.pdf");
        let pdf_bytes = sample_pdf_bytes();
        std::fs::write(&pdf_src, &pdf_bytes).expect("write pdf");
        let batch = state
            .import_files_from_paths(
                &math_id,
                Some(&f_sub.id),
                &[pdf_src.to_string_lossy().into_owned()],
            )
            .await
            .expect("import pdf");
        assert_eq!(batch.imported.len(), 1);
        assert!(batch.failures.is_empty());
        assert_eq!(batch.imported[0].detail.document.page_count, Some(1));
        assert!(batch.imported[0].duplicate_matches.is_empty());
        assert_eq!(std::fs::read(&pdf_src).expect("read original"), pdf_bytes);

        // 4. Duplicate SHA-256 detection on re-import
        let batch2 = state
            .import_files_from_paths(
                &math_id,
                Some(&f_sub.id),
                &[pdf_src.to_string_lossy().into_owned()],
            )
            .await
            .expect("import duplicate pdf");
        assert_eq!(batch2.imported[0].duplicate_matches.len(), 1);

        // 5. Image import from base64 (clipboard paste) generates thumbnail + dimensions
        let png_b64 = STANDARD.encode(sample_png_bytes());
        let img_item = state
            .import_file_bytes(&math_id, None, "clipboard.png", &png_b64)
            .await
            .expect("import png");
        assert!(img_item.detail.document.thumbnail_attachment_id.is_some());
        assert_eq!(img_item.detail.attachments.len(), 2);

        // 6. Reject unsupported file format (fake .pdf with plain text)
        let bad_b64 = STANDARD.encode(b"not a real pdf or image");
        assert!(
            state
                .import_file_bytes(&math_id, None, "fake.pdf", &bad_b64)
                .await
                .is_err()
        );

        // 7. Tagging & filtering
        let tag = state
            .create_tag("重要", Some("#2563eb"))
            .await
            .expect("create tag");
        let doc_id = batch.imported[0].detail.document.id.clone();
        state
            .set_document_tags(&doc_id, std::slice::from_ref(&tag.id))
            .await
            .expect("tag doc");
        let tagged_docs = state
            .list_documents(&DocumentFilter {
                tag_id: Some(tag.id.clone()),
                ..Default::default()
            })
            .await
            .expect("filter by tag");
        assert_eq!(tagged_docs.len(), 1);

        // 8. Backup -> delete document -> restore backup -> verify document returned!
        let backup = state.create_backup().await.expect("create backup");
        state.delete_document(&doc_id).await.expect("delete doc");
        assert!(state.get_document_detail(&doc_id).await.is_err());

        let restored_boot = state
            .restore_backup(&backup.relative_path)
            .await
            .expect("restore backup");
        assert!(
            restored_boot.documents.iter().any(|d| d.id == doc_id),
            "restored backup must recover deleted document"
        );

        // 9. Simulated app restart: drop AppState and re-initialize from same data_root
        drop(state);
        let restarted = AppState::initialize(dir.path().to_path_buf())
            .await
            .expect("re-init");
        let restarted_boot = restarted.bootstrap().await.expect("restarted bootstrap");
        assert_eq!(restarted_boot.documents.len(), 3);
        assert_eq!(restarted_boot.folders.len(), 2);
        assert_eq!(restarted_boot.tags.len(), 1);
    }

    #[tokio::test]
    async fn test_sqlite_pragmas_fk_constraints_moves_and_metadata_search() {
        let dir = TempDir::new().expect("tempdir");
        let state = AppState::initialize(dir.path().to_path_buf())
            .await
            .expect("init");

        // Verify WAL mode and PRAGMA foreign_keys = ON
        {
            let db = state.db.read().await;
            let journal_mode: String = sqlx::query_scalar("PRAGMA journal_mode;")
                .fetch_one(db.pool())
                .await
                .expect("journal_mode");
            assert_eq!(journal_mode.to_lowercase(), "wal");
            let fk_enabled: i64 = sqlx::query_scalar("PRAGMA foreign_keys;")
                .fetch_one(db.pool())
                .await
                .expect("foreign_keys");
            assert_eq!(fk_enabled, 1);
        }

        let boot = state.bootstrap().await.expect("bootstrap");
        let math_id = boot.subjects[0].id.clone();
        let phys_id = boot.subjects[1].id.clone();

        // Create nested folders and test valid move + cross-subject rejection
        let f_root_a = state
            .create_folder(&math_id, None, "線形代数")
            .await
            .expect("folder a");
        let f_root_b = state
            .create_folder(&math_id, None, "解析学")
            .await
            .expect("folder b");
        let f_phys = state
            .create_folder(&phys_id, None, "量子力学")
            .await
            .expect("phys folder");

        // Move f_root_b under f_root_a
        let moved_b = state
            .update_folder(&f_root_b.id, "解析学", Some(&f_root_a.id))
            .await
            .expect("move folder within same subject");
        assert_eq!(moved_b.parent_id.as_deref(), Some(f_root_a.id.as_str()));

        // Cross-subject folder move must fail
        assert!(
            state
                .update_folder(&f_root_b.id, "解析学", Some(&f_phys.id))
                .await
                .is_err()
        );

        // Import image document into f_root_b
        let png_b64 = STANDARD.encode(sample_png_bytes());
        let imported = state
            .import_file_bytes(
                &math_id,
                Some(&f_root_b.id),
                "eigenvalue_sheet.png",
                &png_b64,
            )
            .await
            .expect("import");
        let doc_id = imported.detail.document.id.clone();

        // Rename title to something distinct from filename to verify metadata search
        state
            .update_document(&doc_id, Some("第3回演習問題"), None, None, None, None)
            .await
            .expect("rename");
        let tag = state.create_tag("期末対策", None).await.expect("tag");
        state
            .set_document_tags(&doc_id, std::slice::from_ref(&tag.id))
            .await
            .expect("set tag");

        // Metadata search by title, original filename, tag name, folder name, and subject name
        for query in [
            "第3回演習",
            "eigenvalue_sheet",
            "期末対策",
            "解析学",
            "大学数学",
        ] {
            let found = state
                .list_documents(&DocumentFilter {
                    search_query: Some(query.to_string()),
                    ..Default::default()
                })
                .await
                .expect("search");
            assert_eq!(
                found.len(),
                1,
                "metadata search should match query: {query}"
            );
        }

        // Document move validation: mismatched subject/folder rejected
        assert!(
            state
                .update_document(
                    &doc_id,
                    None,
                    Some(&phys_id),
                    Some(Some(&f_root_b.id)),
                    None,
                    None,
                )
                .await
                .is_err()
        );

        // Move document to phys_id + f_phys, then clear folder back to subject root
        let moved_doc = state
            .update_document(
                &doc_id,
                None,
                Some(&phys_id),
                Some(Some(&f_phys.id)),
                None,
                None,
            )
            .await
            .expect("move doc to physics folder");
        assert_eq!(moved_doc.document.subject_id, phys_id);
        assert_eq!(
            moved_doc.document.folder_id.as_deref(),
            Some(f_phys.id.as_str())
        );

        // Subject with documents cannot be deleted (FK RESTRICT / conflict)
        assert!(state.delete_subject(&phys_id).await.is_err());

        // Deleting folder sets document.folder_id = NULL without deleting document
        state
            .delete_folder(&f_phys.id)
            .await
            .expect("delete folder");
        let after_folder_del = state
            .get_document_detail(&doc_id)
            .await
            .expect("doc must survive folder deletion");
        assert!(after_folder_del.document.folder_id.is_none());

        // Deleting parent folder cascades child folder
        state
            .delete_folder(&f_root_a.id)
            .await
            .expect("delete parent folder");
        assert!(state.list_folders().await.expect("list folders").is_empty());

        // Deleting tag cascades document_tags
        state.delete_tag(&tag.id).await.expect("delete tag");
        let after_tag_del = state
            .get_document_detail(&doc_id)
            .await
            .expect("doc after tag delete");
        assert!(after_tag_del.tags.is_empty());
    }

    #[tokio::test]
    async fn test_phase2_mock_end_to_end_workflow_and_restart_recovery() {
        use crate::agents::adapter::PermissionDecision;
        use crate::domain::{
            GenerateSolutionRequest, RecognizeProblemsRequest, SaveSolutionEditRequest,
            SendChatMessageRequest,
        };

        let dir = TempDir::new().expect("tempdir");
        let state = AppState::initialize(dir.path().to_path_buf())
            .await
            .expect("init");
        let boot = state.bootstrap().await.expect("bootstrap");
        let math_id = boot.subjects[0].id.clone();

        // Find seeded Mock profile
        let profiles = state.list_agent_profiles().await.expect("profiles");
        let mock_profile = profiles
            .iter()
            .find(|p| p.provider_kind == "mock")
            .expect("mock profile seeded")
            .clone();

        let png_b64 = STANDARD.encode(sample_png_bytes());
        let pdf_b64 = STANDARD.encode(sample_pdf_with_pages(2));
        let imported = state
            .import_file_bytes(&math_id, None, "[TEST_REPEATED_PROBLEM].pdf", &pdf_b64)
            .await
            .expect("import two-page PDF");
        let doc_id = imported.detail.document.id.clone();
        let page = |page_number| crate::domain::PageImagePayload {
            page_number,
            png_base64: png_b64.clone(),
            extracted_text: Some(format!("[TEST_PAGE_{page_number}]")),
        };
        let recognize = |range: Option<(i64, i64)>, page_images, crop| RecognizeProblemsRequest {
            document_id: doc_id.clone(),
            agent_profile_id: Some(mock_profile.id.clone()),
            model_name: None,
            recognition_mode: if range.is_some() {
                "page_range"
            } else {
                "printed_only"
            }
            .into(),
            page_range_start: range.map(|(start, _)| start),
            page_range_end: range.map(|(_, end)| end),
            page_images,
            cropped_region_png_base64: crop,
        };

        for page_images in [
            serde_json::json!([]),
            serde_json::json!([{"pageNumber": 1, "pngBase64": "", "extractedText": "  "}]),
        ] {
            let input = serde_json::from_value(serde_json::json!({
                "documentId": doc_id,
                "agentProfileId": mock_profile.id,
                "recognitionMode": "printed_only",
                "pageImages": page_images,
            }))
            .expect("IPC request");
            assert!(matches!(
                state.recognize_problems(&input).await,
                Err(AppError::Validation(_))
            ));
        }
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM agent_runs")
                .fetch_one(state.db.read().await.pool())
                .await
                .expect("run count"),
            0
        );
        let rec = state
            .recognize_problems(&recognize(None, vec![page(1), page(2)], None))
            .await
            .expect("recognize both pages");
        assert!(rec.schema_issues.is_empty());
        assert_eq!(rec.problems.len(), 2);
        let first = &rec.problems[0];
        let second = &rec.problems[1];
        assert_eq!(first.problem_number, second.problem_number);
        assert_eq!(first.source_pages, [1]);
        assert_eq!(second.source_pages, [2]);
        let prob_id = first.id.clone();

        // 2. Split & merge problem editing
        let after_split = state
            .split_problem(&prob_id, "前半の問題文", "後半の小問", Some("(2)"))
            .await
            .expect("split");
        assert_eq!(after_split.len(), 3);
        let split_part = after_split
            .iter()
            .find(|item| item.id != prob_id && item.id != second.id)
            .expect("split part");
        let after_merge = state
            .merge_problems(&prob_id, &split_part.id)
            .await
            .expect("merge");
        assert_eq!(after_merge.len(), 2);
        let second = after_merge
            .iter()
            .find(|problem| problem.id == second.id)
            .expect("other page");
        let edit = crate::domain::UpsertProblemRequest {
            id: Some(prob_id.clone()),
            document_id: doc_id.clone(),
            problem_number: first.problem_number.clone(),
            subproblem_number: first.subproblem_number.clone(),
            subject_category: first.subject_category.clone(),
            field: first.field.clone(),
            statement_markdown: "確認済みの問題文".into(),
            given_conditions: vec!["手動で修正した条件".into()],
            target_objective: first.target_objective.clone(),
            source_pages: first.source_pages.clone(),
            figure_refs: first.figure_refs.clone(),
            uncertainties: first.uncertainties.clone(),
            is_user_verified: true,
        };
        let edited = state
            .upsert_problem(&edit)
            .await
            .expect("confirm first problem");

        // 3. Solution generation + Schema v1 validation + Typst PDF compilation (v1)
        let generated = state
            .generate_solution(&GenerateSolutionRequest {
                document_id: doc_id.clone(),
                problem_id: Some(prob_id.clone()),
                agent_profile_id: Some(mock_profile.id.clone()),
                model_name: Some("mock-math-pro".to_string()),
                mode: "detailed".to_string(),
                target_scope: "single_problem".to_string(),
                page_range_start: None,
                page_range_end: None,
                repair_feedback: None,
                parent_solution_id: None,
            })
            .await
            .expect("generate solution");
        assert!(generated.schema_issues.is_empty());
        assert!(generated.latex_errors.is_empty());
        assert!(generated.pdf_error.is_none());
        assert_eq!(generated.solution.version_number, 1);
        assert!(generated.solution.is_current);
        assert!(generated.solution.pdf_attachment_id.is_some());

        // Verify compiled PDF bytes are real PDF
        let pdf_att_id = generated
            .solution
            .pdf_attachment_id
            .clone()
            .expect("pdf attachment");
        let pdf_bin = state
            .read_attachment_base64(&pdf_att_id)
            .await
            .expect("read pdf");
        let pdf_raw = STANDARD.decode(&pdf_bin.base64_data).expect("decode pdf");
        assert!(pdf_raw.starts_with(b"%PDF-"));

        // 4. Non-destructive manual edit (v2) + restore v1 -> creates v3
        let edited_json = generated.solution.structured_json.replace(
            "ガウス積分の計算と収束の証明",
            "ガウス積分の厳密解答（手動改訂）",
        );
        let v2 = state
            .save_solution_edit(&SaveSolutionEditRequest {
                document_id: doc_id.clone(),
                problem_id: Some(prob_id.clone()),
                parent_solution_id: Some(generated.solution.id.clone()),
                mode: "detailed".to_string(),
                structured_json: edited_json,
                change_summary: Some("タイトル修正".to_string()),
            })
            .await
            .expect("save v2");
        assert_eq!(v2.solution.version_number, 2);
        assert_eq!(v2.solution.validation_status, "user_edited");

        let v3 = state
            .restore_solution_version(&generated.solution.id)
            .await
            .expect("restore v1 as v3");
        assert_eq!(v3.solution.version_number, 3);
        let all_versions = state
            .list_solutions(&doc_id, Some(&prob_id))
            .await
            .expect("list versions");
        assert_eq!(all_versions.len(), 3);
        assert_eq!(all_versions.iter().filter(|s| s.is_current).count(), 1);

        // 5. Chat streaming, session resume, cancellation, and permission response
        let conv = state
            .create_conversation(
                &doc_id,
                Some(&prob_id),
                Some(&v3.solution.id),
                "質問スレッド",
            )
            .await
            .expect("conv");
        // Each request takes the same application path as the recognition IPC command.
        let requests = [
            recognize(Some((2, 2)), vec![page(2)], None),
            recognize(Some((2, 2)), vec![], Some(png_b64.clone())),
            recognize(None, vec![page(1), page(2)], None),
            recognize(None, vec![page(1), page(2)], None),
        ];
        for request in requests {
            let rerun = state
                .recognize_problems(&request)
                .await
                .expect("repeat recognition");
            assert!(rerun.schema_issues.is_empty());
            assert_eq!(
                serde_json::to_value(rerun.problems).expect("serialize problems"),
                serde_json::json!([edited, second])
            );
            let versions = state
                .list_solutions(&doc_id, Some(&prob_id))
                .await
                .expect("versions");
            assert_eq!(
                versions
                    .iter()
                    .map(|version| &version.id)
                    .collect::<Vec<_>>(),
                all_versions
                    .iter()
                    .map(|version| &version.id)
                    .collect::<Vec<_>>()
            );
            let conversations = state
                .list_conversations(&doc_id)
                .await
                .expect("conversations");
            assert_eq!(conversations[0].id, conv.id);
            assert_eq!(
                conversations[0].problem_id.as_deref(),
                Some(prob_id.as_str())
            );
            assert_eq!(
                conversations[0].solution_id.as_deref(),
                Some(v3.solution.id.as_str())
            );
        }

        let ambiguous = state
            .upsert_problem(&crate::domain::UpsertProblemRequest {
                id: None,
                statement_markdown: "同じページ・同じ番号の別の手入力".into(),
                ..edit
            })
            .await
            .expect("add ambiguous candidate");
        let repeated = state
            .recognize_problems(&recognize(Some((1, 1)), vec![page(1)], None))
            .await
            .expect("recognize ambiguous candidate");
        assert_eq!(
            serde_json::to_value(repeated.problems).expect("serialize candidates"),
            serde_json::json!([edited, second, ambiguous])
        );

        let chat_run_id = Uuid::new_v4().to_string();
        let turn1 = state
            .send_chat_message(&SendChatMessageRequest {
                run_id: Some(chat_run_id.clone()),
                conversation_id: conv.id.clone(),
                document_id: doc_id.clone(),
                agent_profile_id: Some(mock_profile.id.clone()),
                model_name: Some("mock-math-pro".to_string()),
                content_markdown: "ヤコビアン r はなぜ必要ですか？".to_string(),
                referenced_problem_id: Some(prob_id.clone()),
                include_problem_context: true,
                include_solution_context: true,
                attached_images: Vec::new(),
            })
            .await
            .expect("chat turn 1");
        assert_eq!(turn1.run_id, chat_run_id);
        assert_eq!(
            turn1.assistant_message.agent_run_id.as_deref(),
            Some(chat_run_id.as_str())
        );
        assert_eq!(turn1.assistant_message.status, "completed");
        assert!(!turn1.assistant_message.content_markdown.is_empty());

        // Second turn reuses/resumes session
        let turn2 = state
            .send_chat_message(&SendChatMessageRequest {
                run_id: None,
                conversation_id: conv.id.clone(),
                document_id: doc_id.clone(),
                agent_profile_id: Some(mock_profile.id.clone()),
                model_name: Some("mock-math-pro".to_string()),
                content_markdown: "極座標の範囲も教えてください".to_string(),
                referenced_problem_id: Some(prob_id.clone()),
                include_problem_context: false,
                include_solution_context: false,
                attached_images: Vec::new(),
            })
            .await
            .expect("chat turn 2");
        assert!(turn2.resumed_external_session);

        // Verify permission decision channel works end-to-end
        let state_arc = Arc::new(state);
        let (perm_tx, mut perm_rx) = tokio::sync::mpsc::unbounded_channel::<(String, String)>();
        state_arc
            .set_agent_event_emitter(Arc::new(move |env| {
                if let crate::domain::AgentEventPayload::PermissionRequested {
                    request_id, ..
                } = &env.event
                {
                    let _ = perm_tx.send((env.run_id.clone(), request_id.clone()));
                }
            }))
            .await;

        let state_for_perm = Arc::clone(&state_arc);
        tokio::spawn(async move {
            if let Some((rid, req_id)) = perm_rx.recv().await {
                let _ = state_for_perm
                    .respond_agent_permission(&rid, &req_id, PermissionDecision::Deny)
                    .await;
            }
        });
        let perm_turn = state_arc
            .send_chat_message(&SendChatMessageRequest {
                run_id: None,
                conversation_id: conv.id.clone(),
                document_id: doc_id.clone(),
                agent_profile_id: Some(mock_profile.id.clone()),
                model_name: None,
                content_markdown: "[TEST_PERMISSION]".to_string(),
                referenced_problem_id: None,
                include_problem_context: false,
                include_solution_context: false,
                attached_images: Vec::new(),
            })
            .await
            .expect("permission turn");
        assert!(
            perm_turn
                .assistant_message
                .content_markdown
                .contains("拒否")
        );

        // 6. Restart persistence check (problems, 3 solution versions, conversation history all survive)
        drop(state_arc);
        let restarted = AppState::initialize(dir.path().to_path_buf())
            .await
            .expect("re-init");
        let after_restart = restarted
            .list_problems(&doc_id)
            .await
            .expect("problems after restart");
        assert_eq!(
            serde_json::to_value(after_restart).expect("serialize restarted problems"),
            serde_json::json!([edited, second, ambiguous])
        );
        assert_eq!(
            restarted
                .list_solutions(&doc_id, Some(&prob_id))
                .await
                .expect("s")
                .len(),
            3
        );
        assert_eq!(restarted.list_messages(&conv.id).await.expect("m").len(), 6);
    }

    /// Live real-harness verification against locally installed & authenticated OpenAI Codex CLI.
    /// Explicitly gated with `#[ignore]` so CI stays hermetic while allowing live verification via:
    /// `cargo test -- --ignored test_live_codex_full_phase2_workflow --nocapture`
    #[tokio::test]
    #[ignore]
    async fn test_live_codex_full_phase2_workflow() {
        use crate::domain::{
            GenerateSolutionRequest, RecognizeProblemsRequest, SendChatMessageRequest,
        };

        let dir = TempDir::new().expect("tempdir");
        let state = AppState::initialize(dir.path().to_path_buf())
            .await
            .expect("init");
        let boot = state.bootstrap().await.expect("bootstrap");
        let math_id = boot.subjects[0].id.clone();

        let profiles = state.list_agent_profiles().await.expect("profiles");
        let codex_profile = profiles
            .iter()
            .find(|p| p.provider_kind == "codex")
            .expect("codex profile")
            .clone();

        let status = state
            .check_agent_status(&codex_profile.id)
            .await
            .expect("check codex status");
        assert!(
            status.installed,
            "codex CLI must be installed for live test"
        );
        assert_eq!(status.auth_status, "authenticated");

        // Load Phase 0 test problem image (`2X + 3 = 11`)
        let img_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("poc")
            .join("mock-acp-agent")
            .join("test_problem.png");
        let img_bytes = std::fs::read(&img_path).expect("read poc test_problem.png");
        let imported = state
            .import_file_bytes(
                &math_id,
                None,
                "test_problem.png",
                &STANDARD.encode(&img_bytes),
            )
            .await
            .expect("import test problem");
        let doc_id = imported.detail.document.id.clone();

        // 1. Live Recognition
        let rec = state
            .recognize_problems(&RecognizeProblemsRequest {
                document_id: doc_id.clone(),
                agent_profile_id: Some(codex_profile.id.clone()),
                model_name: None,
                recognition_mode: "printed_only".to_string(),
                page_range_start: None,
                page_range_end: None,
                page_images: Vec::new(),
                cropped_region_png_base64: None,
            })
            .await
            .expect("live recognition");
        assert!(
            rec.schema_issues.is_empty(),
            "live recognition schema issues: {:?}",
            rec.schema_issues
        );
        assert!(!rec.problems.is_empty());
        let prob_id = rec.problems[0].id.clone();

        // 2. Live Solution Generation + Typst PDF
        let generated = state
            .generate_solution(&GenerateSolutionRequest {
                document_id: doc_id.clone(),
                problem_id: Some(prob_id.clone()),
                agent_profile_id: Some(codex_profile.id.clone()),
                model_name: None,
                mode: "standard".to_string(),
                target_scope: "single_problem".to_string(),
                page_range_start: None,
                page_range_end: None,
                repair_feedback: None,
                parent_solution_id: None,
            })
            .await
            .expect("live solution");
        assert!(
            generated.schema_issues.is_empty(),
            "schema issues: {:?}",
            generated.schema_issues
        );
        assert!(
            generated.latex_errors.is_empty(),
            "latex errors: {:?}",
            generated.latex_errors
        );
        assert!(
            generated.pdf_error.is_none(),
            "pdf error: {:?}",
            generated.pdf_error
        );
        assert!(generated.solution.pdf_attachment_id.is_some());

        // 3. Live Chat streaming + restart persistence
        let conv = state
            .create_conversation(
                &doc_id,
                Some(&prob_id),
                Some(&generated.solution.id),
                "実機確認チャット",
            )
            .await
            .expect("conv");
        let chat = state
            .send_chat_message(&SendChatMessageRequest {
                run_id: None,
                conversation_id: conv.id.clone(),
                document_id: doc_id.clone(),
                agent_profile_id: Some(codex_profile.id.clone()),
                model_name: None,
                content_markdown: "この方程式の解を検算して一言で答えてください。".to_string(),
                referenced_problem_id: Some(prob_id.clone()),
                include_problem_context: true,
                include_solution_context: true,
                attached_images: Vec::new(),
            })
            .await
            .expect("live chat");
        assert_eq!(chat.assistant_message.status, "completed");
        assert!(chat.assistant_message.content_markdown.contains('4'));
    }

    #[tokio::test]
    async fn test_phase3_physics_verified_solution_real_science_mcp_pdf_plot_and_spoof_defense() {
        use crate::domain::{GenerateSolutionRequest, RunScienceToolRequest, UpsertProblemRequest};
        use crate::storage::sha256_hex;

        // Snapshot user's ~/.codex/config.toml before running to guarantee zero mutation
        let codex_cfg_path = std::env::var_os("USERPROFILE")
            .or_else(|| std::env::var_os("HOME"))
            .map(|h| PathBuf::from(h).join(".codex").join("config.toml"));
        let codex_cfg_hash_before = codex_cfg_path
            .as_ref()
            .and_then(|p| std::fs::read(p).ok())
            .map(|b| sha256_hex(&b));

        let dir = TempDir::new().expect("tempdir");
        let state = AppState::initialize(dir.path().to_path_buf())
            .await
            .expect("init");
        let boot = state.bootstrap().await.expect("bootstrap");
        let phys_id = boot.subjects[1].id.clone();

        let profiles = state.list_agent_profiles().await.expect("profiles");
        let mock_profile = profiles
            .iter()
            .find(|p| p.provider_kind == "mock")
            .expect("mock profile")
            .clone();

        // 1. Live probe of local Python Science MCP Server (21 tools, zero generic python exec)
        let mcp_status = state
            .check_science_mcp_status()
            .await
            .expect("science mcp status");
        assert!(
            mcp_status.server_ready,
            "Science MCP must be ready: {:?}",
            mcp_status.detail_note
        );
        assert_eq!(mcp_status.tool_count, 21);
        assert!(
            !mcp_status
                .tools
                .iter()
                .any(|t| t.name == "run_python" || t.name == "shell")
        );

        // Import a physics problem image & add structured physics problem
        let imported = state
            .import_file_bytes(
                &phys_id,
                None,
                "damped_oscillation.png",
                &STANDARD.encode(sample_png_bytes()),
            )
            .await
            .expect("import physics problem");
        let doc_id = imported.detail.document.id.clone();

        let prob = state
            .upsert_problem(&UpsertProblemRequest {
                id: None,
                document_id: doc_id.clone(),
                problem_number: "第1問".to_string(),
                subproblem_number: Some("(1)".to_string()),
                subject_category: "physics".to_string(),
                field: Some("classical_mechanics".to_string()),
                statement_markdown:
                    "減衰調和振動子 $x''(t) + 2x'(t) + 5x(t) = 0$ を $x(0)=1, x'(0)=0$ のもとで解き、運動方程式の次元整合性と変位グラフを示せ。"
                        .to_string(),
                given_conditions: vec!["x(0) = 1".to_string(), "x'(0) = 0".to_string()],
                target_objective: Some("x(t) の厳密解とグラフ".to_string()),
                source_pages: vec![1],
                figure_refs: Vec::new(),
                uncertainties: Vec::new(),
                is_user_verified: true,
            })
            .await
            .expect("upsert physics problem");

        // 2. Security test: AI spoofing `source_kind: "science_mcp"` without executing any tool
        // MUST be forcibly downgraded to `ai_self_check`.
        let spoof_prob = state
            .upsert_problem(&UpsertProblemRequest {
                id: None,
                document_id: doc_id.clone(),
                problem_number: "第2問".to_string(),
                subproblem_number: None,
                subject_category: "mathematics".to_string(),
                field: Some("calculus".to_string()),
                statement_markdown: "[TEST_SPOOF_SCIENCE_MCP] 詐称防御検証".to_string(),
                given_conditions: Vec::new(),
                target_objective: None,
                source_pages: vec![1],
                figure_refs: Vec::new(),
                uncertainties: Vec::new(),
                is_user_verified: true,
            })
            .await
            .expect("upsert spoof problem");

        let spoof_sol = state
            .generate_solution(&GenerateSolutionRequest {
                document_id: doc_id.clone(),
                problem_id: Some(spoof_prob.id.clone()),
                agent_profile_id: Some(mock_profile.id.clone()),
                model_name: Some("mock-math-pro".to_string()),
                mode: "verified".to_string(),
                target_scope: "single_problem".to_string(),
                page_range_start: None,
                page_range_end: None,
                repair_feedback: None,
                parent_solution_id: None,
            })
            .await
            .expect("generate spoof solution");
        let spoof_verifs = state
            .list_solution_verifications(&spoof_sol.solution.id)
            .await
            .expect("spoof verifs");
        assert!(!spoof_verifs.is_empty());
        assert!(
            spoof_verifs
                .iter()
                .all(|v| v.source_kind == "ai_self_check"),
            "Unbacked AI science_mcp claim MUST be downgraded to ai_self_check: {spoof_verifs:?}"
        );

        // 3. Verified mode solution generation via MockAdapter emitting real MCP tool events + SVG plot
        let verified_sol = state
            .generate_solution(&GenerateSolutionRequest {
                document_id: doc_id.clone(),
                problem_id: Some(prob.id.clone()),
                agent_profile_id: Some(mock_profile.id.clone()),
                model_name: Some("mock-math-pro".to_string()),
                mode: "verified".to_string(),
                target_scope: "single_problem".to_string(),
                page_range_start: None,
                page_range_end: None,
                repair_feedback: None,
                parent_solution_id: None,
            })
            .await
            .expect("generate verified physics solution");
        assert!(verified_sol.schema_issues.is_empty());
        assert!(verified_sol.latex_errors.is_empty());
        assert!(verified_sol.pdf_error.is_none());
        assert!(verified_sol.solution.pdf_attachment_id.is_some());

        let v1_verifs = state
            .list_solution_verifications(&verified_sol.solution.id)
            .await
            .expect("v1 verifs");
        assert!(
            v1_verifs
                .iter()
                .any(|v| v.source_kind == "science_mcp" && v.status == "passed")
        );
        assert!(v1_verifs.iter().any(|v| v.source_kind == "ai_self_check"));

        let v1_arts = state
            .list_solution_artifacts(&verified_sol.solution.id)
            .await
            .expect("v1 artifacts");
        assert_eq!(v1_arts.len(), 1);
        assert!(v1_arts[0].relative_path.starts_with("attachments/plots/"));

        // 4. Real Python Science MCP direct execution:
        // (a) Electromagnetism unit conversion & dimensional analysis
        let dim_run = state
            .run_science_tool(&RunScienceToolRequest {
                document_id: doc_id.clone(),
                solution_id: Some(verified_sol.solution.id.clone()),
                tool_name: "check_dimensions".to_string(),
                arguments: serde_json::json!({
                    "lhs_expression": "F",
                    "rhs_expression": "q * v * B",
                    "unit_map": { "F": "N", "q": "C", "v": "m/s", "B": "T" }
                }),
                attach_to_solution_pdf: false,
            })
            .await
            .expect("real check_dimensions");
        assert_eq!(dim_run.execution.status, "succeeded");
        assert_eq!(
            dim_run.verification.as_ref().map(|v| v.status.as_str()),
            Some("passed")
        );

        // (b) Real ODE solver (damped oscillator)
        let ode_run = state
            .run_science_tool(&RunScienceToolRequest {
                document_id: doc_id.clone(),
                solution_id: Some(verified_sol.solution.id.clone()),
                tool_name: "solve_ode".to_string(),
                arguments: serde_json::json!({
                    "equation": "Derivative(y(t), t, 2) + 2*Derivative(y(t), t) + 5*y(t) = 0",
                    "func_name": "y",
                    "variable": "t",
                    "ics": { "y(0)": "1", "y'(0)": "0" }
                }),
                attach_to_solution_pdf: false,
            })
            .await
            .expect("real solve_ode");
        assert_eq!(ode_run.execution.status, "succeeded");

        // (c) Real Matplotlib SVG plot generation + automatic embedding into solution PDF (creates v2)
        let plot_run = state
            .run_science_tool(&RunScienceToolRequest {
                document_id: doc_id.clone(),
                solution_id: Some(verified_sol.solution.id.clone()),
                tool_name: "plot_function".to_string(),
                arguments: serde_json::json!({
                    "expressions": ["exp(-t) * (cos(2*t) + sin(2*t)/2)", "exp(-t)"],
                    "variable": "t",
                    "x_range": [0, 6],
                    "labels": ["x(t)", "envelope"],
                    "title": "Damped Harmonic Oscillator",
                    "output_format": "svg"
                }),
                attach_to_solution_pdf: true,
            })
            .await
            .expect("real plot_function + embed");
        assert_eq!(plot_run.execution.status, "succeeded");
        assert_eq!(plot_run.created_artifacts.len(), 1);
        assert!(
            plot_run.created_artifacts[0]
                .relative_path
                .starts_with("attachments/plots/")
        );
        assert_eq!(plot_run.created_artifacts[0].mime_type, "image/svg+xml");

        let updated_sol = plot_run
            .updated_solution
            .expect("updated solution v2 with real SVG plot");
        assert_eq!(updated_sol.solution.version_number, 2);
        assert!(updated_sol.pdf_error.is_none());
        let pdf_id = updated_sol
            .solution
            .pdf_attachment_id
            .clone()
            .expect("v2 pdf");
        let pdf_bin = state
            .read_attachment_base64(&pdf_id)
            .await
            .expect("read v2 pdf");
        let pdf_raw = STANDARD.decode(&pdf_bin.base64_data).expect("decode pdf");
        assert!(pdf_raw.starts_with(b"%PDF-"));

        // Verify Typst source contains embedded SVG figure
        let typ_id = updated_sol
            .solution
            .typst_source_attachment_id
            .clone()
            .expect("typst source");
        let typ_bin = state
            .read_attachment_base64(&typ_id)
            .await
            .expect("read typ");
        let typ_text =
            String::from_utf8(STANDARD.decode(&typ_bin.base64_data).expect("decode typ"))
                .expect("utf8");
        assert!(typ_text.contains("image(\"artifacts/fig_"));

        // 5. Restart persistence check for tool_executions, verification_results & solution_artifacts
        drop(state);
        let restarted = AppState::initialize(dir.path().to_path_buf())
            .await
            .expect("re-init");
        let execs = restarted
            .list_document_tool_executions(&doc_id)
            .await
            .expect("list execs");
        assert!(
            execs.len() >= 5,
            "expected at least 5 tool executions, got {}",
            execs.len()
        );
        let v2_arts = restarted
            .list_solution_artifacts(&updated_sol.solution.id)
            .await
            .expect("v2 artifacts after restart");
        assert!(!v2_arts.is_empty());

        // 6. Verify user's global ~/.codex/config.toml was NEVER modified
        let codex_cfg_hash_after = codex_cfg_path
            .as_ref()
            .and_then(|p| std::fs::read(p).ok())
            .map(|b| sha256_hex(&b));
        assert_eq!(
            codex_cfg_hash_before, codex_cfg_hash_after,
            "User global ~/.codex/config.toml must NEVER be mutated"
        );
    }

    /// Live real-harness verification: OpenAI Codex + real Python Science MCP Server over `-c` scoped config,
    /// proving real MCP tool invocation + `~/.codex/config.toml` SHA-256 non-mutation.
    /// Run via: `cargo test --lib -- --ignored test_live_codex_phase3_science_mcp --nocapture`
    #[tokio::test]
    #[ignore]
    async fn test_live_codex_phase3_science_mcp() {
        use crate::domain::{GenerateSolutionRequest, UpsertProblemRequest};
        use crate::storage::sha256_hex;

        let codex_cfg_path = std::env::var_os("USERPROFILE")
            .or_else(|| std::env::var_os("HOME"))
            .map(|h| PathBuf::from(h).join(".codex").join("config.toml"));
        let hash_before = codex_cfg_path
            .as_ref()
            .and_then(|p| std::fs::read(p).ok())
            .map(|b| sha256_hex(&b));

        let dir = TempDir::new().expect("tempdir");
        let state = AppState::initialize(dir.path().to_path_buf())
            .await
            .expect("init");
        let boot = state.bootstrap().await.expect("bootstrap");
        let phys_id = boot.subjects[1].id.clone();

        let profiles = state.list_agent_profiles().await.expect("profiles");
        let codex = profiles
            .iter()
            .find(|p| p.provider_kind == "codex")
            .expect("codex profile")
            .clone();

        let imported = state
            .import_file_bytes(
                &phys_id,
                None,
                "live_physics.png",
                &STANDARD.encode(sample_png_bytes()),
            )
            .await
            .expect("import");
        let doc_id = imported.detail.document.id.clone();

        let prob = state
            .upsert_problem(&UpsertProblemRequest {
                id: None,
                document_id: doc_id.clone(),
                problem_number: "第1問".to_string(),
                subproblem_number: None,
                subject_category: "physics".to_string(),
                field: Some("classical_mechanics".to_string()),
                statement_markdown:
                    "減衰調和振動 $x''(t) + 2x'(t) + 5x(t) = 0$（初期条件 $x(0)=1, x'(0)=0$）を science_mcp の `solve_ode` と `check_dimensions`（$F=ma$）および `plot_function`（$t \\in [0, 6]$）を必ず使って解き、グラフ付きで解答せよ。"
                        .to_string(),
                given_conditions: vec!["x(0) = 1".to_string(), "x'(0) = 0".to_string()],
                target_objective: Some("x(t) の厳密解、次元検証、グラフ".to_string()),
                source_pages: vec![1],
                figure_refs: Vec::new(),
                uncertainties: Vec::new(),
                is_user_verified: true,
            })
            .await
            .expect("upsert prob");

        let res = state
            .generate_solution(&GenerateSolutionRequest {
                document_id: doc_id.clone(),
                problem_id: Some(prob.id.clone()),
                agent_profile_id: Some(codex.id.clone()),
                model_name: None,
                mode: "verified".to_string(),
                target_scope: "single_problem".to_string(),
                page_range_start: None,
                page_range_end: None,
                repair_feedback: None,
                parent_solution_id: None,
            })
            .await
            .expect("live codex verified solution");
        assert!(
            res.schema_issues.is_empty(),
            "schema issues: {:?}",
            res.schema_issues
        );
        assert!(
            res.latex_errors.is_empty(),
            "latex errors: {:?}",
            res.latex_errors
        );
        assert!(res.pdf_error.is_none(), "pdf error: {:?}", res.pdf_error);

        let execs = state
            .list_document_tool_executions(&doc_id)
            .await
            .expect("execs");
        let verifs = state
            .list_solution_verifications(&res.solution.id)
            .await
            .expect("verifs");
        let arts = state
            .list_solution_artifacts(&res.solution.id)
            .await
            .expect("arts");
        println!(
            "[live_codex_phase3] tool_executions={} ({:?}), verifications={}, artifacts={}, pdf={:?}",
            execs.len(),
            execs
                .iter()
                .map(|e| format!("{}:{}:{}", e.server_name, e.tool_name, e.status))
                .collect::<Vec<_>>(),
            verifs.len(),
            arts.len(),
            res.solution.pdf_attachment_id
        );
        assert!(
            execs
                .iter()
                .any(|e| e.server_name == "science_mcp" && e.status == "succeeded"),
            "Codex must have successfully executed at least one science_mcp tool"
        );
        assert!(
            verifs
                .iter()
                .any(|v| v.source_kind == "science_mcp" && v.status == "passed"),
            "At least one authentic science_mcp verification must be recorded"
        );
        assert!(
            !arts.is_empty(),
            "Generated plot artifact from Codex + science_mcp must be attached to the solution"
        );
        assert!(
            res.solution.pdf_attachment_id.is_some(),
            "PDF must be generated"
        );

        let hash_after = codex_cfg_path
            .as_ref()
            .and_then(|p| std::fs::read(p).ok())
            .map(|b| sha256_hex(&b));
        assert_eq!(
            hash_before, hash_after,
            "~/.codex/config.toml must remain untouched"
        );
    }
}

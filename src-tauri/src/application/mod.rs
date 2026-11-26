pub mod ai_service;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use tokio::sync::RwLock;
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
use crate::security::DiagnosticLogger;
use crate::storage::AppStorage;

pub struct AppState {
    pub(crate) db: RwLock<Database>,
    pub(crate) storage: AppStorage,
    pub(crate) logger: DiagnosticLogger,
    pub(crate) agents: Arc<AgentManager>,
}

impl AppState {
    pub async fn initialize(data_root: PathBuf) -> AppResult<Self> {
        let storage = AppStorage::initialize(data_root)?;
        let logger = DiagnosticLogger::new(&storage.logs_dir());
        let db = Database::connect(&storage.database_path()).await?;
        let workspaces_dir = storage.root().join("workspaces");
        let agents = Arc::new(AgentManager::new(workspaces_dir));
        logger.log_info("app_started", "Theorem core initialized");
        Ok(Self {
            db: RwLock::new(db),
            storage,
            logger,
            agents,
        })
    }

    pub async fn set_agent_event_emitter(&self, emitter: EventEmitter) {
        self.agents.set_emitter(emitter).await;
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
        let db = self.db.read().await;
        let item = document::import_bytes(
            db.pool(),
            &self.storage,
            subject_id,
            folder_id,
            filename,
            &bytes,
        )
        .await?;
        self.logger.log_info(
            "document_imported_bytes",
            &format!("document_id={}", item.detail.document.id),
        );
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
        {
            let mut db_guard = self.db.write().await;
            db_guard.close().await;
            let restore_res = self.storage.restore_backup_archive(backup_rel_path);
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
        let mut doc = lopdf::Document::with_version("1.5");
        let pages_id = doc.new_object_id();
        let page_id = doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()],
        });
        doc.objects.insert(
            pages_id,
            lopdf::Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Kids" => vec![page_id.into()],
                "Count" => 1,
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

        // Import image problem
        let png_b64 = STANDARD.encode(sample_png_bytes());
        let imported = state
            .import_file_bytes(&math_id, None, "gaussian_integral.png", &png_b64)
            .await
            .expect("import png");
        let doc_id = imported.detail.document.id.clone();

        // 1. Problem recognition via MockAdapter
        let rec = state
            .recognize_problems(&RecognizeProblemsRequest {
                document_id: doc_id.clone(),
                agent_profile_id: Some(mock_profile.id.clone()),
                model_name: Some("mock-math-pro".to_string()),
                recognition_mode: "printed_only".to_string(),
                page_range_start: None,
                page_range_end: None,
                page_images: Vec::new(),
                cropped_region_png_base64: None,
            })
            .await
            .expect("recognize");
        assert!(rec.schema_issues.is_empty());
        assert_eq!(rec.problems.len(), 1);
        let prob_id = rec.problems[0].id.clone();

        // 2. Split & merge problem editing
        let after_split = state
            .split_problem(&prob_id, "前半の問題文", "後半の小問", Some("(2)"))
            .await
            .expect("split");
        assert_eq!(after_split.len(), 2);
        let after_merge = state
            .merge_problems(&after_split[0].id, &after_split[1].id)
            .await
            .expect("merge");
        assert_eq!(after_merge.len(), 1);

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
        let turn1 = state
            .send_chat_message(&SendChatMessageRequest {
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
        assert_eq!(turn1.assistant_message.status, "completed");
        assert!(!turn1.assistant_message.content_markdown.is_empty());

        // Second turn reuses/resumes session
        let turn2 = state
            .send_chat_message(&SendChatMessageRequest {
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
        assert_eq!(restarted.list_problems(&doc_id).await.expect("p").len(), 1);
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
}

use std::path::{Path, PathBuf};

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use tokio::sync::RwLock;
use uuid::Uuid;

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
    db: RwLock<Database>,
    storage: AppStorage,
    logger: DiagnosticLogger,
}

impl AppState {
    pub async fn initialize(data_root: PathBuf) -> AppResult<Self> {
        let storage = AppStorage::initialize(data_root)?;
        let logger = DiagnosticLogger::new(&storage.logs_dir());
        let db = Database::connect(&storage.database_path()).await?;
        logger.log_info("app_started", "FormulaDesk core initialized");
        Ok(Self {
            db: RwLock::new(db),
            storage,
            logger,
        })
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

        let database_bytes = self.storage.file_size("database/formuladesk.db");
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
}

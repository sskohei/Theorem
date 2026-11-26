use std::fs;
use std::io::Cursor;
use std::path::Path;

use chrono::Utc;
use image::ImageFormat;
use sqlx::{Pool, Sqlite};
use uuid::Uuid;

use crate::database::library_repo::{self, NewAttachmentRecord};
use crate::domain::{AttachmentInfo, ImportedDocumentItem};
use crate::error::{AppError, AppResult};
use crate::storage::{AppStorage, sha256_hex, write_bytes_atomic_absolute};

/// Hard file size cap (100 MiB) to guard local memory and disk against accidental giant inputs.
pub const MAX_IMPORT_BYTES: usize = 100 * 1024 * 1024;
const THUMB_MAX_DIM: u32 = 320;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DetectedFormat {
    Pdf,
    Jpeg,
    Png,
    Webp,
}

impl DetectedFormat {
    pub fn extension(self) -> &'static str {
        match self {
            DetectedFormat::Pdf => "pdf",
            DetectedFormat::Jpeg => "jpg",
            DetectedFormat::Png => "png",
            DetectedFormat::Webp => "webp",
        }
    }

    pub fn mime_type(self) -> &'static str {
        match self {
            DetectedFormat::Pdf => "application/pdf",
            DetectedFormat::Jpeg => "image/jpeg",
            DetectedFormat::Png => "image/png",
            DetectedFormat::Webp => "image/webp",
        }
    }

    pub fn attachment_kind(self) -> &'static str {
        match self {
            DetectedFormat::Pdf => "original_pdf",
            DetectedFormat::Jpeg | DetectedFormat::Png | DetectedFormat::Webp => "original_image",
        }
    }

    pub fn source_type(self) -> &'static str {
        match self {
            DetectedFormat::Pdf => "pdf",
            DetectedFormat::Jpeg | DetectedFormat::Png | DetectedFormat::Webp => "image",
        }
    }
}

/// Validates file format strictly by magic bytes (never trusting filename extensions alone).
pub fn detect_supported_format(bytes: &[u8], filename: &str) -> AppResult<DetectedFormat> {
    if bytes.is_empty() {
        return Err(AppError::Validation(format!(
            "空のファイルは取り込めません: {filename}"
        )));
    }
    if bytes.len() > MAX_IMPORT_BYTES {
        return Err(AppError::Validation(format!(
            "ファイルサイズが上限（100 MB）を超えています: {filename}"
        )));
    }

    // PDF magic header (`%PDF-`, allowing up to 1024 bytes leading whitespace/junk per PDF spec)
    let probe_len = bytes.len().min(1024);
    if bytes[..probe_len].windows(5).any(|w| w == b"%PDF-") {
        return Ok(DetectedFormat::Pdf);
    }

    // PNG: 89 50 4E 47 0D 0A 1A 0A
    if bytes.len() >= 8 && &bytes[..8] == b"\x89PNG\r\n\x1a\n" {
        return Ok(DetectedFormat::Png);
    }

    // JPEG: FF D8 FF
    if bytes.len() >= 3 && bytes[0] == 0xFF && bytes[1] == 0xD8 && bytes[2] == 0xFF {
        return Ok(DetectedFormat::Jpeg);
    }

    // WebP: RIFF....WEBP
    if bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        return Ok(DetectedFormat::Webp);
    }

    Err(AppError::UnsupportedFormat(format!(
        "PDF / JPG / PNG / WebP 以外の形式は取り込めません: {filename}"
    )))
}

fn count_pdf_pages(bytes: &[u8]) -> Option<i64> {
    lopdf::Document::load_mem(bytes)
        .ok()
        .map(|doc| doc.get_pages().len() as i64)
}

fn inspect_image_and_thumbnail(
    bytes: &[u8],
    format: DetectedFormat,
) -> AppResult<(i64, i64, Vec<u8>)> {
    let img_fmt = match format {
        DetectedFormat::Png => ImageFormat::Png,
        DetectedFormat::Jpeg => ImageFormat::Jpeg,
        DetectedFormat::Webp => ImageFormat::WebP,
        DetectedFormat::Pdf => {
            return Err(AppError::Validation(
                "PDF に対して画像デコードは実行できません".into(),
            ));
        }
    };
    let img = image::load_from_memory_with_format(bytes, img_fmt).map_err(|e| {
        AppError::UnsupportedFormat(format!("画像データの読み込みに失敗しました: {e}"))
    })?;
    let w = i64::from(img.width());
    let h = i64::from(img.height());
    let thumb = img.thumbnail(THUMB_MAX_DIM, THUMB_MAX_DIM);
    let mut thumb_bytes = Vec::new();
    thumb
        .write_to(&mut Cursor::new(&mut thumb_bytes), ImageFormat::Png)
        .map_err(|e| AppError::Storage(format!("サムネイル生成に失敗しました: {e}")))?;
    Ok((w, h, thumb_bytes))
}

fn title_from_filename(filename: &str) -> String {
    let stem = Path::new(filename)
        .file_stem()
        .map(|s| s.to_string_lossy().trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "無題の問題".to_string());
    stem.chars().take(120).collect()
}

/// Imports raw file bytes into managed storage & SQLite transactionally.
/// - Never alters any source file on disk
/// - Writes atomically under `attachments/originals/<uuid>.<ext>`
/// - Detects SHA-256 duplicates against existing originals (notifying caller)
/// - Rolls back written files if DB transaction fails
pub async fn import_bytes(
    pool: &Pool<Sqlite>,
    storage: &AppStorage,
    subject_id: &str,
    folder_id: Option<&str>,
    original_filename: &str,
    bytes: &[u8],
) -> AppResult<ImportedDocumentItem> {
    let format = detect_supported_format(bytes, original_filename)?;
    let sha256 = sha256_hex(bytes);
    let duplicate_matches = library_repo::find_duplicate_originals(pool, &sha256).await?;

    let doc_id = Uuid::new_v4().to_string();
    let att_id = Uuid::new_v4().to_string();
    let rel_path = format!("attachments/originals/{att_id}.{}", format.extension());

    let (width, height, page_count, thumb_payload) = match format {
        DetectedFormat::Pdf => {
            let pages = count_pdf_pages(bytes).unwrap_or(1).max(1);
            (None, None, Some(pages), None)
        }
        DetectedFormat::Jpeg | DetectedFormat::Png | DetectedFormat::Webp => {
            let (w, h, thumb) = inspect_image_and_thumbnail(bytes, format)?;
            (Some(w), Some(h), Some(1), Some(thumb))
        }
    };

    // Write original file atomically BEFORE DB transaction
    storage.write_atomic(&rel_path, bytes)?;
    let mut written_paths = vec![rel_path.clone()];

    // Write image thumbnail atomically if generated
    let thumb_record = if let Some(thumb_bytes) = thumb_payload {
        let tid = Uuid::new_v4().to_string();
        let t_rel = format!("thumbnails/{tid}.png");
        if let Err(err) = storage.write_atomic(&t_rel, &thumb_bytes) {
            for p in &written_paths {
                let _ = storage.delete_if_exists(p);
            }
            return Err(err);
        }
        written_paths.push(t_rel.clone());
        Some((tid, t_rel, thumb_bytes))
    } else {
        None
    };

    let title = title_from_filename(original_filename);
    let ts = Utc::now().to_rfc3339();

    let db_result = (async {
        let mut tx = pool.begin().await?;
        sqlx::query(
            "INSERT INTO documents
             (id, subject_id, folder_id, title, source_type, primary_attachment_id, page_count,
              recognition_mode, study_status, is_favorite, last_opened_at, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'printed_only', 'unstarted', 0, ?8, ?8, ?8)",
        )
        .bind(&doc_id)
        .bind(subject_id)
        .bind(folder_id)
        .bind(&title)
        .bind(format.source_type())
        .bind(&att_id)
        .bind(page_count)
        .bind(&ts)
        .execute(&mut *tx)
        .await?;

        library_repo::insert_attachment_conn(
            &mut tx,
            &NewAttachmentRecord {
                id: att_id.clone(),
                document_id: Some(doc_id.clone()),
                kind: format.attachment_kind().to_string(),
                original_filename: original_filename.to_string(),
                relative_path: rel_path.clone(),
                mime_type: format.mime_type().to_string(),
                byte_size: bytes.len() as i64,
                sha256: sha256.clone(),
                width,
                height,
                page_count,
            },
        )
        .await?;

        if let Some((tid, t_rel, t_bytes)) = thumb_record {
            let t_sha = sha256_hex(&t_bytes);
            library_repo::insert_attachment_conn(
                &mut tx,
                &NewAttachmentRecord {
                    id: tid,
                    document_id: Some(doc_id.clone()),
                    kind: "thumbnail".to_string(),
                    original_filename: format!("{title}_thumb.png"),
                    relative_path: t_rel,
                    mime_type: "image/png".to_string(),
                    byte_size: t_bytes.len() as i64,
                    sha256: t_sha,
                    width: None,
                    height: None,
                    page_count: None,
                },
            )
            .await?;
        }

        tx.commit().await?;
        Ok::<(), AppError>(())
    })
    .await;

    if let Err(err) = db_result {
        for p in &written_paths {
            let _ = storage.delete_if_exists(p);
        }
        return Err(err);
    }

    let detail = library_repo::get_document_detail(pool, &doc_id).await?;
    Ok(ImportedDocumentItem {
        detail,
        duplicate_matches,
    })
}

/// Imports a file from an external OS path (selected via native dialog or OS drag-and-drop).
/// Never modifies the original file on disk.
pub async fn import_from_path(
    pool: &Pool<Sqlite>,
    storage: &AppStorage,
    subject_id: &str,
    folder_id: Option<&str>,
    source_path: &Path,
) -> AppResult<ImportedDocumentItem> {
    let meta = fs::metadata(source_path).map_err(|_| {
        AppError::NotFound(format!(
            "取り込み元ファイルが見つかりません: {}",
            source_path.display()
        ))
    })?;
    if !meta.is_file() {
        return Err(AppError::Validation(
            "フォルダではなくファイルを選択してください".into(),
        ));
    }
    if meta.len() as usize > MAX_IMPORT_BYTES {
        return Err(AppError::Validation(
            "ファイルサイズが上限（100 MB）を超えています".into(),
        ));
    }
    let filename = source_path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "problem".to_string());
    let bytes = fs::read(source_path)?;
    import_bytes(pool, storage, subject_id, folder_id, &filename, &bytes).await
}

/// Persists a PNG thumbnail rendered client-side by PDF.js for a PDF document.
pub async fn save_thumbnail_png(
    pool: &Pool<Sqlite>,
    storage: &AppStorage,
    document_id: &str,
    png_bytes: &[u8],
) -> AppResult<AttachmentInfo> {
    let format = detect_supported_format(png_bytes, "thumbnail.png")?;
    if format != DetectedFormat::Png {
        return Err(AppError::Validation(
            "サムネイルは PNG 形式である必要があります".into(),
        ));
    }
    // Confirm document exists
    let _ = library_repo::get_document_detail(pool, document_id).await?;

    let tid = Uuid::new_v4().to_string();
    let rel = format!("thumbnails/{tid}.png");
    let sha = sha256_hex(png_bytes);
    storage.write_atomic(&rel, png_bytes)?;

    let mut tx = pool.begin().await?;
    let info = library_repo::insert_attachment_conn(
        &mut tx,
        &NewAttachmentRecord {
            id: tid,
            document_id: Some(document_id.to_string()),
            kind: "thumbnail".to_string(),
            original_filename: format!("{document_id}_thumb.png"),
            relative_path: rel.clone(),
            mime_type: "image/png".to_string(),
            byte_size: png_bytes.len() as i64,
            sha256: sha,
            width: None,
            height: None,
            page_count: None,
        },
    )
    .await;

    match info {
        Ok(att) => {
            tx.commit().await?;
            Ok(att)
        }
        Err(err) => {
            let _ = storage.delete_if_exists(&rel);
            Err(err)
        }
    }
}

/// Exports a managed attachment to a user-chosen destination path (atomic write).
pub async fn export_attachment(
    pool: &Pool<Sqlite>,
    storage: &AppStorage,
    attachment_id: &str,
    destination: &Path,
) -> AppResult<()> {
    let att = library_repo::get_attachment(pool, attachment_id).await?;
    let bytes = storage.read_bytes(&att.relative_path)?;
    write_bytes_atomic_absolute(destination, &bytes)?;
    Ok(())
}

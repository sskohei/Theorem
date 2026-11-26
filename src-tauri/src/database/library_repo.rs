use std::collections::HashMap;

use chrono::Utc;
use sqlx::{Pool, Row, Sqlite, SqliteConnection};
use uuid::Uuid;

use crate::domain::{
    AttachmentInfo, DocumentDetail, DocumentFilter, DocumentSummary, DuplicateMatch, Folder,
    Subject, Tag,
};
use crate::error::{AppError, AppResult};

fn now_iso() -> String {
    Utc::now().to_rfc3339()
}

fn validate_non_empty_name(raw: &str, label: &str) -> AppResult<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(AppError::Validation(format!("{label}を入力してください")));
    }
    if trimmed.chars().count() > 160 {
        return Err(AppError::Validation(format!(
            "{label}は 160 文字以内で入力してください"
        )));
    }
    Ok(trimmed.to_string())
}

fn validate_subject_category(cat: &str) -> AppResult<&str> {
    match cat {
        "mathematics" | "physics" | "other" => Ok(cat),
        _ => Err(AppError::Validation(format!(
            "不正な科目カテゴリです: {cat}"
        ))),
    }
}

// ============================================================================
// Subjects
// ============================================================================

pub async fn list_subjects(pool: &Pool<Sqlite>) -> AppResult<Vec<Subject>> {
    let rows = sqlx::query(
        "SELECT id, name, category, description, sort_order, created_at, updated_at
         FROM subjects ORDER BY sort_order ASC, created_at ASC",
    )
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|r| Subject {
            id: r.get("id"),
            name: r.get("name"),
            category: r.get("category"),
            description: r.get("description"),
            sort_order: r.get("sort_order"),
            created_at: r.get("created_at"),
            updated_at: r.get("updated_at"),
        })
        .collect())
}

pub async fn create_subject(
    pool: &Pool<Sqlite>,
    name: &str,
    category: &str,
    description: Option<&str>,
) -> AppResult<Subject> {
    let clean_name = validate_non_empty_name(name, "科目名")?;
    let clean_cat = validate_subject_category(category)?;
    let max_order: i64 = sqlx::query_scalar("SELECT COALESCE(MAX(sort_order), -1) FROM subjects")
        .fetch_one(pool)
        .await?;
    let id = Uuid::new_v4().to_string();
    let ts = now_iso();
    let desc = description
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    sqlx::query(
        "INSERT INTO subjects (id, name, category, description, sort_order, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
    )
    .bind(&id)
    .bind(&clean_name)
    .bind(clean_cat)
    .bind(&desc)
    .bind(max_order + 1)
    .bind(&ts)
    .bind(&ts)
    .execute(pool)
    .await?;

    Ok(Subject {
        id,
        name: clean_name,
        category: clean_cat.to_string(),
        description: desc,
        sort_order: max_order + 1,
        created_at: ts.clone(),
        updated_at: ts,
    })
}

pub async fn update_subject(
    pool: &Pool<Sqlite>,
    id: &str,
    name: &str,
    category: &str,
    description: Option<&str>,
) -> AppResult<Subject> {
    let clean_name = validate_non_empty_name(name, "科目名")?;
    let clean_cat = validate_subject_category(category)?;
    let desc = description
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    let ts = now_iso();

    let affected = sqlx::query(
        "UPDATE subjects SET name = ?1, category = ?2, description = ?3, updated_at = ?4 WHERE id = ?5",
    )
    .bind(&clean_name)
    .bind(clean_cat)
    .bind(&desc)
    .bind(&ts)
    .bind(id)
    .execute(pool)
    .await?
    .rows_affected();

    if affected == 0 {
        return Err(AppError::NotFound("指定された科目が見つかりません".into()));
    }

    let r = sqlx::query(
        "SELECT id, name, category, description, sort_order, created_at, updated_at FROM subjects WHERE id = ?1",
    )
    .bind(id)
    .fetch_one(pool)
    .await?;

    Ok(Subject {
        id: r.get("id"),
        name: r.get("name"),
        category: r.get("category"),
        description: r.get("description"),
        sort_order: r.get("sort_order"),
        created_at: r.get("created_at"),
        updated_at: r.get("updated_at"),
    })
}

pub async fn delete_subject(pool: &Pool<Sqlite>, id: &str) -> AppResult<()> {
    let doc_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM documents WHERE subject_id = ?1")
        .bind(id)
        .fetch_one(pool)
        .await?;
    if doc_count > 0 {
        return Err(AppError::Conflict(format!(
            "この科目には {doc_count} 件の問題が登録されているため削除できません。先に問題を別科目へ移動または削除してください。"
        )));
    }
    let affected = sqlx::query("DELETE FROM subjects WHERE id = ?1")
        .bind(id)
        .execute(pool)
        .await?
        .rows_affected();
    if affected == 0 {
        return Err(AppError::NotFound("指定された科目が見つかりません".into()));
    }
    Ok(())
}

// ============================================================================
// Folders (Hierarchical)
// ============================================================================

pub async fn list_folders(pool: &Pool<Sqlite>) -> AppResult<Vec<Folder>> {
    let rows = sqlx::query(
        "SELECT id, subject_id, parent_id, name, auto_upload_drive, sort_order, created_at, updated_at
         FROM folders ORDER BY sort_order ASC, name ASC",
    )
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|r| Folder {
            id: r.get("id"),
            subject_id: r.get("subject_id"),
            parent_id: r.get("parent_id"),
            name: r.get("name"),
            auto_upload_drive: r.get::<i64, _>("auto_upload_drive") != 0,
            sort_order: r.get("sort_order"),
            created_at: r.get("created_at"),
            updated_at: r.get("updated_at"),
        })
        .collect())
}

pub async fn create_folder(
    pool: &Pool<Sqlite>,
    subject_id: &str,
    parent_id: Option<&str>,
    name: &str,
) -> AppResult<Folder> {
    let clean_name = validate_non_empty_name(name, "フォルダ名")?;

    if let Some(pid) = parent_id {
        let parent_subject: Option<String> =
            sqlx::query_scalar("SELECT subject_id FROM folders WHERE id = ?1")
                .bind(pid)
                .fetch_optional(pool)
                .await?;
        match parent_subject {
            None => {
                return Err(AppError::NotFound("親フォルダが見つかりません".into()));
            }
            Some(ps) if ps != subject_id => {
                return Err(AppError::Validation(
                    "親フォルダと異なる科目にはフォルダを作成できません".into(),
                ));
            }
            _ => {}
        }
    }

    let id = Uuid::new_v4().to_string();
    let ts = now_iso();
    let max_order: i64 = sqlx::query_scalar(
        "SELECT COALESCE(MAX(sort_order), -1) FROM folders WHERE subject_id = ?1 AND COALESCE(parent_id, '') = COALESCE(?2, '')",
    )
    .bind(subject_id)
    .bind(parent_id)
    .fetch_one(pool)
    .await?;

    sqlx::query(
        "INSERT INTO folders (id, subject_id, parent_id, name, auto_upload_drive, sort_order, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, 0, ?5, ?6, ?7)",
    )
    .bind(&id)
    .bind(subject_id)
    .bind(parent_id)
    .bind(&clean_name)
    .bind(max_order + 1)
    .bind(&ts)
    .bind(&ts)
    .execute(pool)
    .await?;

    Ok(Folder {
        id,
        subject_id: subject_id.to_string(),
        parent_id: parent_id.map(ToString::to_string),
        name: clean_name,
        auto_upload_drive: false,
        sort_order: max_order + 1,
        created_at: ts.clone(),
        updated_at: ts,
    })
}

pub async fn update_folder(
    pool: &Pool<Sqlite>,
    id: &str,
    name: &str,
    new_parent_id: Option<&str>,
) -> AppResult<Folder> {
    let clean_name = validate_non_empty_name(name, "フォルダ名")?;

    let current_subject: String =
        sqlx::query_scalar("SELECT subject_id FROM folders WHERE id = ?1")
            .bind(id)
            .fetch_optional(pool)
            .await?
            .ok_or_else(|| AppError::NotFound("指定されたフォルダが見つかりません".into()))?;

    // Prevent circular folder hierarchy (moving a folder into itself or its descendant)
    if let Some(target_pid) = new_parent_id {
        if target_pid == id {
            return Err(AppError::Validation(
                "フォルダを自分自身の中へ移動できません".into(),
            ));
        }
        let mut cursor = Some(target_pid.to_string());
        let mut depth = 0usize;
        while let Some(cur) = cursor {
            depth += 1;
            if depth > 128 {
                return Err(AppError::Validation("フォルダ階層が深すぎます".into()));
            }
            let row = sqlx::query("SELECT subject_id, parent_id FROM folders WHERE id = ?1")
                .bind(&cur)
                .fetch_optional(pool)
                .await?;
            let Some(row) = row else {
                return Err(AppError::NotFound(
                    "移動先の親フォルダが見つかりません".into(),
                ));
            };
            let subj: String = row.get("subject_id");
            if subj != current_subject {
                return Err(AppError::Validation(
                    "異なる科目のフォルダ配下へ移動できません".into(),
                ));
            }
            let next_parent: Option<String> = row.get("parent_id");
            if next_parent.as_deref() == Some(id) {
                return Err(AppError::Validation(
                    "フォルダを配下の子孫フォルダへ移動できません（循環参照防止）".into(),
                ));
            }
            cursor = next_parent;
        }
    }

    let ts = now_iso();
    sqlx::query("UPDATE folders SET name = ?1, parent_id = ?2, updated_at = ?3 WHERE id = ?4")
        .bind(&clean_name)
        .bind(new_parent_id)
        .bind(&ts)
        .bind(id)
        .execute(pool)
        .await?;

    let r = sqlx::query(
        "SELECT id, subject_id, parent_id, name, auto_upload_drive, sort_order, created_at, updated_at
         FROM folders WHERE id = ?1",
    )
    .bind(id)
    .fetch_one(pool)
    .await?;

    Ok(Folder {
        id: r.get("id"),
        subject_id: r.get("subject_id"),
        parent_id: r.get("parent_id"),
        name: r.get("name"),
        auto_upload_drive: r.get::<i64, _>("auto_upload_drive") != 0,
        sort_order: r.get("sort_order"),
        created_at: r.get("created_at"),
        updated_at: r.get("updated_at"),
    })
}

pub async fn set_folder_auto_upload(
    pool: &Pool<Sqlite>,
    id: &str,
    auto_upload_drive: bool,
) -> AppResult<Folder> {
    let ts = now_iso();
    let affected =
        sqlx::query("UPDATE folders SET auto_upload_drive = ?1, updated_at = ?2 WHERE id = ?3")
            .bind(if auto_upload_drive { 1 } else { 0 })
            .bind(&ts)
            .bind(id)
            .execute(pool)
            .await?
            .rows_affected();
    if affected == 0 {
        return Err(AppError::NotFound(
            "指定されたフォルダが見つかりません".into(),
        ));
    }

    let r = sqlx::query(
        "SELECT id, subject_id, parent_id, name, auto_upload_drive, sort_order, created_at, updated_at
         FROM folders WHERE id = ?1",
    )
    .bind(id)
    .fetch_one(pool)
    .await?;

    Ok(Folder {
        id: r.get("id"),
        subject_id: r.get("subject_id"),
        parent_id: r.get("parent_id"),
        name: r.get("name"),
        auto_upload_drive: r.get::<i64, _>("auto_upload_drive") != 0,
        sort_order: r.get("sort_order"),
        created_at: r.get("created_at"),
        updated_at: r.get("updated_at"),
    })
}

pub async fn delete_folder(pool: &Pool<Sqlite>, id: &str) -> AppResult<()> {
    let affected = sqlx::query("DELETE FROM folders WHERE id = ?1")
        .bind(id)
        .execute(pool)
        .await?
        .rows_affected();
    if affected == 0 {
        return Err(AppError::NotFound(
            "指定されたフォルダが見つかりません".into(),
        ));
    }
    Ok(())
}

// ============================================================================
// Tags
// ============================================================================

pub async fn list_tags(pool: &Pool<Sqlite>) -> AppResult<Vec<Tag>> {
    let rows = sqlx::query("SELECT id, name, color, created_at FROM tags ORDER BY name ASC")
        .fetch_all(pool)
        .await?;
    Ok(rows
        .into_iter()
        .map(|r| Tag {
            id: r.get("id"),
            name: r.get("name"),
            color: r.get("color"),
            created_at: r.get("created_at"),
        })
        .collect())
}

pub async fn create_tag(pool: &Pool<Sqlite>, name: &str, color: Option<&str>) -> AppResult<Tag> {
    let clean_name = validate_non_empty_name(name, "タグ名")?;
    let id = Uuid::new_v4().to_string();
    let ts = now_iso();
    let clean_color = color
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    sqlx::query("INSERT INTO tags (id, name, color, created_at) VALUES (?1, ?2, ?3, ?4)")
        .bind(&id)
        .bind(&clean_name)
        .bind(&clean_color)
        .bind(&ts)
        .execute(pool)
        .await?;

    Ok(Tag {
        id,
        name: clean_name,
        color: clean_color,
        created_at: ts,
    })
}

pub async fn delete_tag(pool: &Pool<Sqlite>, id: &str) -> AppResult<()> {
    let affected = sqlx::query("DELETE FROM tags WHERE id = ?1")
        .bind(id)
        .execute(pool)
        .await?
        .rows_affected();
    if affected == 0 {
        return Err(AppError::NotFound("指定されたタグが見つかりません".into()));
    }
    Ok(())
}

pub async fn set_document_tags(
    pool: &Pool<Sqlite>,
    document_id: &str,
    tag_ids: &[String],
) -> AppResult<Vec<Tag>> {
    let mut tx = pool.begin().await?;
    let exists: Option<String> = sqlx::query_scalar("SELECT id FROM documents WHERE id = ?1")
        .bind(document_id)
        .fetch_optional(&mut *tx)
        .await?;
    if exists.is_none() {
        return Err(AppError::NotFound(
            "対象ドキュメントが見つかりません".into(),
        ));
    }

    sqlx::query("DELETE FROM document_tags WHERE document_id = ?1")
        .bind(document_id)
        .execute(&mut *tx)
        .await?;

    let ts = now_iso();
    for tag_id in tag_ids {
        sqlx::query(
            "INSERT OR IGNORE INTO document_tags (document_id, tag_id, created_at) VALUES (?1, ?2, ?3)",
        )
        .bind(document_id)
        .bind(tag_id)
        .bind(&ts)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;

    list_tags_for_document(pool, document_id).await
}

pub async fn list_tags_for_document(pool: &Pool<Sqlite>, document_id: &str) -> AppResult<Vec<Tag>> {
    let rows = sqlx::query(
        "SELECT t.id, t.name, t.color, t.created_at
         FROM tags t
         INNER JOIN document_tags dt ON dt.tag_id = t.id
         WHERE dt.document_id = ?1
         ORDER BY t.name ASC",
    )
    .bind(document_id)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|r| Tag {
            id: r.get("id"),
            name: r.get("name"),
            color: r.get("color"),
            created_at: r.get("created_at"),
        })
        .collect())
}

// ============================================================================
// Documents & Attachments
// ============================================================================

pub async fn find_duplicate_originals(
    pool: &Pool<Sqlite>,
    sha256: &str,
) -> AppResult<Vec<DuplicateMatch>> {
    let rows = sqlx::query(
        "SELECT a.id AS attachment_id, a.original_filename, a.sha256, d.id AS document_id, d.title AS document_title
         FROM attachments a
         INNER JOIN documents d ON d.id = a.document_id
         WHERE a.sha256 = ?1 AND a.kind IN ('original_pdf', 'original_image')
         ORDER BY a.created_at DESC",
    )
    .bind(sha256)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|r| DuplicateMatch {
            document_id: r.get("document_id"),
            document_title: r.get("document_title"),
            attachment_id: r.get("attachment_id"),
            original_filename: r.get("original_filename"),
            sha256: r.get("sha256"),
        })
        .collect())
}

pub struct NewAttachmentRecord {
    pub id: String,
    pub document_id: Option<String>,
    pub kind: String,
    pub original_filename: String,
    pub relative_path: String,
    pub mime_type: String,
    pub byte_size: i64,
    pub sha256: String,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub page_count: Option<i64>,
}

pub async fn insert_attachment_conn(
    conn: &mut SqliteConnection,
    rec: &NewAttachmentRecord,
) -> AppResult<AttachmentInfo> {
    let ts = now_iso();
    sqlx::query(
        "INSERT INTO attachments
         (id, document_id, kind, original_filename, relative_path, mime_type, byte_size, sha256, width, height, page_count, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
    )
    .bind(&rec.id)
    .bind(&rec.document_id)
    .bind(&rec.kind)
    .bind(&rec.original_filename)
    .bind(&rec.relative_path)
    .bind(&rec.mime_type)
    .bind(rec.byte_size)
    .bind(&rec.sha256)
    .bind(rec.width)
    .bind(rec.height)
    .bind(rec.page_count)
    .bind(&ts)
    .execute(conn)
    .await?;

    Ok(AttachmentInfo {
        id: rec.id.clone(),
        document_id: rec.document_id.clone(),
        kind: rec.kind.clone(),
        original_filename: rec.original_filename.clone(),
        relative_path: rec.relative_path.clone(),
        mime_type: rec.mime_type.clone(),
        byte_size: rec.byte_size,
        sha256: rec.sha256.clone(),
        width: rec.width,
        height: rec.height,
        page_count: rec.page_count,
        created_at: ts,
    })
}

pub async fn get_attachment(pool: &Pool<Sqlite>, id: &str) -> AppResult<AttachmentInfo> {
    let r = sqlx::query(
        "SELECT id, document_id, kind, original_filename, relative_path, mime_type, byte_size, sha256, width, height, page_count, created_at
         FROM attachments WHERE id = ?1",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::NotFound("添付ファイルが見つかりません".into()))?;

    Ok(row_to_attachment(&r))
}

pub async fn list_attachments_for_document(
    pool: &Pool<Sqlite>,
    document_id: &str,
) -> AppResult<Vec<AttachmentInfo>> {
    let rows = sqlx::query(
        "SELECT id, document_id, kind, original_filename, relative_path, mime_type, byte_size, sha256, width, height, page_count, created_at
         FROM attachments WHERE document_id = ?1 ORDER BY created_at ASC",
    )
    .bind(document_id)
    .fetch_all(pool)
    .await?;
    Ok(rows.iter().map(row_to_attachment).collect())
}

pub async fn list_documents(
    pool: &Pool<Sqlite>,
    filter: &DocumentFilter,
) -> AppResult<Vec<DocumentSummary>> {
    let rows = sqlx::query(
        "SELECT d.id, d.subject_id, d.folder_id, d.title, d.source_type, d.primary_attachment_id,
                d.page_count, d.recognition_mode, d.study_status, d.is_favorite, d.last_opened_at,
                d.created_at, d.updated_at,
                s.name AS subject_name,
                f.name AS folder_name,
                (SELECT a.original_filename FROM attachments a WHERE a.id = d.primary_attachment_id LIMIT 1) AS primary_filename,
                (SELECT a.id FROM attachments a WHERE a.document_id = d.id AND a.kind = 'thumbnail' ORDER BY a.created_at DESC LIMIT 1) AS thumbnail_attachment_id
         FROM documents d
         INNER JOIN subjects s ON s.id = d.subject_id
         LEFT JOIN folders f ON f.id = d.folder_id
         ORDER BY d.updated_at DESC",
    )
    .fetch_all(pool)
    .await?;

    // Fetch document_tags and tag names in a single batch query
    let tag_rows = sqlx::query(
        "SELECT dt.document_id, dt.tag_id, t.name AS tag_name
         FROM document_tags dt
         INNER JOIN tags t ON t.id = dt.tag_id",
    )
    .fetch_all(pool)
    .await?;
    let mut tags_by_doc: HashMap<String, Vec<(String, String)>> = HashMap::new();
    for tr in tag_rows {
        let doc_id: String = tr.get("document_id");
        let tag_id: String = tr.get("tag_id");
        let tag_name: String = tr.get("tag_name");
        tags_by_doc
            .entry(doc_id)
            .or_default()
            .push((tag_id, tag_name));
    }

    let search_lower = filter
        .search_query
        .as_deref()
        .map(|s| s.trim().to_lowercase())
        .filter(|s| !s.is_empty());

    let mut docs: Vec<DocumentSummary> = rows
        .into_iter()
        .filter_map(|r| {
            let id: String = r.get("id");
            let subject_name: String = r.get("subject_name");
            let folder_name: Option<String> = r.get("folder_name");
            let primary_filename: Option<String> = r.get("primary_filename");
            let doc_tags = tags_by_doc.remove(&id).unwrap_or_default();
            let tag_ids: Vec<String> = doc_tags.iter().map(|(tid, _)| tid.clone()).collect();
            let tag_names: Vec<String> = doc_tags.into_iter().map(|(_, name)| name).collect();

            let doc = DocumentSummary {
                id,
                subject_id: r.get("subject_id"),
                folder_id: r.get("folder_id"),
                title: r.get("title"),
                source_type: r.get("source_type"),
                primary_attachment_id: r.get("primary_attachment_id"),
                thumbnail_attachment_id: r.get("thumbnail_attachment_id"),
                page_count: r.get("page_count"),
                recognition_mode: r.get("recognition_mode"),
                study_status: r.get("study_status"),
                is_favorite: r.get::<i64, _>("is_favorite") != 0,
                last_opened_at: r.get("last_opened_at"),
                tag_ids,
                created_at: r.get("created_at"),
                updated_at: r.get("updated_at"),
            };

            if let Some(ref sid) = filter.subject_id
                && &doc.subject_id != sid
            {
                return None;
            }
            if let Some(ref fid) = filter.folder_id
                && doc.folder_id.as_deref() != Some(fid.as_str())
            {
                return None;
            }
            if let Some(ref tid) = filter.tag_id
                && !doc.tag_ids.contains(tid)
            {
                return None;
            }
            if let Some(ref status) = filter.study_status
                && &doc.study_status != status
            {
                return None;
            }
            if filter.favorites_only && !doc.is_favorite {
                return None;
            }
            if filter.recent_only && doc.last_opened_at.is_none() {
                return None;
            }
            if let Some(ref q) = search_lower {
                let matched = doc.title.to_lowercase().contains(q)
                    || subject_name.to_lowercase().contains(q)
                    || folder_name
                        .as_deref()
                        .is_some_and(|n| n.to_lowercase().contains(q))
                    || primary_filename
                        .as_deref()
                        .is_some_and(|n| n.to_lowercase().contains(q))
                    || tag_names.iter().any(|n| n.to_lowercase().contains(q));
                if !matched {
                    return None;
                }
            }
            Some(doc)
        })
        .collect();

    if filter.recent_only {
        docs.sort_by(|a, b| b.last_opened_at.cmp(&a.last_opened_at));
    }

    Ok(docs)
}

pub async fn get_document_detail(
    pool: &Pool<Sqlite>,
    document_id: &str,
) -> AppResult<DocumentDetail> {
    let r = sqlx::query(
        "SELECT d.id, d.subject_id, d.folder_id, d.title, d.source_type, d.primary_attachment_id,
                d.page_count, d.recognition_mode, d.study_status, d.is_favorite, d.last_opened_at,
                d.created_at, d.updated_at,
                (SELECT a.id FROM attachments a WHERE a.document_id = d.id AND a.kind = 'thumbnail' ORDER BY a.created_at DESC LIMIT 1) AS thumbnail_attachment_id
         FROM documents d
         WHERE d.id = ?1",
    )
    .bind(document_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::NotFound("対象ドキュメントが見つかりません".into()))?;

    let tags = list_tags_for_document(pool, document_id).await?;
    let attachments = list_attachments_for_document(pool, document_id).await?;

    let summary = DocumentSummary {
        id: r.get("id"),
        subject_id: r.get("subject_id"),
        folder_id: r.get("folder_id"),
        title: r.get("title"),
        source_type: r.get("source_type"),
        primary_attachment_id: r.get("primary_attachment_id"),
        thumbnail_attachment_id: r.get("thumbnail_attachment_id"),
        page_count: r.get("page_count"),
        recognition_mode: r.get("recognition_mode"),
        study_status: r.get("study_status"),
        is_favorite: r.get::<i64, _>("is_favorite") != 0,
        last_opened_at: r.get("last_opened_at"),
        tag_ids: tags.iter().map(|t| t.id.clone()).collect(),
        created_at: r.get("created_at"),
        updated_at: r.get("updated_at"),
    };

    Ok(DocumentDetail {
        document: summary,
        attachments,
        tags,
    })
}

#[allow(clippy::too_many_arguments)]
pub async fn update_document(
    pool: &Pool<Sqlite>,
    id: &str,
    title: Option<&str>,
    subject_id: Option<&str>,
    folder_id: Option<Option<&str>>,
    is_favorite: Option<bool>,
    recognition_mode: Option<&str>,
    study_status: Option<&str>,
) -> AppResult<DocumentDetail> {
    let existing = get_document_detail(pool, id).await?;
    let new_title = match title {
        Some(t) => validate_non_empty_name(t, "問題タイトル")?,
        None => existing.document.title,
    };
    let new_subject = subject_id
        .map(ToString::to_string)
        .unwrap_or_else(|| existing.document.subject_id.clone());
    let subject_changed = new_subject != existing.document.subject_id;
    let new_folder = match folder_id {
        Some(opt) => opt.map(ToString::to_string),
        None => {
            if subject_changed {
                None
            } else {
                existing.document.folder_id
            }
        }
    };
    if let Some(ref fid) = new_folder {
        let folder_subject: Option<String> =
            sqlx::query_scalar("SELECT subject_id FROM folders WHERE id = ?1")
                .bind(fid)
                .fetch_optional(pool)
                .await?;
        match folder_subject {
            None => {
                return Err(AppError::NotFound("移動先フォルダが見つかりません".into()));
            }
            Some(fs) if fs != new_subject => {
                return Err(AppError::Validation(
                    "選択したフォルダは指定された科目に属していません".into(),
                ));
            }
            _ => {}
        }
    }
    let new_fav = is_favorite.unwrap_or(existing.document.is_favorite);
    let new_mode = match recognition_mode {
        Some(m) => {
            if !matches!(
                m,
                "printed_only" | "include_handwriting" | "page_range" | "handwritten_answer"
            ) {
                return Err(AppError::Validation(format!("不正な認識モードです: {m}")));
            }
            m.to_string()
        }
        None => existing.document.recognition_mode,
    };
    let new_study_status = match study_status {
        Some(st) => {
            if !matches!(st, "unstarted" | "in_progress" | "solved" | "review_needed") {
                return Err(AppError::Validation(format!(
                    "不正な学習ステータスです: {st}"
                )));
            }
            st.to_string()
        }
        None => existing.document.study_status,
    };

    let ts = now_iso();
    sqlx::query(
        "UPDATE documents
         SET title = ?1, subject_id = ?2, folder_id = ?3, is_favorite = ?4, recognition_mode = ?5, study_status = ?6, updated_at = ?7
         WHERE id = ?8",
    )
    .bind(&new_title)
    .bind(&new_subject)
    .bind(&new_folder)
    .bind(if new_fav { 1 } else { 0 })
    .bind(&new_mode)
    .bind(&new_study_status)
    .bind(&ts)
    .bind(id)
    .execute(pool)
    .await?;

    get_document_detail(pool, id).await
}

pub async fn touch_document_opened(pool: &Pool<Sqlite>, id: &str) -> AppResult<String> {
    let ts = now_iso();
    let affected = sqlx::query("UPDATE documents SET last_opened_at = ?1 WHERE id = ?2")
        .bind(&ts)
        .bind(id)
        .execute(pool)
        .await?
        .rows_affected();
    if affected == 0 {
        return Err(AppError::NotFound(
            "対象ドキュメントが見つかりません".into(),
        ));
    }
    Ok(ts)
}

/// Deletes a document and returns relative paths of orphaned attachments for filesystem cleanup.
pub async fn delete_document(pool: &Pool<Sqlite>, id: &str) -> AppResult<Vec<String>> {
    let mut tx = pool.begin().await?;
    let rel_paths: Vec<String> =
        sqlx::query_scalar("SELECT relative_path FROM attachments WHERE document_id = ?1")
            .bind(id)
            .fetch_all(&mut *tx)
            .await?;

    let affected = sqlx::query("DELETE FROM documents WHERE id = ?1")
        .bind(id)
        .execute(&mut *tx)
        .await?
        .rows_affected();
    if affected == 0 {
        return Err(AppError::NotFound(
            "対象ドキュメントが見つかりません".into(),
        ));
    }

    // Remove attachment rows that were detached via ON DELETE SET NULL
    sqlx::query("DELETE FROM attachments WHERE document_id IS NULL")
        .execute(&mut *tx)
        .await?;

    tx.commit().await?;
    Ok(rel_paths)
}

pub async fn counts(pool: &Pool<Sqlite>) -> AppResult<(i64, i64)> {
    let docs: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM documents")
        .fetch_one(pool)
        .await?;
    let atts: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM attachments")
        .fetch_one(pool)
        .await?;
    Ok((docs, atts))
}

fn row_to_attachment(r: &sqlx::sqlite::SqliteRow) -> AttachmentInfo {
    AttachmentInfo {
        id: r.get("id"),
        document_id: r.get("document_id"),
        kind: r.get("kind"),
        original_filename: r.get("original_filename"),
        relative_path: r.get("relative_path"),
        mime_type: r.get("mime_type"),
        byte_size: r.get("byte_size"),
        sha256: r.get("sha256"),
        width: r.get("width"),
        height: r.get("height"),
        page_count: r.get("page_count"),
        created_at: r.get("created_at"),
    }
}

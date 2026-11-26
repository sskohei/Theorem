use chrono::Utc;
use sqlx::{Pool, Row, Sqlite};
use uuid::Uuid;

use crate::domain::{
    CreateReferenceMaterialRequest, FullTextSearchHit, FullTextSearchRequest, ReferenceMaterial,
};
use crate::error::{AppError, AppResult};

fn now_iso() -> String {
    Utc::now().to_rfc3339()
}

fn validate_reference_kind(kind: &str) -> AppResult<&str> {
    match kind {
        "lecture_note" | "textbook_excerpt" | "formula_sheet" | "handwritten_attempt" => Ok(kind),
        _ => Err(AppError::Validation(format!(
            "不正な参考資料種別です: {kind}"
        ))),
    }
}

fn row_to_reference(row: &sqlx::sqlite::SqliteRow) -> ReferenceMaterial {
    ReferenceMaterial {
        id: row.get("id"),
        subject_id: row.get("subject_id"),
        document_id: row.get("document_id"),
        title: row.get("title"),
        kind: row.get("kind"),
        content_markdown: row.get("content_markdown"),
        attachment_id: row.get("attachment_id"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }
}

pub async fn list_reference_materials(
    pool: &Pool<Sqlite>,
    document_id: Option<&str>,
    subject_id: Option<&str>,
) -> AppResult<Vec<ReferenceMaterial>> {
    let rows = match (document_id, subject_id) {
        (Some(doc_id), Some(sub_id)) => {
            sqlx::query(
                "SELECT id, subject_id, document_id, title, kind, content_markdown, attachment_id, created_at, updated_at
                 FROM reference_materials
                 WHERE document_id = ?1 OR (document_id IS NULL AND subject_id = ?2)
                 ORDER BY updated_at DESC",
            )
            .bind(doc_id)
            .bind(sub_id)
            .fetch_all(pool)
            .await?
        }
        (Some(doc_id), None) => {
            sqlx::query(
                "SELECT id, subject_id, document_id, title, kind, content_markdown, attachment_id, created_at, updated_at
                 FROM reference_materials
                 WHERE document_id = ?1
                    OR (document_id IS NULL AND subject_id = (SELECT subject_id FROM documents WHERE id = ?1))
                 ORDER BY updated_at DESC",
            )
            .bind(doc_id)
            .fetch_all(pool)
            .await?
        }
        (None, Some(sub_id)) => {
            sqlx::query(
                "SELECT id, subject_id, document_id, title, kind, content_markdown, attachment_id, created_at, updated_at
                 FROM reference_materials
                 WHERE subject_id = ?1
                 ORDER BY updated_at DESC",
            )
            .bind(sub_id)
            .fetch_all(pool)
            .await?
        }
        (None, None) => {
            sqlx::query(
                "SELECT id, subject_id, document_id, title, kind, content_markdown, attachment_id, created_at, updated_at
                 FROM reference_materials
                 ORDER BY updated_at DESC",
            )
            .fetch_all(pool)
            .await?
        }
    };

    Ok(rows.iter().map(row_to_reference).collect())
}

pub async fn get_reference_materials_by_ids(
    pool: &Pool<Sqlite>,
    ids: &[String],
) -> AppResult<Vec<ReferenceMaterial>> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let mut results = Vec::new();
    for id in ids {
        if let Some(row) = sqlx::query(
            "SELECT id, subject_id, document_id, title, kind, content_markdown, attachment_id, created_at, updated_at
             FROM reference_materials
             WHERE id = ?1",
        )
        .bind(id)
        .fetch_optional(pool)
        .await?
        {
            results.push(row_to_reference(&row));
        }
    }
    Ok(results)
}

pub async fn create_reference_material(
    pool: &Pool<Sqlite>,
    req: &CreateReferenceMaterialRequest,
    attachment_id: Option<&str>,
) -> AppResult<ReferenceMaterial> {
    let title = req.title.trim();
    if title.is_empty() {
        return Err(AppError::Validation(
            "資料タイトルを入力してください".into(),
        ));
    }
    let content = req.content_markdown.trim();
    if content.is_empty() {
        return Err(AppError::Validation(
            "資料の内容（Markdown / テキスト）を入力してください".into(),
        ));
    }
    let kind = validate_reference_kind(&req.kind)?;
    let id = Uuid::new_v4().to_string();
    let ts = now_iso();

    sqlx::query(
        "INSERT INTO reference_materials
         (id, subject_id, document_id, title, kind, content_markdown, attachment_id, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
    )
    .bind(&id)
    .bind(req.subject_id.as_deref())
    .bind(req.document_id.as_deref())
    .bind(title)
    .bind(kind)
    .bind(content)
    .bind(attachment_id)
    .bind(&ts)
    .bind(&ts)
    .execute(pool)
    .await?;

    Ok(ReferenceMaterial {
        id,
        subject_id: req.subject_id.clone(),
        document_id: req.document_id.clone(),
        title: title.to_string(),
        kind: kind.to_string(),
        content_markdown: content.to_string(),
        attachment_id: attachment_id.map(ToString::to_string),
        created_at: ts.clone(),
        updated_at: ts,
    })
}

pub async fn delete_reference_material(pool: &Pool<Sqlite>, id: &str) -> AppResult<()> {
    let res = sqlx::query("DELETE FROM reference_materials WHERE id = ?1")
        .bind(id)
        .execute(pool)
        .await?;
    if res.rows_affected() == 0 {
        return Err(AppError::NotFound(
            "指定された参考資料が見つかりません".into(),
        ));
    }
    Ok(())
}

/// Rebuilds the entire SQLite FTS5 cross-entity index transactionally.
pub async fn rebuild_search_index(pool: &Pool<Sqlite>) -> AppResult<usize> {
    let mut tx = pool.begin().await?;
    sqlx::query("DELETE FROM search_index_fts")
        .execute(&mut *tx)
        .await?;

    sqlx::query(
        "INSERT INTO search_index_fts(entity_type, entity_id, document_id, title, body)
         SELECT 'document', id, id, title, title FROM documents",
    )
    .execute(&mut *tx)
    .await?;

    sqlx::query(
        "INSERT INTO search_index_fts(entity_type, entity_id, document_id, title, body)
         SELECT 'problem', id, document_id,
                problem_number || COALESCE(' ' || subproblem_number, ''),
                statement_markdown || ' ' || COALESCE(target_objective, '') || ' ' || COALESCE(field, '')
         FROM problems",
    )
    .execute(&mut *tx)
    .await?;

    sqlx::query(
        "INSERT INTO search_index_fts(entity_type, entity_id, document_id, title, body)
         SELECT 'solution', id, document_id,
                'v' || version_number || ' (' || mode || ')',
                structured_json || ' ' || COALESCE(change_summary, '')
         FROM solutions",
    )
    .execute(&mut *tx)
    .await?;

    sqlx::query(
        "INSERT INTO search_index_fts(entity_type, entity_id, document_id, title, body)
         SELECT 'message', m.id, c.document_id, c.title, m.content_markdown
         FROM messages m
         JOIN conversations c ON c.id = m.conversation_id",
    )
    .execute(&mut *tx)
    .await?;

    sqlx::query(
        "INSERT INTO search_index_fts(entity_type, entity_id, document_id, title, body)
         SELECT 'reference', id, COALESCE(document_id, ''), title, content_markdown
         FROM reference_materials",
    )
    .execute(&mut *tx)
    .await?;

    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM search_index_fts")
        .fetch_one(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(count.max(0) as usize)
}

fn make_excerpt(text: &str, needle: &str, radius: usize) -> String {
    let lower_text = text.to_lowercase();
    let lower_needle = needle.to_lowercase();
    if let Some(byte_pos) = lower_text.find(&lower_needle) {
        let char_pos = text[..byte_pos].chars().count();
        let start = char_pos.saturating_sub(radius);
        let chars: Vec<char> = text.chars().collect();
        let end = (char_pos + needle.chars().count() + radius).min(chars.len());
        let mut snippet: String = chars[start..end].iter().collect();
        if start > 0 {
            snippet.insert(0, '…');
        }
        if end < chars.len() {
            snippet.push('…');
        }
        snippet.replace('\n', " ")
    } else {
        let head: String = text.chars().take(120).collect();
        head.replace('\n', " ")
    }
}

/// Cross-entity full-text search across documents, problems, solutions, chat messages,
/// and reference materials. Uses SQLite FTS5 (`trigram`) for >= 3 char queries and
/// a parameterized substring fallback for 1–2 char queries (e.g. 2-kanji Japanese terms).
pub async fn full_text_search(
    pool: &Pool<Sqlite>,
    req: &FullTextSearchRequest,
) -> AppResult<Vec<FullTextSearchHit>> {
    let raw_query = req.query.trim();
    if raw_query.is_empty() {
        return Ok(Vec::new());
    }
    let limit = req.limit.unwrap_or(50).clamp(1, 200) as i64;
    let allowed_scopes: Vec<&str> = if req.scopes.is_empty() {
        vec!["document", "problem", "solution", "message", "reference"]
    } else {
        req.scopes
            .iter()
            .map(String::as_str)
            .filter(|s| {
                matches!(
                    *s,
                    "document" | "problem" | "solution" | "message" | "reference"
                )
            })
            .collect()
    };
    if allowed_scopes.is_empty() {
        return Ok(Vec::new());
    }

    let use_fts_match = raw_query.chars().count() >= 3;
    let rows = if use_fts_match {
        // Quote query string safely for FTS5 literal phrase matching (prevents syntax injection)
        let fts_query = format!("\"{}\"", raw_query.replace('"', "\"\""));
        sqlx::query(
            "SELECT f.entity_type,
                    f.entity_id,
                    f.document_id,
                    f.title,
                    snippet(search_index_fts, 4, '【', '】', '…', 24) AS snippet,
                    COALESCE(d.title, f.title) AS doc_title,
                    d.subject_id AS doc_subject_id,
                    d.study_status AS doc_study_status,
                    COALESCE(d.is_favorite, 0) AS doc_is_favorite
             FROM search_index_fts f
             LEFT JOIN documents d ON d.id = f.document_id
             WHERE search_index_fts MATCH ?1
             ORDER BY bm25(search_index_fts)
             LIMIT ?2",
        )
        .bind(&fts_query)
        .bind(limit * 3)
        .fetch_all(pool)
        .await?
    } else {
        let like_pat = format!("%{}%", raw_query.replace('%', "\\%").replace('_', "\\_"));
        sqlx::query(
            "SELECT f.entity_type,
                    f.entity_id,
                    f.document_id,
                    f.title,
                    f.body AS snippet,
                    COALESCE(d.title, f.title) AS doc_title,
                    d.subject_id AS doc_subject_id,
                    d.study_status AS doc_study_status,
                    COALESCE(d.is_favorite, 0) AS doc_is_favorite
             FROM search_index_fts f
             LEFT JOIN documents d ON d.id = f.document_id
             WHERE f.title LIKE ?1 ESCAPE '\\' OR f.body LIKE ?1 ESCAPE '\\'
             LIMIT ?2",
        )
        .bind(&like_pat)
        .bind(limit * 3)
        .fetch_all(pool)
        .await?
    };

    let mut hits = Vec::new();
    for row in rows {
        let entity_type: String = row.get("entity_type");
        if !allowed_scopes.contains(&entity_type.as_str()) {
            continue;
        }
        let doc_subject_id: Option<String> = row.get("doc_subject_id");
        if let Some(ref filter_sub) = req.subject_id
            && doc_subject_id.as_deref() != Some(filter_sub.as_str())
        {
            continue;
        }
        let doc_study_status: Option<String> = row.get("doc_study_status");
        if let Some(ref filter_status) = req.study_status
            && doc_study_status.as_deref() != Some(filter_status.as_str())
        {
            continue;
        }
        let doc_is_fav: i64 = row.get("doc_is_favorite");
        if req.favorites_only && doc_is_fav == 0 {
            continue;
        }

        let raw_snippet: String = row.get("snippet");
        let snippet = if use_fts_match {
            raw_snippet.replace('\n', " ")
        } else {
            make_excerpt(&raw_snippet, raw_query, 48)
        };
        let document_id: String = row.get("document_id");
        hits.push(FullTextSearchHit {
            entity_type,
            entity_id: row.get("entity_id"),
            document_id: if document_id.is_empty() {
                None
            } else {
                Some(document_id)
            },
            document_title: row.get("doc_title"),
            title: row.get("title"),
            snippet,
            study_status: doc_study_status,
            is_favorite: doc_is_fav != 0,
        });
        if hits.len() >= limit as usize {
            break;
        }
    }

    Ok(hits)
}

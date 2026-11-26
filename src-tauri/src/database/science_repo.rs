use chrono::Utc;
use sqlx::{Pool, Row, Sqlite};
use uuid::Uuid;

use crate::domain::{SolutionArtifactItem, ToolExecutionRecord, VerificationResultRecord};
use crate::error::{AppError, AppResult};

fn now_iso() -> String {
    Utc::now().to_rfc3339()
}

fn validate_tool_status(status: &str) -> AppResult<&str> {
    match status {
        "running" | "succeeded" | "failed" | "timeout" | "cancelled" => Ok(status),
        _ => Err(AppError::Validation(format!(
            "不正なツール実行ステータスです: {status}"
        ))),
    }
}

fn validate_source_kind(kind: &str) -> AppResult<&str> {
    match kind {
        "science_mcp" | "ai_self_check" | "user_check" => Ok(kind),
        _ => Err(AppError::Validation(format!(
            "不正な検証ソース種別です: {kind}"
        ))),
    }
}

fn validate_verification_status(status: &str) -> AppResult<&str> {
    match status {
        "passed" | "failed" | "inconclusive" | "not_run" | "error" => Ok(status),
        _ => Err(AppError::Validation(format!(
            "不正な検証ステータスです: {status}"
        ))),
    }
}

pub async fn recover_interrupted_tool_executions(pool: &Pool<Sqlite>) -> AppResult<()> {
    let ts = now_iso();
    sqlx::query(
        "UPDATE tool_executions
         SET status = 'cancelled',
             error_message = COALESCE(error_message, 'アプリ終了または再起動によりツール実行が中断されました'),
             finished_at = COALESCE(finished_at, ?1)
         WHERE status = 'running'",
    )
    .bind(&ts)
    .execute(pool)
    .await?;
    Ok(())
}

pub struct NewToolExecution<'a> {
    pub id: Option<&'a str>,
    pub agent_run_id: Option<&'a str>,
    pub document_id: Option<&'a str>,
    pub solution_id: Option<&'a str>,
    pub server_name: &'a str,
    pub tool_name: &'a str,
    pub arguments_json: &'a str,
    pub result_json: Option<&'a str>,
    pub status: &'a str,
    pub duration_ms: Option<i64>,
    pub error_message: Option<&'a str>,
    pub created_at: Option<&'a str>,
    pub finished_at: Option<&'a str>,
}

pub async fn insert_tool_execution(
    pool: &Pool<Sqlite>,
    rec: &NewToolExecution<'_>,
) -> AppResult<ToolExecutionRecord> {
    let status = validate_tool_status(rec.status)?;
    let id = rec
        .id
        .map(ToString::to_string)
        .unwrap_or_else(|| Uuid::new_v4().to_string());
    let created = rec
        .created_at
        .map(ToString::to_string)
        .unwrap_or_else(now_iso);
    let finished = rec.finished_at.map(ToString::to_string);

    sqlx::query(
        "INSERT INTO tool_executions
         (id, agent_run_id, document_id, solution_id, server_name, tool_name,
          arguments_json, result_json, status, duration_ms, error_message, created_at, finished_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
    )
    .bind(&id)
    .bind(rec.agent_run_id)
    .bind(rec.document_id)
    .bind(rec.solution_id)
    .bind(rec.server_name)
    .bind(rec.tool_name)
    .bind(rec.arguments_json)
    .bind(rec.result_json)
    .bind(status)
    .bind(rec.duration_ms)
    .bind(rec.error_message)
    .bind(&created)
    .bind(&finished)
    .execute(pool)
    .await?;

    Ok(ToolExecutionRecord {
        id,
        agent_run_id: rec.agent_run_id.map(ToString::to_string),
        document_id: rec.document_id.map(ToString::to_string),
        solution_id: rec.solution_id.map(ToString::to_string),
        server_name: rec.server_name.to_string(),
        tool_name: rec.tool_name.to_string(),
        arguments_json: rec.arguments_json.to_string(),
        result_json: rec.result_json.map(ToString::to_string),
        status: status.to_string(),
        duration_ms: rec.duration_ms,
        error_message: rec.error_message.map(ToString::to_string),
        created_at: created,
        finished_at: finished,
    })
}

pub async fn list_tool_executions_for_document(
    pool: &Pool<Sqlite>,
    document_id: &str,
) -> AppResult<Vec<ToolExecutionRecord>> {
    let rows = sqlx::query(
        "SELECT te.id, te.agent_run_id, te.document_id, te.solution_id, te.server_name,
                te.tool_name, te.arguments_json, te.result_json, te.status, te.duration_ms,
                te.error_message, te.created_at, te.finished_at
         FROM tool_executions te
         LEFT JOIN agent_runs ar ON ar.id = te.agent_run_id
         WHERE te.document_id = ?1 OR ar.document_id = ?1
         ORDER BY te.created_at DESC
         LIMIT 200",
    )
    .bind(document_id)
    .fetch_all(pool)
    .await?;

    Ok(rows.iter().map(row_to_tool_execution).collect())
}

pub async fn list_tool_executions_for_run(
    pool: &Pool<Sqlite>,
    agent_run_id: &str,
) -> AppResult<Vec<ToolExecutionRecord>> {
    let rows = sqlx::query(
        "SELECT id, agent_run_id, document_id, solution_id, server_name, tool_name,
                arguments_json, result_json, status, duration_ms, error_message,
                created_at, finished_at
         FROM tool_executions
         WHERE agent_run_id = ?1
         ORDER BY created_at ASC",
    )
    .bind(agent_run_id)
    .fetch_all(pool)
    .await?;

    Ok(rows.iter().map(row_to_tool_execution).collect())
}

fn row_to_tool_execution(r: &sqlx::sqlite::SqliteRow) -> ToolExecutionRecord {
    ToolExecutionRecord {
        id: r.get("id"),
        agent_run_id: r.get("agent_run_id"),
        document_id: r.get("document_id"),
        solution_id: r.get("solution_id"),
        server_name: r.get("server_name"),
        tool_name: r.get("tool_name"),
        arguments_json: r.get("arguments_json"),
        result_json: r.get("result_json"),
        status: r.get("status"),
        duration_ms: r.get("duration_ms"),
        error_message: r.get("error_message"),
        created_at: r.get("created_at"),
        finished_at: r.get("finished_at"),
    }
}

pub struct NewVerificationResult<'a> {
    pub solution_id: &'a str,
    pub tool_execution_id: Option<&'a str>,
    pub source_kind: &'a str,
    pub target_description: &'a str,
    pub assumptions: &'a [String],
    pub method: &'a str,
    pub status: &'a str,
    pub detail_json: Option<&'a str>,
}

pub async fn insert_verification_result(
    pool: &Pool<Sqlite>,
    rec: &NewVerificationResult<'_>,
) -> AppResult<VerificationResultRecord> {
    let source_kind = validate_source_kind(rec.source_kind)?;
    let status = validate_verification_status(rec.status)?;
    let id = Uuid::new_v4().to_string();
    let created_at = now_iso();
    let assumptions_json =
        serde_json::to_string(rec.assumptions).map_err(|e| AppError::Database(e.to_string()))?;

    sqlx::query(
        "INSERT INTO verification_results
         (id, solution_id, tool_execution_id, source_kind, target_description,
          assumptions_json, method, status, detail_json, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
    )
    .bind(&id)
    .bind(rec.solution_id)
    .bind(rec.tool_execution_id)
    .bind(source_kind)
    .bind(rec.target_description)
    .bind(&assumptions_json)
    .bind(rec.method)
    .bind(status)
    .bind(rec.detail_json)
    .bind(&created_at)
    .execute(pool)
    .await?;

    Ok(VerificationResultRecord {
        id,
        solution_id: rec.solution_id.to_string(),
        tool_execution_id: rec.tool_execution_id.map(ToString::to_string),
        source_kind: source_kind.to_string(),
        target_description: rec.target_description.to_string(),
        assumptions: rec.assumptions.to_vec(),
        method: rec.method.to_string(),
        status: status.to_string(),
        detail_json: rec.detail_json.map(ToString::to_string),
        created_at,
    })
}

pub async fn list_verification_results_for_solution(
    pool: &Pool<Sqlite>,
    solution_id: &str,
) -> AppResult<Vec<VerificationResultRecord>> {
    let rows = sqlx::query(
        "SELECT id, solution_id, tool_execution_id, source_kind, target_description,
                assumptions_json, method, status, detail_json, created_at
         FROM verification_results
         WHERE solution_id = ?1
         ORDER BY CASE source_kind WHEN 'science_mcp' THEN 0 WHEN 'user_check' THEN 1 ELSE 2 END,
                  created_at ASC",
    )
    .bind(solution_id)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|r| {
            let assumptions_raw: Option<String> = r.get("assumptions_json");
            let assumptions = assumptions_raw
                .and_then(|s| serde_json::from_str::<Vec<String>>(&s).ok())
                .unwrap_or_default();
            VerificationResultRecord {
                id: r.get("id"),
                solution_id: r.get("solution_id"),
                tool_execution_id: r.get("tool_execution_id"),
                source_kind: r.get("source_kind"),
                target_description: r.get("target_description"),
                assumptions,
                method: r.get("method"),
                status: r.get("status"),
                detail_json: r.get("detail_json"),
                created_at: r.get("created_at"),
            }
        })
        .collect())
}

pub async fn insert_solution_artifact(
    pool: &Pool<Sqlite>,
    solution_id: &str,
    attachment_id: &str,
    artifact_type: &str,
    caption: Option<&str>,
    source_tool_execution_id: Option<&str>,
) -> AppResult<String> {
    if !matches!(artifact_type, "plot" | "diagram" | "crop" | "table") {
        return Err(AppError::Validation(format!(
            "不正なアーティファクト種別です: {artifact_type}"
        )));
    }
    let max_order: i64 = sqlx::query_scalar(
        "SELECT COALESCE(MAX(sort_order), -1) FROM solution_artifacts WHERE solution_id = ?1",
    )
    .bind(solution_id)
    .fetch_one(pool)
    .await?;

    let id = Uuid::new_v4().to_string();
    let ts = now_iso();
    sqlx::query(
        "INSERT INTO solution_artifacts
         (id, solution_id, attachment_id, artifact_type, caption, source_tool_execution_id, sort_order, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
    )
    .bind(&id)
    .bind(solution_id)
    .bind(attachment_id)
    .bind(artifact_type)
    .bind(caption)
    .bind(source_tool_execution_id)
    .bind(max_order + 1)
    .bind(&ts)
    .execute(pool)
    .await?;
    Ok(id)
}

pub async fn list_solution_artifacts(
    pool: &Pool<Sqlite>,
    solution_id: &str,
) -> AppResult<Vec<SolutionArtifactItem>> {
    let rows = sqlx::query(
        "SELECT sa.id, sa.solution_id, sa.attachment_id, sa.artifact_type, sa.caption,
                sa.source_tool_execution_id, sa.sort_order, sa.created_at,
                a.relative_path, a.mime_type, a.original_filename, a.byte_size
         FROM solution_artifacts sa
         INNER JOIN attachments a ON a.id = sa.attachment_id
         WHERE sa.solution_id = ?1
         ORDER BY sa.sort_order ASC, sa.created_at ASC",
    )
    .bind(solution_id)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|r| SolutionArtifactItem {
            id: r.get("id"),
            solution_id: r.get("solution_id"),
            attachment_id: r.get("attachment_id"),
            artifact_type: r.get("artifact_type"),
            caption: r.get("caption"),
            source_tool_execution_id: r.get("source_tool_execution_id"),
            sort_order: r.get("sort_order"),
            relative_path: r.get("relative_path"),
            mime_type: r.get("mime_type"),
            original_filename: r.get("original_filename"),
            byte_size: r.get("byte_size"),
            created_at: r.get("created_at"),
        })
        .collect())
}

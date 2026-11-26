use chrono::Utc;
use sqlx::{Pool, Row, Sqlite};
use uuid::Uuid;

use crate::domain::{
    AgentProfile, ChatMessage, Conversation, Problem, SaveAgentProfileRequest, Solution,
    UpsertProblemRequest,
};
use crate::error::{AppError, AppResult};
use crate::security::keyring;

fn now_iso() -> String {
    Utc::now().to_rfc3339()
}

fn parse_string_vec(raw: Option<String>) -> Vec<String> {
    raw.and_then(|s| serde_json::from_str::<Vec<String>>(&s).ok())
        .unwrap_or_default()
}

fn parse_i64_vec(raw: Option<String>) -> Vec<i64> {
    raw.and_then(|s| serde_json::from_str::<Vec<i64>>(&s).ok())
        .unwrap_or_default()
}

fn validate_subject_category(cat: &str) -> AppResult<&str> {
    match cat {
        "mathematics" | "physics" | "other" => Ok(cat),
        _ => Err(AppError::Validation(format!(
            "不正な科目カテゴリです: {cat}"
        ))),
    }
}

fn validate_solution_mode(mode: &str) -> AppResult<&str> {
    match mode {
        "standard" | "concise" | "detailed" | "rigorous" | "beginner" | "hint" | "verified"
        | "review" => Ok(mode),
        _ => Err(AppError::Validation(format!(
            "不正な解答生成モードです: {mode}"
        ))),
    }
}

fn validate_provider_kind(kind: &str) -> AppResult<&str> {
    match kind {
        "codex" | "claude_code" | "antigravity" | "custom_acp" | "mock" => Ok(kind),
        _ => Err(AppError::Validation(format!(
            "不正なプロバイダー種別です: {kind}"
        ))),
    }
}

fn validate_auth_mode(mode: &str) -> AppResult<&str> {
    match mode {
        "official_harness" | "api_key_keyring" => Ok(mode),
        _ => Err(AppError::Validation(format!("不正な認証方式です: {mode}"))),
    }
}

// ============================================================================
// Startup Crash Recovery (docs/database.md Section 5.3)
// ============================================================================

pub async fn recover_interrupted_state(pool: &Pool<Sqlite>) -> AppResult<()> {
    let ts = now_iso();
    sqlx::query(
        "UPDATE agent_runs
         SET status = 'cancelled',
             error_code = COALESCE(error_code, 'APP_RESTARTED'),
             error_message = COALESCE(error_message, 'アプリ終了または再起動により実行が中断されました'),
             finished_at = COALESCE(finished_at, ?1)
         WHERE status = 'running'",
    )
    .bind(&ts)
    .execute(pool)
    .await?;

    sqlx::query(
        "UPDATE messages
         SET status = 'cancelled', updated_at = ?1
         WHERE status = 'streaming'",
    )
    .bind(&ts)
    .execute(pool)
    .await?;

    sqlx::query(
        "UPDATE agent_sessions
         SET status = 'paused', updated_at = ?1
         WHERE status = 'active'",
    )
    .bind(&ts)
    .execute(pool)
    .await?;

    Ok(())
}

// ============================================================================
// Problems
// ============================================================================

pub async fn list_problems(pool: &Pool<Sqlite>, document_id: &str) -> AppResult<Vec<Problem>> {
    let rows = sqlx::query(
        "SELECT id, document_id, problem_number, subproblem_number, sort_order, subject_category,
                field, statement_markdown, given_conditions_json, target_objective,
                source_pages_json, figure_refs_json, uncertainties_json, is_user_verified,
                created_at, updated_at
         FROM problems
         WHERE document_id = ?1
         ORDER BY sort_order ASC, created_at ASC",
    )
    .bind(document_id)
    .fetch_all(pool)
    .await?;

    Ok(rows.iter().map(row_to_problem).collect())
}

pub async fn get_problem(pool: &Pool<Sqlite>, problem_id: &str) -> AppResult<Problem> {
    let row = sqlx::query(
        "SELECT id, document_id, problem_number, subproblem_number, sort_order, subject_category,
                field, statement_markdown, given_conditions_json, target_objective,
                source_pages_json, figure_refs_json, uncertainties_json, is_user_verified,
                created_at, updated_at
         FROM problems
         WHERE id = ?1",
    )
    .bind(problem_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::NotFound("対象の問題が見つかりません".into()))?;

    Ok(row_to_problem(&row))
}

pub async fn upsert_problem(
    pool: &Pool<Sqlite>,
    input: &UpsertProblemRequest,
) -> AppResult<Problem> {
    let problem_number = input.problem_number.trim();
    if problem_number.is_empty() {
        return Err(AppError::Validation("問題番号を入力してください".into()));
    }
    if input.statement_markdown.trim().is_empty() {
        return Err(AppError::Validation("問題文を入力してください".into()));
    }
    let cat = validate_subject_category(&input.subject_category)?;
    let ts = now_iso();
    let given_json = serde_json::to_string(&input.given_conditions)
        .map_err(|e| AppError::Database(e.to_string()))?;
    let pages_json = serde_json::to_string(&input.source_pages)
        .map_err(|e| AppError::Database(e.to_string()))?;
    let figs_json =
        serde_json::to_string(&input.figure_refs).map_err(|e| AppError::Database(e.to_string()))?;
    let unc_json = serde_json::to_string(&input.uncertainties)
        .map_err(|e| AppError::Database(e.to_string()))?;

    let sub_num = input
        .subproblem_number
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let field = input
        .field
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let target = input
        .target_objective
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());

    if let Some(ref id) = input.id {
        let affected = sqlx::query(
            "UPDATE problems
             SET problem_number = ?1, subproblem_number = ?2, subject_category = ?3,
                 field = ?4, statement_markdown = ?5, given_conditions_json = ?6,
                 target_objective = ?7, source_pages_json = ?8, figure_refs_json = ?9,
                 uncertainties_json = ?10, is_user_verified = ?11, updated_at = ?12
             WHERE id = ?13 AND document_id = ?14",
        )
        .bind(problem_number)
        .bind(sub_num)
        .bind(cat)
        .bind(field)
        .bind(input.statement_markdown.trim())
        .bind(&given_json)
        .bind(target)
        .bind(&pages_json)
        .bind(&figs_json)
        .bind(&unc_json)
        .bind(if input.is_user_verified { 1 } else { 0 })
        .bind(&ts)
        .bind(id)
        .bind(&input.document_id)
        .execute(pool)
        .await?
        .rows_affected();

        if affected == 0 {
            return Err(AppError::NotFound("更新対象の問題が見つかりません".into()));
        }
        return get_problem(pool, id).await;
    }

    let max_order: i64 = sqlx::query_scalar(
        "SELECT COALESCE(MAX(sort_order), -1) FROM problems WHERE document_id = ?1",
    )
    .bind(&input.document_id)
    .fetch_one(pool)
    .await?;

    let id = Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO problems
         (id, document_id, problem_number, subproblem_number, sort_order, subject_category,
          field, statement_markdown, given_conditions_json, target_objective,
          source_pages_json, figure_refs_json, uncertainties_json, is_user_verified,
          created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?15)",
    )
    .bind(&id)
    .bind(&input.document_id)
    .bind(problem_number)
    .bind(sub_num)
    .bind(max_order + 1)
    .bind(cat)
    .bind(field)
    .bind(input.statement_markdown.trim())
    .bind(&given_json)
    .bind(target)
    .bind(&pages_json)
    .bind(&figs_json)
    .bind(&unc_json)
    .bind(if input.is_user_verified { 1 } else { 0 })
    .bind(&ts)
    .execute(pool)
    .await?;

    get_problem(pool, &id).await
}

pub async fn replace_document_problems(
    pool: &Pool<Sqlite>,
    document_id: &str,
    items: &[UpsertProblemRequest],
) -> AppResult<Vec<Problem>> {
    let mut tx = pool.begin().await?;
    sqlx::query("DELETE FROM problems WHERE document_id = ?1")
        .bind(document_id)
        .execute(&mut *tx)
        .await?;

    let ts = now_iso();
    for (idx, item) in items.iter().enumerate() {
        let cat = validate_subject_category(&item.subject_category)?;
        let id = Uuid::new_v4().to_string();
        let given_json = serde_json::to_string(&item.given_conditions)
            .map_err(|e| AppError::Database(e.to_string()))?;
        let pages_json = serde_json::to_string(&item.source_pages)
            .map_err(|e| AppError::Database(e.to_string()))?;
        let figs_json = serde_json::to_string(&item.figure_refs)
            .map_err(|e| AppError::Database(e.to_string()))?;
        let unc_json = serde_json::to_string(&item.uncertainties)
            .map_err(|e| AppError::Database(e.to_string()))?;

        sqlx::query(
            "INSERT INTO problems
             (id, document_id, problem_number, subproblem_number, sort_order, subject_category,
              field, statement_markdown, given_conditions_json, target_objective,
              source_pages_json, figure_refs_json, uncertainties_json, is_user_verified,
              created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?15)",
        )
        .bind(&id)
        .bind(document_id)
        .bind(item.problem_number.trim())
        .bind(
            item.subproblem_number
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty()),
        )
        .bind(idx as i64)
        .bind(cat)
        .bind(
            item.field
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty()),
        )
        .bind(item.statement_markdown.trim())
        .bind(&given_json)
        .bind(
            item.target_objective
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty()),
        )
        .bind(&pages_json)
        .bind(&figs_json)
        .bind(&unc_json)
        .bind(if item.is_user_verified { 1 } else { 0 })
        .bind(&ts)
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;
    list_problems(pool, document_id).await
}

pub async fn delete_problem(pool: &Pool<Sqlite>, problem_id: &str) -> AppResult<()> {
    let affected = sqlx::query("DELETE FROM problems WHERE id = ?1")
        .bind(problem_id)
        .execute(pool)
        .await?
        .rows_affected();
    if affected == 0 {
        return Err(AppError::NotFound("対象の問題が見つかりません".into()));
    }
    Ok(())
}

/// Splits one problem into two consecutive problems, preserving ordering.
pub async fn split_problem(
    pool: &Pool<Sqlite>,
    problem_id: &str,
    first_statement: &str,
    second_statement: &str,
    second_subproblem_number: Option<&str>,
) -> AppResult<Vec<Problem>> {
    let orig = get_problem(pool, problem_id).await?;
    if first_statement.trim().is_empty() || second_statement.trim().is_empty() {
        return Err(AppError::Validation(
            "分割後の前後それぞれの問題文を入力してください".into(),
        ));
    }

    let mut tx = pool.begin().await?;
    let ts = now_iso();

    // Shift subsequent problems by +1 to keep order clean
    sqlx::query(
        "UPDATE problems SET sort_order = sort_order + 1, updated_at = ?1
         WHERE document_id = ?2 AND sort_order > ?3",
    )
    .bind(&ts)
    .bind(&orig.document_id)
    .bind(orig.sort_order)
    .execute(&mut *tx)
    .await?;

    sqlx::query(
        "UPDATE problems SET statement_markdown = ?1, is_user_verified = 1, updated_at = ?2 WHERE id = ?3",
    )
    .bind(first_statement.trim())
    .bind(&ts)
    .bind(&orig.id)
    .execute(&mut *tx)
    .await?;

    let new_id = Uuid::new_v4().to_string();
    let given_json = serde_json::to_string(&orig.given_conditions)
        .map_err(|e| AppError::Database(e.to_string()))?;
    let pages_json =
        serde_json::to_string(&orig.source_pages).map_err(|e| AppError::Database(e.to_string()))?;
    let figs_json =
        serde_json::to_string(&orig.figure_refs).map_err(|e| AppError::Database(e.to_string()))?;
    let unc_json = serde_json::to_string(&orig.uncertainties)
        .map_err(|e| AppError::Database(e.to_string()))?;
    let sub_label = second_subproblem_number
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToString::to_string)
        .or_else(|| Some("(2)".to_string()));

    sqlx::query(
        "INSERT INTO problems
         (id, document_id, problem_number, subproblem_number, sort_order, subject_category,
          field, statement_markdown, given_conditions_json, target_objective,
          source_pages_json, figure_refs_json, uncertainties_json, is_user_verified,
          created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, 1, ?14, ?14)",
    )
    .bind(&new_id)
    .bind(&orig.document_id)
    .bind(&orig.problem_number)
    .bind(&sub_label)
    .bind(orig.sort_order + 1)
    .bind(&orig.subject_category)
    .bind(&orig.field)
    .bind(second_statement.trim())
    .bind(&given_json)
    .bind(&orig.target_objective)
    .bind(&pages_json)
    .bind(&figs_json)
    .bind(&unc_json)
    .bind(&ts)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    list_problems(pool, &orig.document_id).await
}

/// Merges `secondary_id` into `primary_id` (must belong to the same document).
pub async fn merge_problems(
    pool: &Pool<Sqlite>,
    primary_id: &str,
    secondary_id: &str,
) -> AppResult<Vec<Problem>> {
    if primary_id == secondary_id {
        return Err(AppError::Validation("同一の問題は結合できません".into()));
    }
    let p1 = get_problem(pool, primary_id).await?;
    let p2 = get_problem(pool, secondary_id).await?;
    if p1.document_id != p2.document_id {
        return Err(AppError::Validation(
            "異なるドキュメントの問題は結合できません".into(),
        ));
    }

    let merged_statement = format!("{}\n\n{}", p1.statement_markdown, p2.statement_markdown);
    let mut given = p1.given_conditions;
    for item in p2.given_conditions {
        if !given.contains(&item) {
            given.push(item);
        }
    }
    let mut pages = p1.source_pages;
    for p in p2.source_pages {
        if !pages.contains(&p) {
            pages.push(p);
        }
    }
    pages.sort_unstable();
    let mut figs = p1.figure_refs;
    for f in p2.figure_refs {
        if !figs.contains(&f) {
            figs.push(f);
        }
    }
    let mut uncs = p1.uncertainties;
    for u in p2.uncertainties {
        if !uncs.contains(&u) {
            uncs.push(u);
        }
    }

    let mut tx = pool.begin().await?;
    let ts = now_iso();
    sqlx::query(
        "UPDATE problems
         SET statement_markdown = ?1, given_conditions_json = ?2, source_pages_json = ?3,
             figure_refs_json = ?4, uncertainties_json = ?5, is_user_verified = 1, updated_at = ?6
         WHERE id = ?7",
    )
    .bind(&merged_statement)
    .bind(serde_json::to_string(&given).map_err(|e| AppError::Database(e.to_string()))?)
    .bind(serde_json::to_string(&pages).map_err(|e| AppError::Database(e.to_string()))?)
    .bind(serde_json::to_string(&figs).map_err(|e| AppError::Database(e.to_string()))?)
    .bind(serde_json::to_string(&uncs).map_err(|e| AppError::Database(e.to_string()))?)
    .bind(&ts)
    .bind(&p1.id)
    .execute(&mut *tx)
    .await?;

    sqlx::query("DELETE FROM problems WHERE id = ?1")
        .bind(&p2.id)
        .execute(&mut *tx)
        .await?;

    tx.commit().await?;
    list_problems(pool, &p1.document_id).await
}

fn row_to_problem(r: &sqlx::sqlite::SqliteRow) -> Problem {
    Problem {
        id: r.get("id"),
        document_id: r.get("document_id"),
        problem_number: r.get("problem_number"),
        subproblem_number: r.get("subproblem_number"),
        sort_order: r.get("sort_order"),
        subject_category: r.get("subject_category"),
        field: r.get("field"),
        statement_markdown: r.get("statement_markdown"),
        given_conditions: parse_string_vec(r.get("given_conditions_json")),
        target_objective: r.get("target_objective"),
        source_pages: parse_i64_vec(r.get("source_pages_json")),
        figure_refs: parse_string_vec(r.get("figure_refs_json")),
        uncertainties: parse_string_vec(r.get("uncertainties_json")),
        is_user_verified: r.get::<i64, _>("is_user_verified") != 0,
        created_at: r.get("created_at"),
        updated_at: r.get("updated_at"),
    }
}

// ============================================================================
// Solutions (Non-destructive Versioning)
// ============================================================================

pub struct NewSolutionVersion<'a> {
    pub document_id: &'a str,
    pub problem_id: Option<&'a str>,
    pub parent_solution_id: Option<&'a str>,
    pub mode: &'a str,
    pub structured_json: &'a str,
    pub validation_status: &'a str,
    pub typst_source_attachment_id: Option<&'a str>,
    pub pdf_attachment_id: Option<&'a str>,
    pub agent_run_id: Option<&'a str>,
    pub model_name: Option<&'a str>,
    pub change_summary: Option<&'a str>,
}

/// Non-destructively creates a new solution version (v1, v2, ...) and marks it `is_current = 1`.
/// Past versions are NEVER overwritten.
pub async fn create_solution_version(
    pool: &Pool<Sqlite>,
    rec: &NewSolutionVersion<'_>,
) -> AppResult<Solution> {
    let mode = validate_solution_mode(rec.mode)?;
    if !matches!(rec.validation_status, "valid" | "invalid" | "user_edited") {
        return Err(AppError::Validation("不正な検証ステータスです".into()));
    }

    let mut tx = pool.begin().await?;
    let max_ver: i64 = sqlx::query_scalar(
        "SELECT COALESCE(MAX(version_number), 0)
         FROM solutions
         WHERE document_id = ?1 AND COALESCE(problem_id, '') = COALESCE(?2, '')",
    )
    .bind(rec.document_id)
    .bind(rec.problem_id)
    .fetch_one(&mut *tx)
    .await?;

    let ts = now_iso();
    // Clear previous is_current within the same (document_id, problem_id) scope
    sqlx::query(
        "UPDATE solutions
         SET is_current = 0, updated_at = ?1
         WHERE document_id = ?2 AND COALESCE(problem_id, '') = COALESCE(?3, '') AND is_current = 1",
    )
    .bind(&ts)
    .bind(rec.document_id)
    .bind(rec.problem_id)
    .execute(&mut *tx)
    .await?;

    let id = Uuid::new_v4().to_string();
    let next_version = max_ver + 1;
    sqlx::query(
        "INSERT INTO solutions
         (id, document_id, problem_id, version_number, parent_solution_id, is_current, mode,
          schema_version, structured_json, validation_status, typst_source_attachment_id,
          pdf_attachment_id, agent_run_id, model_name, change_summary, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, 1, ?6, 1, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?14)",
    )
    .bind(&id)
    .bind(rec.document_id)
    .bind(rec.problem_id)
    .bind(next_version)
    .bind(rec.parent_solution_id)
    .bind(mode)
    .bind(rec.structured_json)
    .bind(rec.validation_status)
    .bind(rec.typst_source_attachment_id)
    .bind(rec.pdf_attachment_id)
    .bind(rec.agent_run_id)
    .bind(rec.model_name)
    .bind(rec.change_summary)
    .bind(&ts)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    get_solution(pool, &id).await
}

pub async fn update_solution_pdf_attachments(
    pool: &Pool<Sqlite>,
    solution_id: &str,
    typst_source_attachment_id: Option<&str>,
    pdf_attachment_id: Option<&str>,
    validation_status: &str,
) -> AppResult<Solution> {
    let ts = now_iso();
    let affected = sqlx::query(
        "UPDATE solutions
         SET typst_source_attachment_id = ?1, pdf_attachment_id = ?2, validation_status = ?3, updated_at = ?4
         WHERE id = ?5",
    )
    .bind(typst_source_attachment_id)
    .bind(pdf_attachment_id)
    .bind(validation_status)
    .bind(&ts)
    .bind(solution_id)
    .execute(pool)
    .await?
    .rows_affected();
    if affected == 0 {
        return Err(AppError::NotFound("解答バージョンが見つかりません".into()));
    }
    get_solution(pool, solution_id).await
}

pub async fn list_solutions(
    pool: &Pool<Sqlite>,
    document_id: &str,
    problem_id: Option<&str>,
) -> AppResult<Vec<Solution>> {
    let rows = if let Some(pid) = problem_id {
        sqlx::query(
            "SELECT id, document_id, problem_id, version_number, parent_solution_id, is_current,
                    mode, schema_version, structured_json, validation_status,
                    typst_source_attachment_id, pdf_attachment_id, agent_run_id, model_name,
                    change_summary, created_at, updated_at
             FROM solutions
             WHERE document_id = ?1 AND problem_id = ?2
             ORDER BY version_number DESC",
        )
        .bind(document_id)
        .bind(pid)
        .fetch_all(pool)
        .await?
    } else {
        sqlx::query(
            "SELECT id, document_id, problem_id, version_number, parent_solution_id, is_current,
                    mode, schema_version, structured_json, validation_status,
                    typst_source_attachment_id, pdf_attachment_id, agent_run_id, model_name,
                    change_summary, created_at, updated_at
             FROM solutions
             WHERE document_id = ?1
             ORDER BY is_current DESC, version_number DESC, created_at DESC",
        )
        .bind(document_id)
        .fetch_all(pool)
        .await?
    };

    Ok(rows.iter().map(row_to_solution).collect())
}

pub async fn get_solution(pool: &Pool<Sqlite>, solution_id: &str) -> AppResult<Solution> {
    let row = sqlx::query(
        "SELECT id, document_id, problem_id, version_number, parent_solution_id, is_current,
                mode, schema_version, structured_json, validation_status,
                typst_source_attachment_id, pdf_attachment_id, agent_run_id, model_name,
                change_summary, created_at, updated_at
         FROM solutions
         WHERE id = ?1",
    )
    .bind(solution_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::NotFound("解答バージョンが見つかりません".into()))?;
    Ok(row_to_solution(&row))
}

fn row_to_solution(r: &sqlx::sqlite::SqliteRow) -> Solution {
    Solution {
        id: r.get("id"),
        document_id: r.get("document_id"),
        problem_id: r.get("problem_id"),
        version_number: r.get("version_number"),
        parent_solution_id: r.get("parent_solution_id"),
        is_current: r.get::<i64, _>("is_current") != 0,
        mode: r.get("mode"),
        schema_version: r.get("schema_version"),
        structured_json: r.get("structured_json"),
        validation_status: r.get("validation_status"),
        typst_source_attachment_id: r.get("typst_source_attachment_id"),
        pdf_attachment_id: r.get("pdf_attachment_id"),
        agent_run_id: r.get("agent_run_id"),
        model_name: r.get("model_name"),
        change_summary: r.get("change_summary"),
        created_at: r.get("created_at"),
        updated_at: r.get("updated_at"),
    }
}

// ============================================================================
// Conversations & Messages
// ============================================================================

pub async fn list_conversations(
    pool: &Pool<Sqlite>,
    document_id: &str,
) -> AppResult<Vec<Conversation>> {
    let rows = sqlx::query(
        "SELECT id, document_id, problem_id, solution_id, title, created_at, updated_at
         FROM conversations
         WHERE document_id = ?1
         ORDER BY updated_at DESC",
    )
    .bind(document_id)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|r| Conversation {
            id: r.get("id"),
            document_id: r.get("document_id"),
            problem_id: r.get("problem_id"),
            solution_id: r.get("solution_id"),
            title: r.get("title"),
            created_at: r.get("created_at"),
            updated_at: r.get("updated_at"),
        })
        .collect())
}

pub async fn create_conversation(
    pool: &Pool<Sqlite>,
    document_id: &str,
    problem_id: Option<&str>,
    solution_id: Option<&str>,
    title: &str,
) -> AppResult<Conversation> {
    let clean_title = title.trim();
    let final_title = if clean_title.is_empty() {
        "新しいチャット"
    } else {
        clean_title
    };
    let id = Uuid::new_v4().to_string();
    let ts = now_iso();
    sqlx::query(
        "INSERT INTO conversations (id, document_id, problem_id, solution_id, title, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)",
    )
    .bind(&id)
    .bind(document_id)
    .bind(problem_id)
    .bind(solution_id)
    .bind(final_title)
    .bind(&ts)
    .execute(pool)
    .await?;

    Ok(Conversation {
        id,
        document_id: document_id.to_string(),
        problem_id: problem_id.map(ToString::to_string),
        solution_id: solution_id.map(ToString::to_string),
        title: final_title.to_string(),
        created_at: ts.clone(),
        updated_at: ts,
    })
}

pub async fn delete_conversation(pool: &Pool<Sqlite>, conversation_id: &str) -> AppResult<()> {
    let affected = sqlx::query("DELETE FROM conversations WHERE id = ?1")
        .bind(conversation_id)
        .execute(pool)
        .await?
        .rows_affected();
    if affected == 0 {
        return Err(AppError::NotFound("対象チャットが見つかりません".into()));
    }
    Ok(())
}

pub async fn list_messages(
    pool: &Pool<Sqlite>,
    conversation_id: &str,
) -> AppResult<Vec<ChatMessage>> {
    let rows = sqlx::query(
        "SELECT id, conversation_id, role, content_markdown, status, agent_run_id,
                referenced_problem_id, created_at, updated_at
         FROM messages
         WHERE conversation_id = ?1
         ORDER BY created_at ASC, rowid ASC",
    )
    .bind(conversation_id)
    .fetch_all(pool)
    .await?;

    let att_rows = sqlx::query(
        "SELECT ma.message_id, ma.attachment_id
         FROM message_attachments ma
         INNER JOIN messages m ON m.id = ma.message_id
         WHERE m.conversation_id = ?1
         ORDER BY ma.created_at ASC",
    )
    .bind(conversation_id)
    .fetch_all(pool)
    .await?;

    let mut att_map: std::collections::HashMap<String, Vec<String>> =
        std::collections::HashMap::new();
    for ar in att_rows {
        let mid: String = ar.get("message_id");
        let aid: String = ar.get("attachment_id");
        att_map.entry(mid).or_default().push(aid);
    }

    Ok(rows
        .into_iter()
        .map(|r| {
            let id: String = r.get("id");
            let attachment_ids = att_map.remove(&id).unwrap_or_default();
            ChatMessage {
                id,
                conversation_id: r.get("conversation_id"),
                role: r.get("role"),
                content_markdown: r.get("content_markdown"),
                status: r.get("status"),
                agent_run_id: r.get("agent_run_id"),
                referenced_problem_id: r.get("referenced_problem_id"),
                attachment_ids,
                created_at: r.get("created_at"),
                updated_at: r.get("updated_at"),
            }
        })
        .collect())
}

#[allow(clippy::too_many_arguments)]
pub async fn insert_message(
    pool: &Pool<Sqlite>,
    conversation_id: &str,
    role: &str,
    content_markdown: &str,
    status: &str,
    agent_run_id: Option<&str>,
    referenced_problem_id: Option<&str>,
    attachment_ids: &[String],
) -> AppResult<ChatMessage> {
    let id = Uuid::new_v4().to_string();
    let ts = now_iso();
    let mut tx = pool.begin().await?;
    sqlx::query(
        "INSERT INTO messages
         (id, conversation_id, role, content_markdown, status, agent_run_id, referenced_problem_id, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8)",
    )
    .bind(&id)
    .bind(conversation_id)
    .bind(role)
    .bind(content_markdown)
    .bind(status)
    .bind(agent_run_id)
    .bind(referenced_problem_id)
    .bind(&ts)
    .execute(&mut *tx)
    .await?;

    for aid in attachment_ids {
        let ma_id = Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO message_attachments (id, message_id, attachment_id, created_at)
             VALUES (?1, ?2, ?3, ?4)",
        )
        .bind(&ma_id)
        .bind(&id)
        .bind(aid)
        .bind(&ts)
        .execute(&mut *tx)
        .await?;
    }

    sqlx::query("UPDATE conversations SET updated_at = ?1 WHERE id = ?2")
        .bind(&ts)
        .bind(conversation_id)
        .execute(&mut *tx)
        .await?;

    tx.commit().await?;
    Ok(ChatMessage {
        id,
        conversation_id: conversation_id.to_string(),
        role: role.to_string(),
        content_markdown: content_markdown.to_string(),
        status: status.to_string(),
        agent_run_id: agent_run_id.map(ToString::to_string),
        referenced_problem_id: referenced_problem_id.map(ToString::to_string),
        attachment_ids: attachment_ids.to_vec(),
        created_at: ts.clone(),
        updated_at: ts,
    })
}

pub async fn update_message_content(
    pool: &Pool<Sqlite>,
    message_id: &str,
    content_markdown: &str,
    status: &str,
) -> AppResult<()> {
    let ts = now_iso();
    sqlx::query(
        "UPDATE messages SET content_markdown = ?1, status = ?2, updated_at = ?3 WHERE id = ?4",
    )
    .bind(content_markdown)
    .bind(status)
    .bind(&ts)
    .bind(message_id)
    .execute(pool)
    .await?;
    Ok(())
}

// ============================================================================
// Agent Profiles, Sessions, and Runs
// ============================================================================

pub async fn seed_default_agent_profiles_if_empty(pool: &Pool<Sqlite>) -> AppResult<()> {
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM agent_profiles")
        .fetch_one(pool)
        .await?;
    if count > 0 {
        return Ok(());
    }

    let ts = now_iso();
    let defaults = [
        ("OpenAI Codex", "codex", "official_harness", 1i64, 1i64),
        (
            "Google Antigravity (ACP)",
            "antigravity",
            "official_harness",
            1i64,
            0i64,
        ),
        (
            "Anthropic Claude Code",
            "claude_code",
            "official_harness",
            1i64,
            0i64,
        ),
        (
            "Mock Agent (テスト用)",
            "mock",
            "official_harness",
            1i64,
            0i64,
        ),
    ];

    for (name, kind, auth_mode, enabled, is_default) in defaults {
        let id = Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO agent_profiles
             (id, name, provider_kind, executable_path, args_json, default_model, auth_mode,
              credential_ref, capabilities_cache_json, is_enabled, is_default, created_at, updated_at)
             VALUES (?1, ?2, ?3, NULL, '[]', NULL, ?4, NULL, NULL, ?5, ?6, ?7, ?7)",
        )
        .bind(&id)
        .bind(name)
        .bind(kind)
        .bind(auth_mode)
        .bind(enabled)
        .bind(is_default)
        .bind(&ts)
        .execute(pool)
        .await?;
    }
    Ok(())
}

pub async fn list_agent_profiles(pool: &Pool<Sqlite>) -> AppResult<Vec<AgentProfile>> {
    let rows = sqlx::query(
        "SELECT id, name, provider_kind, executable_path, args_json, default_model, auth_mode,
                credential_ref, is_enabled, is_default, created_at, updated_at
         FROM agent_profiles
         ORDER BY is_default DESC, created_at ASC",
    )
    .fetch_all(pool)
    .await?;

    Ok(rows.iter().map(row_to_agent_profile).collect())
}

pub async fn get_agent_profile(pool: &Pool<Sqlite>, id: &str) -> AppResult<AgentProfile> {
    let row = sqlx::query(
        "SELECT id, name, provider_kind, executable_path, args_json, default_model, auth_mode,
                credential_ref, is_enabled, is_default, created_at, updated_at
         FROM agent_profiles
         WHERE id = ?1",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::NotFound("エージェントプロファイルが見つかりません".into()))?;
    Ok(row_to_agent_profile(&row))
}

pub async fn get_agent_credential_ref(pool: &Pool<Sqlite>, id: &str) -> AppResult<Option<String>> {
    let cred: Option<String> =
        sqlx::query_scalar("SELECT credential_ref FROM agent_profiles WHERE id = ?1")
            .bind(id)
            .fetch_optional(pool)
            .await?
            .flatten();
    Ok(cred)
}

pub async fn set_agent_credential_ref(
    pool: &Pool<Sqlite>,
    id: &str,
    credential_ref: Option<&str>,
) -> AppResult<AgentProfile> {
    let ts = now_iso();
    let affected =
        sqlx::query("UPDATE agent_profiles SET credential_ref = ?1, updated_at = ?2 WHERE id = ?3")
            .bind(credential_ref)
            .bind(&ts)
            .bind(id)
            .execute(pool)
            .await?
            .rows_affected();
    if affected == 0 {
        return Err(AppError::NotFound(
            "エージェントプロファイルが見つかりません".into(),
        ));
    }
    get_agent_profile(pool, id).await
}

pub async fn save_agent_profile(
    pool: &Pool<Sqlite>,
    req: &SaveAgentProfileRequest,
) -> AppResult<AgentProfile> {
    let name = req.name.trim();
    if name.is_empty() {
        return Err(AppError::Validation(
            "プロファイル名を入力してください".into(),
        ));
    }
    let kind = validate_provider_kind(&req.provider_kind)?;
    let auth_mode = validate_auth_mode(&req.auth_mode)?;
    let args_json =
        serde_json::to_string(&req.args).map_err(|e| AppError::Database(e.to_string()))?;
    let exe_path = req
        .executable_path
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let model = req
        .default_model
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());

    let mut tx = pool.begin().await?;
    let ts = now_iso();
    if req.is_default {
        sqlx::query("UPDATE agent_profiles SET is_default = 0, updated_at = ?1")
            .bind(&ts)
            .execute(&mut *tx)
            .await?;
    }

    let target_id = if let Some(ref existing_id) = req.id {
        let affected = sqlx::query(
            "UPDATE agent_profiles
             SET name = ?1, provider_kind = ?2, executable_path = ?3, args_json = ?4,
                 default_model = ?5, auth_mode = ?6, is_enabled = ?7, is_default = ?8, updated_at = ?9
             WHERE id = ?10",
        )
        .bind(name)
        .bind(kind)
        .bind(exe_path)
        .bind(&args_json)
        .bind(model)
        .bind(auth_mode)
        .bind(if req.is_enabled { 1 } else { 0 })
        .bind(if req.is_default { 1 } else { 0 })
        .bind(&ts)
        .bind(existing_id)
        .execute(&mut *tx)
        .await?
        .rows_affected();
        if affected == 0 {
            return Err(AppError::NotFound(
                "更新対象のエージェントプロファイルが見つかりません".into(),
            ));
        }
        existing_id.clone()
    } else {
        let id = Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO agent_profiles
             (id, name, provider_kind, executable_path, args_json, default_model, auth_mode,
              credential_ref, capabilities_cache_json, is_enabled, is_default, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, NULL, NULL, ?8, ?9, ?10, ?10)",
        )
        .bind(&id)
        .bind(name)
        .bind(kind)
        .bind(exe_path)
        .bind(&args_json)
        .bind(model)
        .bind(auth_mode)
        .bind(if req.is_enabled { 1 } else { 0 })
        .bind(if req.is_default { 1 } else { 0 })
        .bind(&ts)
        .execute(&mut *tx)
        .await?;
        id
    };

    tx.commit().await?;
    get_agent_profile(pool, &target_id).await
}

pub async fn delete_agent_profile(pool: &Pool<Sqlite>, id: &str) -> AppResult<()> {
    let cred_ref = get_agent_credential_ref(pool, id).await?;
    let affected = sqlx::query("DELETE FROM agent_profiles WHERE id = ?1")
        .bind(id)
        .execute(pool)
        .await?
        .rows_affected();
    if affected == 0 {
        return Err(AppError::NotFound(
            "対象のエージェントプロファイルが見つかりません".into(),
        ));
    }
    if let Some(cref) = cred_ref {
        let _ = keyring::delete_secret(&cref);
    }
    Ok(())
}

fn row_to_agent_profile(r: &sqlx::sqlite::SqliteRow) -> AgentProfile {
    let cred_ref: Option<String> = r.get("credential_ref");
    let has_keyring_secret = cred_ref
        .as_deref()
        .and_then(|k| keyring::read_secret(k).ok().flatten())
        .is_some();

    AgentProfile {
        id: r.get("id"),
        name: r.get("name"),
        provider_kind: r.get("provider_kind"),
        executable_path: r.get("executable_path"),
        args: parse_string_vec(r.get("args_json")),
        default_model: r.get("default_model"),
        auth_mode: r.get("auth_mode"),
        has_keyring_secret,
        is_enabled: r.get::<i64, _>("is_enabled") != 0,
        is_default: r.get::<i64, _>("is_default") != 0,
        created_at: r.get("created_at"),
        updated_at: r.get("updated_at"),
    }
}

pub struct StoredAgentSession {
    pub id: String,
    pub agent_profile_id: String,
    pub external_session_id: Option<String>,
    pub workspace_rel_path: String,
    pub status: String,
    pub resumable: bool,
}

pub async fn find_resumable_session(
    pool: &Pool<Sqlite>,
    conversation_id: &str,
    agent_profile_id: &str,
) -> AppResult<Option<StoredAgentSession>> {
    let row = sqlx::query(
        "SELECT id, agent_profile_id, external_session_id, workspace_rel_path, status, resumable
         FROM agent_sessions
         WHERE conversation_id = ?1 AND agent_profile_id = ?2 AND resumable = 1
               AND status IN ('active', 'paused') AND external_session_id IS NOT NULL
         ORDER BY updated_at DESC
         LIMIT 1",
    )
    .bind(conversation_id)
    .bind(agent_profile_id)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|r| StoredAgentSession {
        id: r.get("id"),
        agent_profile_id: r.get("agent_profile_id"),
        external_session_id: r.get("external_session_id"),
        workspace_rel_path: r.get("workspace_rel_path"),
        status: r.get("status"),
        resumable: r.get::<i64, _>("resumable") != 0,
    }))
}

#[allow(clippy::too_many_arguments)]
pub async fn upsert_agent_session(
    pool: &Pool<Sqlite>,
    id: &str,
    agent_profile_id: &str,
    conversation_id: Option<&str>,
    external_session_id: Option<&str>,
    workspace_rel_path: &str,
    status: &str,
    resumable: bool,
) -> AppResult<()> {
    let ts = now_iso();
    sqlx::query(
        "INSERT INTO agent_sessions
         (id, agent_profile_id, conversation_id, external_session_id, workspace_rel_path, status, resumable, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8)
         ON CONFLICT(id) DO UPDATE SET
           external_session_id = excluded.external_session_id,
           status = excluded.status,
           resumable = excluded.resumable,
           updated_at = excluded.updated_at",
    )
    .bind(id)
    .bind(agent_profile_id)
    .bind(conversation_id)
    .bind(external_session_id)
    .bind(workspace_rel_path)
    .bind(status)
    .bind(if resumable { 1 } else { 0 })
    .bind(&ts)
    .execute(pool)
    .await?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub async fn create_agent_run(
    pool: &Pool<Sqlite>,
    id: &str,
    agent_session_id: Option<&str>,
    agent_profile_id: &str,
    purpose: &str,
    document_id: Option<&str>,
    problem_id: Option<&str>,
    model_name: Option<&str>,
) -> AppResult<()> {
    let ts = now_iso();
    sqlx::query(
        "INSERT INTO agent_runs
         (id, agent_session_id, agent_profile_id, purpose, document_id, problem_id, model_name, status, started_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'running', ?8)",
    )
    .bind(id)
    .bind(agent_session_id)
    .bind(agent_profile_id)
    .bind(purpose)
    .bind(document_id)
    .bind(problem_id)
    .bind(model_name)
    .bind(&ts)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn finish_agent_run(
    pool: &Pool<Sqlite>,
    id: &str,
    status: &str,
    usage_json: Option<&str>,
    error_code: Option<&str>,
    error_message: Option<&str>,
) -> AppResult<()> {
    let ts = now_iso();
    sqlx::query(
        "UPDATE agent_runs
         SET status = ?1, usage_json = COALESCE(?2, usage_json),
             error_code = ?3, error_message = ?4, finished_at = ?5
         WHERE id = ?6",
    )
    .bind(status)
    .bind(usage_json)
    .bind(error_code)
    .bind(error_message)
    .bind(&ts)
    .bind(id)
    .execute(pool)
    .await?;
    Ok(())
}

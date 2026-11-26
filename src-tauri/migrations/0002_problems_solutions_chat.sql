-- Phase 2: Problems, Solutions, Conversations, Messages, Agent Profiles/Sessions/Runs
-- Strictly follows docs/database.md Section 3.1, 3.2, 4, and 6.
PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS problems (
    id TEXT PRIMARY KEY NOT NULL,
    document_id TEXT NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    problem_number TEXT NOT NULL,
    subproblem_number TEXT,
    sort_order INTEGER NOT NULL DEFAULT 0,
    subject_category TEXT NOT NULL CHECK (subject_category IN ('mathematics', 'physics', 'other')),
    field TEXT,
    statement_markdown TEXT NOT NULL,
    given_conditions_json TEXT,
    target_objective TEXT,
    source_pages_json TEXT,
    figure_refs_json TEXT,
    uncertainties_json TEXT,
    is_user_verified INTEGER NOT NULL DEFAULT 0 CHECK (is_user_verified IN (0, 1)),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_problems_document_order
    ON problems(document_id, sort_order);

CREATE TABLE IF NOT EXISTS agent_profiles (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL UNIQUE,
    provider_kind TEXT NOT NULL CHECK (provider_kind IN ('codex', 'claude_code', 'antigravity', 'custom_acp', 'mock')),
    executable_path TEXT,
    args_json TEXT,
    default_model TEXT,
    auth_mode TEXT NOT NULL DEFAULT 'official_harness' CHECK (auth_mode IN ('official_harness', 'api_key_keyring')),
    credential_ref TEXT,
    capabilities_cache_json TEXT,
    is_enabled INTEGER NOT NULL DEFAULT 1 CHECK (is_enabled IN (0, 1)),
    is_default INTEGER NOT NULL DEFAULT 0 CHECK (is_default IN (0, 1)),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS conversations (
    id TEXT PRIMARY KEY NOT NULL,
    document_id TEXT NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    problem_id TEXT REFERENCES problems(id) ON DELETE SET NULL,
    solution_id TEXT,
    title TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_conversations_document
    ON conversations(document_id, updated_at DESC);

CREATE TABLE IF NOT EXISTS agent_sessions (
    id TEXT PRIMARY KEY NOT NULL,
    agent_profile_id TEXT NOT NULL REFERENCES agent_profiles(id) ON DELETE CASCADE,
    conversation_id TEXT REFERENCES conversations(id) ON DELETE SET NULL,
    external_session_id TEXT,
    workspace_rel_path TEXT NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('active', 'paused', 'closed', 'crashed')),
    resumable INTEGER NOT NULL DEFAULT 0 CHECK (resumable IN (0, 1)),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS agent_runs (
    id TEXT PRIMARY KEY NOT NULL,
    agent_session_id TEXT REFERENCES agent_sessions(id) ON DELETE SET NULL,
    agent_profile_id TEXT NOT NULL REFERENCES agent_profiles(id) ON DELETE RESTRICT,
    purpose TEXT NOT NULL CHECK (purpose IN ('extraction', 'solution', 'chat', 'verification', 'revision')),
    document_id TEXT REFERENCES documents(id) ON DELETE SET NULL,
    problem_id TEXT REFERENCES problems(id) ON DELETE SET NULL,
    model_name TEXT,
    status TEXT NOT NULL CHECK (status IN ('running', 'completed', 'cancelled', 'failed')),
    usage_json TEXT,
    error_code TEXT,
    error_message TEXT,
    started_at TEXT NOT NULL,
    finished_at TEXT
);

CREATE INDEX IF NOT EXISTS idx_agent_runs_session
    ON agent_runs(agent_session_id, started_at DESC);

CREATE TABLE IF NOT EXISTS solutions (
    id TEXT PRIMARY KEY NOT NULL,
    document_id TEXT NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    problem_id TEXT REFERENCES problems(id) ON DELETE CASCADE,
    version_number INTEGER NOT NULL,
    parent_solution_id TEXT REFERENCES solutions(id) ON DELETE SET NULL,
    is_current INTEGER NOT NULL DEFAULT 1 CHECK (is_current IN (0, 1)),
    mode TEXT NOT NULL CHECK (mode IN ('standard', 'concise', 'detailed', 'rigorous', 'beginner', 'hint', 'verified', 'review')),
    schema_version INTEGER NOT NULL DEFAULT 1,
    structured_json TEXT NOT NULL,
    validation_status TEXT NOT NULL CHECK (validation_status IN ('valid', 'invalid', 'user_edited')),
    typst_source_attachment_id TEXT REFERENCES attachments(id) ON DELETE SET NULL,
    pdf_attachment_id TEXT REFERENCES attachments(id) ON DELETE SET NULL,
    agent_run_id TEXT REFERENCES agent_runs(id) ON DELETE SET NULL,
    model_name TEXT,
    change_summary TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE UNIQUE INDEX IF NOT EXISTS uq_solutions_current
    ON solutions(document_id, COALESCE(problem_id, '')) WHERE is_current = 1;

CREATE UNIQUE INDEX IF NOT EXISTS uq_solutions_version
    ON solutions(document_id, COALESCE(problem_id, ''), version_number);

CREATE TABLE IF NOT EXISTS solution_artifacts (
    id TEXT PRIMARY KEY NOT NULL,
    solution_id TEXT NOT NULL REFERENCES solutions(id) ON DELETE CASCADE,
    attachment_id TEXT NOT NULL REFERENCES attachments(id) ON DELETE RESTRICT,
    artifact_type TEXT NOT NULL CHECK (artifact_type IN ('plot', 'diagram', 'crop', 'table')),
    caption TEXT,
    source_tool_execution_id TEXT,
    sort_order INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS messages (
    id TEXT PRIMARY KEY NOT NULL,
    conversation_id TEXT NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
    role TEXT NOT NULL CHECK (role IN ('user', 'assistant', 'system')),
    content_markdown TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'completed' CHECK (status IN ('streaming', 'completed', 'cancelled', 'failed')),
    agent_run_id TEXT REFERENCES agent_runs(id) ON DELETE SET NULL,
    referenced_problem_id TEXT REFERENCES problems(id) ON DELETE SET NULL,
    revision_patch_json TEXT,
    revision_status TEXT CHECK (revision_status IS NULL OR revision_status IN ('pending', 'approved', 'rejected')),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_messages_conversation_created
    ON messages(conversation_id, created_at ASC);

CREATE TABLE IF NOT EXISTS message_attachments (
    id TEXT PRIMARY KEY NOT NULL,
    message_id TEXT NOT NULL REFERENCES messages(id) ON DELETE CASCADE,
    attachment_id TEXT NOT NULL REFERENCES attachments(id) ON DELETE RESTRICT,
    created_at TEXT NOT NULL
);

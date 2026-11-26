-- Phase 3: Science MCP Tool Executions & Verification Results
-- Follows docs/database.md Section 3.2 (Tables 16 & 17), Section 4, and Section 6.
PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS tool_executions (
    id TEXT PRIMARY KEY NOT NULL,
    agent_run_id TEXT REFERENCES agent_runs(id) ON DELETE CASCADE,
    document_id TEXT REFERENCES documents(id) ON DELETE CASCADE,
    solution_id TEXT REFERENCES solutions(id) ON DELETE SET NULL,
    server_name TEXT NOT NULL,
    tool_name TEXT NOT NULL,
    arguments_json TEXT NOT NULL,
    result_json TEXT,
    status TEXT NOT NULL CHECK (status IN ('running', 'succeeded', 'failed', 'timeout', 'cancelled')),
    duration_ms INTEGER,
    error_message TEXT,
    created_at TEXT NOT NULL,
    finished_at TEXT
);

CREATE INDEX IF NOT EXISTS idx_tool_executions_run
    ON tool_executions(agent_run_id, created_at ASC);

CREATE INDEX IF NOT EXISTS idx_tool_executions_document
    ON tool_executions(document_id, created_at DESC);

CREATE TABLE IF NOT EXISTS verification_results (
    id TEXT PRIMARY KEY NOT NULL,
    solution_id TEXT NOT NULL REFERENCES solutions(id) ON DELETE CASCADE,
    tool_execution_id TEXT REFERENCES tool_executions(id) ON DELETE SET NULL,
    source_kind TEXT NOT NULL CHECK (source_kind IN ('science_mcp', 'ai_self_check', 'user_check')),
    target_description TEXT NOT NULL,
    assumptions_json TEXT,
    method TEXT NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('passed', 'failed', 'inconclusive', 'not_run', 'error')),
    detail_json TEXT,
    created_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_verification_results_solution
    ON verification_results(solution_id, status);

-- Phase 4: Google Drive Export, Folder/File Mappings, and Persistent Upload Job Queue
-- Follows docs/database.md Section 3.3 (Tables 18..21), Section 4, and Section 6.
-- Security & Data Safety Invariants:
-- - OAuth tokens and client secrets are NEVER stored in SQLite (OS keyring ref only).
-- - drive_files.document_id uses ON DELETE SET NULL so deleting a local document NEVER deletes Drive files.
-- - Partial unique index uq_drive_jobs_active prevents duplicate concurrent uploads for the same logical role.
PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS drive_connections (
    id TEXT PRIMARY KEY NOT NULL,
    account_email TEXT NOT NULL UNIQUE,
    account_display_name TEXT,
    credential_ref TEXT NOT NULL,
    scopes_json TEXT NOT NULL,
    root_folder_drive_id TEXT,
    root_folder_name TEXT NOT NULL DEFAULT 'Theorem',
    upload_mode TEXT NOT NULL DEFAULT 'manual'
        CHECK (upload_mode IN ('manual', 'auto', 'folder_auto')),
    version_policy TEXT NOT NULL DEFAULT 'update_latest'
        CHECK (version_policy IN ('update_latest', 'create_new', 'manual_version')),
    include_past_versions INTEGER NOT NULL DEFAULT 0,
    include_solution_source INTEGER NOT NULL DEFAULT 0,
    include_artifacts INTEGER NOT NULL DEFAULT 0,
    status TEXT NOT NULL DEFAULT 'connected'
        CHECK (status IN ('connected', 'reauth_required', 'disconnected')),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS drive_folders (
    id TEXT PRIMARY KEY NOT NULL,
    connection_id TEXT NOT NULL REFERENCES drive_connections(id) ON DELETE CASCADE,
    entity_type TEXT NOT NULL
        CHECK (entity_type IN ('root', 'subject', 'folder', 'document')),
    entity_id TEXT,
    drive_folder_id TEXT NOT NULL,
    parent_drive_folder_id TEXT,
    folder_name TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE UNIQUE INDEX IF NOT EXISTS uq_drive_folders_entity
    ON drive_folders(connection_id, entity_type, COALESCE(entity_id, ''));

CREATE TABLE IF NOT EXISTS drive_files (
    id TEXT PRIMARY KEY NOT NULL,
    connection_id TEXT NOT NULL REFERENCES drive_connections(id) ON DELETE CASCADE,
    document_id TEXT REFERENCES documents(id) ON DELETE SET NULL,
    logical_role TEXT NOT NULL,
    drive_file_id TEXT NOT NULL,
    drive_folder_id TEXT NOT NULL,
    drive_filename TEXT NOT NULL,
    web_view_link TEXT,
    last_uploaded_attachment_id TEXT REFERENCES attachments(id) ON DELETE SET NULL,
    last_uploaded_sha256 TEXT,
    remote_md5_checksum TEXT,
    remote_modified_time TEXT,
    remote_version TEXT,
    sync_status TEXT NOT NULL DEFAULT 'uploaded'
        CHECK (sync_status IN ('uploaded', 'update_required', 'conflict', 'remote_deleted', 'error')),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE UNIQUE INDEX IF NOT EXISTS uq_drive_files_logical
    ON drive_files(connection_id, document_id, logical_role)
    WHERE document_id IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_drive_files_document
    ON drive_files(document_id, updated_at DESC);

CREATE TABLE IF NOT EXISTS drive_upload_jobs (
    id TEXT PRIMARY KEY NOT NULL,
    connection_id TEXT NOT NULL REFERENCES drive_connections(id) ON DELETE CASCADE,
    document_id TEXT NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    logical_role TEXT NOT NULL,
    attachment_id TEXT REFERENCES attachments(id) ON DELETE SET NULL,
    drive_file_id TEXT,
    target_filename TEXT NOT NULL,
    idempotency_key TEXT NOT NULL,
    force_overwrite INTEGER NOT NULL DEFAULT 0,
    save_as_new INTEGER NOT NULL DEFAULT 0,
    status TEXT NOT NULL DEFAULT 'pending'
        CHECK (status IN ('pending', 'uploading', 'uploaded', 'update_required', 'failed', 'conflict', 'cancelled')),
    attempt_count INTEGER NOT NULL DEFAULT 0,
    next_retry_at TEXT,
    last_error_code TEXT,
    last_error_message TEXT,
    conflict_remote_md5 TEXT,
    conflict_remote_modified_time TEXT,
    conflict_remote_version TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE UNIQUE INDEX IF NOT EXISTS uq_drive_jobs_active
    ON drive_upload_jobs(connection_id, document_id, logical_role)
    WHERE status IN ('pending', 'uploading');

CREATE INDEX IF NOT EXISTS idx_drive_jobs_schedule
    ON drive_upload_jobs(status, next_retry_at);

CREATE INDEX IF NOT EXISTS idx_drive_jobs_document
    ON drive_upload_jobs(document_id, updated_at DESC);

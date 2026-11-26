-- Phase 1 Initial Library Schema (docs/database.md Section 3.1 & 6)
PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS subjects (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL UNIQUE,
    category TEXT NOT NULL CHECK (category IN ('mathematics', 'physics', 'other')),
    description TEXT,
    sort_order INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS folders (
    id TEXT PRIMARY KEY NOT NULL,
    subject_id TEXT NOT NULL REFERENCES subjects(id) ON DELETE CASCADE,
    parent_id TEXT REFERENCES folders(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    auto_upload_drive INTEGER NOT NULL DEFAULT 0 CHECK (auto_upload_drive IN (0, 1)),
    sort_order INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE UNIQUE INDEX IF NOT EXISTS uq_folders_hierarchy
    ON folders(subject_id, COALESCE(parent_id, ''), name);

CREATE INDEX IF NOT EXISTS idx_folders_subject_parent
    ON folders(subject_id, parent_id);

CREATE TABLE IF NOT EXISTS documents (
    id TEXT PRIMARY KEY NOT NULL,
    subject_id TEXT NOT NULL REFERENCES subjects(id) ON DELETE RESTRICT,
    folder_id TEXT REFERENCES folders(id) ON DELETE SET NULL,
    title TEXT NOT NULL,
    source_type TEXT NOT NULL CHECK (source_type IN ('pdf', 'image', 'multi_image')),
    primary_attachment_id TEXT,
    page_count INTEGER,
    recognition_mode TEXT NOT NULL DEFAULT 'printed_only'
        CHECK (recognition_mode IN ('printed_only', 'include_handwriting', 'page_range', 'handwritten_answer')),
    study_status TEXT NOT NULL DEFAULT 'unstarted'
        CHECK (study_status IN ('unstarted', 'in_progress', 'solved', 'review_needed')),
    is_favorite INTEGER NOT NULL DEFAULT 0 CHECK (is_favorite IN (0, 1)),
    last_opened_at TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS attachments (
    id TEXT PRIMARY KEY NOT NULL,
    document_id TEXT REFERENCES documents(id) ON DELETE SET NULL,
    kind TEXT NOT NULL CHECK (kind IN (
        'original_pdf',
        'original_image',
        'solution_pdf',
        'diagram',
        'plot',
        'thumbnail',
        'chat_image',
        'source_json',
        'source_typst'
    )),
    original_filename TEXT NOT NULL,
    relative_path TEXT NOT NULL UNIQUE,
    mime_type TEXT NOT NULL,
    byte_size INTEGER NOT NULL,
    sha256 TEXT NOT NULL,
    width INTEGER,
    height INTEGER,
    page_count INTEGER,
    created_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_documents_subject_folder
    ON documents(subject_id, folder_id);
CREATE INDEX IF NOT EXISTS idx_documents_last_opened
    ON documents(last_opened_at DESC);
CREATE INDEX IF NOT EXISTS idx_documents_favorite
    ON documents(is_favorite) WHERE is_favorite = 1;

CREATE INDEX IF NOT EXISTS idx_attachments_sha256_kind
    ON attachments(sha256, kind);
CREATE INDEX IF NOT EXISTS idx_attachments_document
    ON attachments(document_id, kind);

CREATE TABLE IF NOT EXISTS tags (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL UNIQUE,
    color TEXT,
    created_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS document_tags (
    document_id TEXT NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    tag_id TEXT NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
    created_at TEXT NOT NULL,
    PRIMARY KEY (document_id, tag_id)
);

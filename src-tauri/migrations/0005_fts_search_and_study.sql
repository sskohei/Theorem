-- Phase 5: Reference Materials & SQLite FTS5 Cross-Entity Full-Text Search
-- Follows docs/database.md Section 6 (`0005_fts_search_and_study.sql`).
PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS reference_materials (
    id TEXT PRIMARY KEY NOT NULL,
    subject_id TEXT REFERENCES subjects(id) ON DELETE SET NULL,
    document_id TEXT REFERENCES documents(id) ON DELETE CASCADE,
    title TEXT NOT NULL,
    kind TEXT NOT NULL CHECK (kind IN ('lecture_note', 'textbook_excerpt', 'formula_sheet', 'handwritten_attempt')),
    content_markdown TEXT NOT NULL,
    attachment_id TEXT REFERENCES attachments(id) ON DELETE SET NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_reference_materials_document
    ON reference_materials(document_id, updated_at DESC);

CREATE INDEX IF NOT EXISTS idx_reference_materials_subject
    ON reference_materials(subject_id, updated_at DESC);

CREATE INDEX IF NOT EXISTS idx_documents_study_status
    ON documents(study_status);

-- Unified FTS5 virtual table with `trigram` tokenizer for Japanese/CJK + English substring search
CREATE VIRTUAL TABLE IF NOT EXISTS search_index_fts USING fts5(
    entity_type UNINDEXED,
    entity_id UNINDEXED,
    document_id UNINDEXED,
    title,
    body,
    tokenize = 'trigram'
);

-- Keep FTS5 synchronized with documents
CREATE TRIGGER IF NOT EXISTS trg_fts_documents_insert AFTER INSERT ON documents BEGIN
    INSERT INTO search_index_fts(entity_type, entity_id, document_id, title, body)
    VALUES ('document', NEW.id, NEW.id, NEW.title, NEW.title);
END;

CREATE TRIGGER IF NOT EXISTS trg_fts_documents_update AFTER UPDATE OF title ON documents BEGIN
    DELETE FROM search_index_fts WHERE entity_type = 'document' AND entity_id = OLD.id;
    INSERT INTO search_index_fts(entity_type, entity_id, document_id, title, body)
    VALUES ('document', NEW.id, NEW.id, NEW.title, NEW.title);
END;

CREATE TRIGGER IF NOT EXISTS trg_fts_documents_delete AFTER DELETE ON documents BEGIN
    DELETE FROM search_index_fts WHERE entity_type = 'document' AND entity_id = OLD.id;
END;

-- Keep FTS5 synchronized with problems
CREATE TRIGGER IF NOT EXISTS trg_fts_problems_insert AFTER INSERT ON problems BEGIN
    INSERT INTO search_index_fts(entity_type, entity_id, document_id, title, body)
    VALUES (
        'problem',
        NEW.id,
        NEW.document_id,
        NEW.problem_number || COALESCE(' ' || NEW.subproblem_number, ''),
        NEW.statement_markdown || ' ' || COALESCE(NEW.target_objective, '') || ' ' || COALESCE(NEW.field, '')
    );
END;

CREATE TRIGGER IF NOT EXISTS trg_fts_problems_update AFTER UPDATE OF problem_number, subproblem_number, statement_markdown, target_objective, field ON problems BEGIN
    DELETE FROM search_index_fts WHERE entity_type = 'problem' AND entity_id = OLD.id;
    INSERT INTO search_index_fts(entity_type, entity_id, document_id, title, body)
    VALUES (
        'problem',
        NEW.id,
        NEW.document_id,
        NEW.problem_number || COALESCE(' ' || NEW.subproblem_number, ''),
        NEW.statement_markdown || ' ' || COALESCE(NEW.target_objective, '') || ' ' || COALESCE(NEW.field, '')
    );
END;

CREATE TRIGGER IF NOT EXISTS trg_fts_problems_delete AFTER DELETE ON problems BEGIN
    DELETE FROM search_index_fts WHERE entity_type = 'problem' AND entity_id = OLD.id;
END;

-- Keep FTS5 synchronized with solutions
CREATE TRIGGER IF NOT EXISTS trg_fts_solutions_insert AFTER INSERT ON solutions BEGIN
    INSERT INTO search_index_fts(entity_type, entity_id, document_id, title, body)
    VALUES (
        'solution',
        NEW.id,
        NEW.document_id,
        'v' || NEW.version_number || ' (' || NEW.mode || ')',
        NEW.structured_json || ' ' || COALESCE(NEW.change_summary, '')
    );
END;

CREATE TRIGGER IF NOT EXISTS trg_fts_solutions_update AFTER UPDATE OF structured_json, change_summary, mode ON solutions BEGIN
    DELETE FROM search_index_fts WHERE entity_type = 'solution' AND entity_id = OLD.id;
    INSERT INTO search_index_fts(entity_type, entity_id, document_id, title, body)
    VALUES (
        'solution',
        NEW.id,
        NEW.document_id,
        'v' || NEW.version_number || ' (' || NEW.mode || ')',
        NEW.structured_json || ' ' || COALESCE(NEW.change_summary, '')
    );
END;

CREATE TRIGGER IF NOT EXISTS trg_fts_solutions_delete AFTER DELETE ON solutions BEGIN
    DELETE FROM search_index_fts WHERE entity_type = 'solution' AND entity_id = OLD.id;
END;

-- Keep FTS5 synchronized with chat messages
CREATE TRIGGER IF NOT EXISTS trg_fts_messages_insert AFTER INSERT ON messages BEGIN
    INSERT INTO search_index_fts(entity_type, entity_id, document_id, title, body)
    SELECT 'message', NEW.id, c.document_id, c.title, NEW.content_markdown
    FROM conversations c WHERE c.id = NEW.conversation_id;
END;

CREATE TRIGGER IF NOT EXISTS trg_fts_messages_update AFTER UPDATE OF content_markdown ON messages BEGIN
    DELETE FROM search_index_fts WHERE entity_type = 'message' AND entity_id = OLD.id;
    INSERT INTO search_index_fts(entity_type, entity_id, document_id, title, body)
    SELECT 'message', NEW.id, c.document_id, c.title, NEW.content_markdown
    FROM conversations c WHERE c.id = NEW.conversation_id;
END;

CREATE TRIGGER IF NOT EXISTS trg_fts_messages_delete AFTER DELETE ON messages BEGIN
    DELETE FROM search_index_fts WHERE entity_type = 'message' AND entity_id = OLD.id;
END;

-- Keep FTS5 synchronized with reference materials
CREATE TRIGGER IF NOT EXISTS trg_fts_references_insert AFTER INSERT ON reference_materials BEGIN
    INSERT INTO search_index_fts(entity_type, entity_id, document_id, title, body)
    VALUES ('reference', NEW.id, COALESCE(NEW.document_id, ''), NEW.title, NEW.content_markdown);
END;

CREATE TRIGGER IF NOT EXISTS trg_fts_references_update AFTER UPDATE OF title, content_markdown ON reference_materials BEGIN
    DELETE FROM search_index_fts WHERE entity_type = 'reference' AND entity_id = OLD.id;
    INSERT INTO search_index_fts(entity_type, entity_id, document_id, title, body)
    VALUES ('reference', NEW.id, COALESCE(NEW.document_id, ''), NEW.title, NEW.content_markdown);
END;

CREATE TRIGGER IF NOT EXISTS trg_fts_references_delete AFTER DELETE ON reference_materials BEGIN
    DELETE FROM search_index_fts WHERE entity_type = 'reference' AND entity_id = OLD.id;
END;

-- Backfill existing Phase 1..4 records into FTS5 index immediately upon migration
INSERT INTO search_index_fts(entity_type, entity_id, document_id, title, body)
SELECT 'document', id, id, title, title FROM documents;

INSERT INTO search_index_fts(entity_type, entity_id, document_id, title, body)
SELECT
    'problem',
    id,
    document_id,
    problem_number || COALESCE(' ' || subproblem_number, ''),
    statement_markdown || ' ' || COALESCE(target_objective, '') || ' ' || COALESCE(field, '')
FROM problems;

INSERT INTO search_index_fts(entity_type, entity_id, document_id, title, body)
SELECT
    'solution',
    id,
    document_id,
    'v' || version_number || ' (' || mode || ')',
    structured_json || ' ' || COALESCE(change_summary, '')
FROM solutions;

INSERT INTO search_index_fts(entity_type, entity_id, document_id, title, body)
SELECT 'message', m.id, c.document_id, c.title, m.content_markdown
FROM messages m
JOIN conversations c ON c.id = m.conversation_id;

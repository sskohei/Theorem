use std::fs;
use std::io;
use std::path::Path;

use crate::database::{Database, snapshot_legacy_database};
use crate::error::{AppError, AppResult};

use super::{AppStorage, LEGACY_DATABASE_REL_PATH, LIBRARY_FOLDERS, SETTINGS_REL_PATH};

const LEGACY_APP_IDENTIFIER: &str = "dev.formuladesk.app";
const CURRENT_APP_IDENTIFIER: &str = "dev.theorem.desktop";

/// Publishes the copied library as a unit. An incomplete staging directory has
/// no effect on either library and a later startup can retry from the source.
pub(crate) async fn migrate_legacy_library(target: &Path) -> AppResult<()> {
    if target.file_name().and_then(|name| name.to_str()) != Some(CURRENT_APP_IDENTIFIER)
        || !target_is_empty(target)?
    {
        return Ok(());
    }
    let Some(parent) = target.parent() else {
        return Ok(());
    };
    let source = parent.join(LEGACY_APP_IDENTIFIER);
    if !source.join(LEGACY_DATABASE_REL_PATH).is_file() {
        return Ok(());
    }
    let source_meta = fs::symlink_metadata(&source)?;
    if is_link(&source_meta) {
        return Err(AppError::Security("Legacy library is a link".into()));
    }
    let legacy_db = source.join(LEGACY_DATABASE_REL_PATH);
    for path in [
        source.join("database"),
        legacy_db.clone(),
        legacy_db.with_extension("db-wal"),
        legacy_db.with_extension("db-shm"),
    ] {
        if let Ok(meta) = fs::symlink_metadata(&path)
            && is_link(&meta)
        {
            return Err(AppError::Security("Legacy database path is a link".into()));
        }
    }

    let staging = tempfile::Builder::new()
        .prefix(".theorem-import-")
        .tempdir_in(parent)?;
    let storage = AppStorage::initialize(staging.path().to_path_buf())?;
    snapshot_legacy_database(&legacy_db, &storage.database_path()).await?;
    for name in LIBRARY_FOLDERS
        .into_iter()
        .chain(["backups", SETTINGS_REL_PATH])
    {
        copy_entry(&source.join(name), &staging.path().join(name))?;
    }
    storage.load_settings()?;
    Database::connect(&storage.database_path())
        .await?
        .close()
        .await;

    if !target_is_empty(target)? {
        return Ok(());
    }
    match fs::remove_dir(target) {
        Ok(()) => {}
        Err(err) if err.kind() == io::ErrorKind::NotFound => {}
        Err(_) if !target_is_empty(target)? => return Ok(()),
        Err(err) => return Err(err.into()),
    }
    match super::retry_file_operation(|| fs::rename(staging.path(), target)) {
        Ok(()) => Ok(()),
        Err(_) if !target_is_empty(target)? => Ok(()),
        Err(err) => Err(err.into()),
    }
}

fn target_is_empty(path: &Path) -> AppResult<bool> {
    match fs::symlink_metadata(path) {
        Ok(meta) if meta.is_dir() && !is_link(&meta) => Ok(fs::read_dir(path)?.next().is_none()),
        Ok(_) => Ok(false),
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(true),
        Err(err) => Err(err.into()),
    }
}

fn is_link(meta: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        meta.file_attributes() & 0x400 != 0 // FILE_ATTRIBUTE_REPARSE_POINT
    }
    #[cfg(not(windows))]
    meta.file_type().is_symlink()
}

fn copy_entry(source: &Path, dest: &Path) -> AppResult<()> {
    let meta = match fs::symlink_metadata(source) {
        Ok(meta) => meta,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(err) => return Err(err.into()),
    };
    if is_link(&meta) {
        return Err(AppError::Security("Legacy library contains a link".into()));
    }
    if meta.is_dir() {
        fs::create_dir_all(dest)?;
        for entry in fs::read_dir(source)? {
            let entry = entry?;
            copy_entry(&entry.path(), &dest.join(entry.file_name()))?;
        }
    } else if meta.is_file() {
        fs::copy(source, dest)?;
        fs::OpenOptions::new().write(true).open(dest)?.sync_all()?;
    } else {
        return Err(AppError::Security(
            "Legacy library contains an unsupported entry".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::{Engine, engine::general_purpose::STANDARD};
    use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode};
    use sqlx::{Connection, SqliteConnection};
    use std::io::Write;
    use zip::{ZipWriter, write::SimpleFileOptions};

    use crate::application::AppState;

    const DOC: &str = "old-document";
    const ATTACHMENT: &str = "old-original";
    const ORIGINAL: &str = "attachments/originals/legacy.png";

    #[tokio::test]
    async fn phase1_wal_migration_collision_retry_and_both_zip_formats() {
        let parent = tempfile::tempdir().expect("isolated application directories");
        let legacy = parent.path().join(LEGACY_APP_IDENTIFIER);
        let current = parent.path().join(CURRENT_APP_IDENTIFIER);
        fs::create_dir_all(legacy.join("database")).expect("legacy database dir");
        fs::create_dir_all(legacy.join("attachments/originals")).expect("legacy attachments");
        fs::create_dir_all(legacy.join("backups")).expect("legacy backups");
        let png = STANDARD.decode("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAusB9Y9Z6zcAAAAASUVORK5CYII=").expect("fixture PNG");
        fs::write(legacy.join(ORIGINAL), &png).expect("write original");
        let settings = serde_json::to_vec(&serde_json::json!({
            "language": "ja",
            "theme": "dark",
            "defaultSubjectId": "legacy-subject",
            "defaultRecognitionMode": "include_handwriting",
            "leftSidebarWidth": 312,
            "rightSidebarWidth": 360
        }))
        .expect("Phase 1 settings");

        let legacy_path = legacy.join(LEGACY_DATABASE_REL_PATH);
        let mut writer = SqliteConnection::connect_with(
            &SqliteConnectOptions::new()
                .filename(&legacy_path)
                .create_if_missing(true)
                .journal_mode(SqliteJournalMode::Wal),
        )
        .await
        .expect("open legacy sqlite");
        let mut phase1 = sqlx::migrate!("./migrations");
        phase1
            .migrations
            .to_mut()
            .retain(|migration| migration.version == 1);
        phase1.run(&mut writer).await.expect("apply Phase 1 schema");
        sqlx::query("PRAGMA wal_checkpoint(TRUNCATE)")
            .execute(&mut writer)
            .await
            .expect("checkpoint base schema");
        sqlx::query("PRAGMA wal_autocheckpoint=0")
            .execute(&mut writer)
            .await
            .expect("disable auto-checkpoint");
        sqlx::raw_sql(
            "BEGIN;
            INSERT INTO subjects (id, name, category, created_at, updated_at) VALUES
              ('legacy-subject', 'Legacy', 'mathematics', '2026-11-25', '2026-11-25');
            INSERT INTO documents (id, subject_id, title, source_type, primary_attachment_id, created_at, updated_at)
              VALUES ('old-document', 'legacy-subject', 'WAL only', 'image', 'old-original', '2026-11-25', '2026-11-25');
            INSERT INTO attachments (id, document_id, kind, original_filename, relative_path, mime_type, byte_size, sha256, created_at)
              VALUES ('old-original', 'old-document', 'original_image', 'legacy.png',
                      'attachments/originals/legacy.png', 'image/png', 68, 'fixture', '2026-11-25');
            COMMIT;"
        ).execute(&mut writer).await.expect("commit documents in WAL");
        let wal_path = legacy_path.with_extension("db-wal");
        assert!(fs::metadata(&wal_path).expect("WAL").len() > 0);
        let base_db = fs::read(&legacy_path).expect("base sqlite file");
        let wal = fs::read(&wal_path).expect("WAL bytes");

        // Legacy ZIPs used the old filename and a checkpoint-independent snapshot.
        let zip_path = legacy.join("backups/formuladesk-backup-fixture.zip");
        let snapshot = parent.path().join("phase1-snapshot.db");
        snapshot_legacy_database(&legacy_path, &snapshot)
            .await
            .expect("Phase 1 backup snapshot");
        let mut zip = ZipWriter::new(fs::File::create(&zip_path).expect("old ZIP"));
        for (name, contents) in [
            (
                LEGACY_DATABASE_REL_PATH,
                fs::read(snapshot).expect("snapshot"),
            ),
            (SETTINGS_REL_PATH, settings.clone()),
            (ORIGINAL, png.clone()),
        ] {
            zip.start_file(name, SimpleFileOptions::default())
                .expect("ZIP entry");
            zip.write_all(&contents).expect("ZIP contents");
        }
        zip.finish().expect("finish ZIP");

        // Simulate an abandoned staging directory and a failed attempt before publication.
        fs::create_dir(parent.path().join(".theorem-import-interrupted"))
            .expect("interrupted import");
        fs::write(legacy.join(SETTINGS_REL_PATH), b"invalid").expect("bad settings");
        assert!(AppState::initialize(current.clone()).await.is_err());
        assert!(!current.exists());
        fs::write(legacy.join(SETTINGS_REL_PATH), &settings).expect("repair settings");

        let state = AppState::initialize(current.clone())
            .await
            .expect("retry startup");
        assert_eq!(
            state.bootstrap().await.expect("bootstrap").settings.theme,
            "dark"
        );
        let detail = state
            .get_document_detail(DOC)
            .await
            .expect("migrated document");
        assert_eq!(detail.document.title, "WAL only");
        assert_eq!(
            detail.document.primary_attachment_id.as_deref(),
            Some(ATTACHMENT)
        );
        assert_eq!(
            state
                .read_attachment_base64(ATTACHMENT)
                .await
                .expect("migrated original")
                .base64_data,
            STANDARD.encode(&png)
        );
        assert_eq!(fs::read(&legacy_path).expect("original database"), base_db);
        assert_eq!(fs::read(&wal_path).expect("original WAL"), wal);

        state
            .delete_document(DOC)
            .await
            .expect("remove migrated document in new library");
        state.db.read().await.close().await;
        drop(state);
        let state = AppState::initialize(current)
            .await
            .expect("open existing Theorem library");
        assert!(state.get_document_detail(DOC).await.is_err());

        // The existing library was preserved. Import the copied legacy backup explicitly.
        let old_backup = state.list_backups().expect("available backups").remove(0);
        let boot = state
            .restore_backup(&old_backup.relative_path)
            .await
            .expect("restore Phase 1 ZIP");
        assert_eq!(
            boot.settings.default_subject_id.as_deref(),
            Some("legacy-subject")
        );
        assert_eq!(
            boot.documents[0].primary_attachment_id.as_deref(),
            Some(ATTACHMENT)
        );
        assert_eq!(
            state
                .read_attachment_base64(ATTACHMENT)
                .await
                .expect("restored legacy original")
                .base64_data,
            STANDARD.encode(&png)
        );
        let new_backup = state.create_backup().await.expect("create Theorem ZIP");
        state
            .delete_document(DOC)
            .await
            .expect("remove restored document");
        let boot = state
            .restore_backup(&new_backup.relative_path)
            .await
            .expect("restore Theorem ZIP");
        assert_eq!(boot.settings.theme, "dark");
        assert_eq!(boot.documents[0].id, DOC);
        assert_eq!(
            state
                .read_attachment_base64(ATTACHMENT)
                .await
                .expect("restored Theorem original")
                .base64_data,
            STANDARD.encode(&png)
        );
        state.db.read().await.close().await;
        writer.close().await.expect("close legacy SQLite");
    }
}

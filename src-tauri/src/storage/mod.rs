use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use chrono::Utc;
use sha2::{Digest, Sha256};
use uuid::Uuid;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

use crate::domain::{AppSettings, BackupInfo};
use crate::error::{AppError, AppResult};
use crate::security::{normalize_relative_path, resolve_under_root};

const SETTINGS_REL_PATH: &str = "settings.json";
const BACKUPS_DIR_REL: &str = "backups";
const DATABASE_REL_PATH: &str = "database/formuladesk.db";

#[derive(Debug, Clone)]
pub struct AppStorage {
    root: PathBuf,
}

impl AppStorage {
    /// Initializes the managed FormulaDesk directory hierarchy (docs/architecture.md Section 6).
    pub fn initialize(root: PathBuf) -> AppResult<Self> {
        let subdirs = [
            "database",
            "attachments/originals",
            "attachments/solutions",
            "attachments/diagrams",
            "attachments/plots",
            "sources",
            "thumbnails",
            "templates",
            "workspaces",
            "backups",
            "logs",
            "tmp",
        ];
        fs::create_dir_all(&root)?;
        for sub in subdirs {
            fs::create_dir_all(root.join(sub))?;
        }
        let storage = Self { root };
        // Ensure settings.json exists with defaults
        if !storage.root.join(SETTINGS_REL_PATH).exists() {
            storage.save_settings(&AppSettings::default())?;
        }
        Ok(storage)
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn database_path(&self) -> PathBuf {
        self.root.join(DATABASE_REL_PATH)
    }

    pub fn logs_dir(&self) -> PathBuf {
        self.root.join("logs")
    }

    pub fn resolve(&self, relative_path: &str) -> AppResult<PathBuf> {
        resolve_under_root(&self.root, relative_path)
    }

    /// Writes bytes atomically via a same-directory temporary file + `sync_all` + rename.
    pub fn write_atomic(&self, relative_path: &str, bytes: &[u8]) -> AppResult<PathBuf> {
        let target = self.resolve(relative_path)?;
        write_bytes_atomic_absolute(&target, bytes)?;
        Ok(target)
    }

    pub fn read_bytes(&self, relative_path: &str) -> AppResult<Vec<u8>> {
        let target = self.resolve(relative_path)?;
        if !target.exists() {
            return Err(AppError::NotFound(format!(
                "ファイルが見つかりません: {relative_path}"
            )));
        }
        Ok(fs::read(target)?)
    }

    pub fn delete_if_exists(&self, relative_path: &str) -> AppResult<()> {
        let target = self.resolve(relative_path)?;
        if target.exists() {
            fs::remove_file(target)?;
        }
        Ok(())
    }

    pub fn load_settings(&self) -> AppResult<AppSettings> {
        let path = self.resolve(SETTINGS_REL_PATH)?;
        if !path.exists() {
            return Ok(AppSettings::default());
        }
        let raw = fs::read_to_string(path)?;
        serde_json::from_str::<AppSettings>(&raw)
            .map_err(|e| AppError::Storage(format!("settings.json の解析に失敗しました: {e}")))
    }

    pub fn save_settings(&self, settings: &AppSettings) -> AppResult<()> {
        let json = serde_json::to_string_pretty(settings)
            .map_err(|e| AppError::Storage(format!("設定のシリアライズに失敗しました: {e}")))?;
        self.write_atomic(SETTINGS_REL_PATH, json.as_bytes())?;
        Ok(())
    }

    pub fn dir_size(&self, relative_dir: &str) -> u64 {
        let Ok(path) = self.resolve(relative_dir) else {
            return 0;
        };
        compute_dir_size(&path)
    }

    pub fn file_size(&self, relative_file: &str) -> u64 {
        let Ok(path) = self.resolve(relative_file) else {
            return 0;
        };
        fs::metadata(path).map(|m| m.len()).unwrap_or(0)
    }

    /// Removes leftover temporary files and stale workspace folders.
    pub fn cleanup_temp(&self) -> AppResult<u64> {
        let mut freed = 0u64;
        for dir_rel in ["tmp", "workspaces"] {
            let dir = self.resolve(dir_rel)?;
            if dir.exists() {
                for entry in fs::read_dir(&dir)? {
                    let entry = entry?;
                    let path = entry.path();
                    freed += compute_dir_size(&path);
                    if path.is_dir() {
                        let _ = fs::remove_dir_all(&path);
                    } else {
                        let _ = fs::remove_file(&path);
                    }
                }
            }
        }
        Ok(freed)
    }

    /// Creates a ZIP archive backup from an already-consistent SQLite snapshot + attachments + sources + settings.json.
    /// Never includes OS keychain credentials.
    pub fn create_backup_archive(&self, db_snapshot_path: &Path) -> AppResult<BackupInfo> {
        let stamp = Utc::now().format("%Y%m%d-%H%M%S");
        let base_filename = format!("formuladesk-backup-{stamp}.zip");
        let base_rel = format!("{BACKUPS_DIR_REL}/{base_filename}");
        let (filename, rel_path) = if self.resolve(&base_rel)?.exists() {
            let short_id = &Uuid::new_v4().to_string()[..8];
            let unique_name = format!("formuladesk-backup-{stamp}-{short_id}.zip");
            let unique_rel = format!("{BACKUPS_DIR_REL}/{unique_name}");
            (unique_name, unique_rel)
        } else {
            (base_filename, base_rel)
        };
        let final_path = self.resolve(&rel_path)?;
        let tmp_path = final_path.with_extension(format!("zip.{}.tmp", Uuid::new_v4()));
        let canonical_root = fs::canonicalize(&self.root)?;

        let write_result = (|| -> AppResult<()> {
            let file = File::create(&tmp_path)?;
            let mut zip = ZipWriter::new(file);
            let options =
                SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);

            // 1. Consistent database snapshot
            zip.start_file(DATABASE_REL_PATH, options)
                .map_err(|e| AppError::Backup(e.to_string()))?;
            let db_bytes = fs::read(db_snapshot_path)?;
            zip.write_all(&db_bytes)?;

            // 2. settings.json
            let settings_path = self.resolve(SETTINGS_REL_PATH)?;
            if settings_path.exists() {
                zip.start_file(SETTINGS_REL_PATH, options)
                    .map_err(|e| AppError::Backup(e.to_string()))?;
                zip.write_all(&fs::read(settings_path)?)?;
            }

            // 3. attachments/, thumbnails/, sources/, templates/
            for folder in ["attachments", "thumbnails", "sources", "templates"] {
                let abs = self.resolve(folder)?;
                if abs.exists() {
                    add_dir_to_zip(&canonical_root, &abs, &mut zip, options)?;
                }
            }

            zip.finish().map_err(|e| AppError::Backup(e.to_string()))?;
            Ok(())
        })();

        if let Err(err) = write_result {
            let _ = fs::remove_file(&tmp_path);
            return Err(err);
        }

        replace_file_atomic(&tmp_path, &final_path)?;
        let byte_size = fs::metadata(&final_path).map(|m| m.len()).unwrap_or(0);
        Ok(BackupInfo {
            filename,
            relative_path: rel_path,
            byte_size,
            created_at: Utc::now().to_rfc3339(),
        })
    }

    pub fn list_backups(&self) -> AppResult<Vec<BackupInfo>> {
        let dir = self.resolve(BACKUPS_DIR_REL)?;
        let mut items = Vec::new();
        if !dir.exists() {
            return Ok(items);
        }
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) != Some("zip") {
                continue;
            }
            let filename = entry.file_name().to_string_lossy().into_owned();
            let meta = entry.metadata()?;
            let created_at = meta
                .modified()
                .ok()
                .map(|st| chrono::DateTime::<Utc>::from(st).to_rfc3339())
                .unwrap_or_else(|| Utc::now().to_rfc3339());
            items.push(BackupInfo {
                relative_path: format!("{BACKUPS_DIR_REL}/{filename}"),
                filename,
                byte_size: meta.len(),
                created_at,
            });
        }
        items.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        Ok(items)
    }

    /// Validates and extracts a backup archive safely into managed storage.
    /// Rejects any ZIP-slip traversal entries or unexpected paths outside allowed roots.
    pub fn restore_backup_archive(&self, backup_rel_path: &str) -> AppResult<()> {
        let archive_path = self.resolve(backup_rel_path)?;
        if !archive_path.exists() {
            return Err(AppError::NotFound(format!(
                "バックアップファイルが見つかりません: {backup_rel_path}"
            )));
        }

        let file = File::open(&archive_path)?;
        let mut archive = ZipArchive::new(file).map_err(|e| AppError::Backup(e.to_string()))?;

        // First pass: validate EVERY entry before modifying any disk state
        let mut has_database = false;
        let mut entries_to_restore: Vec<(usize, String)> = Vec::new();
        for i in 0..archive.len() {
            let entry = archive
                .by_index(i)
                .map_err(|e| AppError::Backup(e.to_string()))?;
            if entry.is_dir() {
                continue;
            }
            let norm = normalize_relative_path(entry.name())?;
            let allowed = norm == DATABASE_REL_PATH
                || norm == SETTINGS_REL_PATH
                || norm.starts_with("attachments/")
                || norm.starts_with("thumbnails/")
                || norm.starts_with("sources/")
                || norm.starts_with("templates/");
            if !allowed {
                return Err(AppError::Security(format!(
                    "バックアップ内に許可されていないパスが含まれています: {norm}"
                )));
            }
            if norm == DATABASE_REL_PATH {
                has_database = true;
            }
            entries_to_restore.push((i, norm));
        }

        if !has_database {
            return Err(AppError::Backup(
                "バックアップに formuladesk.db が含まれていません".into(),
            ));
        }

        // Remove stale WAL/SHM files before replacing database
        for suffix in ["-wal", "-shm"] {
            let side = PathBuf::from(format!("{}{}", self.database_path().display(), suffix));
            if side.exists() {
                let _ = fs::remove_file(side);
            }
        }

        // Second pass: atomic restore of each entry
        for (idx, rel_path) in entries_to_restore {
            let mut entry = archive
                .by_index(idx)
                .map_err(|e| AppError::Backup(e.to_string()))?;
            let mut buf = Vec::new();
            entry.read_to_end(&mut buf)?;
            self.write_atomic(&rel_path, &buf)?;
        }

        Ok(())
    }
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn write_bytes_atomic_absolute(target: &Path, bytes: &[u8]) -> AppResult<()> {
    let parent = target
        .parent()
        .ok_or_else(|| AppError::Storage("保存先親ディレクトリが特定できません".into()))?;
    fs::create_dir_all(parent)?;
    let tmp_name = format!(
        ".{}.{}.tmp",
        target
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "file".to_string()),
        Uuid::new_v4()
    );
    let tmp_path = parent.join(tmp_name);

    let write_res = (|| -> std::io::Result<()> {
        let mut f = File::create(&tmp_path)?;
        f.write_all(bytes)?;
        f.sync_all()?;
        Ok(())
    })();

    if let Err(err) = write_res {
        let _ = fs::remove_file(&tmp_path);
        return Err(err.into());
    }

    replace_file_atomic(&tmp_path, target)
}

fn replace_file_atomic(tmp_path: &Path, target: &Path) -> AppResult<()> {
    // On Windows, fs::rename may fail if the destination already exists; handle cleanly.
    if let Err(first_err) = fs::rename(tmp_path, target) {
        if target.exists() {
            let backup_old = target.with_extension(format!("old.{}", Uuid::new_v4()));
            fs::rename(target, &backup_old)?;
            match fs::rename(tmp_path, target) {
                Ok(()) => {
                    let _ = fs::remove_file(&backup_old);
                }
                Err(second_err) => {
                    let _ = fs::rename(&backup_old, target);
                    let _ = fs::remove_file(tmp_path);
                    return Err(second_err.into());
                }
            }
        } else {
            let _ = fs::remove_file(tmp_path);
            return Err(first_err.into());
        }
    }
    Ok(())
}

fn compute_dir_size(path: &Path) -> u64 {
    let Ok(meta) = fs::metadata(path) else {
        return 0;
    };
    if meta.is_file() {
        return meta.len();
    }
    let mut total = 0u64;
    if let Ok(entries) = fs::read_dir(path) {
        for entry in entries.flatten() {
            total += compute_dir_size(&entry.path());
        }
    }
    total
}

fn add_dir_to_zip(
    root: &Path,
    dir: &Path,
    zip: &mut ZipWriter<File>,
    options: SimpleFileOptions,
) -> AppResult<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            add_dir_to_zip(root, &path, zip, options)?;
        } else if path.is_file() {
            let rel = path
                .strip_prefix(root)
                .map_err(|e| AppError::Backup(e.to_string()))?;
            let rel_str = rel.to_string_lossy().replace('\\', "/");
            zip.start_file(rel_str, options)
                .map_err(|e| AppError::Backup(e.to_string()))?;
            zip.write_all(&fs::read(&path)?)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_atomic_write_overwrite_and_read() {
        let dir = TempDir::new().expect("tempdir");
        let storage = AppStorage::initialize(dir.path().to_path_buf()).expect("init");
        storage
            .write_atomic("attachments/originals/sample.txt", b"v1")
            .expect("write v1");
        assert_eq!(
            storage
                .read_bytes("attachments/originals/sample.txt")
                .expect("read v1"),
            b"v1"
        );
        storage
            .write_atomic("attachments/originals/sample.txt", b"v2-updated")
            .expect("write v2");
        assert_eq!(
            storage
                .read_bytes("attachments/originals/sample.txt")
                .expect("read v2"),
            b"v2-updated"
        );
    }

    #[test]
    fn test_restore_rejects_zip_slip_archive() {
        let dir = TempDir::new().expect("tempdir");
        let storage = AppStorage::initialize(dir.path().to_path_buf()).expect("init");
        let bad_zip = storage.resolve("backups/malicious.zip").expect("resolve");
        {
            let f = File::create(&bad_zip).expect("create");
            let mut zip = ZipWriter::new(f);
            let opts = SimpleFileOptions::default();
            zip.start_file("../escape.txt", opts).expect("start");
            zip.write_all(b"evil").expect("write");
            zip.finish().expect("finish");
        }
        let err = storage
            .restore_backup_archive("backups/malicious.zip")
            .expect_err("must reject traversal entry");
        assert!(matches!(err, AppError::Security(_)));
    }
}

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Component, Path, PathBuf};

use chrono::Utc;
use serde_json::json;

use crate::error::{AppError, AppResult};

/// Validates and resolves a relative path strictly inside `root`.
///
/// Rejects:
/// - Empty paths, absolute paths, Windows drive/UNC prefixes, `..` traversal components
/// - Symlinks pointing outside `root` (checked via canonicalization of the target or nearest existing ancestor)
pub fn resolve_under_root(root: &Path, relative_path: &str) -> AppResult<PathBuf> {
    let trimmed = relative_path.trim();
    if trimmed.is_empty() {
        return Err(AppError::Security("空のパスは許可されていません".into()));
    }
    if trimmed.contains('\0') {
        return Err(AppError::Security(
            "NULL 文字を含むパスは許可されていません".into(),
        ));
    }

    let normalized = trimmed.replace('\\', "/");
    let rel = Path::new(&normalized);
    if rel.is_absolute() {
        return Err(AppError::Security(
            "絶対パスによる管理領域アクセスは禁止されています".into(),
        ));
    }

    let mut clean_rel = PathBuf::new();
    for comp in rel.components() {
        match comp {
            Component::Normal(part) => clean_rel.push(part),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(AppError::Security(format!(
                    "パストラバーサルまたは不正なパス要素を検出しました: {relative_path}"
                )));
            }
        }
    }

    if clean_rel.as_os_str().is_empty() {
        return Err(AppError::Security("有効な相対パスではありません".into()));
    }

    let canonical_root = fs::canonicalize(root).map_err(|e| {
        AppError::Security(format!(
            "管理領域ルートの検証に失敗しました: {}",
            redact_secrets(&e.to_string())
        ))
    })?;

    let candidate = canonical_root.join(&clean_rel);

    // Walk every existing prefix to detect symlinks escaping `canonical_root`
    let mut cursor = canonical_root.clone();
    for comp in clean_rel.components() {
        if let Component::Normal(part) = comp {
            cursor.push(part);
            if let Ok(meta) = fs::symlink_metadata(&cursor)
                && meta.file_type().is_symlink()
            {
                let target = fs::canonicalize(&cursor).map_err(|_| {
                    AppError::Security("シンボリックリンクの解決に失敗しました".into())
                })?;
                if !target.starts_with(&canonical_root) {
                    return Err(AppError::Security(
                        "管理領域外を指すシンボリックリンクは拒否されました".into(),
                    ));
                }
            }
        }
    }

    if candidate.exists() {
        let canonical_candidate = fs::canonicalize(&candidate)?;
        if !canonical_candidate.starts_with(&canonical_root) {
            return Err(AppError::Security(
                "解決後パスが管理領域外に出ています".into(),
            ));
        }
        Ok(canonical_candidate)
    } else {
        Ok(candidate)
    }
}

/// Normalizes a relative path string to forward slashes and verifies safety.
pub fn normalize_relative_path(relative_path: &str) -> AppResult<String> {
    let normalized = relative_path.trim().replace('\\', "/");
    let rel = Path::new(&normalized);
    if normalized.is_empty() || rel.is_absolute() || normalized.contains('\0') {
        return Err(AppError::Security("不正な相対パス形式です".into()));
    }
    let mut parts: Vec<String> = Vec::new();
    for comp in rel.components() {
        match comp {
            Component::Normal(s) => parts.push(s.to_string_lossy().into_owned()),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(AppError::Security(
                    "相対パスに上位参照は使用できません".into(),
                ));
            }
        }
    }
    if parts.is_empty() {
        return Err(AppError::Security("空の相対パスです".into()));
    }
    Ok(parts.join("/"))
}

/// Redacts API keys, OAuth tokens, passwords, and bearer credentials from log/error strings.
pub fn redact_secrets(input: &str) -> String {
    let mut out = input.to_string();
    let prefixes = [
        "sk-",
        "AIza",
        "gho_",
        "ghp_",
        "ya29.",
        "Bearer ",
        "bearer ",
        "refresh_token=",
        "access_token=",
        "client_secret=",
        "password=",
    ];

    for prefix in prefixes {
        let mut search_from = 0usize;
        while let Some(rel_idx) = out[search_from..].find(prefix) {
            let start = search_from + rel_idx;
            let value_start = start + prefix.len();
            let value_end = out[value_start..]
                .find(|c: char| {
                    c.is_whitespace() || matches!(c, '"' | '\'' | '&' | ',' | ';' | ')' | '}')
                })
                .map(|off| value_start + off)
                .unwrap_or(out.len());
            out.replace_range(value_start..value_end, "[REDACTED]");
            search_from = value_start + "[REDACTED]".len();
            if search_from >= out.len() {
                break;
            }
        }
    }

    // Never log full problem statements or long raw payloads in diagnostic entries
    if out.chars().count() > 512 {
        let truncated: String = out.chars().take(512).collect();
        format!("{truncated}...[TRUNCATED]")
    } else {
        out
    }
}

/// Diagnostic JSONL logger that guarantees secrets and full texts are never persisted to disk.
#[derive(Debug, Clone)]
pub struct DiagnosticLogger {
    log_file: PathBuf,
}

impl DiagnosticLogger {
    pub fn new(logs_dir: &Path) -> Self {
        Self {
            log_file: logs_dir.join("formuladesk.log.jsonl"),
        }
    }

    pub fn log_info(&self, event: &str, message: &str) {
        self.write_entry("INFO", event, message);
    }

    pub fn log_warn(&self, event: &str, message: &str) {
        self.write_entry("WARN", event, message);
    }

    pub fn log_error(&self, event: &str, message: &str) {
        self.write_entry("ERROR", event, message);
    }

    fn write_entry(&self, level: &str, event: &str, message: &str) {
        let entry = json!({
            "timestamp": Utc::now().to_rfc3339(),
            "level": level,
            "event": event,
            "message": redact_secrets(message),
        });
        if let Ok(mut file) = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.log_file)
        {
            let _ = writeln!(file, "{entry}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_resolve_under_root_blocks_traversal() {
        let dir = TempDir::new().expect("tempdir");
        assert!(resolve_under_root(dir.path(), "../secret.txt").is_err());
        assert!(resolve_under_root(dir.path(), "a/../../secret.txt").is_err());
        assert!(resolve_under_root(dir.path(), "/etc/passwd").is_err());
        assert!(resolve_under_root(dir.path(), r"C:\Windows\win.ini").is_err());
        assert!(resolve_under_root(dir.path(), "").is_err());

        let ok = resolve_under_root(dir.path(), "attachments/originals/test.pdf")
            .expect("valid relative path");
        let canon_root = fs::canonicalize(dir.path()).expect("canon");
        assert!(ok.starts_with(&canon_root));
    }

    #[test]
    fn test_redact_secrets_masks_tokens_and_keys() {
        let raw = "auth failed Bearer eyJhbGciOi.secret token sk-proj-abc12345 client_secret=topsecret123 password=hunter2";
        let redacted = redact_secrets(raw);
        assert!(!redacted.contains("eyJhbGciOi"));
        assert!(!redacted.contains("abc12345"));
        assert!(!redacted.contains("topsecret123"));
        assert!(!redacted.contains("hunter2"));
        assert!(redacted.contains("[REDACTED]"));
    }

    #[test]
    fn test_diagnostic_logger_never_writes_raw_secrets() {
        let dir = TempDir::new().expect("tempdir");
        let logger = DiagnosticLogger::new(dir.path());
        logger.log_error("auth_error", "failed with key sk-secret-value-999");
        let content =
            fs::read_to_string(dir.path().join("formuladesk.log.jsonl")).expect("read log");
        assert!(!content.contains("secret-value-999"));
        assert!(content.contains("[REDACTED]"));
    }
}

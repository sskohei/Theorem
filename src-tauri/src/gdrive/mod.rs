pub mod client;
pub mod oauth;

use serde::{Deserialize, Serialize};
use url::Url;

use crate::error::{AppError, AppResult};

pub const GOOGLE_AUTH_ENDPOINT: &str = "https://accounts.google.com/o/oauth2/v2/auth";
pub const GOOGLE_TOKEN_ENDPOINT: &str = "https://oauth2.googleapis.com/token";
pub const GOOGLE_DRIVE_API_BASE: &str = "https://www.googleapis.com/drive/v3";
pub const GOOGLE_DRIVE_UPLOAD_BASE: &str = "https://www.googleapis.com/upload/drive/v3";

pub const KEYRING_OAUTH_CLIENT_SECRET_REF: &str = "gdrive:oauth_client_secret";
pub const MAX_DRIVE_UPLOAD_ATTEMPTS: i64 = 6;

/// Configurable endpoints for Google OAuth + Drive API v3.
/// Production uses official Google endpoints; automated tests override this with a local Mock HTTP server.
#[derive(Debug, Clone)]
pub struct DriveEndpoints {
    pub auth_endpoint: String,
    pub token_endpoint: String,
    pub api_base: String,
    pub upload_base: String,
}

impl Default for DriveEndpoints {
    fn default() -> Self {
        Self {
            auth_endpoint: GOOGLE_AUTH_ENDPOINT.to_string(),
            token_endpoint: GOOGLE_TOKEN_ENDPOINT.to_string(),
            api_base: GOOGLE_DRIVE_API_BASE.to_string(),
            upload_base: GOOGLE_DRIVE_UPLOAD_BASE.to_string(),
        }
    }
}

/// Token payload stored exclusively inside OS keychain (`crate::security::keyring`), NEVER in SQLite.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OAuthTokenBundle {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_at_epoch: Option<i64>,
    pub token_type: String,
}

impl OAuthTokenBundle {
    pub fn is_expiring_soon(&self, now_epoch: i64) -> bool {
        match self.expires_at_epoch {
            Some(exp) => now_epoch + 60 >= exp,
            None => false,
        }
    }
}

pub fn connection_token_keyring_ref(connection_id: &str) -> String {
    format!("gdrive:token:{connection_id}")
}

/// Computes exponential backoff duration in seconds with deterministic small jitter:
/// attempt 1 -> ~5s, 2 -> ~15s, 3 -> ~45s, 4 -> ~120s, 5+ -> ~300s.
/// Honors server `Retry-After` when provided (clamped to 1..=600s).
pub fn compute_backoff_seconds(attempt: i64, retry_after_secs: Option<u64>) -> u64 {
    if let Some(ra) = retry_after_secs {
        return ra.clamp(1, 600);
    }
    let base: u64 = match attempt {
        ..=1 => 5,
        2 => 15,
        3 => 45,
        4 => 120,
        _ => 300,
    };
    let mut rand_byte = [0u8; 1];
    let jitter_pct = if getrandom::fill(&mut rand_byte).is_ok() {
        u64::from(rand_byte[0] % 21) // 0..20%
    } else {
        10
    };
    base + (base * jitter_pct) / 100
}

/// Sanitizes a filename or folder name for Google Drive display (keeps Unicode Japanese names intact).
pub fn sanitize_drive_name(raw: &str, fallback: &str) -> String {
    let cleaned: String = raw
        .chars()
        .map(|c| {
            if matches!(c, '/' | '\\' | '\0' | '\r' | '\n' | '\t') {
                '_'
            } else {
                c
            }
        })
        .collect();
    let trimmed = cleaned.trim();
    if trimmed.is_empty() {
        fallback.to_string()
    } else {
        trimmed.chars().take(120).collect()
    }
}

/// Opens the OAuth authorization URL strictly in the OS default system browser (NEVER WebView).
/// Validates the URL scheme first and avoids shell command interpolation to prevent injection.
pub fn open_system_browser(raw_url: &str) -> AppResult<()> {
    let parsed =
        Url::parse(raw_url).map_err(|e| AppError::Validation(format!("不正な URL です: {e}")))?;
    let scheme = parsed.scheme();
    let allowed = scheme == "https"
        || (scheme == "http" && matches!(parsed.host_str(), Some("127.0.0.1") | Some("localhost")));
    if !allowed {
        return Err(AppError::Security(
            "HTTPS またはローカル検証 URL 以外をシステムブラウザで開くことは禁止されています"
                .into(),
        ));
    }

    #[cfg(windows)]
    {
        use std::ffi::OsStr;
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::UI::Shell::ShellExecuteW;
        use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

        let op: Vec<u16> = OsStr::new("open")
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        let target: Vec<u16> = OsStr::new(parsed.as_str())
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        // SAFETY: Both `op` and `target` are valid NUL-terminated UTF-16 strings; null parent HWND opens in default browser.
        let res = unsafe {
            ShellExecuteW(
                std::ptr::null_mut(),
                op.as_ptr(),
                target.as_ptr(),
                std::ptr::null(),
                std::ptr::null(),
                SW_SHOWNORMAL,
            )
        };
        if (res as isize) <= 32 {
            return Err(AppError::Drive(
                "システムブラウザの起動に失敗しました（ShellExecuteW）".into(),
            ));
        }
        Ok(())
    }

    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(parsed.as_str())
            .spawn()
            .map_err(|e| AppError::Drive(format!("システムブラウザの起動に失敗しました: {e}")))?;
        Ok(())
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        std::process::Command::new("xdg-open")
            .arg(parsed.as_str())
            .spawn()
            .map_err(|e| AppError::Drive(format!("システムブラウザの起動に失敗しました: {e}")))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_backoff_seconds_progression_and_retry_after() {
        let b1 = compute_backoff_seconds(1, None);
        let b2 = compute_backoff_seconds(2, None);
        let b3 = compute_backoff_seconds(3, None);
        let b4 = compute_backoff_seconds(4, None);
        let b5 = compute_backoff_seconds(5, None);
        assert!((5..=6).contains(&b1));
        assert!((15..=18).contains(&b2));
        assert!((45..=54).contains(&b3));
        assert!((120..=144).contains(&b4));
        assert!((300..=360).contains(&b5));
        assert_eq!(compute_backoff_seconds(1, Some(90)), 90);
        assert_eq!(compute_backoff_seconds(1, Some(9999)), 600);
    }

    #[test]
    fn test_open_system_browser_rejects_dangerous_schemes() {
        assert!(open_system_browser("javascript:alert(1)").is_err());
        assert!(open_system_browser("file:///C:/Windows/System32/cmd.exe").is_err());
        assert!(open_system_browser("http://evil.example.com/phish").is_err());
    }
}

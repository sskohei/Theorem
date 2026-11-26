use std::time::Duration;

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use url::Url;

use crate::security::redact_secrets;

/// Strict minimum OAuth 2.0 scope (Non-sensitive scope).
/// NEVER add `drive` or `drive.readonly` (ADR-0006).
pub const DRIVE_FILE_SCOPE: &str = "https://www.googleapis.com/auth/drive.file";

#[derive(Debug, Error)]
pub enum OAuthError {
    #[error("許可されていない OAuth スコープが要求されました: {0}（drive.file のみ許可）")]
    DisallowedScope(String),
    #[error("暗号学的乱数生成に失敗しました: {0}")]
    RandomFailed(String),
    #[error("ローカルコールバックサーバーエラー: {0}")]
    Io(#[from] std::io::Error),
    #[error("OAuth 認証がタイムアウトしました（ブラウザでの承認を完了してください）")]
    Timeout,
    #[error("OAuth 認証がユーザーによりキャンセルまたは拒否されました: {0}")]
    AccessDenied(String),
    #[error("OAuth state が一致しません（CSRF 保護により拒否されました）")]
    StateMismatch,
    #[error("OAuth コールバックに認可コードが含まれていません")]
    MissingCode,
    #[error("OAuth 通信エラー: {0}")]
    Http(String),
    #[error("OAuth トークンが失効または無効化されました（再認証が必要です）: {0}")]
    ReauthRequired(String),
    #[error("OAuth トークンエンドポイントエラー ({status}): {body}")]
    TokenEndpoint { status: u16, body: String },
    #[error("URL 解析エラー: {0}")]
    Url(#[from] url::ParseError),
}

#[derive(Debug, Clone)]
pub struct PkcePair {
    pub code_verifier: String,
    pub code_challenge: String,
    pub state: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenResponse {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_in: Option<u64>,
    #[serde(default = "default_bearer")]
    pub token_type: String,
    pub scope: Option<String>,
}

fn default_bearer() -> String {
    "Bearer".to_string()
}

/// Generates RFC 7636 PKCE verifier/challenge (`S256`) and CSRF `state` using OS CSPRNG.
pub fn generate_pkce() -> Result<PkcePair, OAuthError> {
    let mut verifier_raw = [0u8; 32];
    getrandom::fill(&mut verifier_raw).map_err(|e| OAuthError::RandomFailed(e.to_string()))?;
    let mut state_raw = [0u8; 16];
    getrandom::fill(&mut state_raw).map_err(|e| OAuthError::RandomFailed(e.to_string()))?;

    let code_verifier = URL_SAFE_NO_PAD.encode(verifier_raw);
    let digest = Sha256::digest(code_verifier.as_bytes());
    let code_challenge = URL_SAFE_NO_PAD.encode(digest);
    let state = URL_SAFE_NO_PAD.encode(state_raw);

    Ok(PkcePair {
        code_verifier,
        code_challenge,
        state,
    })
}

/// Enforces that ONLY `https://www.googleapis.com/auth/drive.file` is requested.
/// Explicitly rejects `drive`, `drive.readonly`, or any extra scope.
pub fn validate_scopes(scopes: &[&str]) -> Result<(), OAuthError> {
    if scopes.is_empty() {
        return Err(OAuthError::DisallowedScope("<empty>".to_string()));
    }
    for s in scopes {
        if *s != DRIVE_FILE_SCOPE {
            return Err(OAuthError::DisallowedScope((*s).to_string()));
        }
    }
    Ok(())
}

pub fn build_authorize_url(
    auth_endpoint: &str,
    client_id: &str,
    redirect_uri: &str,
    pkce: &PkcePair,
    scopes: &[&str],
) -> Result<String, OAuthError> {
    validate_scopes(scopes)?;
    let mut url = Url::parse(auth_endpoint)?;
    url.query_pairs_mut()
        .append_pair("client_id", client_id)
        .append_pair("redirect_uri", redirect_uri)
        .append_pair("response_type", "code")
        .append_pair("scope", &scopes.join(" "))
        .append_pair("code_challenge", &pkce.code_challenge)
        .append_pair("code_challenge_method", "S256")
        .append_pair("state", &pkce.state)
        .append_pair("access_type", "offline")
        .append_pair("prompt", "consent");
    Ok(url.to_string())
}

pub struct LoopbackServer {
    listener: TcpListener,
    pub redirect_uri: String,
}

impl LoopbackServer {
    /// Binds an ephemeral local loopback listener strictly on `127.0.0.1:0`.
    pub async fn bind() -> Result<Self, OAuthError> {
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let port = listener.local_addr()?.port();
        Ok(Self {
            listener,
            redirect_uri: format!("http://127.0.0.1:{port}/callback"),
        })
    }

    /// Waits for the OAuth redirect callback up to `timeout`, ignoring auxiliary requests
    /// such as `/favicon.ico`, verifying CSRF `state`, and immediately shutting down.
    pub async fn wait_for_code(
        self,
        expected_state: &str,
        timeout: Duration,
    ) -> Result<String, OAuthError> {
        let deadline = tokio::time::Instant::now() + timeout;
        loop {
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            if remaining.is_zero() {
                return Err(OAuthError::Timeout);
            }
            let (mut socket, _) = tokio::time::timeout(remaining, self.listener.accept())
                .await
                .map_err(|_| OAuthError::Timeout)??;

            let mut buf = [0u8; 8192];
            let n = tokio::time::timeout(Duration::from_secs(5), socket.read(&mut buf))
                .await
                .map_err(|_| OAuthError::Timeout)??;
            if n == 0 {
                continue;
            }
            let req_str = String::from_utf8_lossy(&buf[..n]);
            let first_line = req_str.lines().next().unwrap_or_default();
            let path_and_query = first_line.split_whitespace().nth(1).unwrap_or("/");

            // Ignore browser favicon or unrelated probes while waiting for `/callback`
            if path_and_query.starts_with("/favicon") {
                let not_found =
                    "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
                let _ = socket.write_all(not_found.as_bytes()).await;
                let _ = socket.shutdown().await;
                continue;
            }

            let parsed = Url::parse(&format!("http://127.0.0.1{path_and_query}"))?;
            let mut code: Option<String> = None;
            let mut state: Option<String> = None;
            let mut oauth_error: Option<String> = None;
            for (k, v) in parsed.query_pairs() {
                match k.as_ref() {
                    "code" => code = Some(v.into_owned()),
                    "state" => state = Some(v.into_owned()),
                    "error" => oauth_error = Some(v.into_owned()),
                    _ => {}
                }
            }

            let (http_status, html) = if oauth_error.is_some() {
                (
                    "400 Bad Request",
                    "<!doctype html><html lang=\"ja\"><head><meta charset=\"utf-8\"><title>Theorem Google Drive</title></head><body style=\"font-family:sans-serif;padding:2rem\"><h2>認証がキャンセルされました</h2><p>このタブを閉じて Theorem に戻ってください。</p></body></html>",
                )
            } else if state.as_deref() != Some(expected_state) {
                (
                    "400 Bad Request",
                    "<!doctype html><html lang=\"ja\"><head><meta charset=\"utf-8\"><title>Theorem Google Drive</title></head><body style=\"font-family:sans-serif;padding:2rem\"><h2>セキュリティ検証エラー</h2><p>state が一致しません。このタブを閉じて Theorem から再度お試しください。</p></body></html>",
                )
            } else {
                (
                    "200 OK",
                    "<!doctype html><html lang=\"ja\"><head><meta charset=\"utf-8\"><title>Theorem Google Drive</title></head><body style=\"font-family:sans-serif;padding:2rem\"><h2>Theorem の Google Drive 認証が完了しました</h2><p>このブラウザタブを閉じてアプリに戻ってください。</p></body></html>",
                )
            };

            let resp = format!(
                "HTTP/1.1 {http_status}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                html.len(),
                html
            );
            let _ = socket.write_all(resp.as_bytes()).await;
            let _ = socket.shutdown().await;

            if let Some(err) = oauth_error {
                return Err(OAuthError::AccessDenied(err));
            }
            if state.as_deref() != Some(expected_state) {
                return Err(OAuthError::StateMismatch);
            }
            return code.ok_or(OAuthError::MissingCode);
        }
    }
}

pub async fn exchange_code_for_token(
    http: &reqwest::Client,
    token_endpoint: &str,
    client_id: &str,
    client_secret: Option<&str>,
    code: &str,
    code_verifier: &str,
    redirect_uri: &str,
) -> Result<TokenResponse, OAuthError> {
    let mut form: Vec<(&str, &str)> = vec![
        ("code", code),
        ("client_id", client_id),
        ("code_verifier", code_verifier),
        ("redirect_uri", redirect_uri),
        ("grant_type", "authorization_code"),
    ];
    if let Some(secret) = client_secret.filter(|s| !s.trim().is_empty()) {
        form.push(("client_secret", secret.trim()));
    }
    let resp = http
        .post(token_endpoint)
        .form(&form)
        .send()
        .await
        .map_err(|e| OAuthError::Http(redact_secrets(&e.to_string())))?;
    let status = resp.status().as_u16();
    if !resp.status().is_success() {
        let body = redact_secrets(&resp.text().await.unwrap_or_default());
        return Err(OAuthError::TokenEndpoint { status, body });
    }
    resp.json::<TokenResponse>()
        .await
        .map_err(|e| OAuthError::Http(redact_secrets(&e.to_string())))
}

pub async fn refresh_access_token(
    http: &reqwest::Client,
    token_endpoint: &str,
    client_id: &str,
    client_secret: Option<&str>,
    refresh_token: &str,
) -> Result<TokenResponse, OAuthError> {
    let mut form: Vec<(&str, &str)> = vec![
        ("client_id", client_id),
        ("refresh_token", refresh_token),
        ("grant_type", "refresh_token"),
    ];
    if let Some(secret) = client_secret.filter(|s| !s.trim().is_empty()) {
        form.push(("client_secret", secret.trim()));
    }
    let resp = http
        .post(token_endpoint)
        .form(&form)
        .send()
        .await
        .map_err(|e| OAuthError::Http(redact_secrets(&e.to_string())))?;
    let status = resp.status().as_u16();
    if !resp.status().is_success() {
        let body = redact_secrets(&resp.text().await.unwrap_or_default());
        if status == 400 || status == 401 || body.contains("invalid_grant") {
            return Err(OAuthError::ReauthRequired(body));
        }
        return Err(OAuthError::TokenEndpoint { status, body });
    }
    resp.json::<TokenResponse>()
        .await
        .map_err(|e| OAuthError::Http(redact_secrets(&e.to_string())))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_scopes_enforces_drive_file_only() {
        assert!(validate_scopes(&[DRIVE_FILE_SCOPE]).is_ok());
        assert!(validate_scopes(&[]).is_err());
        assert!(validate_scopes(&["https://www.googleapis.com/auth/drive"]).is_err());
        assert!(validate_scopes(&["https://www.googleapis.com/auth/drive.readonly"]).is_err());
        assert!(
            validate_scopes(&[
                DRIVE_FILE_SCOPE,
                "https://www.googleapis.com/auth/drive.readonly"
            ])
            .is_err()
        );
    }

    #[test]
    fn test_pkce_s256_challenge_matches_sha256_of_verifier() {
        let pkce = generate_pkce().expect("pkce");
        assert!(pkce.code_verifier.len() >= 43);
        let recomputed = URL_SAFE_NO_PAD.encode(Sha256::digest(pkce.code_verifier.as_bytes()));
        assert_eq!(pkce.code_challenge, recomputed);
        assert!(!pkce.state.is_empty());
    }

    #[tokio::test]
    async fn test_loopback_server_validates_state_and_skips_favicon() {
        let server = LoopbackServer::bind().await.expect("bind");
        let redirect = server.redirect_uri.clone();
        let base = redirect.trim_end_matches("/callback").to_string();
        let http = reqwest::Client::new();

        let client_task = tokio::spawn(async move {
            let _ = http.get(format!("{base}/favicon.ico")).send().await;
            let _ = http
                .get(format!("{redirect}?code=auth-code-xyz&state=expected-s1"))
                .send()
                .await;
        });

        let code = server
            .wait_for_code("expected-s1", Duration::from_secs(5))
            .await
            .expect("code");
        client_task.await.expect("join");
        assert_eq!(code, "auth-code-xyz");

        // Mismatched state must be rejected (CSRF protection)
        let server2 = LoopbackServer::bind().await.expect("bind2");
        let redirect2 = server2.redirect_uri.clone();
        let http2 = reqwest::Client::new();
        let t2 = tokio::spawn(async move {
            let _ = http2
                .get(format!("{redirect2}?code=auth-code&state=wrong-state"))
                .send()
                .await;
        });
        let err = server2
            .wait_for_code("right-state", Duration::from_secs(5))
            .await;
        t2.await.expect("join2");
        assert!(matches!(err, Err(OAuthError::StateMismatch)));
    }
}

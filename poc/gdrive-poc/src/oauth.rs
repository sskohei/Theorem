use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use url::Url;

pub const DRIVE_FILE_SCOPE: &str = "https://www.googleapis.com/auth/drive.file";

#[derive(Debug, Error)]
pub enum OAuthError {
    #[error("invalid OAuth scope requested: {0} (only drive.file is permitted)")]
    DisallowedScope(String),
    #[error("random byte generation failed: {0}")]
    RandomFailed(String),
    #[error("loopback listener error: {0}")]
    Io(#[from] std::io::Error),
    #[error("OAuth state mismatch (CSRF protection triggered)")]
    StateMismatch,
    #[error("OAuth callback missing authorization code")]
    MissingCode,
    #[error("HTTP error during token exchange: {0}")]
    Http(#[from] reqwest::Error),
    #[error("OAuth token endpoint returned error {status}: {body}")]
    TokenEndpoint { status: u16, body: String },
    #[error("URL error: {0}")]
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
    pub token_type: String,
    pub scope: Option<String>,
}

/// Generates RFC 7636 PKCE verifier/challenge (S256) and CSRF state using OS CSPRNG.
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

/// Validates that only minimal `drive.file` scope is requested (never `drive` or `drive.readonly`).
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
    pub async fn bind() -> Result<Self, OAuthError> {
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let port = listener.local_addr()?.port();
        Ok(Self {
            listener,
            redirect_uri: format!("http://127.0.0.1:{port}/callback"),
        })
    }

    /// Waits for a single OAuth redirect callback and verifies the CSRF `state`.
    pub async fn wait_for_code(self, expected_state: &str) -> Result<String, OAuthError> {
        let (mut socket, _) = self.listener.accept().await?;
        let mut buf = [0u8; 4096];
        let n = socket.read(&mut buf).await?;
        let req_str = String::from_utf8_lossy(&buf[..n]);
        let first_line = req_str.lines().next().unwrap_or_default();
        let path_and_query = first_line.split_whitespace().nth(1).unwrap_or("/");
        let parsed = Url::parse(&format!("http://127.0.0.1{path_and_query}"))?;

        let mut code: Option<String> = None;
        let mut state: Option<String> = None;
        for (k, v) in parsed.query_pairs() {
            if k == "code" {
                code = Some(v.into_owned());
            } else if k == "state" {
                state = Some(v.into_owned());
            }
        }

        let html = "<!doctype html><html><body><h2>FormulaDesk OAuth 認証が完了しました。このタブを閉じてください。</h2></body></html>";
        let resp = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            html.len(),
            html
        );
        let _ = socket.write_all(resp.as_bytes()).await;
        let _ = socket.shutdown().await;

        if state.as_deref() != Some(expected_state) {
            return Err(OAuthError::StateMismatch);
        }
        code.ok_or(OAuthError::MissingCode)
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
    if let Some(secret) = client_secret {
        form.push(("client_secret", secret));
    }
    let resp = http.post(token_endpoint).form(&form).send().await?;
    let status = resp.status().as_u16();
    if !resp.status().is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(OAuthError::TokenEndpoint { status, body });
    }
    Ok(resp.json::<TokenResponse>().await?)
}

use serde::Serialize;
use thiserror::Error;

use crate::security::redact_secrets;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("入力値が不正です: {0}")]
    Validation(String),

    #[error("対象が見つかりません: {0}")]
    NotFound(String),

    #[error("競合が発生しました: {0}")]
    Conflict(String),

    #[error("非対応のファイル形式です: {0}")]
    UnsupportedFormat(String),

    #[error("セキュリティ制約により操作が拒否されました: {0}")]
    Security(String),

    #[error("ストレージ操作に失敗しました: {0}")]
    Storage(String),

    #[error("データベース操作に失敗しました: {0}")]
    Database(String),

    #[error("バックアップ操作に失敗しました: {0}")]
    Backup(String),

    #[error("AI エージェント処理に失敗しました: {0}")]
    Agent(String),

    #[error("出力データのスキーマ検証に失敗しました: {0}")]
    SchemaValidation(String),

    #[error("LaTeX → Typst 数式変換に失敗しました: {0}")]
    LatexConversion(String),

    #[error("PDF 生成に失敗しました: {0}")]
    PdfGeneration(String),

    #[error("OS キーチェーン操作に失敗しました: {0}")]
    Keyring(String),
}

impl From<sqlx::Error> for AppError {
    fn from(err: sqlx::Error) -> Self {
        match &err {
            sqlx::Error::RowNotFound => AppError::NotFound("対象レコードが見つかりません".into()),
            sqlx::Error::Database(db_err) => {
                let msg = db_err.message();
                if msg.contains("UNIQUE constraint failed") {
                    AppError::Conflict("同じ名前の項目がすでに存在します".into())
                } else if msg.contains("FOREIGN KEY constraint failed") {
                    AppError::Conflict("関連データが存在するため削除または更新できません".into())
                } else {
                    AppError::Database(redact_secrets(msg))
                }
            }
            _ => AppError::Database(redact_secrets(&err.to_string())),
        }
    }
}

impl From<std::io::Error> for AppError {
    fn from(err: std::io::Error) -> Self {
        AppError::Storage(redact_secrets(&err.to_string()))
    }
}

/// Structured IPC error serialized across the Tauri command boundary.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiError {
    pub code: String,
    pub message: String,
    pub detail: Option<String>,
    pub recoverable: bool,
}

impl From<AppError> for ApiError {
    fn from(err: AppError) -> Self {
        let (code, message, detail, recoverable) = match err {
            AppError::Validation(msg) => ("VALIDATION_ERROR", msg, None, true),
            AppError::NotFound(msg) => ("NOT_FOUND", msg, None, true),
            AppError::Conflict(msg) => ("CONFLICT", msg, None, true),
            AppError::UnsupportedFormat(msg) => ("UNSUPPORTED_FORMAT", msg, None, true),
            AppError::Security(msg) => (
                "SECURITY_VIOLATION",
                "セキュリティ制約により操作が拒否されました".to_string(),
                Some(redact_secrets(&msg)),
                false,
            ),
            AppError::Storage(msg) => (
                "STORAGE_ERROR",
                "ファイルの読み書きに失敗しました".to_string(),
                Some(redact_secrets(&msg)),
                true,
            ),
            AppError::Database(msg) => (
                "DATABASE_ERROR",
                "データベースの操作に失敗しました".to_string(),
                Some(redact_secrets(&msg)),
                true,
            ),
            AppError::Backup(msg) => (
                "BACKUP_ERROR",
                "バックアップまたは復元処理に失敗しました".to_string(),
                Some(redact_secrets(&msg)),
                true,
            ),
            AppError::Agent(msg) => (
                "AGENT_ERROR",
                "AI エージェントとの通信または実行に失敗しました".to_string(),
                Some(redact_secrets(&msg)),
                true,
            ),
            AppError::SchemaValidation(msg) => (
                "SCHEMA_VALIDATION_ERROR",
                "AI 応答の構造化データ検証に失敗しました".to_string(),
                Some(redact_secrets(&msg)),
                true,
            ),
            AppError::LatexConversion(msg) => (
                "LATEX_CONVERSION_ERROR",
                "LaTeX 数式から Typst への変換に失敗しました".to_string(),
                Some(redact_secrets(&msg)),
                true,
            ),
            AppError::PdfGeneration(msg) => (
                "PDF_GENERATION_ERROR",
                "Typst による解答 PDF 生成に失敗しました".to_string(),
                Some(redact_secrets(&msg)),
                true,
            ),
            AppError::Keyring(msg) => (
                "KEYRING_ERROR",
                "OS キーチェーンへの認証情報アクセスに失敗しました".to_string(),
                Some(redact_secrets(&msg)),
                true,
            ),
        };

        ApiError {
            code: code.to_string(),
            message,
            detail,
            recoverable,
        }
    }
}

pub type AppResult<T> = Result<T, AppError>;

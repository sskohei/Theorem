pub mod ai_repo;
pub mod drive_repo;
pub mod library_repo;
pub mod science_repo;

use std::path::Path;
use std::str::FromStr;
use std::time::Duration;

use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous};
use sqlx::{Pool, Sqlite};

use crate::error::{AppError, AppResult};

#[derive(Debug, Clone)]
pub struct Database {
    pool: Pool<Sqlite>,
}

impl Database {
    /// Connects to SQLite with WAL mode, `PRAGMA foreign_keys = ON`, 5s busy timeout,
    /// and applies embedded `sqlx` migrations.
    pub async fn connect(db_path: &Path) -> AppResult<Self> {
        let url = format!("sqlite://{}", db_path.to_string_lossy().replace('\\', "/"));
        let opts = SqliteConnectOptions::from_str(&url)?
            .create_if_missing(true)
            .foreign_keys(true)
            .journal_mode(SqliteJournalMode::Wal)
            .synchronous(SqliteSynchronous::Normal)
            .busy_timeout(Duration::from_secs(5));

        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect_with(opts)
            .await?;

        sqlx::migrate!("./migrations")
            .run(&pool)
            .await
            .map_err(|e| AppError::Database(format!("マイグレーション適用エラー: {e}")))?;

        let db = Self { pool };
        db.seed_default_subjects_if_empty().await?;
        ai_repo::seed_default_agent_profiles_if_empty(&db.pool).await?;
        ai_repo::recover_interrupted_state(&db.pool).await?;
        science_repo::recover_interrupted_tool_executions(&db.pool).await?;
        drive_repo::recover_interrupted_drive_jobs(&db.pool).await?;
        Ok(db)
    }

    pub fn pool(&self) -> &Pool<Sqlite> {
        &self.pool
    }

    pub async fn close(&self) {
        let _ = sqlx::query("PRAGMA wal_checkpoint(TRUNCATE);")
            .execute(&self.pool)
            .await;
        self.pool.close().await;
    }

    /// Creates a transactionally consistent snapshot of the SQLite database using `VACUUM INTO`.
    pub async fn snapshot_to(&self, dest: &Path) -> AppResult<()> {
        if dest.exists() {
            let _ = std::fs::remove_file(dest);
        }
        let escaped = dest
            .to_string_lossy()
            .replace('\\', "/")
            .replace('\'', "''");
        let sql = format!("VACUUM INTO '{escaped}'");
        sqlx::query(&sql).execute(&self.pool).await?;
        Ok(())
    }

    async fn seed_default_subjects_if_empty(&self) -> AppResult<()> {
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM subjects")
            .fetch_one(&self.pool)
            .await?;
        if count == 0 {
            library_repo::create_subject(
                &self.pool,
                "大学数学",
                "mathematics",
                Some("微積分・線形代数・集合位相・複素解析などの大学数学問題"),
            )
            .await?;
            library_repo::create_subject(
                &self.pool,
                "大学物理",
                "physics",
                Some("力学・解析力学・電磁気学・熱統計力学・量子力学などの大学物理問題"),
            )
            .await?;
        }
        Ok(())
    }
}

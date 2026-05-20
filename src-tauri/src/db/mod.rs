use sqlx::{sqlite::SqlitePoolOptions, Pool, Sqlite};
use std::path::PathBuf;

use crate::error::AppError;

pub type DbPool = Pool<Sqlite>;

pub async fn init_db(app_dir: PathBuf) -> Result<DbPool, AppError> {
    let db_path = app_dir.join("profiles.db");
    let db_url = format!("sqlite:{}", db_path.display());

    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect(&db_url)
        .await
        .map_err(|e| AppError::Tunnel(format!("DB connection failed: {}", e)))?;

    // Run migrations inline (sqlx::migrate! requires compile-time DB)
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS profiles (
            id TEXT PRIMARY KEY,
            label TEXT NOT NULL,
            host TEXT NOT NULL,
            port INTEGER NOT NULL DEFAULT 22,
            username TEXT NOT NULL,
            auth_type TEXT NOT NULL CHECK(auth_type IN ('password', 'key_inline', 'key_file', 'agent')),
            password_enc BLOB,
            private_key_enc BLOB,
            key_passphrase_enc BLOB,
            identity_file_path TEXT,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_profiles_label ON profiles(label);
        CREATE INDEX IF NOT EXISTS idx_profiles_host ON profiles(host);
        "#
    )
    .execute(&pool)
    .await
    .map_err(|e| AppError::Tunnel(format!("Migration failed: {}", e)))?;

    Ok(pool)
}
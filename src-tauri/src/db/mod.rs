use sqlx::{sqlite::{SqlitePoolOptions, SqliteConnectOptions}, Pool, Sqlite};
use std::path::PathBuf;

use crate::error::AppError;

pub type DbPool = Pool<Sqlite>;

pub async fn init_db(app_dir: PathBuf) -> Result<DbPool, AppError> {
    let db_path = app_dir.join("profiles.db");
    
    // Use SqliteConnectOptions with PathBuf for proper path handling
    let options = SqliteConnectOptions::new()
        .filename(&db_path)
        .create_if_missing(true);

    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(options)
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
    .map_err(|e| AppError::Tunnel(format!("Profiles migration failed: {}", e)))?;

    // Logs table
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS logs (
            id TEXT PRIMARY KEY,
            timestamp TEXT NOT NULL,
            level TEXT NOT NULL CHECK(level IN ('info', 'warn', 'error', 'debug')),
            message TEXT NOT NULL,
            profile_id TEXT,
            FOREIGN KEY (profile_id) REFERENCES profiles(id) ON DELETE SET NULL
        );

        CREATE INDEX IF NOT EXISTS idx_logs_timestamp ON logs(timestamp DESC);
        CREATE INDEX IF NOT EXISTS idx_logs_level ON logs(level);
        "#
    )
    .execute(&pool)
    .await
    .map_err(|e| AppError::Tunnel(format!("Logs migration failed: {}", e)))?;

    Ok(pool)
}
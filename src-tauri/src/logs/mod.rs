use serde::Serialize;
use uuid::Uuid;

use crate::db::DbPool;
use crate::error::AppError;

#[derive(Debug, Serialize, sqlx::FromRow, Clone)]
pub struct LogEntry {
    pub id: String,
    pub timestamp: String,
    pub level: String,
    pub message: String,
    pub profile_id: Option<String>,
}

pub async fn insert_log(
    pool: &DbPool,
    level: &str,
    message: &str,
    profile_id: Option<&str>,
) -> Result<LogEntry, AppError> {
    let id = Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();

    sqlx::query(
        r#"INSERT INTO logs (id, timestamp, level, message, profile_id) VALUES (?1, ?2, ?3, ?4, ?5)"#,
    )
    .bind(&id)
    .bind(&now)
    .bind(level)
    .bind(message)
    .bind(profile_id)
    .execute(pool)
    .await
    .map_err(|e| AppError::Tunnel(format!("Log insert failed: {}", e)))?;

    Ok(LogEntry {
        id,
        timestamp: now,
        level: level.to_string(),
        message: message.to_string(),
        profile_id: profile_id.map(|s| s.to_string()),
    })
}

pub async fn get_logs(
    pool: &DbPool,
    limit: Option<u32>,
) -> Result<Vec<LogEntry>, AppError> {
    let limit = limit.unwrap_or(100).min(1000);
    let rows = sqlx::query_as::<_, LogEntry>(
        "SELECT id, timestamp, level, message, profile_id FROM logs ORDER BY timestamp DESC LIMIT ?1",
    )
    .bind(limit)
    .fetch_all(pool)
    .await
    .map_err(|e| AppError::Tunnel(format!("Log query failed: {}", e)))?;
    Ok(rows)
}

pub async fn get_logs_by_level(
    pool: &DbPool,
    level: &str,
    limit: Option<u32>,
) -> Result<Vec<LogEntry>, AppError> {
    let limit = limit.unwrap_or(100).min(1000);
    let rows = sqlx::query_as::<_, LogEntry>(
        "SELECT id, timestamp, level, message, profile_id FROM logs WHERE level = ?1 ORDER BY timestamp DESC LIMIT ?2",
    )
    .bind(level)
    .bind(limit)
    .fetch_all(pool)
    .await
    .map_err(|e| AppError::Tunnel(format!("Log query failed: {}", e)))?;
    Ok(rows)
}

pub async fn prune_old_logs(pool: &DbPool, max_age_days: i64) -> Result<u64, AppError> {
    let result = sqlx::query("DELETE FROM logs WHERE timestamp < datetime('now', ?1)")
        .bind(format!("-{} days", max_age_days))
        .execute(pool)
        .await
        .map_err(|e| AppError::Tunnel(format!("Log prune failed: {}", e)))?;
    Ok(result.rows_affected())
}

pub async fn clear_logs(pool: &DbPool) -> Result<(), AppError> {
    sqlx::query("DELETE FROM logs")
        .execute(pool)
        .await
        .map_err(|e| AppError::Tunnel(format!("Log clear failed: {}", e)))?;
    Ok(())
}
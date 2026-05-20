use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;
use chrono::Utc;

use crate::db::DbPool;
use crate::crypto::{encrypt, decrypt};
use crate::error::AppError;

#[derive(Debug, Serialize, Deserialize, FromRow)]
pub struct Profile {
    pub id: String,
    pub label: String,
    pub host: String,
    pub port: i64,
    pub username: String,
    pub auth_type: String,
    #[serde(skip_serializing)]
    pub password_enc: Option<Vec<u8>>,
    #[serde(skip_serializing)]
    pub private_key_enc: Option<Vec<u8>>,
    #[serde(skip_serializing)]
    pub key_passphrase_enc: Option<Vec<u8>>,
    pub identity_file_path: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Deserialize)]
pub struct CreateProfileRequest {
    pub label: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub auth_type: String,
    pub password: Option<String>,
    pub private_key: Option<String>,
    pub key_passphrase: Option<String>,
    pub identity_file_path: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateProfileRequest {
    pub id: String,
    pub label: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub auth_type: String,
    pub password: Option<String>,
    pub private_key: Option<String>,
    pub key_passphrase: Option<String>,
    pub identity_file_path: Option<String>,
}

pub async fn create_profile(
    pool: &DbPool,
    master_key: &[u8; 32],
    req: CreateProfileRequest,
) -> Result<Profile, AppError> {
    let id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();

    let password_enc = req.password
        .map(|p| encrypt(p.as_bytes(), master_key))
        .transpose()?;

    let private_key_enc = req.private_key
        .map(|k| encrypt(k.as_bytes(), master_key))
        .transpose()?;

    let key_passphrase_enc = req.key_passphrase
        .map(|p| encrypt(p.as_bytes(), master_key))
        .transpose()?;

    sqlx::query(
        r#"
        INSERT INTO profiles (id, label, host, port, username, auth_type, password_enc, private_key_enc, key_passphrase_enc, identity_file_path, created_at, updated_at)
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
        "#
    )
    .bind(&id)
    .bind(&req.label)
    .bind(&req.host)
    .bind(req.port as i64)
    .bind(&req.username)
    .bind(&req.auth_type)
    .bind(password_enc.as_ref().map(|v| v.as_slice()))
    .bind(private_key_enc.as_ref().map(|v| v.as_slice()))
    .bind(key_passphrase_enc.as_ref().map(|v| v.as_slice()))
    .bind(&req.identity_file_path)
    .bind(&now)
    .bind(&now)
    .execute(pool)
    .await
    .map_err(|e| AppError::Tunnel(format!("Insert failed: {}", e)))?;

    Ok(Profile {
        id,
        label: req.label,
        host: req.host,
        port: req.port as i64,
        username: req.username,
        auth_type: req.auth_type,
        password_enc,
        private_key_enc,
        key_passphrase_enc,
        identity_file_path: req.identity_file_path,
        created_at: now.clone(),
        updated_at: now,
    })
}

pub async fn update_profile(
    pool: &DbPool,
    master_key: &[u8; 32],
    req: UpdateProfileRequest,
) -> Result<Profile, AppError> {
    let now = Utc::now().to_rfc3339();

    let password_enc: Option<Vec<u8>> = match &req.password {
        Some(p) if !p.is_empty() => Some(encrypt(p.as_bytes(), master_key)?),
        _ => None,
    };
    let private_key_enc: Option<Vec<u8>> = match &req.private_key {
        Some(k) if !k.is_empty() => Some(encrypt(k.as_bytes(), master_key)?),
        _ => None,
    };
    let key_passphrase_enc: Option<Vec<u8>> = match &req.key_passphrase {
        Some(p) if !p.is_empty() => Some(encrypt(p.as_bytes(), master_key)?),
        _ => None,
    };

    sqlx::query(
        r#"
        UPDATE profiles SET
            label = ?1,
            host = ?2,
            port = ?3,
            username = ?4,
            auth_type = ?5,
            password_enc = CASE WHEN ?6 IS NOT NULL THEN ?6 ELSE password_enc END,
            private_key_enc = CASE WHEN ?7 IS NOT NULL THEN ?7 ELSE private_key_enc END,
            key_passphrase_enc = CASE WHEN ?8 IS NOT NULL THEN ?8 ELSE key_passphrase_enc END,
            identity_file_path = CASE WHEN ?9 IS NOT NULL THEN ?9 ELSE identity_file_path END,
            updated_at = ?10
        WHERE id = ?11
        "#,
    )
    .bind(&req.label)
    .bind(&req.host)
    .bind(req.port as i64)
    .bind(&req.username)
    .bind(&req.auth_type)
    .bind(password_enc.as_ref().map(|v| v.as_slice()))
    .bind(private_key_enc.as_ref().map(|v| v.as_slice()))
    .bind(key_passphrase_enc.as_ref().map(|v| v.as_slice()))
    .bind(&req.identity_file_path)
    .bind(&now)
    .bind(&req.id)
    .execute(pool)
    .await
    .map_err(|e| AppError::Tunnel(format!("Update failed: {}", e)))?;

    let profile = sqlx::query_as::<_, Profile>("SELECT * FROM profiles WHERE id = ?1")
        .bind(&req.id)
        .fetch_one(pool)
        .await
        .map_err(|e| AppError::Tunnel(format!("Failed to fetch updated profile: {}", e)))?;

    Ok(profile)
}

pub async fn get_profiles(pool: &DbPool) -> Result<Vec<Profile>, AppError> {
    let profiles = sqlx::query_as::<_, Profile>(
        "SELECT id, label, host, port, username, auth_type, password_enc, private_key_enc, key_passphrase_enc, identity_file_path, created_at, updated_at FROM profiles ORDER BY label"
    )
    .fetch_all(pool)
    .await
    .map_err(|e| AppError::Tunnel(format!("Query failed: {}", e)))?;

    Ok(profiles)
}

pub async fn get_profile_by_id(pool: &DbPool, id: &str) -> Result<Profile, AppError> {
    let profile = sqlx::query_as::<_, Profile>(
        "SELECT * FROM profiles WHERE id = ?1"
    )
    .bind(id)
    .fetch_one(pool)
    .await
    .map_err(|e| AppError::Tunnel(format!("Query failed: {}", e)))?;

    Ok(profile)
}

pub async fn delete_profile(pool: &DbPool, id: &str) -> Result<(), AppError> {
    sqlx::query("DELETE FROM profiles WHERE id = ?1")
        .bind(id)
        .execute(pool)
        .await
        .map_err(|e| AppError::Tunnel(format!("Delete failed: {}", e)))?;

    Ok(())
}
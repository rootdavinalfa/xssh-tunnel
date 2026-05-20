// lib.rs — all application logic lives here
use tokio::sync::Mutex;
use tauri::{Emitter, Manager};

pub mod crypto;
pub mod db;
pub mod error;
pub mod logs;
pub mod profiles;
pub mod ssh;
pub mod tunnel;

use crypto::keychain::get_or_create_master_key;
use db::{init_db, DbPool};
use error::AppError;
use logs::LogEntry;
use profiles::{create_profile, update_profile, get_profiles, get_profile_by_id, delete_profile, CreateProfileRequest, UpdateProfileRequest};
use ssh::config_parser::{parse_ssh_config, SshConfigEntry, ParseResult};
use tunnel::{Tunnel, TunnelConfig};

struct AppState {
    db: DbPool,
    master_key: [u8; 32],
    tunnel: Mutex<Option<Tunnel>>,
}

async fn emit_log(
    app_handle: &tauri::AppHandle,
    db: &DbPool,
    level: &str,
    message: &str,
    profile_id: Option<&str>,
) {
    let result = logs::insert_log(db, level, message, profile_id).await;
    if let Ok(entry) = result {
        let _ = app_handle.emit("log-entry", &entry);
    }
}

// Profile commands
#[tauri::command]
async fn create_profile_cmd(app: tauri::AppHandle, state: tauri::State<'_, AppState>, req: CreateProfileRequest) -> Result<profiles::Profile, AppError> {
    let profile = create_profile(&state.db, &state.master_key, req).await?;
    emit_log(&app, &state.db, "info", &format!("Profile created: {}", profile.label), Some(&profile.id)).await;
    Ok(profile)
}

#[tauri::command]
async fn get_profiles_cmd(state: tauri::State<'_, AppState>) -> Result<Vec<profiles::Profile>, AppError> {
    get_profiles(&state.db).await
}

#[tauri::command]
async fn get_profile_by_id_cmd(
    state: tauri::State<'_, AppState>,
    id: String,
) -> Result<profiles::Profile, AppError> {
    get_profile_by_id(&state.db, &id).await
}

#[tauri::command]
async fn delete_profile_cmd(app: tauri::AppHandle, state: tauri::State<'_, AppState>, id: String) -> Result<(), AppError> {
    // Get profile name before deleting
    let profile = get_profile_by_id(&state.db, &id).await?;
    let label = profile.label.clone();
    delete_profile(&state.db, &id).await?;
    emit_log(&app, &state.db, "info", &format!("Profile deleted: {}", label), Some(&id)).await;
    Ok(())
}

#[tauri::command]
async fn update_profile_cmd(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    req: UpdateProfileRequest,
) -> Result<profiles::Profile, AppError> {
    let profile = update_profile(&state.db, &state.master_key, req).await?;
    emit_log(&app, &state.db, "info", &format!("Profile updated: {}", profile.label), Some(&profile.id)).await;
    Ok(profile)
}

// Tunnel commands (updated to use profiles)
#[tauri::command]
async fn connect_tunnel(app: tauri::AppHandle, state: tauri::State<'_, AppState>, profile_id: String) -> Result<String, AppError> {
    // Check if already connected (drop guard immediately)
    {
        let tunnel_guard = state.tunnel.lock().await;
        if tunnel_guard.is_some() {
            return Err(AppError::AlreadyConnected);
        }
    }

    let profile = get_profile_by_id(&state.db, &profile_id).await?;
    emit_log(&app, &state.db, "info", &format!("Connecting to {}...", profile.host), Some(&profile_id)).await;

    app.emit("connection-state", "connecting").unwrap();

    // Decrypt credentials based on auth type
    let (username, password) = match profile.auth_type.as_str() {
        "password" => {
            let pass = profile.password_enc
                .and_then(|enc| crypto::decrypt(&enc, &state.master_key).ok())
                .and_then(|bytes| String::from_utf8(bytes).ok())
                .unwrap_or_default();
            (profile.username, Some(pass))
        }
        _ => (profile.username, None),
    };

    emit_log(&app, &state.db, "info", "SSH authentication successful", Some(&profile_id)).await;
    app.emit("connection-state", "authenticating").unwrap();

    let config = TunnelConfig {
        ssh_host: profile.host,
        ssh_port: profile.port as u16,
        ssh_username: username,
        ssh_password: password.unwrap_or_default(),
    };

    let mut tunnel = Tunnel::new();
    // Wrap in a block to catch connection errors and reset state
    match tunnel.start(config).await {
        Ok(()) => {
            emit_log(&app, &state.db, "info", "Tunnel active", Some(&profile_id)).await;
            app.emit("connection-state", "tunnel-active").unwrap();

            // Store tunnel (acquire lock again)
            let mut tunnel_guard = state.tunnel.lock().await;
            *tunnel_guard = Some(tunnel);
            Ok("Connected".to_string())
        }
        Err(e) => {
            emit_log(&app, &state.db, "error", &format!("Connection failed: {}", e), Some(&profile_id)).await;
            app.emit("connection-state", "disconnected").unwrap();
            Err(e)
        }
    }
}

#[tauri::command]
async fn disconnect_tunnel(app: tauri::AppHandle, state: tauri::State<'_, AppState>) -> Result<String, AppError> {
    // Take tunnel from state (drop guard immediately)
    let tunnel = {
        let mut tunnel_guard = state.tunnel.lock().await;
        tunnel_guard.take()
    };
    
    if let Some(mut tunnel) = tunnel {
        tunnel.stop().await?;
        emit_log(&app, &state.db, "info", "Disconnected from tunnel", None).await;
        app.emit("connection-state", "disconnected").unwrap();
        Ok("Disconnected".to_string())
    } else {
        Err(AppError::NotConnected)
    }
}

#[tauri::command]
async fn get_logs_cmd(
    state: tauri::State<'_, AppState>,
    level: Option<String>,
    limit: Option<u32>,
) -> Result<Vec<LogEntry>, AppError> {
    if let Some(lvl) = &level {
        logs::get_logs_by_level(&state.db, lvl, limit).await
    } else {
        logs::get_logs(&state.db, limit).await
    }
}

#[tauri::command]
async fn clear_logs_cmd(state: tauri::State<'_, AppState>) -> Result<(), AppError> {
    logs::clear_logs(&state.db).await
}

#[tauri::command]
async fn parse_ssh_config_cmd() -> Result<ParseResult, AppError> {
    parse_ssh_config(None)
}

#[tauri::command]
async fn import_ssh_config_cmd(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    selected_hosts: Vec<String>,
) -> Result<Vec<profiles::Profile>, AppError> {
    let parse_result = parse_ssh_config(None)?;
    let mut imported = Vec::new();

    for entry in parse_result.entries {
        if selected_hosts.contains(&entry.host_aliases[0]) {
            let label = entry.host_aliases[0].clone();
            let auth_type = if entry.identity_file.is_some() { "key_file" } else { "agent" };

            let req = profiles::CreateProfileRequest {
                label: label.clone(),
                host: entry.hostname.clone(),
                port: entry.port.unwrap_or(22),
                username: entry.user.unwrap_or_else(|| "root".to_string()),
                auth_type: auth_type.to_string(),
                password: None,
                private_key: None,
                key_passphrase: None,
                identity_file_path: entry.identity_file.clone(),
            };

            let profile = create_profile(&state.db, &state.master_key, req).await?;
            emit_log(&app, &state.db, "info", &format!("Imported profile: {}", label), Some(&profile.id)).await;
            imported.push(profile);
        }
    }

    emit_log(&app, &state.db, "info", &format!("SSH config import complete: {} profiles imported", imported.len()), None).await;
    Ok(imported)
}

#[tauri::command]
fn greet(name: String) -> String {
    format!("Hello, {}! You've been greeted from Rust.", name)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .invoke_handler(tauri::generate_handler![
            greet,
            create_profile_cmd,
            update_profile_cmd,
            get_profiles_cmd,
            get_profile_by_id_cmd,
            delete_profile_cmd,
            connect_tunnel,
            disconnect_tunnel,
            get_logs_cmd,
            clear_logs_cmd,
            parse_ssh_config_cmd,
            import_ssh_config_cmd,
        ])
        .setup(|app| {
            let app_handle = app.handle();
            let app_dir = app_handle.path().app_data_dir()
                .map_err(|e| {
                    eprintln!("Failed to get app data dir: {}", e);
                    e
                })?;
            
            // Create app data directory if it doesn't exist
            if let Err(e) = std::fs::create_dir_all(&app_dir) {
                eprintln!("Failed to create app data directory: {}", e);
            }
            
            // Initialize database
            let db = match tauri::async_runtime::block_on(init_db(app_dir)) {
                Ok(db) => db,
                Err(e) => {
                    eprintln!("Failed to initialize database: {}", e);
                    return Err(Box::new(std::io::Error::new(
                        std::io::ErrorKind::Other,
                        format!("Database initialization failed: {}", e)
                    )));
                }
            };

            // Get or create master key
            let master_key = match get_or_create_master_key() {
                Ok(key) => key,
                Err(e) => {
                    eprintln!("Failed to get master key: {}", e);
                    return Err(Box::new(std::io::Error::new(
                        std::io::ErrorKind::Other,
                        format!("Keychain initialization failed: {}", e)
                    )));
                }
            };

            // Clone db for log pruning
            let db_clone = db.clone();
            
            app.manage(AppState {
                db,
                master_key,
                tunnel: Mutex::new(None),
            });

            // Prune old logs
            tauri::async_runtime::spawn(async move {
                if let Err(e) = logs::prune_old_logs(&db_clone, 7).await {
                    eprintln!("Failed to prune old logs: {}", e);
                }
            });

            #[cfg(debug_assertions)]
            {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.open_devtools();
                }
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
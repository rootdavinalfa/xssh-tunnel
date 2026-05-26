// lib.rs — all application logic lives here
use tokio::sync::Mutex;
use tauri::{Emitter, Manager};

pub mod crypto;
pub mod db;
pub mod error;
pub mod helper;
pub mod logs;
pub mod profiles;
pub mod ssh;
pub mod tunnel;

use crypto::keychain::get_or_create_master_key;
use db::{init_db, DbPool};
use error::AppError;
use helper::HelperClient;
use logs::LogEntry;
use profiles::{create_profile, update_profile, get_profiles, get_profile_by_id, delete_profile, CreateProfileRequest, UpdateProfileRequest};
use ssh::config_parser::{parse_ssh_config, ParseResult};
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
    {
        let tunnel_guard = state.tunnel.lock().await;
        if tunnel_guard.is_some() {
            return Err(AppError::AlreadyConnected);
        }
    }

    let profile = get_profile_by_id(&state.db, &profile_id).await?;
    emit_log(&app, &state.db, "info", &format!("Connecting to {}...", profile.host), Some(&profile_id)).await;
    app.emit("connection-state", "connecting").unwrap();

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

    emit_log(&app, &state.db, "info", "SSH credentials decrypted", Some(&profile_id)).await;
    app.emit("connection-state", "authenticating").unwrap();

    // Create stats emitter - Arc shared with SOCKS5 proxy
    let stats = std::sync::Arc::new(tunnel::ConnectionStats::new());
    let stats_for_emit = stats.clone();
    let app_for_stats = app.clone();

    // Emit stats every 1 second
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            let snapshot = stats_for_emit.snapshot();
            let _ = app_for_stats.emit("connection-stats", &snapshot);
        }
    });

    // Build tunnel: SSH connect + SOCKS5 proxy
    let mut tunnel = Tunnel::new(profile_id.clone(), stats.clone());
    let config = TunnelConfig {
        ssh_host: profile.host.clone(),
        ssh_port: profile.port as u16,
        ssh_username: username.clone(),
        ssh_password: password.unwrap_or_default(),
    };

    match tunnel.start(config).await {
        Ok(()) => {
            let socks_port = tunnel.socks5_port();
            emit_log(&app, &state.db, "info", &format!("Tunnel active, SOCKS5 proxy on 127.0.0.1:{}", socks_port), Some(&profile_id)).await;
            app.emit("connection-state", "tunnel-active").unwrap();

            // Full proxy setup via helper: pf rules, DNS, CLI env, SOCKS proxy
            match HelperClient::connect() {
                Ok(mut h) => {
                    if let Err(e) = h.setup_proxies(socks_port) {
                        emit_log(&app, &state.db, "warn", &format!("Proxy setup had errors: {}", e), Some(&profile_id)).await;
                    } else {
                        emit_log(&app, &state.db, "info", "Proxies configured: DNS, HTTP/HTTPS, CLI, SOCKS", Some(&profile_id)).await;
                    }
                }
                Err(e) => {
                    emit_log(&app, &state.db, "warn", &format!("Helper not available for proxy setup: {}", e), Some(&profile_id)).await;
                    emit_log(&app, &state.db, "info", "Manual setup: set SOCKS5 proxy to 127.0.0.1:{}", Some(&profile_id)).await;
                }
            }

            // Spawn monitor task to watch for tunnel disconnection
            let app_for_monitor = app.clone();
            let db_for_monitor = state.db.clone();
            let profile_id_for_monitor = profile_id.clone();

            tokio::spawn(async move {
                // Wait for SSH client to be dropped (signals disconnect)
                loop {
                    tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                    let state_for_monitor = app_for_monitor.state::<AppState>();
                    let tunnel_lock = state_for_monitor.tunnel.lock().await;
                    if tunnel_lock.as_ref().map(|t| t.ssh_client.is_none()).unwrap_or(true) {
                        drop(tunnel_lock);
                        app_for_monitor.emit("connection-state", "disconnected").unwrap();
                        emit_log(&app_for_monitor, &db_for_monitor, "info",
                            "Tunnel disconnected", Some(&profile_id_for_monitor)).await;
                        return;
                    }
                }
            });

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
    let tunnel = {
        let mut tunnel_guard = state.tunnel.lock().await;
        tunnel_guard.take()
    };

    if let Some(mut t) = tunnel {
        // Teardown all proxies via helper: pf rules, DNS, CLI env, SOCKS
        match HelperClient::connect() {
            Ok(mut h) => {
                let _ = h.teardown_proxies();
            }
            Err(_) => {}
        }

        t.stop().await?;
        emit_log(&app, &state.db, "info", "Disconnected from tunnel", None).await;
        app.emit("connection-state", "disconnected").unwrap();
        Ok("Disconnected".to_string())
    } else {
        Err(AppError::NotConnected)
    }
}

#[tauri::command]
async fn get_connection_state_cmd(state: tauri::State<'_, AppState>) -> Result<String, AppError> {
    let tunnel_guard = state.tunnel.lock().await;
    if tunnel_guard.is_some() {
        Ok("connected".to_string())
    } else {
        Ok("disconnected".to_string())
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
async fn get_helper_status_cmd() -> Result<helper::HelperStatus, AppError> {
    helper::get_status()
        .map_err(|e| AppError::Tunnel(format!("Failed to get helper status: {}", e)))
}

#[tauri::command]
async fn install_helper_cmd() -> Result<(), AppError> {
    let bundle_path = std::env::current_exe()
        .map_err(|e| AppError::Tunnel(format!("Failed to get app path: {}", e)))?
        .parent()
        .and_then(|p| p.parent())
        .and_then(|p| p.parent())
        .map(|p| p.join("Library").join("LaunchServices").join("xssh-tunnel-helper"))
        .ok_or_else(|| AppError::Tunnel("Failed to resolve helper path".to_string()))?;

    let path_str = bundle_path.to_str()
        .ok_or_else(|| AppError::Tunnel("Invalid helper path".to_string()))?;

    helper::install(path_str)
        .map_err(|e| AppError::Tunnel(format!("Failed to install helper: {}", e)))
}

#[tauri::command]
async fn uninstall_helper_cmd() -> Result<(), AppError> {
    helper::uninstall()
        .map_err(|e| AppError::Tunnel(format!("Failed to uninstall helper: {}", e)))
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
            get_connection_state_cmd,
            get_logs_cmd,
            clear_logs_cmd,
            parse_ssh_config_cmd,
            import_ssh_config_cmd,
            get_helper_status_cmd,
            install_helper_cmd,
            uninstall_helper_cmd,
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
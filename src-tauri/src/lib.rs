// lib.rs — all application logic lives here
use tokio::sync::Mutex;
use tauri::{Emitter, Manager};

pub mod crypto;
pub mod db;
pub mod error;
pub mod profiles;
pub mod ssh;
pub mod tunnel;

use crypto::keychain::get_or_create_master_key;
use db::{init_db, DbPool};
use error::AppError;
use profiles::{create_profile, get_profiles, get_profile_by_id, delete_profile, CreateProfileRequest};
use tunnel::{Tunnel, TunnelConfig};

struct AppState {
    db: DbPool,
    master_key: [u8; 32],
    tunnel: Mutex<Option<Tunnel>>,
}

// Profile commands
#[tauri::command]
async fn create_profile_cmd(state: tauri::State<'_, AppState>, req: CreateProfileRequest) -> Result<profiles::Profile, AppError> {
    create_profile(&state.db, &state.master_key, req).await
}

#[tauri::command]
async fn get_profiles_cmd(state: tauri::State<'_, AppState>) -> Result<Vec<profiles::Profile>, AppError> {
    get_profiles(&state.db).await
}

#[tauri::command]
async fn delete_profile_cmd(state: tauri::State<'_, AppState>, id: String) -> Result<(), AppError> {
    delete_profile(&state.db, &id).await
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

    app.emit("connection-state", "authenticating").unwrap();

    let config = TunnelConfig {
        ssh_host: profile.host,
        ssh_port: profile.port as u16,
        ssh_username: username,
        ssh_password: password.unwrap_or_default(),
    };

    let mut tunnel = Tunnel::new();
    tunnel.start(config).await?;

    app.emit("connection-state", "tunnel-active").unwrap();

    // Store tunnel (acquire lock again)
    let mut tunnel_guard = state.tunnel.lock().await;
    *tunnel_guard = Some(tunnel);
    Ok("Connected".to_string())
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
        app.emit("connection-state", "disconnected").unwrap();
        Ok("Disconnected".to_string())
    } else {
        Err(AppError::NotConnected)
    }
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
            get_profiles_cmd,
            delete_profile_cmd,
            connect_tunnel,
            disconnect_tunnel,
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

            app.manage(AppState {
                db,
                master_key,
                tunnel: Mutex::new(None),
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
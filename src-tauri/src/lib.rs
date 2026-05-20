// lib.rs — all application logic lives here
use tokio::sync::Mutex;
use tauri::{Emitter, Manager};

pub mod error;
pub mod ssh;
pub mod tunnel;

use error::AppError;
use tunnel::{Tunnel, TunnelConfig};

struct AppState {
    tunnel: Mutex<Option<Tunnel>>,
}

#[tauri::command]
async fn connect_tunnel(app: tauri::AppHandle, state: tauri::State<'_, AppState>) -> Result<String, AppError> {
    // Check if already connected (drop guard immediately)
    {
        let tunnel_guard = state.tunnel.lock().await;
        if tunnel_guard.is_some() {
            return Err(AppError::AlreadyConnected);
        }
    }

    app.emit("connection-state", "connecting").unwrap();

    // Hardcoded credentials for M1 testing
    let config = TunnelConfig {
        ssh_host: "your-server.com".to_string(),
        ssh_port: 22,
        ssh_username: "user".to_string(),
        ssh_password: "password".to_string(),
    };

    app.emit("connection-state", "authenticating").unwrap();

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
        .manage(AppState {
            tunnel: Mutex::new(None),
        })
        .invoke_handler(tauri::generate_handler![
            greet,
            connect_tunnel,
            disconnect_tunnel,
        ])
        .setup(|app| {
            #[cfg(debug_assertions)]
            {
                let window = app.get_webview_window("main").unwrap();
                window.open_devtools();
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
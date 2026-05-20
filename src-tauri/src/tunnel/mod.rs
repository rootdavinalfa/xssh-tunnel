pub mod tun_device;
pub mod route_manager;
pub mod socks5;
pub mod packet_router;

pub use tun_device::TunDevice;
pub use route_manager::RouteManager;
pub use socks5::Socks5Engine;
pub use packet_router::PacketRouter;

use std::sync::Arc;

use crate::error::AppError;
use crate::ssh::{SshClient, SshConfig};

pub struct Tunnel {
    ssh_client: Option<Arc<SshClient>>,
    tun_device: Option<TunDevice>,
    router_handle: Option<tokio::task::JoinHandle<Result<(), AppError>>>,
}

impl Tunnel {
    pub fn new() -> Self {
        Tunnel {
            ssh_client: None,
            tun_device: None,
            router_handle: None,
        }
    }

    pub async fn start(&mut self, config: TunnelConfig) -> Result<(), AppError> {
        // 1. Create TUN device
        let tun = TunDevice::create()?;
        let tun_name = tun.name.clone();

        // 2. Connect SSH
        let ssh_config = SshConfig {
            host: config.ssh_host,
            port: config.ssh_port,
            username: config.ssh_username,
            password: config.ssh_password,
        };
        let ssh_client = Arc::new(SshClient::connect(ssh_config).await?);

        // 3. Inject routes (requires root — will fail without it)
        RouteManager::inject_default_route(&tun_name)?;

        // 4. Start packet router in a dedicated blocking thread
        // TUN reads are blocking syscalls — must NOT run on Tokio's async thread pool
        let router = PacketRouter::new(ssh_client.clone());
        let router_handle = tokio::task::spawn_blocking(move || {
            router.blocking_read_loop(tun)
        });

        self.ssh_client = Some(ssh_client);
        self.router_handle = Some(router_handle);

        Ok(())
    }

    pub async fn stop(&mut self) -> Result<(), AppError> {
        // Clean up routes (use stored name if available)
        if let Some(ref tun) = self.tun_device {
            let _ = RouteManager::cleanup_routes(&tun.name);
        }

        // Stop router
        if let Some(handle) = self.router_handle.take() {
            handle.abort();
        }

        // Disconnect SSH
        // Note: Arc prevents direct consumption — we accept SSH may not disconnect cleanly for M1
        // For M1, we just drop the Arc and let the connection close on drop
        self.ssh_client = None;

        Ok(())
    }
}

pub struct TunnelConfig {
    pub ssh_host: String,
    pub ssh_port: u16,
    pub ssh_username: String,
    pub ssh_password: String,
}
pub mod tun_device;
pub mod proxy;
pub mod packet_router;

pub use tun_device::TunDevice;
pub use packet_router::PacketRouter;

use std::sync::Arc;

use crate::error::AppError;
use crate::ssh::{SshClient, SshConfig};

pub struct Tunnel {
    ssh_client: Option<Arc<SshClient>>,
    router_handle: Option<tokio::task::JoinHandle<Result<(), AppError>>>,
    pub tun_name: Option<String>,
}

/// Thread-safe connection statistics.
pub struct ConnectionStats {
    up: std::sync::atomic::AtomicU64,
    down: std::sync::atomic::AtomicU64,
}

impl ConnectionStats {
    pub fn new() -> Self {
        ConnectionStats {
            up: std::sync::atomic::AtomicU64::new(0),
            down: std::sync::atomic::AtomicU64::new(0),
        }
    }
    pub fn add_up(&self, n: u64) {
        self.up.fetch_add(n, std::sync::atomic::Ordering::Relaxed);
    }
    pub fn add_down(&self, n: u64) {
        self.down.fetch_add(n, std::sync::atomic::Ordering::Relaxed);
    }
    pub fn bytes_up(&self) -> u64 {
        self.up.load(std::sync::atomic::Ordering::Relaxed)
    }
    pub fn bytes_down(&self) -> u64 {
        self.down.load(std::sync::atomic::Ordering::Relaxed)
    }
    pub fn snapshot(&self) -> ConnectionStatsSnapshot {
        ConnectionStatsSnapshot {
            bytes_up: self.bytes_up(),
            bytes_down: self.bytes_down(),
        }
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ConnectionStatsSnapshot {
    pub bytes_up: u64,
    pub bytes_down: u64,
}

impl Tunnel {
    pub fn new() -> Self {
        Tunnel {
            ssh_client: None,
            router_handle: None,
            tun_name: None,
        }
    }

    pub async fn start(
        &mut self,
        config: TunnelConfig,
        tun_fd: std::os::unix::io::RawFd,
        tun_name: &str,
        stats: Arc<ConnectionStats>,
    ) -> Result<(), AppError> {
        let _tun = TunDevice::from_fd(tun_fd, tun_name)
            .map_err(|e| AppError::Tunnel(e))?;

        let ssh_config = SshConfig {
            host: config.ssh_host,
            port: config.ssh_port,
            username: config.ssh_username,
            password: config.ssh_password,
        };
        let ssh_client = Arc::new(SshClient::connect(ssh_config).await?);

        let router = PacketRouter::new(ssh_client.clone(), stats, tun_fd);
        let router_handle = tokio::task::spawn_blocking(move || {
            router.blocking_read_loop()
        });

        self.tun_name = Some(tun_name.to_string());
        self.ssh_client = Some(ssh_client);
        self.router_handle = Some(router_handle);

        Ok(())
    }

    pub async fn stop(&mut self) -> Result<(), AppError> {
        if let Some(handle) = self.router_handle.take() {
            handle.abort();
        }
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
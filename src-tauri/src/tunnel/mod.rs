pub mod tun_device;
pub mod socks5;
pub mod proxies;

pub use tun_device::TunDevice;
pub use socks5::Socks5Proxy;

use std::sync::Arc;

use crate::error::AppError;
use crate::ssh::{SshClient, SshConfig};
use crate::tunnel::proxies::ProxyManager;

pub struct Tunnel {
    pub ssh_client: Option<Arc<SshClient>>,
    pub socks5: Socks5Proxy,
    pub proxies: ProxyManager,
    pub stats: Arc<ConnectionStats>,
    pub config: Option<TunnelConfig>,
    pub profile_id: String,
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
    pub fn new(profile_id: String, stats: Arc<ConnectionStats>) -> Self {
        Tunnel {
            ssh_client: None,
            socks5: Socks5Proxy::new(),
            proxies: ProxyManager::new(),
            stats,
            config: None,
            profile_id,
        }
    }

    pub async fn start(
        &mut self,
        config: TunnelConfig,
    ) -> Result<(), AppError> {
        self.config = Some(config.clone());

        let ssh_config = SshConfig {
            host: config.ssh_host,
            port: config.ssh_port,
            username: config.ssh_username,
            password: config.ssh_password,
        };
        let ssh_client = Arc::new(SshClient::connect(ssh_config).await?);

        // Start SOCKS5 proxy on a random port
        let socks_port = self.socks5.start(ssh_client.clone(), self.stats.clone(), 0).await?;
        tracing::info!("SOCKS5 proxy started on 127.0.0.1:{}", socks_port);

        // Start DNS proxy + transparent HTTP/HTTPS proxy
        self.proxies.start(ssh_client.clone(), self.stats.clone());

        self.ssh_client = Some(ssh_client);

        Ok(())
    }

    pub async fn stop(&mut self) -> Result<(), AppError> {
        self.socks5.stop();
        self.proxies.stop();
        self.ssh_client = None;
        Ok(())
    }

    pub fn socks5_port(&self) -> u16 {
        self.socks5.port()
    }
}

#[derive(Clone)]
pub struct TunnelConfig {
    pub ssh_host: String,
    pub ssh_port: u16,
    pub ssh_username: String,
    pub ssh_password: String,
}
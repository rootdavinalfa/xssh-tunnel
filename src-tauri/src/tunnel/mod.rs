pub mod tun_device;
pub mod socks5;
pub mod packet_router;

pub use tun_device::TunDevice;
pub use socks5::Socks5Engine;
pub use packet_router::PacketRouter;

use std::sync::Arc;

use crate::error::AppError;
use crate::ssh::{SshClient, SshConfig};

pub struct Tunnel {
    ssh_client: Option<Arc<SshClient>>,
    router_handle: Option<tokio::task::JoinHandle<Result<(), AppError>>>,
    pub tun_name: Option<String>,
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
    ) -> Result<(), AppError> {
        let tun = TunDevice::from_fd(tun_fd, tun_name)
            .map_err(|e| AppError::Tunnel(e))?;

        let ssh_config = SshConfig {
            host: config.ssh_host,
            port: config.ssh_port,
            username: config.ssh_username,
            password: config.ssh_password,
        };
        let ssh_client = Arc::new(SshClient::connect(ssh_config).await?);

        let router = PacketRouter::new(ssh_client.clone());
        let router_handle = tokio::task::spawn_blocking(move || {
            router.blocking_read_loop(tun)
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
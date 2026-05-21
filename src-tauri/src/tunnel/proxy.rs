use std::os::fd::FromRawFd;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::ssh::SshClient;
use crate::error::AppError;
use crate::tunnel::TunDevice;
use crate::tunnel::ConnectionStats;

/// Bidirectional pipe between TUN device and SSH direct-tcpip channel.
pub async fn pipe_tcp(
    ssh_client: &SshClient,
    dst_ip: &str,
    dst_port: u16,
    tun_device: &TunDevice,
    stats: &std::sync::Arc<ConnectionStats>,
) -> Result<(), AppError> {
    let mut channel = ssh_client.open_tcp_channel(dst_ip, dst_port).await?;
    let (mut ch_read, mut ch_write) = tokio::io::split(channel.into_stream());

    let tun_fd = tun_device.get_fd();

    let up = async {
        let mut buf = [0u8; 4096];
        loop {
            let mut file = unsafe { std::fs::File::from_raw_fd(tun_fd) };
            let n = std::io::Read::read(&mut file, &mut buf)
                .map_err(|e| AppError::Tunnel(format!("tun read error: {}", e)))?;
            std::mem::forget(file);
            if n == 0 { break; }
            ch_write.write_all(&buf[..n]).await
                .map_err(|e| AppError::Tunnel(format!("proxy up: {}", e)))?;
            stats.add_up(n as u64);
        }
        Ok::<_, AppError>(())
    };

    let down = async {
        let mut buf = [0u8; 4096];
        loop {
            let n = ch_read.read(&mut buf).await
                .map_err(|e| AppError::Tunnel(format!("proxy down: {}", e)))?;
            if n == 0 { break; }
            let mut file = unsafe { std::fs::File::from_raw_fd(tun_fd) };
            std::io::Write::write(&mut file, &buf[..n])
                .map_err(|e| AppError::Tunnel(format!("tun write error: {}", e)))?;
            std::mem::forget(file);
            stats.add_down(n as u64);
        }
        Ok::<_, AppError>(())
    };

    tokio::select! {
        r = up => r,
        r = down => r,
    }
}
use std::net::Ipv4Addr;
use std::os::fd::FromRawFd;
use std::sync::Arc;

use crate::error::AppError;
use crate::ssh::SshClient;
use crate::tunnel::proxy;
use crate::tunnel::ConnectionStats;

/// Simplified packet router for M1.
/// Reads raw IP packets from TUN, extracts TCP connections, proxies through SSH.
/// Runs in a dedicated blocking thread to avoid stalling the async runtime.
pub struct PacketRouter {
    ssh_client: Arc<SshClient>,
    pub stats: Arc<ConnectionStats>,
    pub tun_fd: std::os::unix::io::RawFd,
}

impl PacketRouter {
    pub fn new(
        ssh_client: Arc<SshClient>,
        stats: Arc<ConnectionStats>,
        tun_fd: std::os::unix::io::RawFd,
    ) -> Self {
        PacketRouter { ssh_client, stats, tun_fd }
    }

    /// Blocking read loop — runs in a dedicated thread (spawn_blocking).
    /// Parses IP packets from TUN and spawns proxy tasks for each connection.
    pub fn blocking_read_loop(&self) -> Result<(), AppError> {
        let mut buf = vec![0u8; 65536];
        let mut tun = unsafe { std::fs::File::from_raw_fd(self.tun_fd) };
        let rt_handle = tokio::runtime::Handle::current();

        loop {
            let n = std::io::Read::read(&mut tun, &mut buf)
                .map_err(|e| AppError::Tunnel(format!("tun read error: {}", e)))?;
            if n == 0 {
                break;
            }

            let packet = buf[..n].to_vec();

            if let Some((src_ip, src_port, dst_ip, dst_port, _payload)) = Self::parse_tcp_packet(&packet) {
                // Skip our own SSH connection traffic to avoid loops
                if Self::is_ssh_traffic(dst_port) {
                    continue;
                }

                let stream_key = format!("{}:{}->{}:{}", src_ip, src_port, dst_ip, dst_port);
                tracing::info!("Proxying {} to {}:{}", stream_key, dst_ip, dst_port);

                // Spawn async proxy task
                let ssh = self.ssh_client.clone();
                let stats = self.stats.clone();
                let dst_ip_str = dst_ip.to_string();
                let tun_fd = self.tun_fd;

                rt_handle.spawn(async move {
                    let tun = unsafe { std::fs::File::from_raw_fd(tun_fd) };
                    let tun_dev = crate::tunnel::TunDevice::from_fd_raw(tun);
                    if let Err(e) = proxy::pipe_tcp(&ssh, &dst_ip_str, dst_port, &tun_dev, &stats).await {
                        tracing::error!("Proxy {} to {}:{} failed: {}", stream_key, dst_ip_str, dst_port, e);
                    }
                });
            }

            // Re-acquire the file descriptor for next iteration
            std::mem::forget(std::mem::replace(&mut tun, unsafe { std::fs::File::from_raw_fd(self.tun_fd) }));
        }

        Ok(())
    }

    fn parse_tcp_packet(packet: &[u8]) -> Option<(Ipv4Addr, u16, Ipv4Addr, u16, &[u8])> {
        // Minimum IP header = 20 bytes, TCP header = 20 bytes
        if packet.len() < 40 {
            return None;
        }

        // IP version (4) and IHL (header length in 32-bit words)
        let version_ihl = packet[0];
        let ihl = (version_ihl & 0x0F) as usize * 4;

        // Check protocol = TCP (6)
        if packet[9] != 6 {
            return None;
        }

        let src_ip = Ipv4Addr::new(packet[12], packet[13], packet[14], packet[15]);
        let dst_ip = Ipv4Addr::new(packet[16], packet[17], packet[18], packet[19]);

        let tcp_header_start = ihl;
        if packet.len() < tcp_header_start + 20 {
            return None;
        }

        let src_port = u16::from_be_bytes([packet[tcp_header_start], packet[tcp_header_start + 1]]);
        let dst_port = u16::from_be_bytes([packet[tcp_header_start + 2], packet[tcp_header_start + 3]]);
        let tcp_data_offset = ((packet[tcp_header_start + 12] >> 4) as usize) * 4;
        let payload_start = tcp_header_start + tcp_data_offset;

        Some((src_ip, src_port, dst_ip, dst_port, &packet[payload_start..]))
    }

    fn is_ssh_traffic(port: u16) -> bool {
        port == 22
    }
}
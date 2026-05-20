use std::net::Ipv4Addr;
use std::sync::Arc;

use crate::error::AppError;
use crate::ssh::SshClient;

/// Simplified packet router for M1.
/// Reads raw IP packets from TUN, extracts TCP connections, proxies through SSH.
/// Runs in a dedicated blocking thread to avoid stalling the async runtime.
pub struct PacketRouter {
    #[allow(dead_code)]
    ssh_client: Arc<SshClient>,
}

impl PacketRouter {
    pub fn new(ssh_client: Arc<SshClient>) -> Self {
        PacketRouter { ssh_client }
    }

    /// Blocking read loop — runs in a dedicated thread (spawn_blocking).
    /// Parses IP packets from TUN and logs TCP connections.
    pub fn blocking_read_loop(&self, tun_device: crate::tunnel::TunDevice) -> Result<(), AppError> {
        let mut buf = vec![0u8; 65536];

        loop {
            let n = tun_device.blocking_read(&mut buf)?;
            if n == 0 {
                break;
            }

            let packet = &buf[..n];
            if let Some((src_ip, src_port, dst_ip, dst_port, _payload)) = Self::parse_tcp_packet(packet) {
                // Skip our own SSH connection traffic to avoid loops
                if Self::is_ssh_traffic(dst_port) {
                    continue;
                }

                let stream_key = format!("{}:{}->{}:{}", src_ip, src_port, dst_ip, dst_port);

                // For M1, we log the connection. Full proxying will be implemented in M2.
                tracing::info!("Would proxy {} to {}:{}", stream_key, dst_ip, dst_port);
            }
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
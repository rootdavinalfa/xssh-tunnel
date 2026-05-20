use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

use crate::error::AppError;
use crate::ssh::SshClient;

/// SOCKS5 proxy engine that wraps TCP streams over SSH dynamic forwarding.
pub struct Socks5Engine;

impl Socks5Engine {
    /// Handle a SOCKS5 CONNECT request and proxy data through SSH.
    /// For M1, we skip full SOCKS5 handshake and directly proxy after parsing.
    pub async fn handle_stream(
        ssh_client: &SshClient,
        target_host: &str,
        target_port: u16,
        mut local_stream: TcpStream,
    ) -> Result<(), AppError> {
        // Open SSH channel to target and convert to stream
        let channel = ssh_client.open_tcp_channel(target_host, target_port).await?;
        let (mut chan_read, mut chan_write) = tokio::io::split(channel.into_stream());

        // Bidirectional copy between local_stream and SSH channel
        let (mut local_read, mut local_write) = local_stream.split();

        let client_to_remote = async {
            let mut buf = [0u8; 4096];
            loop {
                let n = local_read.read(&mut buf).await?;
                if n == 0 { break; }
                chan_write.write_all(&buf[..n]).await?;
            }
            Ok::<_, std::io::Error>(())
        };

        let remote_to_client = async {
            let mut buf = [0u8; 4096];
            loop {
                let n = chan_read.read(&mut buf).await?;
                if n == 0 { break; }
                local_write.write_all(&buf[..n]).await?;
            }
            Ok::<_, std::io::Error>(())
        };

        tokio::select! {
            res = client_to_remote => { res.map_err(|e| AppError::Tunnel(format!("Proxy error: {}", e)))?; }
            res = remote_to_client => { res.map_err(|e| AppError::Tunnel(format!("Proxy error: {}", e)))?; }
        }

        Ok(())
    }
}
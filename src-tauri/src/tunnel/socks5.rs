use std::sync::Arc;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

use crate::error::AppError;
use crate::ssh::SshClient;
use crate::tunnel::ConnectionStats;

/// SOCKS5 proxy server.
///
/// Listens on a local port and forwards connections through SSH direct-tcpip channels.
/// This is the same approach as `ssh -D` — proven, simple, and works with all apps
/// that support system proxy settings.
pub struct Socks5Proxy {
    listen_handle: Option<tokio::task::JoinHandle<()>>,
    port: u16,
}

impl Socks5Proxy {
    pub fn new() -> Self {
        Socks5Proxy {
            listen_handle: None,
            port: 0,
        }
    }

    /// Start the SOCKS5 proxy on the given port.
    /// Returns the actual port (may differ if port 0 was passed).
    pub async fn start(
        &mut self,
        ssh_client: Arc<SshClient>,
        stats: Arc<ConnectionStats>,
        port: u16,
    ) -> Result<u16, AppError> {
        let listener = TcpListener::bind(format!("127.0.0.1:{}", port))
            .await
            .map_err(|e| AppError::Tunnel(format!("SOCKS5 bind failed: {}", e)))?;

        let actual_port = listener.local_addr()
            .map_err(|e| AppError::Tunnel(format!("SOCKS5 get addr failed: {}", e)))?
            .port();

        tracing::info!("SOCKS5 proxy listening on 127.0.0.1:{}", actual_port);

        let handle = tokio::spawn(async move {
            loop {
                match listener.accept().await {
                    Ok((stream, addr)) => {
                        tracing::debug!("SOCKS5 connection from {}", addr);
                        let ssh = ssh_client.clone();
                        let stats = stats.clone();
                        tokio::spawn(async move {
                            if let Err(e) = handle_socks5(stream, &ssh, &stats).await {
                                tracing::debug!("SOCKS5 {} done: {}", addr, e);
                            }
                        });
                    }
                    Err(e) => {
                        tracing::error!("SOCKS5 accept error: {}", e);
                        break;
                    }
                }
            }
        });

        self.listen_handle = Some(handle);
        self.port = actual_port;
        Ok(actual_port)
    }

    pub fn stop(&mut self) {
        if let Some(handle) = self.listen_handle.take() {
            handle.abort();
        }
    }

    pub fn port(&self) -> u16 {
        self.port
    }
}

/// Handle a single SOCKS5 connection.
async fn handle_socks5(
    mut stream: TcpStream,
    ssh_client: &SshClient,
    stats: &Arc<ConnectionStats>,
) -> Result<(), AppError> {
    // Read SOCKS5 handshake: version, nmethods, methods
    let mut header = [0u8; 2];
    stream.read_exact(&mut header).await
        .map_err(|e| AppError::Tunnel(format!("SOCKS5 handshake read failed: {}", e)))?;

    if header[0] != 5 {
        return Err(AppError::Tunnel(format!("SOCKS5 version {} not supported", header[0])));
    }

    let nmethods = header[1] as usize;
    let mut methods = vec![0u8; nmethods];
    stream.read_exact(&mut methods).await
        .map_err(|e| AppError::Tunnel(format!("SOCKS5 methods read failed: {}", e)))?;

    // We only support no-auth (method 0)
    stream.write_all(&[5, 0]).await
        .map_err(|e| AppError::Tunnel(format!("SOCKS5 handshake write failed: {}", e)))?;

    // Read request: version, cmd, rsv, atyp, dst.addr, dst.port
    let mut request_header = [0u8; 4];
    stream.read_exact(&mut request_header).await
        .map_err(|e| AppError::Tunnel(format!("SOCKS5 request read failed: {}", e)))?;

    if request_header[0] != 5 {
        return Err(AppError::Tunnel("Bad SOCKS5 version in request".to_string()));
    }
    if request_header[1] != 1 {
        // Only support CONNECT (cmd=1)
        stream.write_all(&[5, 7, 0, 1, 0, 0, 0, 0, 0, 0]).await.ok();
        return Err(AppError::Tunnel("SOCKS5 only supports CONNECT".to_string()));
    }

    let atyp = request_header[3];
    let dst_addr = match atyp {
        1 => {
            // IPv4
            let mut addr = [0u8; 4];
            stream.read_exact(&mut addr).await
                .map_err(|e| AppError::Tunnel(format!("SOCKS5 addr read failed: {}", e)))?;
            format!("{}.{}.{}.{}", addr[0], addr[1], addr[2], addr[3])
        }
        3 => {
            // Domain name
            let mut len_buf = [0u8; 1];
            stream.read_exact(&mut len_buf).await
                .map_err(|e| AppError::Tunnel(format!("SOCKS5 domain len read failed: {}", e)))?;
            let domain_len = len_buf[0] as usize;
            let mut domain = vec![0u8; domain_len];
            stream.read_exact(&mut domain).await
                .map_err(|e| AppError::Tunnel(format!("SOCKS5 domain read failed: {}", e)))?;
            String::from_utf8_lossy(&domain).to_string()
        }
        4 => {
            // IPv6
            let mut addr = [0u8; 16];
            stream.read_exact(&mut addr).await
                .map_err(|e| AppError::Tunnel(format!("SOCKS5 addr read failed: {}", e)))?;
            let v6 = std::net::Ipv6Addr::from(addr);
            format!("{}", v6)
        }
        _ => {
            stream.write_all(&[5, 8, 0, 1, 0, 0, 0, 0, 0, 0]).await.ok();
            return Err(AppError::Tunnel(format!("SOCKS5 unknown address type: {}", atyp)));
        }
    };

    let mut port_buf = [0u8; 2];
    stream.read_exact(&mut port_buf).await
        .map_err(|e| AppError::Tunnel(format!("SOCKS5 port read failed: {}", e)))?;
    let dst_port = u16::from_be_bytes(port_buf);

    tracing::info!("SOCKS5 CONNECT {}:{}", dst_addr, dst_port);

    // Open SSH direct-tcpip channel
    let channel = ssh_client.open_tcp_channel(&dst_addr, dst_port).await?;
    let (mut ch_read, mut ch_write) = tokio::io::split(channel.into_stream());

    // Send SOCKS5 success response
    stream.write_all(&[5, 0, 0, 1, 0, 0, 0, 0, 0, 0]).await
        .map_err(|e| AppError::Tunnel(format!("SOCKS5 response write failed: {}", e)))?;

    // Bidirectional pipe
    let (mut s_read, mut s_write) = stream.split();
    let stats_clone = stats.clone();

    let up = async {
        let mut buf = [0u8; 16384];
        loop {
            let n = s_read.read(&mut buf).await
                .map_err(|e| AppError::Tunnel(format!("socks up read: {}", e)))?;
            if n == 0 { break; }
            ch_write.write_all(&buf[..n]).await
                .map_err(|e| AppError::Tunnel(format!("socks up write: {}", e)))?;
            stats_clone.add_up(n as u64);
        }
        Ok::<_, AppError>(())
    };

    let down = async {
        let mut buf = [0u8; 16384];
        loop {
            let n = ch_read.read(&mut buf).await
                .map_err(|e| AppError::Tunnel(format!("socks down read: {}", e)))?;
            if n == 0 { break; }
            s_write.write_all(&buf[..n]).await
                .map_err(|e| AppError::Tunnel(format!("socks down write: {}", e)))?;
            stats.add_down(n as u64);
        }
        Ok::<_, AppError>(())
    };

    tokio::select! {
        r = up => r,
        r = down => r,
    }
}

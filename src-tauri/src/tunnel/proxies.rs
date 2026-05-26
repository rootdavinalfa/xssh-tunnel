/// DNS proxy + transparent HTTP/HTTPS proxy.
///
/// DNS proxy: listens on UDP, forwards queries over TCP through SSH.
/// Transparent proxy: listens on TCP, detects destination from HTTP
/// Host header or TLS SNI, forwards through SSH direct-tcpip.
use std::sync::Arc;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream, UdpSocket};

use crate::error::AppError;
use crate::ssh::SshClient;
use crate::tunnel::ConnectionStats;

/// Port for the transparent TCP proxy (HTTP + HTTPS sniffing)
const TRANSPARENT_PROXY_PORT: u16 = 8080;
/// Port for the DNS proxy (UDP)
const DNS_PROXY_PORT: u16 = 5353;
/// Upstream DNS server forwarded through SSH
const UPSTREAM_DNS: &str = "8.8.8.8";
const UPSTREAM_DNS_PORT: u16 = 53;

// ── Managed proxy handles ──────────────────────────────────────────────

pub struct ProxyManager {
    dns_handle: Option<tokio::task::JoinHandle<()>>,
    http_handle: Option<tokio::task::JoinHandle<()>>,
}

impl ProxyManager {
    pub fn new() -> Self {
        ProxyManager { dns_handle: None, http_handle: None }
    }

    pub fn start(&mut self, ssh: Arc<SshClient>, stats: Arc<ConnectionStats>) {
        let s = ssh.clone();
        let st = stats.clone();
        self.dns_handle = Some(tokio::spawn(async move {
            dns_proxy_loop(s, st).await;
        }));

        let s = ssh.clone();
        let st = stats.clone();
        self.http_handle = Some(tokio::spawn(async move {
            transparent_proxy_loop(s, st).await;
        }));
    }

    pub fn stop(&mut self) {
        if let Some(h) = self.dns_handle.take() { h.abort(); }
        if let Some(h) = self.http_handle.take() { h.abort(); }
    }
}

// ── DNS Proxy ───────────────────────────────────────────────────────────

async fn dns_proxy_loop(ssh: Arc<SshClient>, _stats: Arc<ConnectionStats>) {
    let sock = match UdpSocket::bind(format!("127.0.0.1:{}", DNS_PROXY_PORT)).await {
        Ok(s) => Arc::new(s),
        Err(e) => {
            tracing::error!("DNS proxy bind failed: {}", e);
            return;
        }
    };
    tracing::info!("DNS proxy listening on 127.0.0.1:{}", DNS_PROXY_PORT);

    let mut buf = [0u8; 1500];
    loop {
        let (n, peer) = match sock.recv_from(&mut buf).await {
            Ok(v) => v,
            Err(e) => { tracing::error!("DNS recv error: {}", e); continue; }
        };
        let query = buf[..n].to_vec();
        let ssh_clone = ssh.clone();
        let sock_clone = sock.clone();

        tokio::spawn(async move {
            match ssh_clone.open_tcp_channel(UPSTREAM_DNS, UPSTREAM_DNS_PORT).await {
                Ok(ch) => {
                    let (mut r, mut w) = tokio::io::split(ch.into_stream());
                    let len = (query.len() as u16).to_be_bytes();
                    let mut tcp_query = Vec::with_capacity(2 + query.len());
                    tcp_query.extend_from_slice(&len);
                    tcp_query.extend_from_slice(&query);
                    if w.write_all(&tcp_query).await.is_err() { return; }

                    let mut len_buf = [0u8; 2];
                    if r.read_exact(&mut len_buf).await.is_err() { return; }
                    let resp_len = u16::from_be_bytes(len_buf) as usize;
                    let mut resp = vec![0u8; resp_len];
                    if r.read_exact(&mut resp).await.is_err() { return; }

                    let _ = sock_clone.send_to(&resp, peer).await;
                }
                Err(e) => tracing::warn!("DNS SSH channel failed: {}", e),
            }
        });
    }
}

// ── Transparent HTTP/HTTPS proxy ───────────────────────────────────────

async fn transparent_proxy_loop(ssh: Arc<SshClient>, _stats: Arc<ConnectionStats>) {
    let listener = match TcpListener::bind(format!("127.0.0.1:{}", TRANSPARENT_PROXY_PORT)).await {
        Ok(l) => l,
        Err(e) => {
            tracing::error!("Transparent proxy bind failed: {}", e);
            return;
        }
    };
    tracing::info!("Transparent proxy listening on 127.0.0.1:{}", TRANSPARENT_PROXY_PORT);

    loop {
        let (stream, _) = match listener.accept().await {
            Ok(v) => v,
            Err(e) => { tracing::error!("Transparent accept error: {}", e); continue; }
        };
        let ssh_clone = ssh.clone();
        tokio::spawn(async move {
            if let Err(e) = handle_transparent_conn(stream, ssh_clone).await {
                tracing::debug!("Transparent proxy: {}", e);
            }
        });
    }
}

/// Supported proxy modes
enum ProxyMode {
    /// Direct connect (transparent via pf rdr or direct connection)
    Direct { host: String, port: u16, rest_data: Vec<u8> },
    /// HTTP CONNECT method (browser CONNECT host:port)
    Connect { host: String, port: u16 },
    /// HTTP proxy with full URL (browser GET http://host/path)
    HttpProxy { host: String, port: u16, rest_data: Vec<u8> },
}

async fn detect_proxy_mode(data: &[u8]) -> Option<ProxyMode> {
    if data.len() < 5 { return None; }

    // Check for HTTP CONNECT: "CONNECT host:port HTTP/1.1"
    let s = std::str::from_utf8(data).ok()?;
    if data.starts_with(b"CONNECT ") || data.starts_with(b"connect ") {
        let rest = s.strip_prefix("CONNECT ").or_else(|| s.strip_prefix("connect "))?;
        let host_port = rest.split_whitespace().next()?;
        if let Some((host, port_str)) = host_port.rsplit_once(':') {
            if let Ok(port) = port_str.parse::<u16>() {
                return Some(ProxyMode::Connect { host: host.to_string(), port });
            }
        }
    }

    // Check for HTTP proxy URL: "GET http://host/path HTTP/1.1"
    for method in &["GET ", "POST ", "PUT ", "DELETE ", "PATCH ", "HEAD ", "OPTIONS "] {
        if data.starts_with(method.as_bytes()) {
            let rest = &s[method.len()..].trim_start();
            if let Some(url) = rest.split_whitespace().next() {
                if let Some(parsed_url) = url.strip_prefix("http://") {
                    if let Some((host, _path)) = parsed_url.split_once('/').or_else(|| parsed_url.rsplit_once(':')) {
                        let port = 80;
                        return Some(ProxyMode::HttpProxy { host: host.to_string(), port, rest_data: data.to_vec() });
                    }
                }
            }
        }
    }

    None
}

async fn handle_transparent_conn(
    mut stream: TcpStream,
    ssh: Arc<SshClient>,
) -> Result<(), AppError> {
    let mut peek_buf = [0u8; 4096];
    let n = stream.peek(&mut peek_buf).await
        .map_err(|e| AppError::Tunnel(format!("peek error: {}", e)))?;
    if n < 3 {
        return Err(AppError::Tunnel("too short".to_string()));
    }

    // Try HTTP proxy mode (CONNECT / full URL)
    if let Some(mode) = detect_proxy_mode(&peek_buf[..n]).await {
        match mode {
            ProxyMode::Connect { host, port } => {
                tracing::info!("HTTP proxy CONNECT {}:{}", host, port);
                let channel = ssh.open_tcp_channel(&host, port).await?;
                let (ch_r, ch_w) = tokio::io::split(channel.into_stream());
                stream.write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n").await
                    .map_err(|e| AppError::Tunnel(format!("proxy write: {}", e)))?;
                return pipe_ssh(stream, ch_r, ch_w).await;
            }
            ProxyMode::HttpProxy { host, port, rest_data } => {
                tracing::info!("HTTP proxy URL {}:{}", host, port);
                let channel = ssh.open_tcp_channel(&host, port).await?;
                let (mut ch_r, mut ch_w) = tokio::io::split(channel.into_stream());
                ch_w.write_all(&rest_data).await
                    .map_err(|e| AppError::Tunnel(format!("proxy write: {}", e)))?;
                return pipe_ssh(stream, ch_r, ch_w).await;
            }
            ProxyMode::Direct { .. } => {} // fall through to transparent mode
        }
    }

    // Transparent mode (Host header / TLS SNI) for pf rdr
    let (host, port, rest_data) = if peek_buf[0] == 0x16 && (peek_buf[1] == 0x03 || peek_buf[1] == 0x01) {
        match parse_tls_sni(&peek_buf[..n]) {
            Some(h) => (h, 443, vec![]),
            None => return Err(AppError::Tunnel("no SNI".to_string())),
        }
    } else if peek_buf[0].is_ascii_alphabetic() {
        match parse_http_host(&peek_buf[..n]) {
            Some(h) => (h, 80, peek_buf[..n].to_vec()),
            None => return Err(AppError::Tunnel("no Host".to_string())),
        }
    } else {
        return Err(AppError::Tunnel("unknown protocol".to_string()));
    };

    tracing::info!("Transparent proxy (pf rdr): {}:{}", host, port);
    let channel = ssh.open_tcp_channel(&host, port).await?;
    let (mut ch_r, mut ch_w) = tokio::io::split(channel.into_stream());
    if !rest_data.is_empty() {
        ch_w.write_all(&rest_data).await
            .map_err(|e| AppError::Tunnel(format!("proxy write: {}", e)))?;
    }
    pipe_ssh(stream, ch_r, ch_w).await
}

async fn pipe_ssh(
    mut stream: TcpStream,
    mut ch_r: tokio::io::ReadHalf<impl tokio::io::AsyncRead + Unpin>,
    mut ch_w: tokio::io::WriteHalf<impl tokio::io::AsyncWrite + Unpin>,
) -> Result<(), AppError> {
    let (mut s_r, mut s_w) = stream.split();

    let to_ssh = async {
        let mut buf = [0u8; 16384];
        loop {
            let n = s_r.read(&mut buf).await
                .map_err(|e| AppError::Tunnel(format!("proxy read: {}", e)))?;
            if n == 0 { break; }
            ch_w.write_all(&buf[..n]).await
                .map_err(|e| AppError::Tunnel(format!("proxy write: {}", e)))?;
        }
        Ok::<_, AppError>(())
    };

    let from_ssh = async {
        let mut buf = [0u8; 16384];
        loop {
            let n = ch_r.read(&mut buf).await
                .map_err(|e| AppError::Tunnel(format!("proxy read: {}", e)))?;
            if n == 0 { break; }
            s_w.write_all(&buf[..n]).await
                .map_err(|e| AppError::Tunnel(format!("proxy write: {}", e)))?;
        }
        Ok::<_, AppError>(())
    };

    tokio::select! {
        r = to_ssh => r,
        r = from_ssh => r,
    }
}

fn parse_http_host(data: &[u8]) -> Option<String> {
    let s = std::str::from_utf8(data).ok()?;
    for line in s.lines() {
        let l = line.trim();
        if let Some(val) = l.strip_prefix("Host:").or_else(|| l.strip_prefix("host:")) {
            return Some(val.trim().to_string());
        }
    }
    None
}

fn parse_tls_sni(data: &[u8]) -> Option<String> {
    if data.len() < 50 { return None; }
    let sid_len = data[43] as usize;
    let offset = 44 + sid_len;
    if offset + 2 > data.len() { return None; }
    let cs_len = u16::from_be_bytes([data[offset], data[offset + 1]]) as usize;
    let offset = offset + 2 + cs_len;
    if offset >= data.len() { return None; }
    let comp_len = data[offset] as usize;
    let offset = offset + 1 + comp_len;
    if offset + 2 > data.len() { return None; }
    let ext_len = u16::from_be_bytes([data[offset], data[offset + 1]]) as usize;
    let offset = offset + 2;
    let ext_end = offset + ext_len;
    let mut off = offset;

    while off + 4 <= ext_end.min(data.len()) {
        let ext_type = u16::from_be_bytes([data[off], data[off + 1]]);
        let ext_data_len = u16::from_be_bytes([data[off + 2], data[off + 3]]) as usize;
        off += 4;
        if ext_type == 0 {
            off += 2; // sni_list_len
            if off >= data.len() { return None; }
            let name_type = data[off];
            off += 1;
            if name_type == 0 {
                if off + 2 > data.len() { return None; }
                let name_len = u16::from_be_bytes([data[off], data[off + 1]]) as usize;
                off += 2;
                if off + name_len <= data.len() {
                    return String::from_utf8(data[off..off + name_len].to_vec()).ok();
                }
            }
        }
        off += ext_data_len;
    }
    None
}

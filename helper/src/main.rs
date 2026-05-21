use std::io::{Read, Write};
use std::os::unix::io::{AsRawFd, RawFd};
use std::os::unix::net::{UnixListener, UnixStream};
use std::process::Command;
use std::fs;
use serde::{Deserialize, Serialize};
use tun::AbstractDevice;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

const SOCKET_PATH: &str = "/var/run/xyz.dvnlabs.xsshtunnel.sock";
const TUN_MTU: u16 = 1500;

// Logging: debug builds show all messages, release builds show info+ only
macro_rules! log_debug {
    ($($arg:tt)*) => {
        #[cfg(debug_assertions)]
        eprintln!("[helper][debug] {}", format!($($arg)*));
    };
}
macro_rules! log_info {
    ($($arg:tt)*) => {
        eprintln!("[helper][info] {}", format!($($arg)*));
    };
}
macro_rules! log_warn {
    ($($arg:tt)*) => {
        eprintln!("[helper][warn] {}", format!($($arg)*));
    };
}
macro_rules! log_error {
    ($($arg:tt)*) => {
        eprintln!("[helper][error] {}", format!($($arg)*));
    };
}

#[derive(Deserialize)]
struct Request {
    cmd: String,
    tun_name: Option<String>,
    host_ip: Option<String>,
}

#[derive(Serialize)]
struct Response {
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<ResponseResult>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

#[derive(Serialize)]
struct ResponseResult {
    #[serde(skip_serializing_if = "Option::is_none")]
    tun_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    gateway: Option<String>,
}

fn handle_connection(mut stream: UnixStream) {
    let mut tun_device: Option<(String, RawFd)> = None;

    loop {
        let mut line = String::new();
        loop {
            let mut byte = [0u8; 1];
            if stream.read(&mut byte).unwrap_or(0) == 0 {
                // Client disconnected - cleanup
                log_info!("client disconnected");
                if let Some((ref name, _)) = tun_device {
                    log_info!("cleaning up routes for {}", name);
                    cleanup_routes(name);
                }
                return;
            }
            if byte[0] == b'\n' {
                break;
            }
            line.push(byte[0] as char);
        }

        let req: Request = match serde_json::from_str(&line) {
            Ok(r) => r,
            Err(e) => {
                let resp = Response { ok: false, result: None, error: Some(format!("Invalid JSON: {}", e)) };
                let _ = stream.write_all(format!("{}\n", serde_json::to_string(&resp).unwrap()).as_bytes());
                continue;
            }
        };

        log_debug!("received command: {}", req.cmd);
        match req.cmd.as_str() {
            "create_tun" => {
                match create_tun_device() {
                    Ok((name, fd)) => {
                        // Send fd FIRST, then JSON response
                        send_fd(&stream, fd);
                        let resp = Response {
                            ok: true,
                            result: Some(ResponseResult { tun_name: Some(name.clone()), gateway: None }),
                            error: None,
                        };
                        let resp_str = format!("{}\n", serde_json::to_string(&resp).unwrap());
                        if stream.write_all(resp_str.as_bytes()).is_err() {
                            return;
                        }
                        tun_device = Some((name, fd));
                    }
                    Err(e) => {
                        let resp = Response { ok: false, result: None, error: Some(e) };
                        let _ = stream.write_all(format!("{}\n", serde_json::to_string(&resp).unwrap()).as_bytes());
                    }
                }
            }
            "add_route" => {
                let name = req.tun_name.or_else(|| tun_device.as_ref().map(|(n, _)| n.clone()));
                if let Some(ref name) = name {
                    match inject_routes(name) {
                        Ok(()) => {
                            let resp = Response { ok: true, result: None, error: None };
                            let _ = stream.write_all(format!("{}\n", serde_json::to_string(&resp).unwrap()).as_bytes());
                        }
                        Err(e) => {
                            let resp = Response { ok: false, result: None, error: Some(e) };
                            let _ = stream.write_all(format!("{}\n", serde_json::to_string(&resp).unwrap()).as_bytes());
                        }
                    }
                } else {
                    let resp = Response { ok: false, result: None, error: Some("No TUN device".to_string()) };
                    let _ = stream.write_all(format!("{}\n", serde_json::to_string(&resp).unwrap()).as_bytes());
                }
            }
            "get_gateway" => {
                match get_default_gateway() {
                    Ok(gw) => {
                        let resp = Response {
                            ok: true,
                            result: Some(ResponseResult { tun_name: None, gateway: Some(gw) }),
                            error: None,
                        };
                        let _ = stream.write_all(format!("{}\n", serde_json::to_string(&resp).unwrap()).as_bytes());
                    }
                    Err(e) => {
                        let resp = Response { ok: false, result: None, error: Some(e) };
                        let _ = stream.write_all(format!("{}\n", serde_json::to_string(&resp).unwrap()).as_bytes());
                    }
                }
            }
            "add_host_route" => {
                let host_ip = req.host_ip;
                if let Some(ref ip) = host_ip {
                    match add_host_route(ip) {
                        Ok(()) => {
                            let resp = Response { ok: true, result: None, error: None };
                            let _ = stream.write_all(format!("{}\n", serde_json::to_string(&resp).unwrap()).as_bytes());
                        }
                        Err(e) => {
                            let resp = Response { ok: false, result: None, error: Some(e) };
                            let _ = stream.write_all(format!("{}\n", serde_json::to_string(&resp).unwrap()).as_bytes());
                        }
                    }
                } else {
                    let resp = Response { ok: false, result: None, error: Some("Missing host_ip".to_string()) };
                    let _ = stream.write_all(format!("{}\n", serde_json::to_string(&resp).unwrap()).as_bytes());
                }
            }
            "remove_host_route" => {
                let host_ip = req.host_ip;
                if let Some(ref ip) = host_ip {
                    remove_host_route(ip);
                    let resp = Response { ok: true, result: None, error: None };
                    let _ = stream.write_all(format!("{}\n", serde_json::to_string(&resp).unwrap()).as_bytes());
                } else {
                    let resp = Response { ok: false, result: None, error: Some("Missing host_ip".to_string()) };
                    let _ = stream.write_all(format!("{}\n", serde_json::to_string(&resp).unwrap()).as_bytes());
                }
            }
            "cleanup_routes" => {
                let name = req.tun_name.or_else(|| tun_device.as_ref().map(|(n, _)| n.clone()));
                if let Some(ref name) = name {
                    cleanup_routes(name);
                }
                let resp = Response { ok: true, result: None, error: None };
                let _ = stream.write_all(format!("{}\n", serde_json::to_string(&resp).unwrap()).as_bytes());
            }
            "ping" => {
                let resp = Response { ok: true, result: None, error: None };
                let _ = stream.write_all(format!("{}\n", serde_json::to_string(&resp).unwrap()).as_bytes());
            }
            "shutdown" => {
                if let Some((ref name, _)) = tun_device {
                    cleanup_routes(name);
                }
                break;
            }
            _ => {
                let resp = Response { ok: false, result: None, error: Some(format!("Unknown command: {}", req.cmd)) };
                let _ = stream.write_all(format!("{}\n", serde_json::to_string(&resp).unwrap()).as_bytes());
            }
        }
    }
}

fn create_tun_device() -> Result<(String, RawFd), String> {
    log_debug!("creating TUN device with MTU {}", TUN_MTU);
    let mut config = tun::Configuration::default();
    config.mtu(TUN_MTU).up();

    let device = tun::create(&config)
        .map_err(|e| {
            log_error!("TUN creation failed: {}", e);
            format!("Failed to create TUN device: {}", e)
        })?;

    let name = device.tun_name()
        .map_err(|e| format!("Failed to get TUN name: {}", e))?;
    let fd = device.as_raw_fd();

    std::mem::forget(device);

    log_info!("TUN device {} created", name);
    Ok((name, fd))
}

fn inject_routes(tun_name: &str) -> Result<(), String> {
    log_info!("injecting routes via {}", tun_name);

    // TUN interface needs a moment to settle and an IP address
    std::thread::sleep(std::time::Duration::from_millis(200));

    // Assign IP to the TUN interface (required before adding routes on macOS)
    let ip_config = Command::new("ifconfig")
        .args([tun_name, "10.0.0.1", "10.0.0.2", "up"])
        .output()
        .map_err(|e| format!("Failed to configure TUN IP: {}", e))?;

    if !ip_config.status.success() {
        let stderr = String::from_utf8_lossy(&ip_config.stderr);
        log_warn!("ifconfig {}: {}", tun_name, stderr.trim());
    }

    // Add split default routes through the TUN interface
    for (i, network) in ["0.0.0.0/1", "128.0.0.0/1"].iter().enumerate() {
        log_debug!("route add -net {} -interface {}", network, tun_name);
        let output = Command::new("route")
            .args(["add", "-net", network, "-interface", tun_name])
            .output()
            .map_err(|e| {
                log_error!("route add failed: {}", e);
                format!("Failed to add route: {}", e)
            })?;

        if !output.status.success() || !output.stderr.is_empty() {
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            log_error!("route add {} failed: {}", network, stderr.trim());
            if i == 0 {
                return Err(format!("Route failed ({}): {}", network, stderr.trim()));
            } else {
                let _ = Command::new("route").args(["delete", "-net", "0.0.0.0/1"]).status();
                return Err(format!("Route failed ({}): {}", network, stderr.trim()));
            }
        }
    }

    log_info!("routes injected successfully");
    Ok(())
}

fn cleanup_routes(_tun_name: &str) {
    log_info!("removing routes");
    let _ = Command::new("route")
        .args(["delete", "-net", "0.0.0.0/1"])
        .stderr(std::process::Stdio::null())
        .status();
    let _ = Command::new("route")
        .args(["delete", "-net", "128.0.0.0/1"])
        .stderr(std::process::Stdio::null())
        .status();
}

/// Get the current default gateway IP on macOS
fn get_default_gateway() -> Result<String, String> {
    // macOS: route -n get default returns the gateway
    let output = Command::new("route")
        .args(["-n", "get", "default"])
        .output()
        .map_err(|e| format!("Failed to get default route: {}", e))?;

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Parse "gateway: 192.168.1.1" from output
    for line in stdout.lines() {
        let line = line.trim();
        if line.starts_with("gateway:") {
            let gw = line.split(':').nth(1).unwrap_or("").trim();
            if !gw.is_empty() && gw != "en0" && gw != "en1" {
                log_info!("default gateway: {}", gw);
                return Ok(gw.to_string());
            }
        }
    }

    Err("Could not determine default gateway".to_string())
}

/// Add a host route so SSH traffic bypasses the TUN device
/// This must be called BEFORE inject_routes()
fn add_host_route(host_ip: &str) -> Result<(), String> {
    log_info!("adding host route for {} via default gateway", host_ip);

    // First get the default gateway
    let gateway = get_default_gateway()?;

    // Add host route: route add -host <ip> <gateway>
    let output = Command::new("route")
        .args(["add", "-host", host_ip, &gateway])
        .output()
        .map_err(|e| format!("Failed to add host route: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        // "File exists" means route already present — that's OK
        if !stderr.contains("File exists") {
            log_error!("host route add failed: {}", stderr.trim());
            return Err(format!("Host route failed: {}", stderr.trim()));
        }
    }

    log_info!("host route added: {} -> {}", host_ip, gateway);
    Ok(())
}

/// Remove the host route for the SSH server
fn remove_host_route(host_ip: &str) {
    log_info!("removing host route for {}", host_ip);
    let _ = Command::new("route")
        .args(["delete", "-host", host_ip])
        .stderr(std::process::Stdio::null())
        .status();
}

fn send_fd(stream: &UnixStream, fd: RawFd) {
    use std::os::unix::io::AsRawFd;
    use std::os::raw::c_void;

    #[repr(C)]
    struct Cmsghdr {
        cmsg_len: u32,
        cmsg_level: i32,
        cmsg_type: i32,
    }

    let raw_fd = stream.as_raw_fd();
    let mut buf = [0u8; 1];
    let mut iov = libc::iovec {
        iov_base: buf.as_mut_ptr() as *mut c_void,
        iov_len: buf.len(),
    };

    let cmsg_size = unsafe { libc::CMSG_SPACE(std::mem::size_of::<RawFd>() as u32) as usize };
    let mut cmsg_space = vec![0u8; cmsg_size];

    unsafe {
        let cmsg = cmsg_space.as_mut_ptr() as *mut Cmsghdr;
        (*cmsg).cmsg_len = libc::CMSG_LEN(std::mem::size_of::<RawFd>() as u32) as u32;
        (*cmsg).cmsg_level = libc::SOL_SOCKET;
        (*cmsg).cmsg_type = libc::SCM_RIGHTS;
        *(libc::CMSG_DATA(cmsg as *mut libc::cmsghdr) as *mut RawFd) = fd;
    }

    let mut msg = libc::msghdr {
        msg_name: std::ptr::null_mut(),
        msg_namelen: 0,
        msg_iov: &mut iov,
        msg_iovlen: 1,
        msg_control: cmsg_space.as_mut_ptr() as *mut c_void,
        msg_controllen: cmsg_size as u32,
        msg_flags: 0,
    };

    unsafe {
        let sent = libc::sendmsg(raw_fd, &msg, 0);
        if sent >= 0 {
            log_debug!("TUN fd {} transferred to client", fd);
        } else {
            let errno = unsafe { *libc::__error() };
            log_error!("failed to send fd to client (errno: {})", errno);
        }
    }
}

fn main() {
    log_info!("daemon starting");
    let _ = fs::remove_file(SOCKET_PATH);
    log_debug!("stale socket removed");

    let listener = UnixListener::bind(SOCKET_PATH)
        .expect("Failed to bind socket");
    log_info!("listening on {}", SOCKET_PATH);

    let _ = fs::set_permissions(SOCKET_PATH, fs::Permissions::from_mode(0o777));
    log_debug!("socket permissions set to 0777");

    log_info!("waiting for client connection...");
    if let Ok((stream, _)) = listener.accept() {
        log_info!("client connected");
        handle_connection(stream);
    } else {
        log_error!("failed to accept connection");
    }

    let _ = fs::remove_file(SOCKET_PATH);
    log_info!("daemon exiting");
}
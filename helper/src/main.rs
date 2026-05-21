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

#[derive(Deserialize)]
struct Request {
    cmd: String,
    tun_name: Option<String>,
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
}

fn handle_connection(mut stream: UnixStream) {
    let mut tun_device: Option<(String, RawFd)> = None;

    loop {
        let mut line = String::new();
        loop {
            let mut byte = [0u8; 1];
            if stream.read(&mut byte).unwrap_or(0) == 0 {
                // Client disconnected - cleanup
                if let Some((ref name, _)) = tun_device {
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

        match req.cmd.as_str() {
            "create_tun" => {
                match create_tun_device() {
                    Ok((name, fd)) => {
                        let resp = Response {
                            ok: true,
                            result: Some(ResponseResult { tun_name: Some(name.clone()) }),
                            error: None,
                        };
                        let resp_str = format!("{}\n", serde_json::to_string(&resp).unwrap());
                        if stream.write_all(resp_str.as_bytes()).is_err() {
                            return;
                        }
                        send_fd(&stream, fd);
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
    let mut config = tun::Configuration::default();
    config.mtu(TUN_MTU).up();

    let device = tun::create(&config)
        .map_err(|e| format!("Failed to create TUN device: {}", e))?;

    let name = device.tun_name()
        .map_err(|e| format!("Failed to get TUN name: {}", e))?;
    let fd = device.as_raw_fd();

    std::mem::forget(device);

    Ok((name, fd))
}

fn inject_routes(tun_name: &str) -> Result<(), String> {
    let status = Command::new("route")
        .args(["add", "-net", "0.0.0.0/1", "-interface", tun_name])
        .status()
        .map_err(|e| format!("Failed to add route: {}", e))?;

    if !status.success() {
        return Err("Failed to add default route (0.0.0.0/1)".to_string());
    }

    let status = Command::new("route")
        .args(["add", "-net", "128.0.0.0/1", "-interface", tun_name])
        .status()
        .map_err(|e| format!("Failed to add route: {}", e))?;

    if !status.success() {
        let _ = Command::new("route").args(["delete", "-net", "0.0.0.0/1"]).status();
        return Err("Failed to add default route (128.0.0.0/1)".to_string());
    }

    Ok(())
}

fn cleanup_routes(_tun_name: &str) {
    let _ = Command::new("route").args(["delete", "-net", "0.0.0.0/1"]).status();
    let _ = Command::new("route").args(["delete", "-net", "128.0.0.0/1"]).status();
}

fn send_fd(stream: &UnixStream, fd: RawFd) {
    use std::os::unix::io::AsRawFd;
    use std::os::raw::c_void;

    #[repr(C)]
    struct Cmsghdr {
        cmsg_len: usize,
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
        (*cmsg).cmsg_len = libc::CMSG_LEN(std::mem::size_of::<RawFd>() as u32) as usize;
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
        libc::sendmsg(raw_fd, &msg, 0);
    }
}

fn main() {
    let _ = fs::remove_file(SOCKET_PATH);

    let listener = UnixListener::bind(SOCKET_PATH)
        .expect("Failed to bind socket");

    let _ = fs::set_permissions(SOCKET_PATH, fs::Permissions::from_mode(0o777));

    if let Ok((stream, _)) = listener.accept() {
        handle_connection(stream);
    }

    let _ = fs::remove_file(SOCKET_PATH);
}
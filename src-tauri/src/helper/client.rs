use std::io::{Read, Write};
use std::os::unix::io::{RawFd};
use std::os::unix::net::UnixStream;
use std::time::Duration;

use crate::error::AppError;

const SOCKET_PATH: &str = "/var/run/xyz.dvnlabs.xsshtunnel.sock";

pub struct HelperClient {
    stream: UnixStream,
}

impl HelperClient {
    pub fn connect() -> Result<Self, AppError> {
        let stream = UnixStream::connect(SOCKET_PATH)
            .map_err(|e| AppError::Tunnel(format!("Failed to connect to helper: {}", e)))?;
        stream.set_read_timeout(Some(Duration::from_secs(10)))
            .map_err(|e| AppError::Tunnel(format!("Failed to set read timeout: {}", e)))?;
        Ok(HelperClient { stream })
    }

    pub fn create_tun(&mut self) -> Result<(String, RawFd), AppError> {
        self.send_command(r#"{"cmd":"create_tun"}"#)?;
        // Receive fd BEFORE reading JSON (helper sends fd first)
        let fd = self.recv_fd()?;
        let response = self.read_response()?;

        if response.get("ok").and_then(|v| v.as_bool()) != Some(true) {
            let err = response.get("error").and_then(|v| v.as_str()).unwrap_or("unknown error");
            return Err(AppError::Tunnel(format!("Helper create_tun failed: {}", err)));
        }

        let tun_name = response["result"]["tun_name"].as_str()
            .ok_or_else(|| AppError::Tunnel("Missing tun_name in response".to_string()))?
            .to_string();

        Ok((tun_name, fd))
    }

    pub fn add_route(&mut self, tun_name: &str) -> Result<(), AppError> {
        let cmd = format!(r#"{{"cmd":"add_route","tun_name":"{}"}}"#, tun_name);
        self.send_command(&cmd)?;
        let response = self.read_response()?;
        if response.get("ok").and_then(|v| v.as_bool()) != Some(true) {
            let err = response.get("error").and_then(|v| v.as_str()).unwrap_or("unknown error");
            return Err(AppError::Tunnel(format!("Helper add_route failed: {}", err)));
        }
        Ok(())
    }

    pub fn get_gateway(&mut self) -> Result<String, AppError> {
        self.send_command(r#"{"cmd":"get_gateway"}"#)?;
        let response = self.read_response()?;
        if response.get("ok").and_then(|v| v.as_bool()) != Some(true) {
            let err = response.get("error").and_then(|v| v.as_str()).unwrap_or("unknown error");
            return Err(AppError::Tunnel(format!("Helper get_gateway failed: {}", err)));
        }
        response["result"]["gateway"].as_str()
            .map(|s| s.to_string())
            .ok_or_else(|| AppError::Tunnel("Missing gateway in response".to_string()))
    }

    pub fn add_host_route(&mut self, host_ip: &str) -> Result<(), AppError> {
        let cmd = format!(r#"{{"cmd":"add_host_route","host_ip":"{}"}}"#, host_ip);
        self.send_command(&cmd)?;
        let response = self.read_response()?;
        if response.get("ok").and_then(|v| v.as_bool()) != Some(true) {
            let err = response.get("error").and_then(|v| v.as_str()).unwrap_or("unknown error");
            return Err(AppError::Tunnel(format!("Helper add_host_route failed: {}", err)));
        }
        Ok(())
    }

    pub fn remove_host_route(&mut self, host_ip: &str) -> Result<(), AppError> {
        let cmd = format!(r#"{{"cmd":"remove_host_route","host_ip":"{}"}}"#, host_ip);
        self.send_command(&cmd)?;
        let response = self.read_response()?;
        if response.get("ok").and_then(|v| v.as_bool()) != Some(true) {
            let err = response.get("error").and_then(|v| v.as_str()).unwrap_or("unknown error");
            return Err(AppError::Tunnel(format!("Helper remove_host_route failed: {}", err)));
        }
        Ok(())
    }

    pub fn cleanup_routes(&mut self, tun_name: &str) -> Result<(), AppError> {
        let cmd = format!(r#"{{"cmd":"cleanup_routes","tun_name":"{}"}}"#, tun_name);
        self.send_command(&cmd)?;
        let response = self.read_response()?;
        if response.get("ok").and_then(|v| v.as_bool()) != Some(true) {
            let err = response.get("error").and_then(|v| v.as_str()).unwrap_or("unknown error");
            return Err(AppError::Tunnel(format!("Helper cleanup_routes failed: {}", err)));
        }
        Ok(())
    }

    pub fn send_ping(&mut self) -> Result<(), AppError> {
        self.send_command(r#"{"cmd":"ping"}"#)?;
        let response = self.read_response()?;
        if response.get("ok").and_then(|v| v.as_bool()) != Some(true) {
            return Err(AppError::Tunnel("ping failed".to_string()));
        }
        Ok(())
    }

    pub fn set_socks_proxy(&mut self, port: u16) -> Result<(), AppError> {
        let cmd = format!(r#"{{"cmd":"set_socks_proxy","socks_port":{}}}"#, port);
        self.send_command(&cmd)?;
        let response = self.read_response()?;
        if response.get("ok").and_then(|v| v.as_bool()) != Some(true) {
            let err = response.get("error").and_then(|v| v.as_str()).unwrap_or("unknown error");
            return Err(AppError::Tunnel(format!("Helper set_socks_proxy failed: {}", err)));
        }
        Ok(())
    }

    pub fn clear_socks_proxy(&mut self) -> Result<(), AppError> {
        self.send_command(r#"{"cmd":"clear_socks_proxy"}"#)?;
        let response = self.read_response()?;
        if response.get("ok").and_then(|v| v.as_bool()) != Some(true) {
            let err = response.get("error").and_then(|v| v.as_str()).unwrap_or("unknown error");
            return Err(AppError::Tunnel(format!("Helper clear_socks_proxy failed: {}", err)));
        }
        Ok(())
    }

    /// Full proxy setup: save DNS, set local DNS, pf rules, CLI env, SOCKS
    pub fn setup_proxies(&mut self, socks_port: u16) -> Result<(), AppError> {
        let cmd = format!(r#"{{"cmd":"setup_proxies","socks_port":{}}}"#, socks_port);
        self.send_command(&cmd)?;
        let response = self.read_response()?;
        if response.get("ok").and_then(|v| v.as_bool()) != Some(true) {
            let err = response.get("error").and_then(|v| v.as_str()).unwrap_or("unknown error");
            return Err(AppError::Tunnel(format!("Helper setup_proxies failed: {}", err)));
        }
        Ok(())
    }

    /// Full proxy teardown: pf rules, DNS restore, CLI env, SOCKS
    pub fn teardown_proxies(&mut self) -> Result<(), AppError> {
        self.send_command(r#"{"cmd":"teardown_proxies"}"#)?;
        let response = self.read_response()?;
        if response.get("ok").and_then(|v| v.as_bool()) != Some(true) {
            let err = response.get("error").and_then(|v| v.as_str()).unwrap_or("unknown error");
            return Err(AppError::Tunnel(format!("Helper teardown_proxies failed: {}", err)));
        }
        Ok(())
    }

    fn send_command(&mut self, cmd: &str) -> Result<(), AppError> {
        let msg = format!("{}\n", cmd);
        self.stream.write_all(msg.as_bytes())
            .map_err(|e| AppError::Tunnel(format!("Failed to send command to helper: {}", e)))?;
        Ok(())
    }

    fn read_response(&mut self) -> Result<serde_json::Value, AppError> {
        let mut line = String::new();
        loop {
            let mut byte = [0u8; 1];
            if self.stream.read(&mut byte).map_err(|e| AppError::Tunnel(format!("Failed to read helper response: {}", e)))? == 0 {
                return Err(AppError::Tunnel("Helper disconnected".to_string()));
            }
            if byte[0] == b'\n' {
                break;
            }
            line.push(byte[0] as char);
        }
        serde_json::from_str(&line)
            .map_err(|e| AppError::Tunnel(format!("Invalid helper response: {}", e)))
    }

    fn recv_fd(&mut self) -> Result<RawFd, AppError> {
        use std::os::unix::io::AsRawFd;

        let mut buf = [0u8; 1];
        let mut iov = libc::iovec {
            iov_base: buf.as_mut_ptr() as *mut _,
            iov_len: buf.len(),
        };

        let cmsg_size = unsafe { libc::CMSG_SPACE(std::mem::size_of::<RawFd>() as u32) as usize };
        let mut cmsg_space = vec![0u8; cmsg_size];

        let mut msg = libc::msghdr {
            msg_name: std::ptr::null_mut(),
            msg_namelen: 0,
            msg_iov: &mut iov,
            msg_iovlen: 1,
            msg_control: cmsg_space.as_mut_ptr() as *mut _,
            msg_controllen: cmsg_size as u32,
            msg_flags: 0,
        };

        let ret = unsafe { libc::recvmsg(self.stream.as_raw_fd(), &mut msg, 0) };
        if ret < 0 {
            return Err(AppError::Tunnel("Failed to receive fd from helper".to_string()));
        }

        let cmsg_ptr = unsafe { libc::CMSG_FIRSTHDR(&msg) };
        if cmsg_ptr.is_null() {
            return Err(AppError::Tunnel(
                "No ancillary data received from helper. Restart the helper daemon: sudo killall xssh-tunnel-helper".to_string()
            ));
        }

        let fd = unsafe { *(libc::CMSG_DATA(cmsg_ptr) as *const RawFd) };
        Ok(fd)
    }
}

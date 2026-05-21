use std::io::{Read, Write};
use std::os::unix::io::{RawFd};
use std::os::unix::net::UnixStream;

use crate::error::AppError;

const SOCKET_PATH: &str = "/var/run/xyz.dvnlabs.xsshtunnel.sock";

pub struct HelperClient {
    stream: UnixStream,
}

impl HelperClient {
    pub fn connect() -> Result<Self, AppError> {
        let stream = UnixStream::connect(SOCKET_PATH)
            .map_err(|e| AppError::Tunnel(format!("Failed to connect to helper: {}", e)))?;
        Ok(HelperClient { stream })
    }

    pub fn create_tun(&mut self) -> Result<(String, RawFd), AppError> {
        self.send_command(r#"{"cmd":"create_tun"}"#)?;
        let response = self.read_response()?;

        if response.get("ok").and_then(|v| v.as_bool()) != Some(true) {
            let err = response.get("error").and_then(|v| v.as_str()).unwrap_or("unknown error");
            return Err(AppError::Tunnel(format!("Helper create_tun failed: {}", err)));
        }

        let tun_name = response["result"]["tun_name"].as_str()
            .ok_or_else(|| AppError::Tunnel("Missing tun_name in response".to_string()))?
            .to_string();

        let fd = self.recv_fd()?;
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
            return Err(AppError::Tunnel("No ancillary data received from helper".to_string()));
        }

        let fd = unsafe { *(libc::CMSG_DATA(cmsg_ptr) as *const RawFd) };
        Ok(fd)
    }
}

use std::os::unix::io::{AsRawFd, RawFd};
use tun::{AbstractDevice, Configuration};

use crate::error::AppError;

const TUN_MTU: u16 = 1500;

pub struct TunDevice {
    pub name: String,
    fd: RawFd,
}

impl TunDevice {
    pub fn create() -> Result<Self, AppError> {
        let mut config = Configuration::default();
        config
            .mtu(TUN_MTU)
            .up();

        let device = tun::create(&config)
            .map_err(|e| {
                let msg = format!("{}", e);
                if msg.contains("Operation not permitted") || msg.contains("os error 1") {
                    AppError::Tunnel(format!(
                        "Failed to create TUN device: {}. \
                        On macOS, creating TUN devices requires root privileges. \
                        For development, run: sudo npm run tauri dev",
                        e
                    ))
                } else {
                    AppError::Tunnel(format!("Failed to create TUN device: {}", e))
                }
            })?;

        let name = device.tun_name()
            .map_err(|e| AppError::Tunnel(format!("Failed to get TUN name: {}", e)))?;
        let fd = device.as_raw_fd();

        // Keep device alive by leaking it (we manage via fd)
        // In production, use a proper wrapper that owns the device
        std::mem::forget(device);

        Ok(TunDevice { name, fd })
    }

    pub fn get_fd(&self) -> RawFd {
        self.fd
    }

    /// Blocking read — use ONLY from spawn_blocking threads
    pub fn blocking_read(&self, buf: &mut [u8]) -> Result<usize, AppError> {
        use std::os::unix::io::FromRawFd;
        use std::io::Read;
        let mut file = unsafe { std::fs::File::from_raw_fd(self.fd) };
        let result = file.read(buf)
            .map_err(|e| AppError::Tunnel(format!("TUN read error: {}", e)))?;
        std::mem::forget(file); // Don't close the fd
        Ok(result)
    }

    /// Blocking write — use ONLY from spawn_blocking threads
    pub fn blocking_write(&self, buf: &[u8]) -> Result<usize, AppError> {
        use std::os::unix::io::FromRawFd;
        use std::io::Write;
        let mut file = unsafe { std::fs::File::from_raw_fd(self.fd) };
        let result = file.write(buf)
            .map_err(|e| AppError::Tunnel(format!("TUN write error: {}", e)))?;
        std::mem::forget(file); // Don't close the fd
        Ok(result)
    }
}
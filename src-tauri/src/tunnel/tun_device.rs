use std::os::unix::io::{AsRawFd, RawFd};
use std::io::{Read, Write};

pub struct TunDevice {
    pub name: String,
    fd: RawFd,
}

impl TunDevice {
    /// Wrap an existing fd received from the privileged helper
    pub fn from_fd(fd: RawFd, name: &str) -> Result<Self, String> {
        Ok(TunDevice { name: name.to_string(), fd })
    }

    /// Wrap an owned File (consumes it)
    pub fn from_fd_raw(file: std::fs::File) -> Self {
        let fd = file.as_raw_fd();
        TunDevice { name: String::new(), fd }
    }

    pub fn get_fd(&self) -> RawFd {
        self.fd
    }

    pub fn blocking_read(&self, buf: &mut [u8]) -> Result<usize, String> {
        use std::os::unix::io::FromRawFd;
        let mut file = unsafe { std::fs::File::from_raw_fd(self.fd) };
        let result = file.read(buf)
            .map_err(|e| format!("TUN read error: {}", e))?;
        std::mem::forget(file);
        Ok(result)
    }

    pub fn blocking_write(&self, buf: &[u8]) -> Result<usize, String> {
        use std::os::unix::io::FromRawFd;
        let mut file = unsafe { std::fs::File::from_raw_fd(self.fd) };
        let result = file.write(buf)
            .map_err(|e| format!("TUN write error: {}", e))?;
        std::mem::forget(file);
        Ok(result)
    }
}
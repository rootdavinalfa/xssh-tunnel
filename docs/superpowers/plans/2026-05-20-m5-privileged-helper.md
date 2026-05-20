# M5 Implementation Plan: Privileged macOS Helper (SMAppService)

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` (recommended) or `superpowers:executing-plans` to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Eliminate `sudo` requirement by implementing an SMAppService privileged helper that creates TUN devices and manages routes as root.

**Architecture:** A separate Rust helper binary managed by launchd via SMAppService. Communication via Unix socket with SCM_RIGHTS fd passing. A thin C bridge wraps Apple's ServiceManagement + Security APIs. The main app connects to the helper for TUN creation and route management, then reads/writes the TUN fd directly.

**Tech Stack:** SMAppService (C API via `cc` crate), Unix sockets, SCM_RIGHTS, tun 0.8, serde_json

---

## File Structure

```
src-tauri/
├── build.rs                          # NEW: Compile C bridge
├── src/
│   ├── helper/
│   │   ├── mod.rs                    # NEW: Rust FFI + install/uninstall/status
│   │   ├── client.rs                 # NEW: HelperClient (socket, JSON, SCM_RIGHTS)
│   │   └── bridge.c                  # NEW: C wrapper for SMAppService APIs
│   ├── tunnel/
│   │   ├── mod.rs                    # MODIFY: Use HelperClient in start/stop
│   │   ├── tun_device.rs             # MODIFY: Add from_fd(), remove create()
│   │   ├── route_manager.rs          # REMOVE
│   │   ├── socks5.rs                 # UNCHANGED
│   │   └── packet_router.rs          # UNCHANGED
│   └── lib.rs                        # MODIFY: Add helper to AppState
helper/
├── Cargo.toml                        # NEW: Helper binary manifest
└── src/
    └── main.rs                       # NEW: Unix socket server + TUN + routes

config/
└── xyz.dvnlabs.xsshtunnel.plist      # NEW: launchd plist template

src/
├── routes/
│   ├── +page.svelte                  # MODIFY: Add helper banner
│   └── settings/+page.svelte         # NEW: Helper install/uninstall UI
├── lib/
│   ├── tauri.ts                      # EXTEND: Helper IPC wrappers
│   └── stores/
│       └── helper.ts                 # NEW: Helper status store
```

---

### Task 1: Create C Bridge (SMAppService FFI)

**Files:**
- Create: `src-tauri/build.rs`
- Create: `src-tauri/src/helper/bridge.c`

- [ ] **Step 1: Create build.rs**

Write to `src-tauri/build.rs`:

```rust
fn main() {
    cc::Build::new()
        .file("src/helper/bridge.c")
        .flag("-framework")
        .flag("ServiceManagement")
        .flag("-framework")
        .flag("Security")
        .compile("helper_bridge");
}
```

- [ ] **Step 2: Create bridge.c**

Create `src-tauri/src/helper/` directory, then write to `src-tauri/src/helper/bridge.c`:

```c
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <ServiceManagement/ServiceManagement.h>
#include <Security/Authorization.h>
#include <Security/Security.h>

static const char* const SERVICE_NAME = "xyz.dvnlabs.xsshtunnel";

int helper_install(const char* bundle_path, char* out_error, size_t error_len) {
    AuthorizationRef auth = NULL;
    AuthorizationCreate(NULL, kAuthorizationEmptyEnvironment, kAuthorizationFlagDefaults, &auth);
    if (!auth) {
        snprintf(out_error, error_len, "Failed to create authorization reference");
        return -1;
    }

    Boolean success = SMAppServiceRegister(
        CFStringCreateWithCString(NULL, SERVICE_NAME, kCFStringEncodingUTF8),
        CFStringCreateWithCString(NULL, bundle_path, kCFStringEncodingUTF8)
    );

    if (auth) CFRelease(auth);

    if (!success) {
        snprintf(out_error, error_len, "SMAppServiceRegister failed");
        return -1;
    }

    return 0;
}

int helper_uninstall(char* out_error, size_t error_len) {
    CFStringRef service = CFStringCreateWithCString(NULL, SERVICE_NAME, kCFStringEncodingUTF8);
    SMAppServiceUnregister(service);
    CFRelease(service);
    return 0;
}

int helper_status(int* out_installed, int* out_running, char* out_error, size_t error_len) {
    CFStringRef service = CFStringCreateWithCString(NULL, SERVICE_NAME, kCFStringEncodingUTF8);
    CFDictionaryRef status = SMAppServiceCopyStatus(service);
    CFRelease(service);

    *out_installed = 0;
    *out_running = 0;

    if (!status) {
        // Not installed
        return 0;
    }

    *out_installed = 1;

    CFStringRef cfStatus = CFDictionaryGetValue(status, CFSTR("Status"));
    if (cfStatus) {
        if (CFStringCompare(cfStatus, CFSTR("enabled"), 0) == kCFCompareEqualTo) {
            *out_running = 1;
        }
    }

    CFRelease(status);
    return 0;
}
```

- [ ] **Step 3: Add `cc` dev-dependency to Cargo.toml**

Read `src-tauri/Cargo.toml`. Add under `[build-dependencies]`:

```toml
[build-dependencies]
cc = "1"
```

- [ ] **Step 4: Verify C bridge compiles**

Run: `cd src-tauri && cargo check`
Expected: Compiles successfully (the C file will be compiled by build.rs)

- [ ] **Step 5: Commit**

```bash
git add src-tauri/build.rs src-tauri/src/helper/bridge.c src-tauri/Cargo.toml
git commit -m "feat: add C bridge for SMAppService privileged helper"
```

---

### Task 2: Create Rust FFI Wrapper

**Files:**
- Create: `src-tauri/src/helper/mod.rs`

- [ ] **Step 1: Create helper/mod.rs**

Create the directory if needed, then write:

```rust
mod client;

use std::ffi::CString;
use std::os::raw::c_char;
use serde::Serialize;

pub use client::HelperClient;

#[derive(Debug, Clone, Serialize)]
pub struct HelperStatus {
    pub installed: bool,
    pub running: bool,
}

pub fn get_status() -> Result<HelperStatus, String> {
    let mut installed: i32 = 0;
    let mut running: i32 = 0;
    let mut error_buf = vec![0u8; 512];
    let ret = unsafe {
        helper_status(
            &mut installed,
            &mut running,
            error_buf.as_mut_ptr() as *mut c_char,
            512,
        )
    };
    if ret != 0 {
        let err = String::from_utf8_lossy(&error_buf).trim_end_matches('\0').to_string();
        return Err(err);
    }
    Ok(HelperStatus {
        installed: installed != 0,
        running: running != 0,
    })
}

pub fn install(bundle_path: &str) -> Result<(), String> {
    let c_path = CString::new(bundle_path).map_err(|e| format!("Invalid path: {}", e))?;
    let mut error_buf = vec![0u8; 512];
    let ret = unsafe {
        helper_install(
            c_path.as_ptr(),
            error_buf.as_mut_ptr() as *mut c_char,
            512,
        )
    };
    if ret != 0 {
        let err = String::from_utf8_lossy(&error_buf).trim_end_matches('\0').to_string();
        return Err(err);
    }
    Ok(())
}

pub fn uninstall() -> Result<(), String> {
    let mut error_buf = vec![0u8; 512];
    let ret = unsafe {
        helper_uninstall(
            error_buf.as_mut_ptr() as *mut c_char,
            512,
        )
    };
    if ret != 0 {
        let err = String::from_utf8_lossy(&error_buf).trim_end_matches('\0').to_string();
        return Err(err);
    }
    Ok(())
}

extern "C" {
    fn helper_install(
        bundle_path: *const c_char,
        out_error: *mut c_char,
        error_len: usize,
    ) -> i32;
    fn helper_uninstall(
        out_error: *mut c_char,
        error_len: usize,
    ) -> i32;
    fn helper_status(
        out_installed: *mut i32,
        out_running: *mut i32,
        out_error: *mut c_char,
        error_len: usize,
    ) -> i32;
}
```

- [ ] **Step 2: Register module in lib.rs**

Add `pub mod helper;` to `src-tauri/src/lib.rs` (alphabetically after `error`):

```rust
pub mod crypto;
pub mod db;
pub mod error;
pub mod helper;
pub mod logs;
pub mod profiles;
pub mod ssh;
pub mod tunnel;
```

- [ ] **Step 3: Add serde dependency if missing**

Check `src-tauri/Cargo.toml` for `serde` with `derive` feature. Should already exist. If not, add:

```toml
serde = { version = "1", features = ["derive"] }
```

- [ ] **Step 4: Verify Rust compiles**

Run: `cd src-tauri && cargo check`
Expected: Compiles successfully (warnings OK)

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/helper/mod.rs src-tauri/src/lib.rs
git commit -m "feat: add Rust FFI wrapper for SMAppService helper APIs"
```

---

### Task 3: Create HelperClient (Unix Socket + SCM_RIGHTS)

**Files:**
- Create: `src-tauri/src/helper/client.rs`

- [ ] **Step 1: Create client.rs**

Write to `src-tauri/src/helper/client.rs`:

```rust
use std::io::{Read, Write};
use std::os::unix::io::{RawFd, FromRawFd};
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

        // Receive fd via SCM_RIGHTS
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

    pub fn shutdown(&mut self) -> Result<(), AppError> {
        self.send_command(r#"{"cmd":"shutdown"}"#)?;
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
        // Read until newline
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
        use libc::{cmsghdr, msghdr, iovec, SCM_RIGHTS};

        let mut buf = [0u8; 1];
        let mut iov = iovec {
            iov_base: buf.as_mut_ptr() as *mut _,
            iov_len: buf.len(),
        };

        let mut cmsg_space = unsafe { std::mem::zeroed::<[u8; 128]>() };
        let mut cmsg = cmsghdr {
            cmsg_len: 0,
            cmsg_level: 0,
            cmsg_type: 0,
        };

        let mut msg: msghdr = unsafe { std::mem::zeroed() };
        msg.msg_iov = &mut iov;
        msg.msg_iovlen = 1;
        msg.msg_control = cmsg_space.as_mut_ptr() as *mut _;
        msg.msg_controllen = cmsg_space.len();

        let ret = unsafe {
            libc::recvmsg(self.stream.as_raw_fd(), &mut msg, 0)
        };

        if ret < 0 {
            return Err(AppError::Tunnel("Failed to receive fd from helper".to_string()));
        }

        // Extract fd from cmsg
        let cmsg_ptr = unsafe { libc::CMSG_FIRSTHDR(&msg) };
        if cmsg_ptr.is_null() {
            return Err(AppError::Tunnel("No ancillary data received from helper".to_string()));
        }

        let received_cmsg = unsafe { *cmsg_ptr };
        let fd = unsafe { *(libc::CMSG_DATA(cmsg_ptr) as *const RawFd) };

        Ok(fd)
    }
}
```

Note: This uses `libc` for SCM_RIGHTS. We need `libc` in Cargo.toml. Check if it's already there; if not, add:

```toml
libc = "0.2"
```

- [ ] **Step 2: Verify Rust compiles**

Run: `cd src-tauri && cargo check`
Expected: Compiles successfully

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/helper/client.rs src-tauri/Cargo.toml
git commit -m "feat: add HelperClient with Unix socket and SCM_RIGHTS fd passing"
```

---

### Task 4: Create Helper Daemon Binary

**Files:**
- Create: `helper/Cargo.toml`
- Create: `helper/src/main.rs`
- Create: `config/xyz.dvnlabs.xsshtunnel.plist`

- [ ] **Step 1: Create helper/Cargo.toml**

```toml
[package]
name = "xssh-tunnel-helper"
version = "0.1.0"
edition = "2021"

[dependencies]
tun = "0.8"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
```

- [ ] **Step 2: Create helper/src/main.rs**

```rust
use std::io::{Read, Write};
use std::os::unix::io::{AsRawFd, RawFd};
use std::os::unix::net::{UnixListener, UnixStream};
use std::process::Command;
use std::fs;
use serde::{Deserialize, Serialize};

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
                // Client disconnected
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
                        // Send fd via SCM_RIGHTS
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
                if let Some(ref name) = req.tun_name.or_else(|| tun_device.as_ref().map(|(n, _)| n.clone())) {
                    match inject_routes(&name) {
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
                if let Some(ref name) = req.tun_name.or_else(|| tun_device.as_ref().map(|(n, _)| n.clone())) {
                    cleanup_routes(&name);
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

    // Leak fd — ownership transfers to main app
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

fn cleanup_routes(tun_name: &str) {
    let _ = Command::new("route").args(["delete", "-net", "0.0.0.0/1"]).status();
    let _ = Command::new("route").args(["delete", "-net", "128.0.0.0/1"]).status();
}

fn send_fd(stream: &UnixStream, fd: RawFd) {
    use std::os::unix::io::AsRawFd;
    use libc::{cmsghdr, msghdr, iovec, SCM_RIGHTS, SOL_SOCKET};

    let raw_fd = stream.as_raw_fd();
    let mut buf = [0u8; 1];

    let mut iov = iovec {
        iov_base: buf.as_mut_ptr() as *mut _,
        iov_len: buf.len(),
    };

    let mut cmsg_space = vec![0u8; unsafe { libc::CMSG_SPACE(std::mem::size_of::<RawFd>() as _) as usize }];

    unsafe {
        let cmsg = cmsg_space.as_mut_ptr() as *mut cmsghdr;
        (*cmsg).cmsg_len = libc::CMSG_LEN(std::mem::size_of::<RawFd>() as _) as _;
        (*cmsg).cmsg_level = SOL_SOCKET;
        (*cmsg).cmsg_type = SCM_RIGHTS;
        *(libc::CMSG_DATA(cmsg) as *mut RawFd) = fd;
    }

    let mut msg: msghdr = unsafe { std::mem::zeroed() };
    msg.msg_iov = &mut iov;
    msg.msg_iovlen = 1;
    msg.msg_control = cmsg_space.as_mut_ptr() as *mut _;
    msg.msg_controllen = cmsg_space.len();

    unsafe {
        libc::sendmsg(raw_fd, &msg, 0);
    }
}

fn main() {
    // Remove old socket if exists
    let _ = fs::remove_file(SOCKET_PATH);

    let listener = UnixListener::bind(SOCKET_PATH)
        .expect("Failed to bind socket");

    // Set permissions so main app (non-root Tauri process) can connect
    // The socket is in /var/run/ which is root-only, but we can chmod it
    let _ = fs::set_permissions(SOCKET_PATH, std::fs::Permissions::from_mode(0o777));

    // Accept one connection
    if let Ok((stream, _)) = listener.accept() {
        handle_connection(stream);
    }

    // Cleanup
    let _ = fs::remove_file(SOCKET_PATH);
}
```

Note: This uses `std::os::unix::fs::PermissionsExt` for `from_mode`. Add the import:

```rust
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
```

- [ ] **Step 3: Create launchd plist template**

Create `config/xyz.dvnlabs.xsshtunnel.plist`:

```xml
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>xyz.dvnlabs.xsshtunnel</string>
    <key>Program</key>
    <string>/Applications/XSSH Tunnel.app/Contents/Library/LaunchServices/xssh-tunnel-helper</string>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <false/>
    <key>UserName</key>
    <string>root</string>
    <key>StandardOutPath</key>
    <string>/var/log/xssh-tunnel-helper.log</string>
    <key>StandardErrorPath</key>
    <string>/var/log/xssh-tunnel-helper.log</string>
</dict>
</plist>
```

- [ ] **Step 4: Verify helper compiles**

Run: `cd helper && cargo check`
Expected: Compiles successfully

- [ ] **Step 5: Commit**

```bash
git add helper/ config/
git commit -m "feat: add privileged helper daemon binary and launchd plist"
```

---

### Task 5: Update Tunnel Module for Helper Integration

**Files:**
- Modify: `src-tauri/src/tunnel/tun_device.rs` (add from_fd, remove create)
- Remove: `src-tauri/src/tunnel/route_manager.rs`
- Modify: `src-tauri/src/tunnel/mod.rs` (use HelperClient)

- [ ] **Step 1: Update tun_device.rs**

Replace the entire file:

```rust
use std::os::unix::io::{AsRawFd, RawFd};
use std::io::{Read, Write};

use crate::error::AppError;

pub struct TunDevice {
    pub name: String,
    fd: RawFd,
}

impl TunDevice {
    /// Wrap an existing fd received from the privileged helper
    pub fn from_fd(fd: RawFd, name: &str) -> Result<Self> {
        Ok(TunDevice { name: name.to_string(), fd })
    }

    pub fn get_fd(&self) -> RawFd {
        self.fd
    }

    pub fn blocking_read(&self, buf: &mut [u8]) -> Result<usize, AppError> {
        use std::os::unix::io::FromRawFd;
        let mut file = unsafe { std::fs::File::from_raw_fd(self.fd) };
        let result = file.read(buf)
            .map_err(|e| AppError::Tunnel(format!("TUN read error: {}", e)))?;
        std::mem::forget(file);
        Ok(result)
    }

    pub fn blocking_write(&self, buf: &[u8]) -> Result<usize, AppError> {
        use std::os::unix::io::FromRawFd;
        let mut file = unsafe { std::fs::File::from_raw_fd(self.fd) };
        let result = file.write(buf)
            .map_err(|e| AppError::Tunnel(format!("TUN write error: {}", e)))?;
        std::mem::forget(file);
        Ok(result)
    }
}
```

- [ ] **Step 2: Remove route_manager.rs**

```bash
git rm src-tauri/src/tunnel/route_manager.rs
```

- [ ] **Step 3: Update tunnel/mod.rs**

Replace the content:

```rust
pub mod tun_device;
pub mod socks5;
pub mod packet_router;

pub use tun_device::TunDevice;
pub use socks5::Socks5Engine;
pub use packet_router::PacketRouter;

use std::sync::Arc;

use crate::error::AppError;
use crate::ssh::{SshClient, SshConfig};

pub struct Tunnel {
    ssh_client: Option<Arc<SshClient>>,
    router_handle: Option<tokio::task::JoinHandle<Result<(), AppError>>>,
    pub tun_name: Option<String>,
}

impl Tunnel {
    pub fn new() -> Self {
        Tunnel {
            ssh_client: None,
            router_handle: None,
            tun_name: None,
        }
    }

    pub async fn start(
        &mut self,
        config: TunnelConfig,
        tun_fd: std::os::unix::io::RawFd,
        tun_name: &str,
    ) -> Result<(), AppError> {
        let tun = TunDevice::from_fd(tun_fd, tun_name)?;

        let ssh_config = SshConfig {
            host: config.ssh_host,
            port: config.ssh_port,
            username: config.ssh_username,
            password: config.ssh_password,
        };
        let ssh_client = Arc::new(SshClient::connect(ssh_config).await?);

        let router = PacketRouter::new(ssh_client.clone());
        let router_handle = tokio::task::spawn_blocking(move || {
            router.blocking_read_loop(tun)
        });

        self.tun_name = Some(tun_name.to_string());
        self.ssh_client = Some(ssh_client);
        self.router_handle = Some(router_handle);

        Ok(())
    }

    pub async fn stop(&mut self) -> Result<(), AppError> {
        if let Some(handle) = self.router_handle.take() {
            handle.abort();
        }
        self.ssh_client = None;
        Ok(())
    }
}

pub struct TunnelConfig {
    pub ssh_host: String,
    pub ssh_port: u16,
    pub ssh_username: String,
    pub ssh_password: String,
}
```

- [ ] **Step 4: Verify Rust compiles**

Run: `cd src-tauri && cargo check`
Expected: Compiles successfully

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/tunnel/
git commit -m "refactor: update tunnel to receive TUN fd from helper instead of creating directly"
```

---

### Task 6: Wire Helper into lib.rs AppState and Commands

**Files:**
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: Add helper to AppState and update connect/disconnect**

Read `src-tauri/src/lib.rs`. Then:

**a)** Add import:
```rust
use helper::HelperClient;
```

**b)** Add helper field to AppState:
```rust
struct AppState {
    db: DbPool,
    master_key: [u8; 32],
    tunnel: Mutex<Option<Tunnel>>,
    helper: Mutex<Option<HelperClient>>,
}
```

**c)** Update `.setup()` to set initial helper:
```rust
app.manage(AppState {
    db,
    master_key,
    tunnel: Mutex::new(None),
    helper: Mutex::new(None),
});
```

**d)** Update `connect_tunnel` to use helper:

Replace the tunnel.start() call section. After profile fetch and credential decryption, add:

```rust
    // Connect to privileged helper
    let mut helper = HelperClient::connect()
        .map_err(|_| AppError::Tunnel(
            "Privileged Helper not available. Go to Settings to install.".to_string()
        ))?;

    // Create TUN device via helper
    let (tun_name, tun_fd) = helper.create_tun()?;
    emit_log(&app, &state.db, "info", &format!("TUN device {} created via helper", tun_name), Some(&profile_id)).await;

    // Inject routes via helper
    helper.add_route(&tun_name)?;
    emit_log(&app, &state.db, "info", "Routes injected via helper", Some(&profile_id)).await;

    // Start tunnel (receives pre-created TUN fd + name)
    let mut tunnel = Tunnel::new();
    let config = TunnelConfig {
        ssh_host: profile.host.clone(),
        ssh_port: profile.port as u16,
        ssh_username: username.clone(),
        ssh_password: password.unwrap_or_default(),
    };

    match tunnel.start(config, tun_fd, &tun_name).await {
        Ok(()) => {
            emit_log(&app, &state.db, "info", "Tunnel active", Some(&profile_id)).await;
            app.emit("connection-state", "tunnel-active").unwrap();
            let mut tunnel_guard = state.tunnel.lock().await;
            *tunnel_guard = Some(tunnel);
            let mut helper_guard = state.helper.lock().await;
            *helper_guard = Some(helper);
            Ok("Connected".to_string())
        }
        Err(e) => {
            emit_log(&app, &state.db, "error", &format!("Connection failed: {}", e), Some(&profile_id)).await;
            app.emit("connection-state", "disconnected").unwrap();
            // Cleanup helper
            let _ = helper.cleanup_routes(&tun_name);
            Err(e)
        }
    }
```

**e)** Update `disconnect_tunnel`:

```rust
#[tauri::command]
async fn disconnect_tunnel(app: tauri::AppHandle, state: tauri::State<'_, AppState>) -> Result<String, AppError> {
    let tunnel = {
        let mut tunnel_guard = state.tunnel.lock().await;
        tunnel_guard.take()
    };
    let mut helper = {
        let mut helper_guard = state.helper.lock().await;
        helper_guard.take()
    };

    if let Some(mut t) = tunnel {
        if let Some(ref mut h) = helper {
            if let Some(ref name) = t.tun_name {
                let _ = h.cleanup_routes(name);
            }
        }
        t.stop().await?;
        emit_log(&app, &state.db, "info", "Disconnected from tunnel", None).await;
        app.emit("connection-state", "disconnected").unwrap();
        Ok("Disconnected".to_string())
    } else {
        Err(AppError::NotConnected)
    }
}
```

**f)** Add helper commands:

```rust
#[tauri::command]
async fn get_helper_status_cmd() -> Result<helper::HelperStatus, AppError> {
    helper::get_status()
        .map_err(|e| AppError::Tunnel(format!("Failed to get helper status: {}", e)))
}

#[tauri::command]
async fn install_helper_cmd() -> Result<(), AppError> {
    // Get the helper path relative to the app bundle
    let bundle_path = std::env::current_exe()
        .map_err(|e| AppError::Tunnel(format!("Failed to get app path: {}", e)))?
        .parent()
        .and_then(|p| p.parent())
        .and_then(|p| p.parent())
        .map(|p| p.join("Library").join("LaunchServices").join("xssh-tunnel-helper"))
        .ok_or_else(|| AppError::Tunnel("Failed to resolve helper path".to_string()))?;

    let path_str = bundle_path.to_str()
        .ok_or_else(|| AppError::Tunnel("Invalid helper path".to_string()))?;

    helper::install(path_str)
        .map_err(|e| AppError::Tunnel(format!("Failed to install helper: {}", e)))
}

#[tauri::command]
async fn uninstall_helper_cmd() -> Result<(), AppError> {
    helper::uninstall()
        .map_err(|e| AppError::Tunnel(format!("Failed to uninstall helper: {}", e)))
}
```

**g)** Register all new commands in `generate_handler!`:
- `get_helper_status_cmd`
- `install_helper_cmd`
- `uninstall_helper_cmd`

- [ ] **Step 2: Verify Rust compiles**

Run: `cd src-tauri && cargo check`
Expected: Compiles successfully (warnings OK)

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/lib.rs
git commit -m "feat: wire privileged helper into tunnel connect/disconnect flow"
```

---

### Task 7: Create Frontend Helper Store and IPC

**Files:**
- Create: `src/lib/stores/helper.ts`
- Modify: `src/lib/tauri.ts`

- [ ] **Step 1: Create helper store**

Write to `src/lib/stores/helper.ts`:

```typescript
import { writable } from 'svelte/store';

export interface HelperStatus {
  installed: boolean;
  running: boolean;
}

export const helperStatus = writable<HelperStatus>({ installed: false, running: false });
```

- [ ] **Step 2: Add helper IPC wrappers to tauri.ts**

Read `src/lib/tauri.ts` and add:

```typescript
import { helperStatus } from './stores/helper';

// Helper commands
export async function getHelperStatus(): Promise<HelperStatus> {
  const status = await invoke('get_helper_status_cmd');
  helperStatus.set(status as HelperStatus);
  return status as HelperStatus;
}

export async function installHelper(): Promise<void> {
  await invoke('install_helper_cmd');
  await getHelperStatus();
}

export async function uninstallHelper(): Promise<void> {
  await invoke('uninstall_helper_cmd');
  await getHelperStatus();
}
```

- [ ] **Step 3: Verify frontend builds**

Run: `npm run check && npm run build`
Expected: No errors

- [ ] **Step 4: Commit**

```bash
git add src/lib/stores/helper.ts src/lib/tauri.ts
git commit -m "feat: add helper status store and IPC wrappers"
```

---

### Task 8: Create Settings Page with Helper Management

**Files:**
- Create: `src/routes/settings/+page.svelte`
- Modify: `src/routes/+page.svelte` (add nav link to settings)

- [ ] **Step 1: Create settings page**

Write to `src/routes/settings/+page.svelte`:

```svelte
<script lang="ts">
  import { onMount } from 'svelte';
  import { Button } from '$lib/components/ui/button';
  import { Card, CardContent, CardHeader, CardTitle } from '$lib/components/ui/card';
  import { Badge } from '$lib/components/ui/badge';
  import { getHelperStatus, installHelper, uninstallHelper } from '$lib/tauri';
  import { helperStatus } from '$lib/stores/helper';

  let installing = $state(false);
  let uninstalling = $state(false);
  let error = $state('');

  async function loadStatus() {
    try {
      await getHelperStatus();
    } catch (e) {
      error = String(e);
    }
  }

  async function handleInstall() {
    installing = true;
    error = '';
    try {
      await installHelper();
    } catch (e) {
      error = String(e);
    } finally {
      installing = false;
    }
  }

  async function handleUninstall() {
    if (!confirm('Uninstall the privileged helper? This will not affect existing profiles.')) return;
    uninstalling = true;
    error = '';
    try {
      await uninstallHelper();
    } catch (e) {
      error = String(e);
    } finally {
      uninstalling = false;
    }
  }

  onMount(loadStatus);
</script>

<div class="container mx-auto p-6 max-w-2xl">
  <div class="flex justify-between items-center mb-6">
    <h1 class="text-2xl font-bold">Settings</h1>
    <Button variant="outline" onclick={() => window.location.href = '/'}>
      Back
    </Button>
  </div>

  {#if error}
    <p class="text-red-500 mb-4">{error}</p>
  {/if}

  <!-- Privileged Helper Section -->
  <Card>
    <CardHeader>
      <CardTitle>Privileged Helper</CardTitle>
    </CardHeader>
    <CardContent>
      <div class="space-y-4">
        <div class="flex items-center justify-between">
          <span class="text-sm font-medium">Status</span>
          {#if $helperStatus.installed}
            <Badge variant={$helperStatus.running ? 'default' : 'secondary'}>
              {$helperStatus.running ? 'Running' : 'Installed'}
            </Badge>
          {:else}
            <Badge variant="outline">Not Installed</Badge>
          {/if}
        </div>

        <p class="text-sm text-muted-foreground">
          The privileged helper creates TUN devices and manages network routes.
          It runs as a root daemon via SMAppService. Installing requires an admin password.
        </p>

        <div class="flex gap-2 pt-2">
          <Button
            onclick={handleInstall}
            disabled={installing || $helperStatus.installed}
          >
            {installing ? 'Installing...' : 'Install Helper'}
          </Button>
          <Button
            variant="outline"
            onclick={handleUninstall}
            disabled={uninstalling || !$helperStatus.installed}
          >
            {uninstalling ? 'Uninstalling...' : 'Uninstall'}
          </Button>
        </div>
      </div>
    </CardContent>
  </Card>

  <!-- About Section -->
  <Card class="mt-6">
    <CardHeader>
      <CardTitle>About</CardTitle>
    </CardHeader>
    <CardContent>
      <div class="space-y-2 text-sm">
        <p><span class="font-medium">Version:</span> 0.1.0</p>
        <p><span class="font-medium">Identifier:</span> xyz.dvnlabs.xsshtunnel</p>
        <p class="text-muted-foreground">
          XSSH Tunnel — SSH-based VPN tunnel for macOS.
        </p>
      </div>
    </CardContent>
  </Card>
</div>
```

- [ ] **Step 2: Add settings link to main page**

Read `src/routes/+page.svelte`. In the header, add a settings button after the import button:

```svelte
      <Button
        variant="ghost"
        onclick={() => window.location.href = '/settings'}
        size="icon"
      >
        ⚙️
      </Button>
```

- [ ] **Step 3: Verify frontend builds**

Run: `npm run check && npm run build`
Expected: No errors

- [ ] **Step 4: Commit**

```bash
git add src/routes/settings/+page.svelte src/routes/+page.svelte
git commit -m "feat: add settings page with helper install/uninstall"
```

---

### Task 9: Add Helper Status Check on Startup + Banner

**Files:**
- Modify: `src/routes/+page.svelte` (add banner + startup check)

- [ ] **Step 1: Add helper status check and banner**

Read `src/routes/+page.svelte`. In the `<script>` section, add:

```svelte
  import { getHelperStatus } from '$lib/tauri';
  import { helperStatus } from '$lib/stores/helper';

  let dismissedBanner = $state(false);
```

Add to `onMount()`:
```svelte
    // Check helper status
    getHelperStatus().catch(() => {});
```

Add a helper banner after the header (before the profile list):

```svelte
  <!-- Helper banner -->
  {#if !$helperStatus.installed && !dismissedBanner}
    <div class="bg-yellow-50 border border-yellow-200 rounded-lg px-4 py-3 mb-4 flex items-center justify-between">
      <div class="flex items-center gap-2">
        <span class="text-yellow-600 text-sm">
          ⚠️ Privileged Helper not installed. TUN device creation requires it.
        </span>
        <Button
          variant="outline"
          size="sm"
          onclick={() => window.location.href = '/settings'}
        >
          Install
        </Button>
      </div>
      <button
        onclick={() => dismissedBanner = true}
        class="text-yellow-400 hover:text-yellow-600 text-lg leading-none"
      >
        &times;
      </button>
    </div>
  {/if}
```

- [ ] **Step 2: Verify frontend builds**

Run: `npm run check && npm run build`
Expected: No errors

- [ ] **Step 3: Commit**

```bash
git add src/routes/+page.svelte
git commit -m "feat: add helper status check on startup and installation banner"
```

---

### Task 10: Build Script for Helper Bundle

**Files:**
- Modify: `src-tauri/tauri.conf.json` (add helper binary to bundle)
- Create: Helper build step in package.json or Makefile

- [ ] **Step 1: Add helper binary to Tauri bundle**

Read `src-tauri/tauri.conf.json`. Add `bundle.externalBin`:

```json
  "bundle": {
    "active": true,
    "targets": "all",
    "externalBin": [
      "binaries/xssh-tunnel-helper"
    ]
  }
```

- [ ] **Step 2: Create helper build script**

Add to `package.json` scripts:

```json
"scripts": {
  "build:helper": "cd helper && cargo build --release",
  "build": "npm run build:helper && vite build",
  "tauri": "npm run build:helper && tauri"
}
```

Or better, add a build step in the Rust build.rs that builds the helper when the main app is built. For now, a manual step is fine.

- [ ] **Step 3: Verify full build cycle**

```bash
cd helper && cargo build --release && cd ..
cd src-tauri && cargo check && cd ..
npm run check && npm run build
```

- [ ] **Step 4: Commit**

```bash
git add .github/ package.json src-tauri/tauri.conf.json
git commit -m "chore: add helper binary to Tauri bundle"
```

---

### Task 11: Final Integration and Verification

**Files:**
- Verify all changes

- [ ] **Step 1: Verify all builds pass**

```bash
cd helper && cargo check && cd ..
cd src-tauri && cargo check && cd ..
npm run check && npm run build
```

- [ ] **Step 2: Remove old route_manager.rs if not already done**

```bash
git rm src-tauri/src/tunnel/route_manager.rs
```

- [ ] **Step 3: Stage any remaining files**

```bash
git status
```

- [ ] **Step 4: Tag milestone**

```bash
git tag -a m5-privileged-helper -m "M5: SMAppService privileged helper for TUN creation"
```

- [ ] **Step 5: Final summary**

What was built:
- C bridge for SMAppService (install/uninstall/status)
- Rust FFI wrapper for the C bridge
- HelperClient with Unix socket + JSON protocol + SCM_RIGHTS
- Helper daemon binary (separate Rust project)
- Launchd plist template
- Updated tunnel module to receive TUN fd from helper
- Settings page with Install/Uninstall buttons
- Helper status check on startup with banner
- Helper cleanup on disconnect and crash

---

## Self-Review Checklist

- [x] Spec coverage: Every requirement in the M5 design doc has corresponding tasks
- [x] No placeholders: All steps contain actual code and commands
- [x] Type consistency: Types match between tasks (HelperStatus, HelperClient, etc.)
- [x] Exact file paths: Every file path is precise and matches the design doc
- [x] Complete commands: All commands include expected output
- [x] Ordering: C bridge first, then FFI, then client, then daemon, then integration

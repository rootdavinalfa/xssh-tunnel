# M5 Design: Privileged Helper (SMAppService)

> **Date:** 2026-05-20  
> **Milestone:** M5 — Privileged macOS Helper  
> **Status:** Approved for Implementation

---

## Overview

Eliminate the need for `sudo` by implementing an SMAppService-based privileged helper that creates TUN devices and manages routes. The helper is a separate Rust binary running as root, managed by launchd. Communication happens via a Unix domain socket with `SCM_RIGHTS` for file descriptor passing.

### Goals

- **No sudo required** — Users run the app normally, even in development
- **Helper lifecycle management** — Install/Uninstall from Settings UI
- **Clean on crash** — Helper exits if main app disconnects/crashes
- **Survives macOS updates** — App detects missing helper, guides user to reinstall
- **Non-intrusive** — Subtle banner when helper needs installation, not a blocking prompt

---

## Architecture

### Components

```
┌──────────────────────────────────────┐
│           Main App (Rust/Tauri)      │
│  ┌──────────────────────────────┐    │
│  │  HelperClient (Unix socket)  │    │
│  │  - connect to socket         │    │
│  │  - send JSON commands        │    │
│  │  - receive TUN fd via        │    │
│  │    SCM_RIGHTS                │    │
│  └──────────┬───────────────────┘    │
│  ┌──────────┴───────────────────┐    │
│  │  Tunnel                      │    │
│  │  - use HelperClient for TUN  │    │
│  │  - + route mgmt              │    │
│  │  - direct fd read/write      │    │
│  └──────────────────────────────┘    │
│  ┌──────────────────────────────┐    │
│  │  Settings: Install/Uninstall │    │
│  └──────────────────────────────┘    │
└──────────────┬───────────────────────┘
               │ Unix socket
               │ /var/run/xyz.dvnlabs.xsshtunnel.sock
┌──────────────┴───────────────────────┐
│  Privileged Helper (root via lauchd) │
│  ┌──────────────────────────────┐    │
│  │  create_tun /dev/utun*       │    │
│  │  add_route / cleanup_routes  │    │
│  │  sends fd via SCM_RIGHTS     │    │
│  │  exits on client disconnect  │    │
│  └──────────────────────────────┘    │
└──────────────────────────────────────┘
```

### File Structure

```
src-tauri/
├── build.rs                          # Compile C bridge with cc crate
├── Cargo.toml                        # Add cc dev-dependency
├── src/
│   ├── helper/
│   │   ├── mod.rs                    # Client: connect, send JSON, receive fd
│   │   ├── client.rs                 # HelperClient implementation
│   │   └── bridge.c                  # Thin C wrapper for SMAppService APIs
│   ├── tunnel/
│   │   ├── mod.rs                    # EXTEND: Uses HelperClient
│   │   ├── tun_device.rs             # MODIFY: from_fd() constructor (no create)
│   │   ├── route_manager.rs          # REMOVED: replaced by helper
│   │   ├── socks5.rs                 # Unchanged
│   │   └── packet_router.rs          # Unchanged
│   └── lib.rs                        # EXTEND: manage HelperClient in AppState
helper/
├── Cargo.toml                        # Separate binary
└── src/
    └── main.rs                       # Unix socket server, TUN create, route mgmt

src/
├── routes/
│   ├── +page.svelte                  # EXTEND: Helper status banner
│   └── settings/+page.svelte         # NEW: Settings with helper section
├── lib/
│   ├── tauri.ts                      # EXTEND: Helper IPC wrappers
│   └── stores/
│       └── helper.ts                 # NEW: Helper status store
```

---

## C Bridge (SMAppService FFI)

### bridge.c

A thin C file exposing the minimum needed API surface from ServiceManagement + Security frameworks.

#### Functions

```c
// Install helper binary as a privileged launch daemon via SMAppService
// bundle_path: absolute path to the helper executable inside the app bundle
// Returns 0 on success, -1 with error message in out_error
int helper_install(const char* bundle_path, char* out_error, size_t error_len);

// Unregister the helper and remove its launchd plist
int helper_uninstall(char* out_error, size_t error_len);

// Check helper status
// out_installed: 1 if registered, 0 if not
// out_running: 1 if launchd reports it as running, 0 if not
int helper_status(int* out_installed, int* out_running, char* out_error, size_t error_len);
```

#### Apple API Mapping

| C Function | Apple APIs |
|-----------|-----------|
| `helper_install` | `AuthorizationCreate()` → `SMAppServiceRegister(bundle_path)` |
| `helper_uninstall` | `AuthorizationCreate()` → `SMAppServiceUnregister()` |
| `helper_status` | `SMAppServiceCopyStatus(service_name)` |

### build.rs

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

### Rust FFI (mod.rs)

```rust
mod client;

use std::ffi::{CStr, CString};
use std::os::raw::c_char;

#[derive(Debug, Clone, serde::Serialize)]
pub struct HelperStatus {
    pub installed: bool,
    pub running: bool,
}

pub fn get_status() -> Result<HelperStatus, String> { ... }
pub fn install(bundle_path: &str) -> Result<(), String> { ... }
pub fn uninstall() -> Result<(), String> { ... }

pub use client::HelperClient;
```

### client.rs

```rust
use std::os::unix::net::UnixStream;
use std::os::unix::io::RawFd;

pub struct HelperClient {
    stream: UnixStream,
}

impl HelperClient {
    /// Connect to the helper's Unix socket
    pub fn connect() -> Result<Self>;

    /// Send create_tun command, receive fd via SCM_RIGHTS
    pub fn create_tun(&mut self) -> Result<(String, RawFd)>;

    /// Send add_route command
    pub fn add_route(&mut self, tun_name: &str) -> Result<()>;

    /// Send cleanup_routes command
    pub fn cleanup_routes(&mut self, tun_name: &str) -> Result<()>;

    /// Send shutdown command (graceful exit)
    pub fn shutdown(&mut self) -> Result<()>;
}
```

---

## Helper Daemon

### helper/Cargo.toml

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

### helper/src/main.rs

A Unix socket server that listens on `/var/run/xyz.dvnlabs.xsshtunnel.sock`.

#### Protocol

JSON-based line protocol. Each message is a single line of JSON terminated by `\n`. Responses are also single-line JSON.

**Requests (main app → helper):**

| Command | Payload | Description |
|---------|---------|-------------|
| `create_tun` | `{}` | Create TUN device, send fd back via SCM_RIGHTS |
| `add_route` | `{"tun_name":"utun5"}` | Inject default routes through TUN |
| `cleanup_routes` | `{"tun_name":"utun5"}` | Remove routes |
| `ping` | `{}` | Health check |
| `shutdown` | `{}` | Clean exit |

**Responses:**

```json
// Success
{"ok":true,"result":{"tun_name":"utun5"}}
{"ok":true,"result":null}

// Error
{"ok":false,"error":"Failed to create TUN device: ..."}
```

#### TUN FD Passing

On `create_tun`, the helper:
1. Creates TUN device using the `tun` crate (same as current `TunDevice::create()`)
2. Gets the raw fd from the device
3. Sends the fd via **SCM_RIGHTS** ancillary data on the Unix socket
4. Returns the TUN name via JSON
5. Calls `std::mem::forget(device)` (fd ownership transferred to main app)

The main app wraps the received fd with `TunDevice::from_fd(fd, name)` and uses it normally for read/write.

#### Lifecycle

```rust
fn main() {
    // 1. Open Unix socket
    // 2. Wait for connection (single client)
    // 3. Enter command loop:
    //    - read JSON line
    //    - execute command
    //    - send JSON response
    //    - if client disconnects (EOF), break
    // 4. Cleanup routes
    // 5. Exit
}
```

---

## Integration with Tunnel

### Changes to tunnel/mod.rs

```rust
pub async fn start(&mut self, config: TunnelConfig, helper: &mut HelperClient) -> Result<(), AppError> {
    // 1. Create TUN via helper (receives fd + name)
    let (tun_name, tun_fd) = helper.create_tun()?;
    let tun = TunDevice::from_fd(tun_fd, &tun_name)?;  // NEW: wraps existing fd

    // 2. Connect SSH (unchanged)
    let ssh_client = connect_ssh(&config).await?;

    // 3. Inject routes via helper
    helper.add_route(&tun_name)?;

    // 4. Start packet router (unchanged)
    let router = PacketRouter::new(ssh_client.clone());
    tokio::task::spawn_blocking(move || router.blocking_read_loop(tun));

    self.tun_name = Some(tun_name);
    self.ssh_client = Some(ssh_client);
    self.router_handle = Some(router_handle);
    Ok(())
}

pub async fn stop(&mut self, helper: &mut HelperClient) -> Result<(), AppError> {
    // Cleanup routes via helper
    if let Some(ref name) = self.tun_name {
        let _ = helper.cleanup_routes(name);
    }
    // Abort router, drop SSH client...
}
```

### Changes to tun_device.rs

Keep `blocking_read()` and `blocking_write()`, remove `create()`, add `from_fd()`:

```rust
impl TunDevice {
    /// Wrap an existing fd (received from helper)
    pub fn from_fd(fd: RawFd, name: &str) -> Result<Self> {
        Ok(TunDevice { name: name.to_string(), fd })
    }
    // blocking_read() and blocking_write() unchanged
}
```

### Changes to lib.rs

```rust
struct AppState {
    db: DbPool,
    master_key: [u8; 32],
    tunnel: Mutex<Option<Tunnel>>,
    helper: Mutex<Option<HelperClient>>,  // NEW
}
```

In `connect_tunnel`, before starting the tunnel:
```rust
// Connect to helper
let mut helper = HelperClient::connect()
    .map_err(|_| AppError::Tunnel("Helper not available. Go to Settings to install.".to_string()))?;

let mut tunnel = Tunnel::new();
tunnel.start(config, &mut helper).await?;

// Store helper connection alongside tunnel
state.helper.lock().await.replace(helper);
```

In `disconnect_tunnel`:
```rust
// Stop tunnel with helper
let mut helper = state.helper.lock().await.take();
if let Some(ref mut h) = helper {
    tunnel.stop(h).await?;
}
```

---

## Settings UI

### /settings Route

```
Settings
├── Privileged Helper
│   ├── Status: Installed ✓ / Not Installed ✗
│   ├── Version string
│   ├── [Install Helper] button (admin auth prompt)
│   ├── [Uninstall] button (admin auth prompt)
│   └── Info text explaining what the helper does
└── About
    ├── App version
    ├── Build info
    └── Link to docs
```

### Helper Status Store (src/lib/stores/helper.ts)

```typescript
export interface HelperStatus {
  installed: boolean;
  running: boolean;
}

export const helperStatus = writable<HelperStatus>({ installed: false, running: false });
```

### IPC Wrappers

```typescript
export async function getHelperStatus(): Promise<HelperStatus>;
export async function installHelper(): Promise<void>;
export async function uninstallHelper(): Promise<void>;
```

### Helper Banner on Main Page

A small, dismissable banner at the top of the profile list:

```
╔══════════════════════════════════════════════════════════╗
║ ⚠️  Privileged Helper needs installation. Some features ║
║    may not work.  [Install] [Dismiss]                    ║
╚══════════════════════════════════════════════════════════╝
```

Only shown when `helperStatus.installed === false`. Dismissed state persisted in localStorage or store.

---

## Error Handling

| Scenario | Behavior |
|----------|----------|
| Helper not installed | `connect_tunnel` returns error with Install link. Banner shown. |
| Helper crashed / socket unavailable | `HelperClient::connect()` fails → app shows toast + banner |
| Helper install fails | `install_helper()` returns error → shown in Settings UI |
| macOS update cleared plist | App startup checks helper status → banner if missing |
| TUN creation fails | Helper returns JSON error → displayed to user as toast |
| Main app crashes | Socket closes → helper detects EOF → cleanup routes → exits |
| Helper binary corrupted | `SMAppServiceRegister` fails → reinstall from Settings |
| Helper exits unexpectedly | Socket read returns EOF → main app closes tunnel, shows toast |

---

## Testing

### During Development

1. Build helper: `cd helper && cargo build`
2. Copy helper to app bundle: `cp target/debug/xssh-tunnel-helper ../src-tauri/target/debug/`
3. Run main app: `npm run tauri dev`
4. Go to Settings → Install Helper (enter admin password)
5. Connect to a profile — should work without sudo

### Test Scenarios

- Install helper → verify `/var/run/xyz.dvnlabs.xsshtunnel.sock` exists
- Connect → verify TUN created, routes injected
- Disconnect → verify routes cleaned, helper exits
- Kill main app → verify helper exits within 2 seconds
- Uninstall helper → verify launchd plist removed
- Reinstall → verify works again
- macOS update simulation (remove plist manually) → verify banner appears

---

## Dependencies

### New Dependencies

| Crate | Purpose | Where |
|-------|---------|-------|
| `cc` (dev) | Compile C bridge | `src-tauri/Cargo.toml` |
| `serde` + `serde_json` | JSON protocol | `helper/Cargo.toml` |

### No New Frontend Dependencies

Uses existing shadcn-svelte components (Button, Card, Badge).

---

## Security Considerations

- **SCM_RIGHTS fd passing** — Only the fd is transferred, not the device. The helper can't access the fd after transfer.
- **Socket path ownership** — `/var/run/` is owned by root, only root helpers can create sockets there. Prevents non-root processes from connecting.
- **Single client** — Helper accepts only one client at a time (the main app).
- **No auth required** — Socket path secrecy + root ownership is sufficient for local inter-process communication.
- **Cleanup guarantee** — Helper always cleans up routes on exit, even if unexpected.

---

## Open Questions

1. **Helper binary placement in bundle** — Should be at `Contents/Library/LaunchServices/` per Apple guidelines.
2. **SMAppService vs launchd plist directly** — SMAppService is the recommended API, but we might need a fallback for macOS 12 and earlier.

---

## Success Criteria

- [ ] `npm run tauri dev` works without `sudo`
- [ ] TUN device created and routes injected via helper
- [ ] Helper exits when main app disconnects
- [ ] Helper exits when main app crashes
- [ ] Install/Uninstall buttons in Settings work
- [ ] Banner shown when helper not installed
- [ ] Build passes (`cargo check`, `npm run build`)
- [ ] All existing profiles work
- [ ] Connection logs show helper events

# M6 Design: Connection Stats & Health

> **Date:** 2026-05-21  
> **Milestone:** M6 — Connection Stats, Packet Proxy, Auto-Reconnect  
> **Status:** Approved for Implementation

---

## Overview

Wire up the packet proxy so the TUN device actually forwards traffic through SSH channels. Add connection stats (bytes up/down, uptime), auto-reconnect on connection loss, and heartbeat monitoring of the privileged helper daemon.

### Goals

- **Packet forwarding** — TUN packets actually go through SSH tunnels, not just logged
- **Connection stats** — Live bytes transferred (up/down), uptime, displayed in UI
- **Auto-reconnect** — Exponential backoff on SSH drop, up to 10 attempts
- **Heartbeat** — Detect privileged helper failure, trigger cleanup
- **Reconnect state** — UI shows "Reconnecting..." instead of dead buttons

---

## Architecture

### Packet Flow

```
Kernel → TUN fd → PacketRouter::blocking_read_loop
  → parse TCP packet → extract dst_ip, dst_port
  → SshClient::open_tcp_channel(dst_ip, dst_port)
  → proxy::pipe_tcp(tun, ssh_channel, stats)
    → tun→SSH (up): track bytes_up
    → SSH→tun (down): track bytes_down
```

### File Structure

```
src-tauri/src/tunnel/
├── mod.rs              # Add ConnectionStats, reconnect loop, heartbeat
├── proxy.rs            # RENAMED from socks5.rs — direct-tcpip forwarding + stats
├── packet_router.rs    # Wire actual TCP forwarding
├── tun_device.rs       # Unchanged (minor: from_fd stays, blocking_read/blocking_write returns String)
└── socks5.rs           # REMOVED

src-tauri/src/
└── lib.rs              # Emit stats events, spawn heartbeat, integrate reconnect

src/lib/
├── stores/
│   └── connection.ts   # EXTEND: add StatsSnapshot, reconnect state
├── tauri.ts            # EXTEND: listenConnectionStats
└── components/
    └── connection-stats.svelte  # NEW: live stats bar

src/routes/
└── +page.svelte        # ADD: ConnectionStats component above profile list
```

---

## Feature 1: Packet Proxy

### proxy.rs (renamed from socks5.rs)

Keep the core bidirectional pipe, add stats tracking.

```rust
use crate::ssh::SshClient;
use crate::error::AppError;
use crate::tunnel::TunDevice;
use crate::tunnel::ConnectionStats;

/// Bidirectional pipe between TUN device and SSH direct-tcpip channel.
/// Each TCP connection from the TUN gets its own SSH channel.
pub async fn pipe_tcp(
    ssh_client: &SshClient,
    dst_ip: &str,
    dst_port: u16,
    tun_device: &TunDevice,
    stats: &ConnectionStats,
) -> Result<(), AppError> {
    let mut channel = ssh_client.open_tcp_channel(dst_ip, dst_port).await?;
    let (mut ch_read, mut ch_write) = tokio::io::split(channel.into_stream());

    // TUN → SSH (upload)
    let up = async {
        let mut buf = [0u8; 4096];
        loop {
            let n = tun_device.blocking_read(&mut buf)
                .map_err(|e| AppError::Tunnel(e))?;
            if n == 0 { break; }
            ch_write.write_all(&buf[..n]).await
                .map_err(|e| AppError::Tunnel(format!("TUN write error: {}", e)))?;
            stats.add_up(n as u64);
        }
        Ok::<_, AppError>(())
    };

    // SSH → TUN (download)
    let down = async {
        let mut buf = [0u8; 4096];
        loop {
            let n = ch_read.read(&mut buf).await
                .map_err(|e| AppError::Tunnel(format!("SSH read error: {}", e)))?;
            if n == 0 { break; }
            tun_device.blocking_write(&buf[..n])
                .map_err(|e| AppError::Tunnel(e))?;
            stats.add_down(n as u64);
        }
        Ok::<_, AppError>(())
    };

    tokio::select! {
        r = up => r,
        r = down => r,
    }
}
```

### Changes to packet_router.rs

Replace the `tracing::info!("Would proxy...")` log line with actual proxy spawning. Each new TCP connection spawns a task that calls `proxy::pipe_tcp(...)`.

The router holds `Arc<SshClient>` and `Arc<ConnectionStats>`.

---

## Feature 2: Connection Stats

### Backend (tunnel/mod.rs + lib.rs)

**Atomic counters** shared between proxy tasks and the stats emitter:

```rust
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

pub struct ConnectionStats {
    pub bytes_up: AtomicU64,
    pub bytes_down: AtomicU64,
    pub started_at: Instant,
}

impl ConnectionStats {
    pub fn new() -> Self {
        Self {
            bytes_up: AtomicU64::new(0),
            bytes_down: AtomicU64::new(0),
            started_at: Instant::now(),
        }
    }

    pub fn add_up(&self, n: u64) {
        self.bytes_up.fetch_add(n, Ordering::Relaxed);
    }

    pub fn add_down(&self, n: u64) {
        self.bytes_down.fetch_add(n, Ordering::Relaxed);
    }

    pub fn snapshot(&self) -> StatsSnapshot {
        StatsSnapshot {
            bytes_up: self.bytes_up.load(Ordering::Relaxed),
            bytes_down: self.bytes_down.load(Ordering::Relaxed),
            uptime_secs: self.started_at.elapsed().as_secs(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct StatsSnapshot {
    pub bytes_up: u64,
    pub bytes_down: u64,
    pub uptime_secs: u64,
}
```

**lib.rs — stats emitter task** spawned on connect:

```rust
let stats = Arc::new(ConnectionStats::new());
let app_for_stats = app.clone();
tokio::spawn(async move {
    loop {
        tokio::time::sleep(Duration::from_secs(1)).await;
        let _ = app_for_stats.emit("connection-stats", stats.snapshot());
    }
});
```

**lib.rs — heartbeat task** spawned on connect:

```rust
let mut helper_clone = /* ... */;
tokio::spawn(async move {
    let mut failures = 0;
    loop {
        tokio::time::sleep(Duration::from_secs(10)).await;
        // ping helper
        if helper_clone.send_ping().is_err() {
            failures += 1;
            if failures >= 2 {
                break;  // Helper dead — cleanup triggered
            }
        } else {
            failures = 0;
        }
    }
});
```

### Frontend (connection.ts + connection-stats.svelte)

**Extended store:**

```typescript
export interface StatsSnapshot {
  bytes_up: number;
  bytes_down: number;
  uptime_secs: number;
}

export const connectionStats = writable<StatsSnapshot | null>(null);
```

**Reconnect state:**

```typescript
export type ConnectionState = 
  | 'disconnected' 
  | 'connecting' 
  | 'authenticating' 
  | 'tunnel-active' 
  | 'reconnecting'
  | 'error';
```

**Stats bar component** (shown above profile list when connected):

```
┌──────────────────────────────────────────────┐
│ ● tunnel-active │ ↑ 1.2 MB  ↓ 3.4 MB         │
│ Uptime: 5m 32s  │  24 KB/s                    │
└──────────────────────────────────────────────┘
```

Emits `"connection-stats"` event every second from Rust → listened by frontend.

### IPC

```typescript
export function listenConnectionStats(callback: (stats: StatsSnapshot) => void) {
  return listen('connection-stats', (event) => callback(event.payload));
}
```

---

## Feature 3: Auto-Reconnect

### Protocol

Exponential backoff with a cap. Gives up after 10 attempts (~5 minutes total). User can always manually click Connect to reset.

| Attempt | Delay | Cumulative |
|---------|-------|-----------|
| 1 | 2s | 2s |
| 2 | 4s | 6s |
| 3 | 8s | 14s |
| 4 | 16s | 30s |
| 5 | 32s | 62s |
| 6-10 | 60s | ~5m |

### Backend (tunnel/mod.rs)

The Tunnel struct gains `profile_id` and `config` for reconnect:

```rust
pub struct Tunnel {
    pub ssh_client: Option<Arc<SshClient>>,
    pub router_handle: Option<tokio::task::JoinHandle<()>>,
    pub tun_name: Option<String>,
    pub stats: Arc<ConnectionStats>,
    pub profile_id: String,
    pub config: TunnelConfig,
}
```

**Reconnect spawn** — after tunnel.start() succeeds, spawn a reaper that watches the router thread:

```rust
// After start() in connect_tunnel
let reaper_handle = tokio::spawn(async move {
    // Take the router handle
    let handle = router_handle.lock().await.take().unwrap();
    
    // Wait for router to exit (blocks until connection drops)
    let _ = handle.await;
    
    // Router exited — connection lost
    for attempt in 1..=10 {
        let delay = Duration::from_secs(2u64.pow(attempt).min(60));
        tokio::time::sleep(delay).await;
        
        emit_log(&app, &db, "info", &format!("Reconnecting (attempt {})...", attempt));
        app.emit("connection-state", "reconnecting").unwrap();
        
        match tunnel.start(config, tun_fd, tun_name).await {
            Ok(()) => {
                app.emit("connection-state", "tunnel-active").unwrap();
                return;
            }
            Err(e) => {
                emit_log(&app, &db, "error", &format!("Reconnect {} failed: {}", e));
            }
        }
    }
    
    // Gave up
    app.emit("connection-state", "disconnected").unwrap();
});
```

**Reconnect abort** — if user manually disconnects during reconnect, abort the loop:

```rust
// In disconnect_tunnel
if let Some(handle) = reaper_handle.take() {
    handle.abort();
}
```

### Frontend

The **connection-stats.svelte** shows reconnecting state:

```
┌──────────────────────────────────────────────┐
│ ⟳ Reconnecting... (attempt 3/10)             │
└──────────────────────────────────────────────┘
```

The **ProfileCard** shows buttons based on state:

| State | Button |
|-------|--------|
| disconnected | **Connect** |
| connecting | **Connecting...** (disabled) |
| authenticating | **Authenticating...** (disabled) |
| tunnel-active | **Disconnect** (destructive) |
| reconnecting | **Cancel** (abort reconnect, go to disconnected) |
| error | **Connect** (retry) |

---

## Feature 4: Heartbeat

### Design

Every 10 seconds, ping the privileged helper via the Unix socket command `{"cmd":"ping"}`. Expects `{"ok":true}` response within 2 seconds. Two consecutive failures → consider helper dead → trigger disconnect + cleanup.

### Implementation

Helper's ping handler is already implemented (returns `{"ok":true}`).

```rust
// In the helper heartbeat task (lib.rs)
let mut failures = 0u32;
loop {
    tokio::time::sleep(Duration::from_secs(10)).await;
    match tokio::time::timeout(Duration::from_secs(2), async {
        helper.send_command(r#"{"cmd":"ping"}"#).await
    }).await {
        Ok(Ok(_)) => failures = 0,
        _ => {
            failures += 1;
            if failures >= 2 {
                emit_log(&app, &db, "error", "Privileged helper daemon unresponsive");
                app.emit("connection-state", "disconnected").unwrap();
                break;
            }
        }
    }
}
```

### Failure modes

| Failure | Detected | Action |
|---------|----------|--------|
| SSH drops | Router handle exits | Auto-reconnect with backoff |
| Helper crashes | Heartbeat fails 2x | Emit disconnected, user reconnects |
| Network drop | SSH / TUN I/O error | Router exits → auto-reconnect |
| TUN error | blocking_read returns error | Router exits → auto-reconnect |

---

## Frontend: ConnectionStats Component

### connection-stats.svelte

```svelte
<script lang="ts">
  import { connectionState, connectionStats } from '$lib/stores/connection';

  function formatBytes(bytes: number): string {
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
    if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
    return `${(bytes / (1024 * 1024 * 1024)).toFixed(2)} GB`;
  }

  function formatDuration(secs: number): string {
    const h = Math.floor(secs / 3600);
    const m = Math.floor((secs % 3600) / 60);
    const s = secs % 60;
    if (h > 0) return `${h}h ${m}m ${s}s`;
    if (m > 0) return `${m}m ${s}s`;
    return `${s}s`;
  }
</script>

{#if $connectionState === 'tunnel-active' && $connectionStats}
  <div class="bg-muted rounded-lg px-4 py-3 mb-4">
    <div class="flex items-center justify-between text-sm">
      <div class="flex items-center gap-3">
        <span class="w-2 h-2 rounded-full bg-green-500 animate-pulse"></span>
        <span class="font-medium">Connected</span>
        <span class="text-muted-foreground">|</span>
        <span>↑ {formatBytes($connectionStats.bytes_up)}</span>
        <span>↓ {formatBytes($connectionStats.bytes_down)}</span>
      </div>
      <span class="text-muted-foreground">{formatDuration($connectionStats.uptime_secs)}</span>
    </div>
  </div>
{:else if $connectionState === 'reconnecting'}
  <div class="bg-yellow-50 border border-yellow-200 rounded-lg px-4 py-3 mb-4">
    <span class="text-sm text-yellow-600">⟳ Reconnecting...</span>
  </div>
{/if}
```

### Integration in +page.svelte

Import and add the component above the profile list:

```svelte
import ConnectionStats from '$lib/components/connection-stats.svelte';

<!-- After header, before profile list -->
<ConnectionStats />
<!-- Error message (if any) -->
<!-- Profile list -->
```

---

## Error Handling

| Scenario | Behavior |
|----------|----------|
| Proxy task fails (single connection) | Error logged, other connections unaffected |
| All proxy tasks fail | Router exits → auto-reconnect |
| Reconnect fails all 10 attempts | State → `disconnected`, stats cleared |
| User disconnects during reconnect | Abort reconnect loop, stats cleared |
| Helper dies (heartbeat detects) | State → `disconnected`, keep log entry |

---

## Testing

### Manual

- Connect to a server
- Visit a website through the tunnel (e.g., `curl --interface utun5 http://example.com`)
- Observe bytes up/down in the stats bar
- Kill the SSH server → observe reconnect attempts
- Kill the helper daemon → observe disconnect after 2 heartbeat failures

### Unit tests

- `ConnectionStats::add_up()` / `add_down()` atomic correctness
- `formatBytes()` edge cases (0, 1023, 1024, 1048576)
- `formatDuration()` edge cases (0s, 59s, 60s, 3600s)

---

## Success Criteria

- [ ] TCP packets from TUN are actually forwarded through SSH
- [ ] Bytes up/down displayed live in the stats bar
- [ ] Connection uptime displayed in the stats bar
- [ ] SSH drop triggers auto-reconnect with backoff
- [ ] "Reconnecting..." state shown during reconnect
- [ ] Heartbeat detects helper failure in ~20s
- [ ] User can manually reconnect anytime
- [ ] `cargo check` passes
- [ ] `npm run check && npm run build` passes

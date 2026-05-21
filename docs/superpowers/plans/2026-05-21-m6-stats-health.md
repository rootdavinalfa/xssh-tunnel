# M6 Implementation Plan: Connection Stats & Health

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` (recommended) or `superpowers:executing-plans` to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Wire up packet forwarding through SSH, add live connection stats (bytes, uptime), auto-reconnect on connection loss, and helper heartbeat monitoring.

**Architecture:** Rename socks5.rs to proxy.rs with bidirectional pipe + stats tracking. Add atomic ConnectionStats to tunnel module. Wire packet router to actually open SSH channels and spawn proxy tasks. Add reconnect loop with exponential backoff. Emit stats and reconnect state via Tauri events.

**Tech Stack:** Rust atomics (std::sync::atomic), tokio::select!, Svelte 5 stores, date-fns

---

## File Structure

```
src-tauri/src/tunnel/
├── mod.rs              # ADD: ConnectionStats, reconnect loop
├── proxy.rs            # NEW: renamed from socks5.rs, pipe_tcp + stats
├── packet_router.rs    # MODIFY: forward packets via proxy::pipe_tcp
├── socks5.rs           # REMOVE
├── tun_device.rs       # MODIFY: return String errors, add RawFd type

src-tauri/src/
└── lib.rs              # MODIFY: stats emitter, heartbeat, Tunnel fields

src/lib/
├── stores/
│   └── connection.ts   # EXTEND: StatsSnapshot, reconnect state
├── tauri.ts            # EXTEND: listenConnectionStats
└── components/
    └── connection-stats.svelte  # NEW: live stats bar

src/routes/+page.svelte # ADD: ConnectionStats import + usage
```

---

### Task 1: Rename socks5.rs → proxy.rs and wire packet forwarding

**Files:**
- Create: `src-tauri/src/tunnel/proxy.rs`
- Remove: `src-tauri/src/tunnel/socks5.rs`
- Modify: `src-tauri/src/tunnel/mod.rs` (export proxy instead of socks5)
- Modify: `src-tauri/src/tunnel/packet_router.rs` (call proxy::pipe_tcp)

- [ ] **Step 1: Create proxy.rs**

Write to `src-tauri/src/tunnel/proxy.rs`:

```rust
use crate::ssh::SshClient;
use crate::error::AppError;
use crate::tunnel::TunDevice;
use crate::tunnel::ConnectionStats;

pub async fn pipe_tcp(
    ssh_client: &SshClient,
    dst_ip: &str,
    dst_port: u16,
    tun_device: &TunDevice,
    stats: &ConnectionStats,
) -> Result<(), AppError> {
    let mut channel = ssh_client.open_tcp_channel(dst_ip, dst_port).await?;
    let (mut ch_read, mut ch_write) = tokio::io::split(channel.into_stream());

    let up = async {
        let mut buf = [0u8; 4096];
        loop {
            let n = tun_device.blocking_read(&mut buf)
                .map_err(|e| AppError::Tunnel(e))?;
            if n == 0 { break; }
            ch_write.write_all(&buf[..n]).await
                .map_err(|e| AppError::Tunnel(format!("proxy up write: {}", e)))?;
            stats.add_up(n as u64);
        }
        Ok::<_, AppError>(())
    };

    let down = async {
        let mut buf = [0u8; 4096];
        loop {
            let n = ch_read.read(&mut buf).await
                .map_err(|e| AppError::Tunnel(format!("proxy down read: {}", e)))?;
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

- [ ] **Step 2: Update tunnel/mod.rs exports**

Read `src-tauri/src/tunnel/mod.rs`. Change `pub mod socks5;` to `pub mod proxy;` and update the `pub use` line from `socks5::Socks5Engine` to `pub use proxy::*;`.

- [ ] **Step 3: Remove socks5.rs**

```bash
git rm src-tauri/src/tunnel/socks5.rs
```

- [ ] **Step 4: Wire packet_router.rs to actually forward**

Read `src-tauri/src/tunnel/packet_router.rs`. Replace the line:
```rust
tracing::info!("Would proxy {} to {}:{}", stream_key, dst_ip, dst_port);
```

With actual proxy spawning:
```rust
let ssh = self.ssh_client.clone();
let dst_ip = dst_ip.to_string();
let stats = self.stats.clone();
let tun_fd = self.tun_fd;

tokio::spawn(async move {
    let tun = TunDevice::from_fd(tun_fd, "").unwrap();
    if let Err(e) = proxy::pipe_tcp(&ssh, &dst_ip, dst_port, &tun, &stats).await {
        tracing::error!("Proxy {} failed: {}", stream_key, e);
    }
});
```

The PacketRouter needs to hold `stats: Arc<ConnectionStats>` and `tun_fd: RawFd`. Add these fields to the struct.

- [ ] **Step 5: Verify Rust compiles**

Run: `cd src-tauri && cargo check`
Expected: Compiles (warnings OK)

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/tunnel/
git commit -m "feat: rename socks5 to proxy and wire packet forwarding"
```

---

### Task 2: Add ConnectionStats to tunnel module

**Files:**
- Modify: `src-tauri/src/tunnel/mod.rs`

- [ ] **Step 1: Add ConnectionStats struct**

Add at the top of `src-tauri/src/tunnel/mod.rs` before the Tunnel struct:

```rust
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct StatsSnapshot {
    pub bytes_up: u64,
    pub bytes_down: u64,
    pub uptime_secs: u64,
}

pub struct ConnectionStats {
    bytes_up: AtomicU64,
    bytes_down: AtomicU64,
    started_at: Instant,
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
```

- [ ] **Step 2: Verify Rust compiles**

Run: `cd src-tauri && cargo check`
Expected: Compiles

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/tunnel/mod.rs
git commit -m "feat: add ConnectionStats with atomic counters"
```

---

### Task 3: Wire stats and heartbeat into lib.rs

**Files:**
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: Add stats emitter and heartbeat to connect_tunnel**

Read `src-tauri/src/lib.rs`. After the successful `tunnel.start()` call, add:

```rust
// After tunnel.start() Ok(()) branch, before storing in state:
let stats = Arc::new(ConnectionStats::new());
let stats_for_emit = stats.clone();
let app_for_stats = app.clone();

// Emit stats every 1 second
tokio::spawn(async move {
    loop {
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        let snapshot = stats_for_emit.snapshot();
        let _ = app_for_stats.emit("connection-stats", &snapshot);
    }
});
```

Pass `stats.clone()` to the Tunnel struct when creating it.

- [ ] **Step 2: Add stats field to Tunnel**

Update Tunnel struct to include `stats: Arc<ConnectionStats>`. Update `Tunnel::new()` to accept and store it.

- [ ] **Step 3: Add heartbeat to connect_tunnel**

After the stats emitter, add:

```rust
// Heartbeat: ping helper every 10 seconds
let mut helper_for_heartbeat = /* clone helper connection */;
let app_for_heartbeat = app.clone();
tokio::spawn(async move {
    let mut failures = 0u32;
    loop {
        tokio::time::sleep(std::time::Duration::from_secs(10)).await;
        if helper_for_heartbeat.send_ping().is_err() {
            failures += 1;
            if failures >= 2 {
                let _ = app_for_heartbeat.emit("connection-state", "disconnected");
                break;
            }
        } else {
            failures = 0;
        }
    }
});
```

Add a `send_ping` method to HelperClient:
```rust
pub fn send_ping(&mut self) -> Result<(), AppError> {
    self.send_command(r#"{"cmd":"ping"}"#)?;
    let response = self.read_response()?;
    if response.get("ok").and_then(|v| v.as_bool()) != Some(true) {
        return Err(AppError::Tunnel("ping failed".to_string()));
    }
    Ok(())
}
```

- [ ] **Step 4: Verify Rust compiles**

Run: `cd src-tauri && cargo check`
Expected: Compiles

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/lib.rs src-tauri/src/tunnel/mod.rs src-tauri/src/helper/client.rs
git commit -m "feat: add stats emitter and helper heartbeat to connect flow"
```

---

### Task 4: Add auto-reconnect with exponential backoff

**Files:**
- Modify: `src-tauri/src/tunnel/mod.rs` (add profile_id, config fields + reconnect method)
- Modify: `src-tauri/src/lib.rs` (spawn reconnect reaper)

- [ ] **Step 1: Add reconnect fields to Tunnel**

```rust
pub struct Tunnel {
    pub ssh_client: Option<Arc<SshClient>>,
    pub router_handle: Option<tokio::task::JoinHandle<()>>,
    pub tun_name: Option<String>,
    pub stats: Arc<ConnectionStats>,
    pub config: Option<TunnelConfig>, // stored for reconnect
}

impl Tunnel {
    pub fn new(stats: Arc<ConnectionStats>) -> Self { ... }
    
    pub fn set_config(&mut self, config: TunnelConfig) {
        self.config = Some(config);
    }
}
```

- [ ] **Step 2: Add reconnect method to Tunnel**

```rust
impl Tunnel {
    pub async fn reconnect(
        &mut self,
        helper: &mut HelperClient,
        app: &tauri::AppHandle,
    ) -> Result<(), AppError> {
        let config = self.config.as_ref()
            .ok_or_else(|| AppError::Tunnel("no config for reconnect".to_string()))?;

        let (tun_name, tun_fd) = helper.create_tun()?;
        helper.add_route(&tun_name)?;
        self.start(config.clone(), tun_fd, &tun_name).await?;
        Ok(())
    }
}
```

- [ ] **Step 3: Spawn reconnect reaper in lib.rs**

After `tunnel.start()` succeeds, spawn:

```rust
let tunnel_arc = /* reference to tunnel */;
let helper_arc = /* reference to helper */;
let app_for_reconnect = app.clone();
let db_for_reconnect = state.db.clone();
let profile_id_for_reconnect = profile_id.clone();

tokio::spawn(async move {
    // Wait for router handle to exit
    if let Some(handle) = tunnel_arc.router_handle.take() {
        let _ = handle.await;
    }

    // Router exited — attempt reconnect
    for attempt in 1u32..=10 {
        let delay = std::time::Duration::from_secs(2u64.pow(attempt).min(60));
        tokio::time::sleep(delay).await;

        app_for_reconnect.emit("connection-state", "reconnecting").unwrap();

        match tunnel_arc.reconnect(&mut helper_arc, &app_for_reconnect).await {
            Ok(()) => {
                app_for_reconnect.emit("connection-state", "tunnel-active").unwrap();
                return;
            }
            Err(e) => {
                emit_log(&app_for_reconnect, &db_for_reconnect, "error",
                    &format!("Reconnect attempt {} failed: {}", attempt, e),
                    Some(&profile_id_for_reconnect)).await;
            }
        }
    }

    app_for_reconnect.emit("connection-state", "disconnected").unwrap();
});
```

- [ ] **Step 4: Verify Rust compiles**

Run: `cd src-tauri && cargo check`
Expected: Compiles

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/tunnel/mod.rs src-tauri/src/lib.rs
git commit -m "feat: add auto-reconnect with exponential backoff"
```

---

### Task 5: Update frontend stores for stats and reconnect state

**Files:**
- Modify: `src/lib/stores/connection.ts`
- Modify: `src/lib/tauri.ts`

- [ ] **Step 1: Extend connection.ts**

```typescript
import { writable } from 'svelte/store';

export type ConnectionState =
  | 'disconnected'
  | 'connecting'
  | 'authenticating'
  | 'tunnel-active'
  | 'reconnecting'
  | 'error';

export const connectionState = writable<ConnectionState>('disconnected');
export const connectionError = writable<string>('');

export interface StatsSnapshot {
  bytes_up: number;
  bytes_down: number;
  uptime_secs: number;
}

export const connectionStats = writable<StatsSnapshot | null>(null);
```

- [ ] **Step 2: Add stats listener to tauri.ts**

```typescript
import { listen } from '@tauri-apps/api/event';
import { connectionStats, type StatsSnapshot } from './stores/connection';

export function listenConnectionStats(callback: (stats: StatsSnapshot) => void) {
  return listen<StatsSnapshot>('connection-stats', (event) => {
    callback(event.payload);
  });
}

export function syncConnectionStats() {
  const unlisten = listenConnectionStats((stats) => {
    connectionStats.set(stats);
  });
  return unlisten;
}
```

Also update `syncConnectionState` to handle the `reconnecting` state:
```typescript
export function syncConnectionState() {
  const unlisten = listenConnectionState((state) => {
    connectionState.set(state as ConnectionState);
  });
  return unlisten;
}
```

- [ ] **Step 3: Verify frontend builds**

Run: `npm run check && npm run build`
Expected: No errors

- [ ] **Step 4: Commit**

```bash
git add src/lib/stores/connection.ts src/lib/tauri.ts
git commit -m "feat: add connection stats and reconnect state to frontend stores"
```

---

### Task 6: Create ConnectionStats component

**Files:**
- Create: `src/lib/components/connection-stats.svelte`

- [ ] **Step 1: Create component**

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
        <span class="inline-block w-2 h-2 rounded-full bg-green-500"></span>
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
    <span class="text-sm text-yellow-700">⟳ Reconnecting...</span>
  </div>
{/if}
```

- [ ] **Step 2: Verify frontend builds**

Run: `npm run check && npm run build`
Expected: No errors

- [ ] **Step 3: Commit**

```bash
git add src/lib/components/connection-stats.svelte
git commit -m "feat: add ConnectionStats component with bytes and uptime"
```

---

### Task 7: Update ProfileCard for reconnect state

**Files:**
- Modify: `src/lib/components/profile-card.svelte`

- [ ] **Step 1: Update button logic**

Change the button rendering in ProfileCard:

```svelte
{#if connectionState === 'disconnected' || connectionState === 'error'}
  <Button onclick={() => onConnect(profile.id)} disabled={loading} size="sm">
    Connect
  </Button>
{:else if connectionState === 'reconnecting'}
  <Button onclick={onDisconnect} variant="destructive" size="sm">
    Cancel
  </Button>
{:else if connectionState === 'connecting' || connectionState === 'authenticating'}
  <Button disabled size="sm">
    Connecting...
  </Button>
{:else}
  <Button onclick={onDisconnect} disabled={disconnecting} variant="destructive" size="sm">
    Disconnect
  </Button>
{/if}
```

- [ ] **Step 2: Verify frontend builds**

Run: `npm run check && npm run build`

- [ ] **Step 3: Commit**

```bash
git add src/lib/components/profile-card.svelte
git commit -m "feat: add reconnect and connecting states to ProfileCard"
```

---

### Task 8: Integrate stats into main page

**Files:**
- Modify: `src/routes/+page.svelte`

- [ ] **Step 1: Add ConnectionStats + listen stats**

Add import:
```svelte
import ConnectionStats from '$lib/components/connection-stats.svelte';
import { syncConnectionStats } from '$lib/tauri';
```

Add to `onMount`:
```svelte
const unlistenStats = syncConnectionStats();
```

Add to cleanup:
```svelte
return () => {
  unlisten.then(fn => fn());
  unlistenLogs.then(fn => fn());
  unlistenStats.then(fn => fn());
};
```

Add the component above the profile list:
```svelte
<ConnectionStats />
```

- [ ] **Step 2: Verify frontend builds**

Run: `npm run check && npm run build`

- [ ] **Step 3: Commit**

```bash
git add src/routes/+page.svelte
git commit -m "feat: integrate ConnectionStats into main page"
```

---

### Task 9: Final integration and verification

**Files:**
- Verify all changes

- [ ] **Step 1: Verify all builds**

```bash
cd helper && cargo check && cd ..
cd src-tauri && cargo check && cd ..
npm run check && npm run build
```

- [ ] **Step 2: Stage remaining files**

```bash
git status
git add -A
```

- [ ] **Step 3: Tag milestone**

```bash
git tag -a m6-stats-health -m "M6: Connection stats, packet proxy, auto-reconnect, heartbeat"
```

- [ ] **Step 4: Summary**

What was built:
- proxy.rs: Bidirectional TCP forwarding through SSH with stats
- ConnectionStats: Atomic counters for bytes up/down + uptime
- Stats emitter: 1-second interval event to frontend
- Heartbeat: 10-second ping to privileged helper
- Auto-reconnect: Exponential backoff (2s → 60s, 10 attempts)
- Frontend: ConnectionStats component, reconnect state, ProfileCard states

# M1 — Core Tunnel Design Specification

> **Date:** 2026-05-20
> **Milestone:** M1 — Core Tunnel
> **Status:** Approved

## 1. Goal

Route actual network traffic through an SSH server using a TUN device + userspace TCP/IP stack (smoltcp). By the end of M1, clicking "Connect" in the UI will route all system traffic through a hardcoded SSH server.

## 2. Architecture

```
┌─────────────┐     ┌──────────────┐     ┌─────────────┐     ┌─────────────┐
│   TUN Dev   │────▶│ Packet Router│────▶│  SOCKS5     │────▶│   SSH       │
│  (tun/utun) │     │  (smoltcp)   │     │   Engine     │     │  (russh)    │
└─────────────┘     └──────────────┘     └─────────────┘     └──────┬──────┘
       ▲                                                              │
       │                                                              ▼
       │                                                       ┌─────────────┐
       └───────────────────────────────────────────────────────│   Remote    │
                                                               │   sshd      │
                                                               └─────────────┘
```

## 3. Components

| Component | Responsibility | File |
|-----------|---------------|------|
| **SSH Client** | Connect via russh, open dynamic forwarding channel | `src/ssh/client.rs` |
| **TUN Device** | Create OS TUN interface (macOS utun) | `src/tunnel/tun_device.rs` |
| **Packet Router** | Read raw IP from TUN, feed to smoltcp, extract TCP/UDP streams | `src/tunnel/packet_router.rs` |
| **SOCKS5 Engine** | Wrap streams as SOCKS5 CONNECT over SSH channel | `src/tunnel/socks5.rs` |
| **Route Manager** | Inject default route on connect, cleanup on disconnect | `src/tunnel/route_manager.rs` |
| **Connection Events** | Emit Tauri events: Connecting → Authenticating → Tunnel Active | `src/lib.rs` + frontend store |

## 4. Data Flow

1. User clicks **Connect** on a profile
2. Rust emits `connection-state: "connecting"`
3. Privilege helper creates TUN device + injects default route
4. Rust emits `connection-state: "authenticating"`
5. russh connects to SSH server, opens dynamic forwarding channel (`-D` equivalent)
6. Rust emits `connection-state: "tunnel-active"`
7. smoltcp reads IP packets from TUN fd
8. Each TCP/UDP stream → SOCKS5 engine → SSH channel → remote sshd → internet
9. On disconnect: remove routes, destroy TUN, emit `connection-state: "disconnected"`

## 5. Key Decisions

- **smoltcp 0.11** for userspace TCP/IP (no iptables, works cross-platform)
- **One active tunnel at a time** (simplifies state management for v1)
- **Blocking I/O in a dedicated thread** for TUN reads (simpler than async smoltcp integration)
- **Tauri events** for state streaming (not channels — low frequency, simple states)

## 6. Scope

### In Scope (M1)
- SSH connection with password auth (hardcoded for testing)
- TUN device creation on macOS (utun) — defer Windows/Linux to later milestone
- smoltcp integration with basic TCP routing
- Simple SOCKS5 CONNECT proxy
- Default route injection on macOS
- Connection state events to frontend

### Out of Scope (M2+)
- UDP routing through SOCKS5
- DNS leak prevention
- Auto-reconnect
- Profile encryption / SQLite storage
- Windows/Linux TUN

## 7. File Structure

```
src-tauri/src/
├── main.rs              # Thin passthrough (existing)
├── lib.rs               # Tauri builder + tunnel orchestration
├── tunnel/
│   ├── mod.rs           # Tunnel orchestrator (start/stop, state machine)
│   ├── tun_device.rs    # macOS utun creation
│   ├── packet_router.rs # smoltcp integration
│   ├── socks5.rs        # SOCKS5 CONNECT proxy
│   └── route_manager.rs # macOS route injection/cleanup
└── ssh/
    ├── mod.rs           # SSH connection manager
    └── client.rs        # russh client + dynamic forwarding
```

## 8. Dependencies

```toml
[dependencies]
# ... existing deps ...
russh = "0.44"
russh-keys = "0.44"
tokio = { version = "1", features = ["full"] }
smoltcp = "0.11"
tun = { version = "0.7", features = ["async"] }
```

## 9. Frontend Changes

- Add connection state store (`src/lib/stores/connection.ts`)
- Update `+page.svelte` to show connection state and Connect/Disconnect buttons
- Listen to `connection-state` Tauri event

## 10. Acceptance Criteria

- [ ] `npm run tauri dev` opens app with "Connect" button
- [ ] Clicking "Connect" with hardcoded credentials creates TUN device
- [ ] System traffic routes through SSH server (verify with `curl ipinfo.io`)
- [ ] Connection state updates in real-time (Connecting → Authenticating → Tunnel Active)
- [ ] Clicking "Disconnect" cleans up routes and TUN device
- [ ] No system reboot required after disconnect

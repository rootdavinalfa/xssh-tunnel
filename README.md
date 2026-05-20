# XSSH Tunnel

A cross-platform desktop VPN application that routes all system traffic through any SSH server with **zero server-side configuration required**. Built with Tauri 2, Rust, and SvelteKit.

![Version](https://img.shields.io/badge/version-0.1.0-blue)
![Platform](https://img.shields.io/badge/platform-macOS%20%7C%20Windows%20%7C%20Linux-lightgrey)
![Stack](https://img.shields.io/badge/stack-Tauri%202%20%7C%20Rust%20%7C%20SvelteKit-orange)

## What is XSSH Tunnel?

XSSH Tunnel turns any SSH server into a full VPN in under 60 seconds. Unlike traditional VPN tools that require installing server daemons, configuring firewall rules, or purchasing dedicated VPN infrastructure, XSSH Tunnel works over standard OpenSSH on any Linux server using only client-side technology.

**Design principle: zero server setup**
- The server needs nothing beyond a running `sshd` and a user account
- No root access required on the server
- No custom software to install
- No firewall changes
- No kernel modules

If you can SSH into it, you can VPN through it.

## Features

### Core
- [ ] **One-click connect** — TUN creation, SSH auth, route injection, and DNS proxy startup
- [ ] **Zero server configuration** — Works with any standard SSH server
- [ ] **Multiple connection profiles** — Manage multiple SSH servers with encrypted local storage
- [ ] **Import from SSH config** — Parse existing `~/.ssh/config` files
- [ ] **Cross-platform** — Native builds for macOS, Windows, and Linux

### Security
- [ ] **AES-256-GCM encryption** — All credentials encrypted at rest with OS keychain-backed master key
- [ ] **Host key verification** — Warns on fingerprint mismatches
- [ ] **No credential logging** — Passwords and keys never appear in logs
- [ ] **No cloud sync** — All data stays on your device

### User Experience
- [ ] **System tray integration** — Minimize to tray, control from menu bar
- [ ] **Real-time connection status** — Live state updates: Connecting → Authenticating → Tunnel Active
- [ ] **Auto-reconnect** — Automatic reconnection with exponential backoff
- [ ] **Live traffic stats** — Bytes transferred counter
- [ ] **Virtual scroll logs** — High-performance log viewer

## Tech Stack

| Layer | Technology |
|-------|-----------|
| **Desktop shell** | Tauri 2 |
| **UI framework** | SvelteKit 5 (SPA mode) |
| **UI components** | shadcn-svelte + Tailwind CSS |
| **Data fetching** | TanStack Query |
| **Tables** | TanStack Table |
| **SSH client** | russh 0.44 (pure Rust) |
| **TUN interface** | tun2 (Linux/macOS) + Wintun (Windows) |
| **Userspace TCP/IP** | smoltcp 0.11 |
| **Database** | SQLite via sqlx |
| **Encryption** | AES-256-GCM via ring crate |

## Why SvelteKit?

SvelteKit is chosen over React for:
- **No virtual DOM overhead** — Direct DOM manipulation reduces CPU/memory in the WebView
- **Smaller bundle size** — 30-60% smaller than equivalent React bundles
- **First-class reactivity** — `$state` and `$derived` runes replace `useState`/`useEffect` boilerplate
- **Better performance** — Critical for an always-on system tray application

## Project Structure

```
xssh-tunnel/
├── src/                          # SvelteKit frontend
│   ├── lib/
│   │   ├── components/          # shadcn-svelte components
│   │   ├── stores/              # Svelte stores (connection state)
│   │   └── tauri.ts             # Type-safe Tauri IPC wrappers
│   ├── routes/
│   │   ├── +layout.svelte       # Root layout
│   │   ├── +page.svelte         # Connections list (default)
│   │   ├── connections/         # Add/edit profiles
│   │   ├── settings/            # Import SSH config
│   │   └── logs/                # Live log viewer
│   └── app.html
├── src-tauri/                    # Rust backend
│   ├── src/
│   │   ├── main.rs              # Thin passthrough
│   │   ├── lib.rs               # All application logic
│   │   ├── tunnel/              # TUN device, smoltcp, SOCKS5
│   │   ├── ssh/                 # russh client, config parser
│   │   ├── db/                  # SQLite schema, migrations
│   │   ├── crypto/              # AES-256-GCM, keychain
│   │   └── profiles/            # Profile CRUD operations
│   ├── capabilities/            # Tauri v2 permissions
│   └── tauri.conf.json          # App configuration
└── docs/                         # Documentation
    └── superpowers/
        ├── specs/               # Design specifications
        └── plans/               # Implementation plans
```

## Architecture

### Traffic Flow

When a user clicks Connect:

1. **Privilege helper** creates TUN device (`tun0`/`utun`/Wintun) and injects default route
2. **SSH manager** authenticates and opens dynamic forwarding channel (`ssh -D` equivalent)
3. **Packet router** reads raw IP packets from TUN via smoltcp, reassembles TCP/UDP streams
4. **SOCKS5 engine** wraps each stream as `CONNECT` or `UDP ASSOCIATE` request over SSH channel
5. **Remote sshd** proxies requests through its network interface to the internet
6. **Response packets** travel back through the same path and written to TUN device

### Why smoltcp?

Instead of iptables REDIRECT (used by sshuttle), smoltcp runs entirely in userspace after TUN creation. This means:
- Privilege escalation only once at connect time (not continuously)
- Works on macOS and Windows without compatibility layers
- No per-packet iptables manipulation required

## Development

### Prerequisites

- [Rust](https://rustup.rs/) 1.70+
- [Node.js](https://nodejs.org/) 18+
- Platform-specific tools:
  - **macOS**: Xcode Command Line Tools
  - **Windows**: Visual Studio Build Tools, Wintun driver
  - **Linux**: `libssl-dev`, `libgtk-3-dev`, `libwebkit2gtk-4.0-dev`

### Setup

```bash
# Clone the repository
git clone <repository-url>
cd xssh-tunnel

# Install frontend dependencies
npm install

# Install Rust dependencies (handled automatically by Tauri)
# Run development server
npm run tauri dev
```

### Build

```bash
# Development build
npm run tauri build -- --debug

# Production build
npm run tauri build
```

### Testing

```bash
# Run Rust tests
cd src-tauri && cargo test

# Run frontend tests
npm test
```

## Roadmap

### Milestone 0 — Skeleton (Week 1-2)
- [ ] Tauri 2 + SvelteKit scaffolded
- [ ] shadcn-svelte installed and themed
- [ ] Rust SSH connection to hardcoded host
- [ ] TUN device creation on all 3 platforms

### Milestone 1 — Core Tunnel (Week 3-5)
- [ ] smoltcp integration
- [ ] SOCKS5 engine
- [ ] Traffic routing through SSH
- [ ] Default route injection/cleanup
- [ ] Real-time connection state events

### Milestone 2 — SSH Connection Manager (Week 6-7)
- [ ] SQLite schema and migrations
- [ ] AES-256-GCM field-level encryption
- [ ] Full CRUD UI for profiles
- [ ] SSH agent and private key auth

### Milestone 3 — SSH Config Import (Week 8-9)
- [ ] `~/.ssh/config` parser
- [ ] Import dialog with conflict resolution
- [ ] ProxyJump support
- [ ] Include directive handling

### Milestone 4 — Polish (Week 10-11)
- [ ] System tray integration
- [ ] Auto-reconnect with backoff
- [ ] DNS leak prevention
- [ ] Known host key verification
- [ ] Live log viewer

### Milestone 5 — Beta & Release (Week 12-16)
- [ ] Signed and notarized builds
- [ ] Internal beta testing
- [ ] P2 features based on feedback
- [ ] Public release

## License

[License TBD]

## Contributing

See [AGENTS.md](./AGENTS.md) for development guidelines and skill references.

---

*Built with ❤️ using Tauri 2, Rust, and SvelteKit*

# XSSH Tunnel — Product Requirements Document

**Version:** 1.0.0 · **Status:** Draft · **Date:** May 2026
**Platform:** macOS · Windows · Linux · **Stack:** Tauri 2 · Rust · SvelteKit

---

## Table of Contents

1. [Product Overview](#1-product-overview)
2. [Goals & Non-Goals](#2-goals--non-goals)
3. [Target Users](#3-target-users)
4. [Technical Architecture](#4-technical-architecture)
5. [SSH Connection Manager](#5-ssh-connection-manager)
6. [Feature Requirements](#6-feature-requirements)
7. [Key User Flows](#7-key-user-flows)
8. [Non-Functional Requirements](#8-non-functional-requirements)
9. [Milestones](#9-milestones)
10. [Open Questions](#10-open-questions)

---

## 1. Product Overview

XSSH Tunnel is a cross-platform desktop VPN application built with Tauri 2. It routes all client traffic through any SSH server the user already has access to — with **zero server-side configuration required**. Users connect using existing SSH credentials: host, username, and a password or private key.

Unlike traditional VPN tools that require installing server daemons, configuring firewall rules, or purchasing dedicated VPN infrastructure, XSSH Tunnel works over standard OpenSSH on any Linux server. The VPN tunnel is implemented entirely on the client side using a TUN network interface and a userspace TCP/IP stack.

> **Design principle: zero server setup**
> The server needs nothing beyond a running `sshd` and a user account. No root access, no custom software, no firewall changes, no kernel module. If you can SSH into it, you can VPN through it.

---

## 2. Goals & Non-Goals

### 2.1 Goals

- Let users turn any SSH server into a VPN in under 60 seconds
- Manage multiple SSH connection profiles locally, encrypted at rest
- Import existing SSH profiles from the system SSH config file (`~/.ssh/config`)
- Route all system traffic through the tunnel with a single click
- Work on macOS, Windows, and Linux without platform-specific user steps
- Require admin/root only once per connect (TUN device creation), not for ongoing usage

### 2.2 Non-Goals

- Not a general-purpose SSH client or terminal emulator
- Not a managed VPN service — XSSH Tunnel never operates its own servers
- Not a replacement for WireGuard or OpenVPN where dedicated VPN infrastructure already exists
- No mobile clients in v1 (iOS / Android deferred)
- No multi-hop / chained tunnels in v1

---

## 3. Target Users

| User Type | Description |
|---|---|
| **Developer / DevOps** | Has one or more VPS servers for personal or work projects. Wants to quickly route traffic through a server in a specific region without setting up WireGuard or a full VPN stack. |
| **Remote worker** | Accesses company infrastructure via SSH. Wants a VPN-like experience without asking IT to install server software. |
| **Power user** | Manages multiple SSH servers and wants a clean UI to switch between VPN exit nodes without touching terminal commands. |
| **Privacy-conscious user** | Owns a cheap VPS and wants to route browser traffic through it when on public Wi-Fi. No interest in subscription VPN services. |

---

## 4. Technical Architecture

### 4.1 Technology Stack

| Layer | Technology |
|---|---|
| **Desktop shell** | Tauri 2 (Rust back-end, WebView front-end) |
| **UI framework** | SvelteKit + TypeScript |
| **UI component kit** | shadcn-svelte (Bits UI primitives, Tailwind CSS) |
| **Tanstack Query** | Tanstack Query is a lightweight, type-safe, and reactive data querying library for SvelteKit.
| **Tanstack Table** | Tanstack Table is a powerful and flexible data table component for SvelteKit.
| **SSH client** | `russh` 0.44 — pure-Rust async SSH2, no libssh2 C dependency |
| **Key parsing** | `russh-keys` 0.44 — PEM, OpenSSH, passphrase-protected keys, ssh-agent |
| **TUN interface** | `tun2` (Linux / macOS) + Wintun (Windows via WireGuard driver) |
| **Userspace TCP/IP** | `smoltcp` 0.11 — reads raw IP packets off TUN, reassembles TCP/UDP streams |
| **SOCKS5 proxy engine** | Custom async SOCKS5 engine built on top of smoltcp streams |
| **Local database** | SQLite via `sqlx` (async) — stores encrypted connection profiles |
| **Encryption at rest** | AES-256-GCM via the `ring` crate — key derived from system keychain secret |
| **Privilege helper** | Separate signed binary (elevated) for TUN creation and route injection |
| **OS route management** | `ip route` (Linux), `networksetup` + `route` (macOS), `netsh` (Windows) |

### 4.2 Why SvelteKit over React

SvelteKit is chosen over React for the Tauri front-end for the following reasons:

- **No virtual DOM overhead** — Svelte compiles to direct DOM manipulation, reducing CPU and memory usage inside the WebView, which is especially relevant for the always-on system tray process.
- **Smaller bundle size** — Svelte ships no runtime framework; the compiled output for a typical screen is 30–60% smaller than an equivalent React bundle, improving startup time.
- **First-class reactivity** — Svelte's `$state` and `$derived` runes replace `useState` / `useEffect` boilerplate, making real-time connection status updates and live byte counters straightforward to implement without extra state libraries.
- **shadcn-svelte** provides the same high-quality, accessible component primitives as the React shadcn/ui library, built on Bits UI and Tailwind CSS, so the UI kit is not a compromise.

### 4.3 SvelteKit + Tauri integration

SvelteKit is configured in **SPA mode** (`adapter-static` with `fallback: 'index.html'`) so Tauri serves the pre-built static output without a Node.js server. All back-end logic runs in the Rust Tauri process; the SvelteKit front-end communicates exclusively via `invoke()` calls and Tauri event listeners.

```
src/
├── lib/
│   ├── components/       # shadcn-svelte components (Button, Dialog, Badge, etc.)
│   ├── stores/           # Svelte stores for connection state, profile list
│   └── tauri.ts          # Type-safe wrappers around invoke() and listen()
├── routes/
│   ├── +layout.svelte    # Root layout, system tray aware
│   ├── +page.svelte      # Connections list (default view)
│   ├── connections/
│   │   ├── new/          # Add profile form
│   │   └── [id]/edit/    # Edit profile form
│   ├── settings/         # Import SSH config, preferences
│   └── logs/             # Live connection log viewer
└── app.html
```

### 4.4 Traffic flow

The following describes what happens when a user clicks Connect:

1. The privilege helper creates a TUN device (`tun0` / `utun` / Wintun) and injects a default route pointing all traffic at it.
2. The SSH manager authenticates to the remote server using stored credentials and opens a dynamic forwarding channel (equivalent to `ssh -D`).
3. The packet router reads raw IP packets from the TUN file descriptor via smoltcp, which reassembles them into TCP and UDP streams.
4. Each stream is wrapped as a SOCKS5 `CONNECT` or `UDP ASSOCIATE` request and sent over the SSH channel.
5. The remote `sshd` proxies requests out through its own network interface to the internet.
6. Response packets travel back the same path in reverse and are written back into the TUN device.

> **Why smoltcp instead of iptables REDIRECT?**
> The iptables approach (used by sshuttle) requires client-side root for every packet manipulation and does not work on macOS or Windows without a compatibility layer. smoltcp runs entirely in userspace after the TUN device is created, which means privilege escalation is needed only once at connect time — not continuously.

---

## 5. SSH Connection Manager

The SSH Connection Manager is the core data-management layer of XSSH Tunnel. It is the single source of truth for all stored SSH profiles. Users manage all their servers here without ever leaving the app.

### 5.1 Local Encrypted Storage (SQLite)

All connection profiles are stored in a SQLite database located in the application data directory on the user's machine. The database is never synced to any cloud service or remote server.

#### 5.1.1 Encryption scheme

- On first launch, XSSH Tunnel generates a 256-bit random master key and stores it in the system's secure credential store (macOS Keychain, Windows Credential Manager, Linux libsecret / kwallet).
- All sensitive fields in the database are encrypted with **AES-256-GCM** using this master key before being written to disk.
- **Encrypted fields:** `password`, `private_key_enc`, `key_passphrase_enc`. Non-sensitive fields (`label`, `host`, `port`, `username`) are stored in plaintext to support search and display without decryption overhead.
- Each encrypted field uses a unique random **96-bit nonce** stored alongside the ciphertext.
- The SQLite file itself is a standard SQLite3 database — field-level encryption is used rather than whole-file encryption so the file remains inspectable for debugging without exposing secrets.

#### 5.1.2 Database schema

| Column | Type | Notes |
|---|---|---|
| `id` | TEXT (UUID) | Primary key, generated client-side |
| `label` | TEXT | User-defined display name — plaintext |
| `host` | TEXT | Hostname or IP address — plaintext |
| `port` | INTEGER | SSH port, default 22 — plaintext |
| `username` | TEXT | SSH username — plaintext |
| `auth_type` | TEXT | Enum: `password` \| `key` \| `key_path` \| `agent` — plaintext |
| `password_enc` | BLOB | AES-256-GCM ciphertext of password (nullable) |
| `private_key_enc` | BLOB | AES-256-GCM ciphertext of full PEM / OpenSSH key material (nullable) |
| `key_passphrase_enc` | BLOB | AES-256-GCM ciphertext of key passphrase (nullable) |
| `identity_file_path` | TEXT | Filesystem path to key file — plaintext — used when key is managed externally (nullable) |
| `jump_host_id` | TEXT | FK to another profile for ProxyJump support (nullable) |
| `known_host_key` | TEXT | Server host key fingerprint accepted by the user (nullable) |
| `server_alive_interval` | INTEGER | KeepAlive interval in seconds (nullable) |
| `last_connected_at` | TEXT | ISO-8601 timestamp of most recent successful connection |
| `created_at` | TEXT | ISO-8601 profile creation timestamp |
| `source` | TEXT | Enum: `manual` \| `imported` — tracks origin of the profile |

#### 5.1.3 Data never leaves the device

XSSH Tunnel makes no outbound network calls on behalf of the SSH Connection Manager. Profile data is read from and written to the local SQLite file only. No telemetry, no backup service, no sync endpoint.

### 5.2 Managing Profiles

#### 5.2.1 Creating a profile manually

1. From the Connections screen, the user clicks **+ New Connection**.
2. Required fields: Label, Host, Username.
3. Optional fields: Port (default 22), Authentication method.
4. Authentication method options:
   - **Password** — password field, stored encrypted.
   - **Private key (inline)** — paste key material directly or choose a file. Key material is read, encrypted, and stored in the database. The original file is not modified.
   - **Key file path** — store only the path, not the key content. The app reads the key at connect time. Useful when the key is managed externally.
   - **SSH agent** — delegate authentication to the running `ssh-agent` or equivalent (macOS Keychain, Windows OpenSSH agent, GPG agent).
5. The app validates the connection by attempting an SSH handshake before saving.

#### 5.2.2 Editing and deleting profiles

- All fields except `id` and `created_at` are editable after creation.
- Deleting a profile permanently removes the row and all associated encrypted blobs from the database.
- The currently active VPN connection cannot be edited or deleted until disconnected.

#### 5.2.3 Duplicate and quick-clone

- Users can duplicate an existing profile to create a variant (e.g. same server, different username or port).
- Encrypted fields are re-encrypted under new nonces in the duplicate.

### 5.3 Importing from SSH Config

XSSH Tunnel can parse the user's existing OpenSSH client configuration (`~/.ssh/config`) and import hosts as connection profiles. This removes the need to manually re-enter servers the user has already configured.

#### 5.3.1 Import flow

1. The user opens **Settings → Import from SSH Config**.
2. XSSH Tunnel reads `~/.ssh/config` (and any files referenced by `Include` directives).
3. All `Host` blocks with a concrete hostname are parsed and displayed in a selectable checklist.
4. The user selects which hosts to import (select all or individually).
5. XSSH Tunnel creates a new profile for each selected host, populating all fields it can derive from the config block.
6. If a profile with the same host + username + port already exists, XSSH Tunnel shows a conflict resolution prompt per profile: **Skip**, **Overwrite**, or **Import as duplicate**.
7. Imported profiles are tagged `source = imported` for traceability.

#### 5.3.2 Fields parsed from SSH config

| SSH config keyword | Maps to profile field |
|---|---|
| `Host` (alias) | `label` |
| `HostName` | `host` |
| `User` | `username` |
| `Port` | `port` |
| `IdentityFile` | `identity_file_path` |
| `ProxyJump` | `jump_host_id` (resolved by matching to existing or other imported profiles) |
| `ServerAliveInterval` | `server_alive_interval` |

#### 5.3.3 What is NOT imported

- **Passwords** — SSH config files never contain passwords; the user must add them after import if needed.
- **Private key contents** — only the file path is imported, not the key material itself.
- **Wildcard `Host *` blocks** — these apply globally to all connections and are not valid as individual profiles.
- **`Match` blocks** — complex conditional directives are skipped with a warning shown to the user.

> **Re-import behaviour**
> Running import again does not automatically overwrite existing profiles. Each conflict is surfaced individually so users retain full control over which profiles are updated.

---

## 6. Feature Requirements

### 6.1 Connection lifecycle

| Feature | Requirement | Priority |
|---|---|---|
| One-click connect | Select a profile and click Connect. The app handles TUN creation, SSH auth, route injection, and DNS proxy startup automatically. | P0 |
| One-click disconnect | Tears down the tunnel, removes injected routes, and destroys the TUN device. No reboot required. | P0 |
| Auto-reconnect | If the SSH connection drops unexpectedly, XSSH Tunnel attempts reconnection up to 5 times with exponential backoff before surfacing an error. | P1 |
| Connection status | Real-time display of state: `Disconnected` → `Connecting` → `Authenticating` → `Tunnel active` → `Reconnecting` → `Error`. Driven by Tauri events emitted from the Rust core to the SvelteKit store. | P0 |
| Bytes transferred | Live up/down byte counter shown while connected, updated every second via Tauri event. | P2 |
| System tray / menu bar | App minimises to the system tray. Connection state is visible from the tray icon. Connect/disconnect actions available without opening the main window. | P1 |

### 6.2 SSH manager UI

| Feature | Requirement | Priority |
|---|---|---|
| Profile list | Displays all saved profiles with label, host, and username. Built with shadcn-svelte `Card` components. Sortable by label or last connected. | P0 |
| Profile search | Real-time filter using Svelte's reactive `$derived` — no debounce needed, filters on every keystroke. shadcn-svelte `Input` component. | P1 |
| Add / edit profile | Form using shadcn-svelte `Form`, `Input`, `Select`, `Textarea`. Inline validation, error messages, and a **Test Connection** button that verifies credentials before saving. | P0 |
| Import from SSH config | Accessible from Settings. Uses shadcn-svelte `Dialog` and `Checkbox` list. Conflict resolution shown in the same dialog. | P0 |
| Profile tags / groups | User-defined tags (e.g. `work`, `personal`, `region:sg`) for organising large numbers of profiles. Rendered as shadcn-svelte `Badge` components. | P2 |
| Last connected indicator | Each profile card shows relative time of most recent connection (e.g. `2 days ago`). | P2 |

### 6.3 Security

| Feature | Requirement | Priority |
|---|---|---|
| Encrypted local storage | All credential fields encrypted with AES-256-GCM before SQLite write. Master key stored in OS keychain. | P0 |
| No plaintext credential logs | Log output must never include passwords, key material, or passphrases. Host, port, and username may appear in logs. | P0 |
| Host key verification | On first connect, the server's host key fingerprint is saved to the profile's `known_host_key` field. On subsequent connects, mismatches surface a shadcn-svelte `Alert` requiring explicit user acceptance. | P1 |
| Agent forwarding | Off by default. Can be enabled per profile. Clearly labelled as a security risk in the UI. | P2 |
| Lock on sleep | When the system sleeps, the active tunnel is maintained but the main window requires re-authentication (biometrics or password) to reopen on wake. | P2 |

### 6.4 Platform specifics

| Feature | Requirement | Priority |
|---|---|---|
| macOS — TUN creation | Uses the `utun` kernel interface via Authorization Services for the one-time privilege elevation. No third-party kernel extension required. | P0 |
| Windows — Wintun driver | Bundles the Wintun driver (WireGuard's open-source TUN driver). Prompts UAC once on first install to register the driver. No UAC on subsequent connects. | P0 |
| Linux — TUN permission | Installer adds the user to the `netdev` group or sets the `CAP_NET_ADMIN` capability on the privilege helper binary. No sudo required after install. | P0 |
| DNS leak prevention | Overrides the system DNS resolver to use a local stub resolver that forwards queries over the tunnel. Reverted on disconnect. | P1 |
| IPv6 support | TUN interface is configured with an IPv6 address. IPv6 traffic is routed through the tunnel alongside IPv4. | P2 |

---

## 7. Key User Flows

### 7.1 First launch

1. User downloads and installs XSSH Tunnel.
2. A welcome screen (shadcn-svelte `Card` on a neutral background) explains what the app does and what it requires.
3. User is directed to either add a connection manually or import from SSH config.
4. Master encryption key is generated and stored in the OS keychain silently on first launch.

### 7.2 Import from SSH config and connect

1. User clicks **Import from SSH Config** in the Settings screen.
2. App reads `~/.ssh/config` via Rust `invoke('parse_ssh_config')` and returns a list of host entries to the SvelteKit store.
3. User selects desired hosts in the checklist dialog and clicks **Import**.
4. Profiles appear in the Connections list tagged with an `imported` badge.
5. User selects a profile and clicks **Connect**.
6. App prompts for privilege elevation (one-time OS dialog).
7. TUN device and route are set up. SSH connection authenticates.
8. The Svelte connection store updates to `Tunnel active`; all system traffic now routes through the SSH server.

### 7.3 Add a new server manually and connect

1. User clicks **+ New Connection**.
2. Fills in Label, Host, Username. Selects auth method — e.g. Private key, pastes PEM into the textarea.
3. Clicks **Test Connection** — Rust invokes an SSH handshake, returns success or a structured error to the form.
4. Clicks **Save**. Profile is encrypted and stored in SQLite via `invoke('save_profile')`.
5. User clicks **Connect** on the new profile. Tunnel becomes active.

---

## 8. Non-Functional Requirements

| Requirement | Target |
|---|---|
| Time to tunnel active | Under 5 seconds from clicking Connect on a reachable server with cached host key. |
| Reconnect time | Under 10 seconds from connection drop to restored tunnel on auto-reconnect. |
| CPU usage (idle tunnel) | Under 2% on a modern CPU while connected with no active traffic. SvelteKit's compiled output has no virtual DOM overhead in the WebView. |
| Memory footprint | Under 80 MB RSS while connected. |
| Binary / installer size | Under 30 MB installer on all platforms (Tauri's default; SvelteKit static build is smaller than React equivalent). |
| Startup time | Main window interactive in under 1.5 seconds. SvelteKit compiles to minimal JS with no framework runtime overhead. |
| Database query time | Any profile list or search query completes in under 50 ms for up to 500 profiles. |
| Crash recovery | On unexpected exit, injected routes and TUN device must be cleaned up by the privilege helper watchdog. User must not need a reboot to restore normal networking. |
| Accessibility | All interactive elements meet WCAG 2.1 AA. shadcn-svelte components are built on Bits UI which provides ARIA attributes and keyboard navigation out of the box. |

---

## 9. Milestones

| Milestone | Scope | Target |
|---|---|---|
| **M0 — Skeleton** | Tauri 2 project scaffolded with SvelteKit (adapter-static, SPA mode). shadcn-svelte installed and themed. Rust SSH manager connects and authenticates to a hardcoded host. TUN device created on all three platforms. | Week 2 |
| **M1 — Core tunnel** | smoltcp integration complete. SOCKS5 engine working. Traffic routes through SSH server. Default route injected and cleaned up correctly. Tauri events stream connection state to Svelte store. | Week 5 |
| **M2 — SSH manager** | SQLite schema, field-level AES-256-GCM encryption, and full CRUD UI for profiles using shadcn-svelte Form components. SSH agent and private key auth working. | Week 7 |
| **M3 — SSH config import** | `~/.ssh/config` parser in Rust. Import dialog in SvelteKit with conflict resolution checkboxes. ProxyJump support and `Include` directive handling. | Week 9 |
| **M4 — Polish** | System tray, auto-reconnect with backoff, DNS leak prevention, known host key verification dialog, live log viewer with virtual scroll. | Week 11 |
| **M5 — Beta** | All P0 and P1 features complete. Signed and notarised builds on all platforms. Internal beta testing. | Week 13 |
| **v1.0 — Release** | P2 features prioritised based on beta feedback. Public release. | Week 16 |

---

## 10. Open Questions

| Question | Notes |
|---|---|
| **Split tunneling in v1?** | Routing only specific CIDRs through the tunnel rather than all traffic. Technically straightforward (adjust injected routes) but adds UI complexity. Deferred to v1.1 unless strong user demand surfaces in beta. |
| **Passphrase caching duration** | When a key passphrase is entered at connect time (not stored), how long should it be cached in memory? Options: session only, until disconnect, or until system lock. Needs UX decision before M2. |
| **Multiple simultaneous tunnels** | v1 supports one active tunnel at a time. Multi-tunnel (e.g. route corporate CIDR through one server, everything else through another) is a v2 consideration. |
| **Windows Wintun licence** | Wintun is WHQL-signed by WireGuard. Confirm licence allows bundling in a non-WireGuard app before shipping M5. |
| **Linux packaging format** | AppImage vs deb / rpm. AppImage requires no install and simplifies the privilege helper capability setup but produces a larger file. Needs decision before M5. |
| **SvelteKit CSP headers** | Tauri 2's default Content Security Policy may need to be relaxed or augmented to allow the SvelteKit hydration script. Verify during M0 setup. |

---

*XSSH Tunnel PRD v1.0 · Confidential · May 2026*

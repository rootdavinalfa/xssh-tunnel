# M1 Testing Guide

## Prerequisites
- macOS (M1 only supports macOS TUN for now)
- Root privileges (for route injection)
- An SSH server accessible from your machine

## Setup

1. Edit hardcoded credentials in `src-tauri/src/lib.rs`:
   ```rust
   let config = TunnelConfig {
       ssh_host: "YOUR_SERVER_IP".to_string(),
       ssh_port: 22,
       ssh_username: "YOUR_USERNAME".to_string(),
       ssh_password: "YOUR_PASSWORD".to_string(),
   };
   ```

2. Build and run:
   ```bash
   npm run tauri dev
   ```

## Testing

1. Click **Connect**
2. Enter your macOS password when prompted for route injection (requires root)
3. Status should change: `disconnected` → `connecting` → `authenticating` → `tunnel-active`
4. Verify traffic routing:
   ```bash
   curl ipinfo.io  # Should show your server's IP
   ```
5. Click **Disconnect**
6. Status should return to `disconnected`

## Known Limitations
- Credentials are hardcoded (M2 will add profile management)
- Only TCP traffic is logged (full proxying in M2)
- Requires root for route injection
- Only macOS supported (Windows/Linux TUN in later milestones)
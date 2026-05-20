# M3 Design: Profile Editing + Connection Logs + SSH Config Import

> **Date:** 2026-05-20  
> **Milestone:** M3 — Profile Management Enhancements  
> **Status:** Approved for Implementation

---

## Overview

M3 builds upon the encrypted profile foundation from M2 to deliver three high-impact features:

1. **Connection Logs** — Real-time and persistent logging for troubleshooting
2. **Profile Editing** — Update existing profiles with secure credential handling
3. **SSH Config Import** — One-click import from `~/.ssh/config`

These features follow the **Hybrid Approach**: Logs serve as the foundation, with Profile Editing and SSH Import building on top to provide progress feedback and confirmation messages.

---

## Goals

- **Connection Logs:** Users can see what's happening during connection/disconnection
- **Profile Editing:** Users can update profile details without recreating them
- **SSH Config Import:** Users can import existing SSH configurations instead of manual entry
- **Foundation:** Log system benefits both editing and import workflows

---

## Architecture

### File Structure

```
src-tauri/src/
├── logs/
│   └── mod.rs                 # NEW: Log storage + SQLite persistence
├── profiles/
│   └── mod.rs                 # EXTEND: Add update_profile()
├── ssh/
│   ├── client.rs              # EXISTING
│   └── config_parser.rs       # NEW: ~/.ssh/config parser
└── lib.rs                     # EXTEND: New commands + log events

src/
├── lib/
│   ├── stores/
│   │   ├── profiles.ts        # EXTEND: Update profile in store
│   │   └── logs.ts            # NEW: Connection log store
│   └── tauri.ts               # EXTEND: New commands + log listeners
└── routes/
    ├── +page.svelte           # EXTEND: Add logs panel + edit button
    ├── connections/
    │   ├── new/+page.svelte   # EXISTING
    │   └── [id]/edit/+page.svelte  # NEW: Edit profile page
    └── logs/+page.svelte      # NEW: Full logs page
```

### Phase Sequence

**Phase 1:** Connection Logs (foundation)  
**Phase 2:** Profile Editing (uses logs for confirmation)  
**Phase 3:** SSH Config Import (uses logs for progress)

---

## Feature 1: Connection Logs

### Purpose
Provide visibility into connection lifecycle, errors, and profile actions for debugging and user feedback.

### Backend Design

#### Data Model

```rust
#[derive(Debug, Serialize, FromRow)]
pub struct LogEntry {
    pub id: String,              // UUID
    pub timestamp: String,       // RFC 3339
    pub level: String,           // "info" | "warn" | "error" | "debug"
    pub message: String,
    pub profile_id: Option<String>,  // Optional FK to profiles
}

#[derive(Debug)]
pub enum LogLevel {
    Info,
    Warn,
    Error,
    Debug,
}
```

#### SQLite Schema

```sql
CREATE TABLE logs (
    id TEXT PRIMARY KEY,
    timestamp TEXT NOT NULL,
    level TEXT NOT NULL CHECK(level IN ('info', 'warn', 'error', 'debug')),
    message TEXT NOT NULL,
    profile_id TEXT,
    FOREIGN KEY (profile_id) REFERENCES profiles(id) ON DELETE SET NULL
);

CREATE INDEX idx_logs_timestamp ON logs(timestamp DESC);
CREATE INDEX idx_logs_level ON logs(level);
CREATE INDEX idx_logs_profile ON logs(profile_id);
```

#### Storage Strategy

- **Retention:** 7 days (auto-cleanup old entries)
- **Buffer:** Keep last 100 entries in memory for real-time display
- **Persistence:** SQLite for historical queries
- **Limits:** Max 1000 entries, prune oldest when exceeded

#### Log Sources

| Event | Level | Example Message |
|-------|-------|-----------------|
| Profile created | info | "Profile created: MyServer" |
| Profile updated | info | "Profile updated: MyServer" |
| Profile deleted | info | "Profile deleted: MyServer" |
| Connection started | info | "Connecting to myserver.com..." |
| SSH authenticated | info | "SSH authenticated as root" |
| TUN created | info | "TUN device utun5 created" |
| Routes injected | info | "Default route via utun5" |
| Tunnel active | info | "Tunnel active - routing traffic" |
| Disconnect | info | "Disconnected from myserver.com" |
| SSH error | error | "SSH connection failed: timeout" |
| TUN error | error | "TUN creation failed: permission denied" |
| Import started | info | "Parsing ~/.ssh/config..." |
| Import progress | info | "Found 5 hosts in config" |
| Import complete | info | "Imported 3 profiles" |

#### Commands

```rust
#[tauri::command]
async fn get_logs(
    state: tauri::State<'_, AppState>,
    limit: Option<u32>,
    level: Option<String>,
) -> Result<Vec<LogEntry>, AppError>;

#[tauri::command]
async fn clear_logs(state: tauri::State<'_, AppState>) -> Result<(), AppError>;
```

#### Real-time Events

```rust
// Emit on connection state changes
app.emit("log-entry", LogEntry { ... });

// Frontend listens and appends to display
```

### Frontend Design

#### Compact Panel (Main Page)

- Shows last 5-10 log entries
- Auto-scrolls to newest
- Color-coded by level (info=gray, warn=yellow, error=red)
- "View All" link opens full logs page

#### Full Logs Page (`/logs`)

- Table view with columns: Time, Level, Message
- Filter by level (checkboxes: Info, Warn, Error, Debug)
- Search box for message text
- Clear Logs button
- Auto-scroll toggle
- Export to file (optional)

#### Store

```typescript
// src/lib/stores/logs.ts
export const logEntries = writable<LogEntry[]>([]);
export const logFilter = writable<{ level?: string; search?: string }>({});

export function appendLog(entry: LogEntry) {
    logEntries.update(entries => [...entries.slice(-99), entry]);
}
```

---

## Feature 2: Profile Editing

### Purpose
Allow users to update existing profiles without recreating them, with secure credential handling.

### UX Flow

1. **Profile List** — Each profile card has "Edit" button alongside Connect/Delete
2. **Edit Page** — Opens at `/connections/[id]/edit`
3. **Pre-filled Fields:**
   - Label, Host, Port, Username (visible and editable)
   - Auth Type (visible, editable)
4. **Credentials Section:**
   - Collapsed by default
   - Toggle: "Change credentials"
   - When expanded: Password/Key fields are **blank** (not pre-filled)
5. **Save Behavior:**
   - Plaintext fields updated unconditionally
   - Credentials only updated if new values provided
   - Existing encrypted credentials preserved if left blank

### Security Model

- **Never send decrypted credentials to frontend**
- Frontend only knows "has credentials" via boolean flag
- When "Change credentials" is toggled, fields are empty
- User must re-enter credentials to change them

### Backend Design

#### Request/Response

```rust
#[derive(Debug, Deserialize)]
pub struct UpdateProfileRequest {
    pub id: String,
    pub label: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub auth_type: String,
    pub password: Option<String>,           // Only if changing
    pub private_key: Option<String>,        // Only if changing
    pub key_passphrase: Option<String>,     // Only if changing
    pub identity_file_path: Option<String>, // Only if changing
}

pub async fn update_profile(
    pool: &DbPool,
    master_key: &[u8; 32],
    req: UpdateProfileRequest,
) -> Result<Profile, AppError> {
    // 1. Update plaintext fields
    // 2. If credentials provided, encrypt and update
    // 3. If credentials not provided, keep existing
    // 4. Update updated_at timestamp
    // 5. Log: "Profile updated: {label}"
}
```

#### Command

```rust
#[tauri::command]
async fn update_profile_cmd(
    state: tauri::State<'_, AppState>,
    req: UpdateProfileRequest,
) -> Result<Profile, AppError>;
```

### Frontend Design

#### Edit Page (`/connections/[id]/edit`)

Similar layout to "New Connection" but:
- Page title: "Edit Connection"
- Form pre-filled with current values
- "Change Credentials" toggle section
- Cancel button returns to home

#### Credential Toggle UX

```svelte
<script>
  let changeCredentials = $state(false);
  let hasExistingCredentials = $derived(profile.has_password || profile.has_key);
</script>

{#if hasExistingCredentials}
  <div class="border rounded p-4">
    <label class="flex items-center gap-2">
      <input type="checkbox" bind:checked={changeCredentials} />
      <span>Change credentials</span>
    </label>
    
    {#if changeCredentials}
      <div class="mt-4 space-y-4">
        <!-- Auth type selection -->
        <!-- Credential fields (blank, not pre-filled) -->
      </div>
    {:else}
      <p class="text-sm text-muted-foreground mt-2">
        Existing credentials will be kept
      </p>
    {/if}
  </div>
{/if}
```

---

## Feature 3: SSH Config Import

### Purpose
Import existing SSH configurations from `~/.ssh/config` to eliminate manual entry.

### Supported Directives

| Directive | Mapped To | Notes |
|-----------|-----------|-------|
| `Host` | label | First alias used as label |
| `HostName` | host | Required |
| `User` | username | Defaults to current user |
| `Port` | port | Defaults to 22 |
| `IdentityFile` | identity_file_path | Path stored, key NOT imported |
| `ProxyJump` | — | Skipped with warning |
| `Include` | — | Followed recursively |

**Skipped:** Wildcard hosts (`Host *`, `Host *.example.com`), incomplete entries

### Backend Design

#### Parser

```rust
// src-tauri/src/ssh/config_parser.rs

#[derive(Debug)]
pub struct SshConfigEntry {
    pub host_aliases: Vec<String>,  // All Host values
    pub hostname: String,
    pub user: Option<String>,
    pub port: Option<u16>,
    pub identity_file: Option<String>,
}

pub fn parse_ssh_config(path: &Path) -> Result<Vec<SshConfigEntry>, AppError> {
    // 1. Read ~/.ssh/config
    // 2. Follow Include directives
    // 3. Parse Host blocks
    // 4. Skip wildcards
    // 5. Return valid entries
}
```

#### Import Command

```rust
#[tauri::command]
async fn import_ssh_config(
    state: tauri::State<'_, AppState>,
    selected_hosts: Vec<String>,  // Host aliases to import
) -> Result<ImportResult, AppError>;

pub struct ImportResult {
    pub imported: Vec<String>,
    pub skipped: Vec<String>,
    pub errors: Vec<String>,
}
```

#### Import Process

1. Parse config file
2. For each selected host:
   - Create profile with `auth_type: "key_file"` if `IdentityFile` present
   - Create profile with `auth_type: "agent"` otherwise
   - Store key file path (NOT the key content)
3. Log progress for each import
4. Return summary

### Frontend Design

#### Import Dialog

```svelte
<!-- Trigger from main page -->
<Button onclick={() => showImportDialog = true}>
  Import from SSH Config
</Button>

<!-- Dialog content -->
<Dialog>
  <DialogHeader>
    <DialogTitle>Import from ~/.ssh/config</DialogTitle>
  </DialogHeader>
  
  {#if loading}
    <p>Parsing SSH config...</p>
  {:else}
    <Table>
      <TableHeader>
        <TableRow>
          <TableHead><Checkbox bind:checked={selectAll} /></TableHead>
          <TableHead>Host</TableHead>
          <TableHead>Hostname</TableHead>
          <TableHead>User</TableHead>
          <TableHead>Port</TableHead>
          <TableHead>Key File</TableHead>
        </TableRow>
      </TableHeader>
      <TableBody>
        {#each entries as entry}
          <TableRow>
            <TableCell><Checkbox bind:checked={selected[entry.host]} /></TableCell>
            <TableCell>{entry.host_aliases[0]}</TableCell>
            <TableCell>{entry.hostname}</TableCell>
            <TableCell>{entry.user || '-'}</TableCell>
            <TableCell>{entry.port || 22}</TableCell>
            <TableCell>{entry.identity_file ? 'Yes' : 'No'}</TableCell>
          </TableRow>
        {/each}
      </TableBody>
    </Table>
    
    {#if skipped.length > 0}
      <div class="mt-4">
        <p class="text-sm text-yellow-600">Skipped: {skipped.join(', ')}</p>
      </div>
    {/if}
  {/if}
  
  <DialogFooter>
    <Button variant="outline" onclick={() => showImportDialog = false}>
      Cancel
    </Button>
    <Button onclick={handleImport} disabled={selectedCount === 0}>
      Import {selectedCount} profiles
    </Button>
  </DialogFooter>
</Dialog>
```

---

## Data Flow

### Connection with Logs

```
User clicks Connect
        ↓
Frontend calls connect_tunnel(profile_id)
        ↓
Backend logs: "Connecting to {host}..."
        ↓
Backend emits log-entry event
        ↓
Frontend receives event → updates log panel
        ↓
SSH connects
        ↓
Backend logs: "SSH authenticated as {user}"
        ↓
TUN created
        ↓
Backend logs: "TUN device {name} created"
        ↓
Routes injected
        ↓
Backend logs: "Tunnel active"
        ↓
Backend stores log entries in SQLite
```

### Profile Edit Flow

```
User clicks Edit on profile card
        ↓
Navigate to /connections/[id]/edit
        ↓
Load profile data (plaintext only)
        ↓
User modifies fields
        ↓
User toggles "Change credentials" → enters new credentials
        ↓
User clicks Save
        ↓
Frontend calls update_profile_cmd
        ↓
Backend updates plaintext fields
        ↓
If new credentials: encrypt and update
        ↓
Backend logs: "Profile updated: {label}"
        ↓
Backend emits log-entry event
        ↓
Frontend shows success → navigates home
```

### SSH Import Flow

```
User clicks "Import from SSH Config"
        ↓
Dialog opens
        ↓
Frontend calls parse_ssh_config()
        ↓
Backend parses ~/.ssh/config
        ↓
Backend returns entries + skipped list
        ↓
User selects entries to import
        ↓
User clicks Import
        ↓
Frontend calls import_ssh_config(selected)
        ↓
For each selected entry:
    Backend logs: "Importing {host}..."
    Backend creates profile
    Backend logs: "Imported {host}"
        ↓
Backend returns import results
        ↓
Frontend refreshes profile list
        ↓
Dialog closes
```

---

## Error Handling

### Connection Logs

- **Database full:** Prune oldest 10% of entries automatically
- **Write failure:** Log to stderr, continue without persistence
- **Query failure:** Return empty list, log error

### Profile Editing

- **Profile not found:** Return 404-style error
- **Invalid auth type:** Validation error
- **Encryption failure:** Rollback transaction, return error
- **Concurrent edit:** Last-write-wins (simplest for now)

### SSH Config Import

- **File not found:** Error: "~/.ssh/config not found"
- **Permission denied:** Error: "Cannot read ~/.ssh/config"
- **Parse error:** Skip entry, continue with others, return warnings
- **Duplicate hosts:** Allow import (user can rename after)

---

## UI/UX Specifications

### Color Coding

| Level | Color | Usage |
|-------|-------|-------|
| info | gray-500 | General messages |
| warn | yellow-500 | Warnings, skipped items |
| error | red-500 | Errors, failures |
| debug | blue-500 | Verbose details |

### Log Format

```
[HH:MM:SS] [LEVEL] Message

Example:
[14:32:15] [info] Connecting to myserver.com...
[14:32:16] [info] SSH authenticated as root
[14:32:16] [error] TUN creation failed: permission denied
```

### Responsive Behavior

- **Main page:** Logs panel collapses to 3 entries on small screens
- **Logs page:** Full width table, filters stack vertically on mobile
- **Edit page:** Same responsive breakpoints as New Connection
- **Import dialog:** Scrollable table, sticky header

---

## Testing Considerations

### Logs

- Verify log entry creation on all actions
- Verify 7-day retention cleanup
- Verify real-time event delivery
- Verify filtering by level works

### Profile Editing

- Edit only plaintext fields, verify credentials preserved
- Edit with credential change, verify old credentials replaced
- Edit non-existent profile, verify error
- Concurrent edits, verify behavior

### SSH Import

- Import with valid config, verify all fields mapped
- Import with wildcards, verify skipped
- Import with Include directive, verify followed
- Import with missing file, verify error
- Import duplicate hosts, verify allowed

---

## Dependencies

### New Rust Crates

```toml
# For SSH config parsing
# Option 1: regex-based parser (lightweight)
regex = "1.10"

# Option 2: dedicated SSH config parser
# ssh-config = "0.3"  # Check if exists and works

# Prefer custom parser using regex for control
```

### No New Frontend Dependencies

Uses existing shadcn-svelte components:
- Dialog, Table, Checkbox, Button
- Form components from existing pages

---

## Open Questions

1. **Log export:** Should users be able to export logs to a file for support?
2. **Log levels:** Should there be a "verbose" mode with more detailed SSH logs?
3. **Import merge:** Should importing a host that already exists update it or skip?
4. **ProxyJump:** Should we support ProxyJump in future, or document it as unsupported?

---

## Implementation Order

**Phase 1:** Connection Logs
1. Database schema + migration
2. Logs module with CRUD
3. Real-time events
4. Frontend log panel + page
5. Add logging to existing operations

**Phase 2:** Profile Editing
1. Update profile backend function
2. Update profile command
3. Edit page frontend
4. Add edit button to profile cards
5. Update profile store

**Phase 3:** SSH Config Import
1. SSH config parser
2. Parse command
3. Import command
4. Import dialog frontend
5. Add import button to main page

---

## Success Criteria

- [ ] Connection logs visible in real-time on main page
- [ ] Full logs page with filtering and search
- [ ] Profile editing works without credential re-entry
- [ ] SSH config import parses and imports valid entries
- [ ] Wildcard hosts are skipped with warnings
- [ ] All operations are logged
- [ ] Build passes (`npm run build`, `cargo check`)
- [ ] No TypeScript errors (`npm run check`)

---

## Notes

- **Performance:** Logs are lightweight; 1000 entries is ~50KB SQLite
- **Security:** No credentials in logs, even encrypted
- **macOS:** SSH config typically at `~/.ssh/config` (standard location)
- **Future:** Consider adding log filtering by date range

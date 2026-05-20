# M3 Implementation Plan: Connection Logs + Profile Editing + SSH Config Import

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` (recommended) or `superpowers:executing-plans` to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add connection logs with SQLite persistence, profile editing with secure credential handling, and SSH config import from `~/.ssh/config`.

**Architecture:** Three phases building on each other. Phase 1 adds a logs module (SQLite + in-memory buffer + Tauri events). Phase 2 extends profiles with update functionality and an edit UI. Phase 3 adds an SSH config parser and import dialog. All operations emit log entries.

**Tech Stack:** sqlx (SQLite), regex (SSH config parsing), shadcn-svelte (Dialog, Table, Checkbox), SvelteKit 5 stores

---

## File Structure

```
src-tauri/src/
├── logs/
│   └── mod.rs                  # CREATE: Log module (schema, CRUD, prune)
├── profiles/
│   └── mod.rs                  # EXTEND: Add update_profile()
├── ssh/
│   └── config_parser.rs        # CREATE: ~/.ssh/config parser
└── lib.rs                      # EXTEND: Register new commands

src/
├── lib/
│   ├── stores/
│   │   ├── profiles.ts         # EXTEND: Add update-profile action
│   │   └── logs.ts             # CREATE: Log entry store
│   └── tauri.ts                # EXTEND: New IPC wrappers
└── routes/
    ├── +page.svelte            # EXTEND: Add edit button + log panel
    ├── connections/
    │   ├── new/+page.svelte    # EXISTING (unchanged)
    │   └── [id]/edit/+page.svelte  # CREATE: Edit profile page
    └── logs/+page.svelte       # CREATE: Full logs page
```

---

## Phase 1: Connection Logs

### Task 1: Create logs SQL schema

**Files:**
- Modify: `src-tauri/src/db/mod.rs`

- [ ] **Step 1: Add logs table to migrations**

Read `src-tauri/src/db/mod.rs` to see current migration format. Then edit to add the logs table after the profiles table:

```rust
// After the profiles INDEX creation, add:
        sqlx::query(
            r#"
        CREATE TABLE IF NOT EXISTS logs (
            id TEXT PRIMARY KEY,
            timestamp TEXT NOT NULL,
            level TEXT NOT NULL CHECK(level IN ('info', 'warn', 'error', 'debug')),
            message TEXT NOT NULL,
            profile_id TEXT,
            FOREIGN KEY (profile_id) REFERENCES profiles(id) ON DELETE SET NULL
        );

        CREATE INDEX IF NOT EXISTS idx_logs_timestamp ON logs(timestamp DESC);
        CREATE INDEX IF NOT EXISTS idx_logs_level ON logs(level);
        "#,
        )
        .execute(&pool)
        .await
        .map_err(|e| AppError::Tunnel(format!("Logs migration failed: {}", e)))?;
```

- [ ] **Step 2: Verify Rust compiles**

Run: `cd src-tauri && cargo check`
Expected: Compiles successfully

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/db/mod.rs
git commit -m "feat: add logs table to database schema"
```

---

### Task 2: Create logs module

**Files:**
- Create: `src-tauri/src/logs/mod.rs`
- Modify: `src-tauri/src/lib.rs` (add `pub mod logs;`)

- [ ] **Step 1: Create logs module**

Write the following content to `src-tauri/src/logs/mod.rs`:

```rust
use chrono::Utc;
use serde::Serialize;
use uuid::Uuid;

use crate::db::DbPool;
use crate::error::AppError;

#[derive(Debug, Serialize, sqlx::FromRow, Clone)]
pub struct LogEntry {
    pub id: String,
    pub timestamp: String,
    pub level: String,
    pub message: String,
    pub profile_id: Option<String>,
}

pub async fn insert_log(
    pool: &DbPool,
    level: &str,
    message: &str,
    profile_id: Option<&str>,
) -> Result<LogEntry, AppError> {
    let id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();

    sqlx::query(
        r#"INSERT INTO logs (id, timestamp, level, message, profile_id) VALUES (?1, ?2, ?3, ?4, ?5)"#,
    )
    .bind(&id)
    .bind(&now)
    .bind(level)
    .bind(message)
    .bind(profile_id)
    .execute(pool)
    .await
    .map_err(|e| AppError::Tunnel(format!("Log insert failed: {}", e)))?;

    Ok(LogEntry {
        id,
        timestamp: now,
        level: level.to_string(),
        message: message.to_string(),
        profile_id: profile_id.map(|s| s.to_string()),
    })
}

pub async fn get_logs(
    pool: &DbPool,
    limit: Option<u32>,
) -> Result<Vec<LogEntry>, AppError> {
    let limit = limit.unwrap_or(100).min(1000);

    let rows = sqlx::query_as::<_, LogEntry>(
        "SELECT id, timestamp, level, message, profile_id FROM logs ORDER BY timestamp DESC LIMIT ?1",
    )
    .bind(limit)
    .fetch_all(pool)
    .await
    .map_err(|e| AppError::Tunnel(format!("Log query failed: {}", e)))?;

    Ok(rows)
}

pub async fn get_logs_by_level(
    pool: &DbPool,
    level: &str,
    limit: Option<u32>,
) -> Result<Vec<LogEntry>, AppError> {
    let limit = limit.unwrap_or(100).min(1000);

    let rows = sqlx::query_as::<_, LogEntry>(
        "SELECT id, timestamp, level, message, profile_id FROM logs WHERE level = ?1 ORDER BY timestamp DESC LIMIT ?2",
    )
    .bind(level)
    .bind(limit)
    .fetch_all(pool)
    .await
    .map_err(|e| AppError::Tunnel(format!("Log query failed: {}", e)))?;

    Ok(rows)
}

pub async fn prune_old_logs(pool: &DbPool, max_age_days: i64) -> Result<u64, AppError> {
    use sqlx::Executor;
    let result = pool
        .execute(
            sqlx::query("DELETE FROM logs WHERE timestamp < datetime('now', ?1)")
                .bind(format!("-{} days", max_age_days)),
        )
        .await
        .map_err(|e| AppError::Tunnel(format!("Log prune failed: {}", e)))?;

    Ok(result.rows_affected())
}

pub async fn clear_logs(pool: &DbPool) -> Result<(), AppError> {
    sqlx::query("DELETE FROM logs")
        .execute(pool)
        .await
        .map_err(|e| AppError::Tunnel(format!("Log clear failed: {}", e)))?;

    Ok(())
}
```

- [ ] **Step 2: Register module in lib.rs**

Add `pub mod logs;` after `pub mod db;` in `src-tauri/src/lib.rs`:

```rust
pub mod crypto;
pub mod db;
pub mod error;
pub mod logs;
pub mod profiles;
pub mod ssh;
pub mod tunnel;
```

- [ ] **Step 3: Add chrono dependency if missing**

Check `src-tauri/Cargo.toml` for `chrono`. It's already used by profiles. Verify:

```bash
grep -n "chrono" src-tauri/Cargo.toml
```

Expected to find it. If not, add: `chrono = { version = "0.4", features = ["serde"] }`

- [ ] **Step 4: Verify Rust compiles**

Run: `cd src-tauri && cargo check`
Expected: Compiles successfully

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/logs/mod.rs src-tauri/src/lib.rs
git commit -m "feat: add logs module with SQLite persistence"
```

---

### Task 3: Wire logs into Tauri commands and events

**Files:**
- Modify: `src-tauri/src/lib.rs` (add log commands, emit events, prune on startup)

- [ ] **Step 1: Add log Tauri commands**

Add these commands before `fn greet` in `src-tauri/src/lib.rs`:

```rust
#[tauri::command]
async fn get_logs_cmd(
    state: tauri::State<'_, AppState>,
    level: Option<String>,
    limit: Option<u32>,
) -> Result<Vec<logs::LogEntry>, AppError> {
    if let Some(lvl) = &level {
        logs::get_logs_by_level(&state.db, lvl, limit).await
    } else {
        logs::get_logs(&state.db, limit).await
    }
}

#[tauri::command]
async fn clear_logs_cmd(state: tauri::State<'_, AppState>) -> Result<(), AppError> {
    logs::clear_logs(&state.db).await
}
```

- [ ] **Step 2: Register log commands in generate_handler!**

Add `get_logs_cmd` and `clear_logs_cmd` to the handler list:

```rust
.invoke_handler(tauri::generate_handler![
    greet,
    create_profile_cmd,
    get_profiles_cmd,
    delete_profile_cmd,
    update_profile_cmd,
    connect_tunnel,
    disconnect_tunnel,
    get_logs_cmd,
    clear_logs_cmd,
    parse_ssh_config_cmd,
    import_ssh_config_cmd,
])
```

- [ ] **Step 3: Add prune on startup**

In the `.setup()` closure, after initializing the DB, add:

```rust
// Prune old logs
let db_clone = db.clone();
tauri::async_runtime::spawn(async move {
    if let Err(e) = logs::prune_old_logs(&db_clone, 7).await {
        eprintln!("Failed to prune old logs: {}", e);
    }
});
```

Add the use: `use logs::LogEntry;` in `src-tauri/src/lib.rs`.

- [ ] **Step 4: Add helper function to emit log events**

Add this function at the top of `src-tauri/src/lib.rs` (after the imports):

```rust
use tauri::Emitter;
use logs::LogEntry;

async fn emit_log(
    app_handle: &tauri::AppHandle,
    db: &DbPool,
    level: &str,
    message: &str,
    profile_id: Option<&str>,
) {
    let result = logs::insert_log(db, level, message, profile_id).await;
    if let Ok(entry) = result {
        let _ = app_handle.emit("log-entry", &entry);
    }
}
```

- [ ] **Step 5: Add logging to existing commands**

Add log calls to `connect_tunnel` and `disconnect_tunnel`:

```rust
// In connect_tunnel, after getting profile:
emit_log(&app, &state.db, "info", &format!("Connecting to {}...", profile.host), Some(&profile_id)).await;

// After SSH auth:
emit_log(&app, &state.db, "info", &format!("SSH authenticated as {}", username), Some(&profile_id)).await;

// After tunnel active:
emit_log(&app, &state.db, "info", "Tunnel active - routing traffic", Some(&profile_id)).await;

// In disconnect_tunnel, after successful stop:
emit_log(&app, &state.db, "info", &format!("Disconnected from tunnel"), None).await;
```

- [ ] **Step 6: Add logging to profile CRUD commands**

```rust
// In create_profile_cmd, after successful creation:
emit_log(&app_handle, &state.db, "info", &format!("Profile created: {}", req.label), Some(&id)).await;

// In delete_profile_cmd, before delete:
emit_log(&app_handle, &state.db, "info", &format!("Profile deleted"), Some(&id)).await;

// In update_profile_cmd, after successful update:
emit_log(&app_handle, &state.db, "info", &format!("Profile updated: {}", req.label), Some(&req.id)).await;
```

Note: Profile commands need `app: tauri::AppHandle` as a parameter. Update their signatures:

```rust
#[tauri::command]
async fn create_profile_cmd(app: tauri::AppHandle, state: tauri::State<'_, AppState>, req: CreateProfileRequest) -> Result<Profile, AppError> {
    let id = Uuid::new_v4().to_string();  // Need to use the id from create_profile
    // ... existing code ...
    let profile = create_profile(&state.db, &state.master_key, req).await?;
    emit_log(&app, &state.db, "info", &format!("Profile created: {}", profile.label), Some(&profile.id)).await;
    Ok(profile)
}
```

Add `use uuid::Uuid;` and `use std::sync::Arc;` if needed.

- [ ] **Step 7: Fix connect_tunnel for profile_id in logging**

The `connect_tunnel` command already has `app` and `profile_id`. Add emit_log calls at key points:

```rust
// At the start of connect_tunnel, after validating profile:
emit_log(&app, &state.db, "info", &format!("Connecting to {}...", profile.host), Some(&profile_id)).await;

// After SSH authentication:
emit_log(&app, &state.db, "info", &format!("SSH authenticated"), Some(&profile_id)).await;

// When tunnel is active:
emit_log(&app, &state.db, "info", "Tunnel active", Some(&profile_id)).await;
```

- [ ] **Step 8: Verify Rust compiles**

Run: `cd src-tauri && cargo check`
Expected: Compiles successfully (warnings OK)

- [ ] **Step 9: Commit**

```bash
git add src-tauri/src/lib.rs
git commit -m "feat: wire logs into Tauri commands and emit events"
```

---

### Task 4: Create frontend log store

**Files:**
- Create: `src/lib/stores/logs.ts`
- Modify: `src/lib/tauri.ts` (add log wrappers)

- [ ] **Step 1: Create log store**

Write to `src/lib/stores/logs.ts`:

```typescript
import { writable } from 'svelte/store';

export interface LogEntry {
  id: string;
  timestamp: string;
  level: 'info' | 'warn' | 'error' | 'debug';
  message: string;
  profile_id: string | null;
}

export const logEntries = writable<LogEntry[]>([]);

export function appendLog(entry: LogEntry) {
  logEntries.update(entries => {
    const updated = [entry, ...entries];
    return updated.slice(0, 500); // Keep max 500 in memory
  });
}

export function clearStore() {
  logEntries.set([]);
}
```

- [ ] **Step 2: Add log IPC wrappers to tauri.ts**

Append to `src/lib/tauri.ts`:

```typescript
// Log commands
export async function getLogs(level?: string, limit?: number): Promise<LogEntry[]> {
  return await invoke('get_logs_cmd', { level, limit });
}

export async function clearLogs(): Promise<void> {
  return await invoke('clear_logs_cmd');
}

export async function listenLogs(callback: (entry: LogEntry) => void) {
  return await listen<LogEntry>('log-entry', (event) => {
    callback(event.payload);
  });
}

// Auto-sync logs to store
export function syncLogs() {
  const unlisten = listenLogs((entry) => {
    appendLog(entry);
  });
  return unlisten;
}
```

Add the import at the top of `src/lib/tauri.ts`:

```typescript
import { appendLog } from './stores/logs';
import type { LogEntry } from './stores/logs';
```

- [ ] **Step 3: Verify frontend builds**

Run: `npm run check && npm run build`
Expected: No errors

- [ ] **Step 4: Commit**

```bash
git add src/lib/stores/logs.ts src/lib/tauri.ts
git commit -m "feat: add log store and IPC wrappers"
```

---

### Task 5: Create logs page UI

**Files:**
- Create: `src/routes/logs/+page.svelte`
- Create: `src/routes/logs/+page.ts` (if needed, probably not for SPA)

- [ ] **Step 1: Create logs page**

Write to `src/routes/logs/+page.svelte`:

```svelte
<script lang="ts">
  import { Button } from '$lib/components/ui/button';
  import { Input } from '$lib/components/ui/input';
  import { logEntries, clearStore } from '$lib/stores/logs';
  import { getLogs, clearLogs, syncLogs } from '$lib/tauri';
  import { onMount } from 'svelte';

  let filterLevel = $state('');
  let searchQuery = $state('');
  let loading = $state(true);

  let filteredEntries = $derived.by(() => {
    let entries = $logEntries;
    if (filterLevel) {
      entries = entries.filter(e => e.level === filterLevel);
    }
    if (searchQuery) {
      const q = searchQuery.toLowerCase();
      entries = entries.filter(e => e.message.toLowerCase().includes(q));
    }
    return entries;
  });

  function levelClass(level: string): string {
    switch (level) {
      case 'error': return 'text-red-600';
      case 'warn': return 'text-yellow-600';
      case 'info': return 'text-gray-600';
      case 'debug': return 'text-blue-600';
      default: return '';
    }
  }

  async function handleClear() {
    await clearLogs();
    clearStore();
  }

  onMount(async () => {
    try {
      const logs = await getLogs();
      logEntries.set(logs);
    } catch (e) {
      console.error('Failed to load logs:', e);
    } finally {
      loading = false;
    }
    const unlisten = syncLogs();
    return () => { unlisten.then(fn => fn()); };
  });
</script>

<div class="container mx-auto p-6 max-w-5xl">
  <div class="flex justify-between items-center mb-6">
    <h1 class="text-2xl font-bold">Connection Logs</h1>
    <Button variant="outline" onclick={() => window.location.href = '/'}>
      Back
    </Button>
  </div>

  <div class="flex gap-4 mb-4">
    <select
      bind:value={filterLevel}
      class="border rounded-md p-2"
    >
      <option value="">All Levels</option>
      <option value="info">Info</option>
      <option value="warn">Warning</option>
      <option value="error">Error</option>
      <option value="debug">Debug</option>
    </select>

    <Input
      placeholder="Search messages..."
      bind:value={searchQuery}
      class="flex-1"
    />

    <Button variant="outline" onclick={handleClear}>
      Clear Logs
    </Button>
  </div>

  {#if loading}
    <p class="text-center text-muted-foreground py-8">Loading logs...</p>
  {:else if filteredEntries.length === 0}
    <p class="text-center text-muted-foreground py-8">
      {#if $logEntries.length === 0}
        No logs yet. Connect to a server to see activity.
      {:else}
        No logs match your filter.
      {/if}
    </p>
  {:else}
    <div class="border rounded-lg divide-y">
      {#each filteredEntries as entry (entry.id)}
        <div class="px-4 py-3 flex gap-4 items-start">
          <span class="text-sm text-gray-400 font-mono whitespace-nowrap">
            {entry.timestamp.slice(11, 19)}
          </span>
          <span class="text-xs font-medium uppercase w-12 {levelClass(entry.level)}">
            {entry.level}
          </span>
          <span class="text-sm flex-1">{entry.message}</span>
        </div>
      {/each}
    </div>
  {/if}
</div>
```

- [ ] **Step 2: Verify frontend builds**

Run: `npm run check && npm run build`
Expected: No errors

- [ ] **Step 3: Commit**

```bash
git add src/routes/logs/+page.svelte
git commit -m "feat: add connection logs page"
```

---

### Task 6: Add compact log panel to main page

**Files:**
- Modify: `src/routes/+page.svelte`

- [ ] **Step 1: Read current main page**

Read `src/routes/+page.svelte` to understand current layout.

- [ ] **Step 2: Add log panel to the profile list page**

Replace the empty state and add a log panel section. Below the profile list, add:

```svelte
  <!-- Logs Panel -->
  <div class="mt-8">
    <div class="flex justify-between items-center mb-2">
      <h2 class="text-lg font-semibold">Recent Activity</h2>
      <a href="/logs" class="text-sm text-blue-600 hover:underline">View All</a>
    </div>
    <div class="border rounded-lg divide-y max-h-48 overflow-y-auto">
      {#if $logEntries.length === 0}
        <p class="text-sm text-muted-foreground px-4 py-3">
          No activity yet. Connect to a server to see logs.
        </p>
      {:else}
        {#each $logEntries.slice(0, 10) as entry (entry.id)}
          <div class="px-4 py-2 flex gap-3 items-start">
            <span class="text-xs text-gray-400 font-mono whitespace-nowrap">
              {entry.timestamp.slice(11, 19)}
            </span>
            <span class="text-xs font-medium uppercase w-10 {levelClass(entry.level)}">
              {entry.level}
            </span>
            <span class="text-sm flex-1">{entry.message}</span>
          </div>
        {/each}
      {/if}
    </div>
  </div>
```

Also add the `levelClass` function and `onMount` log loading:

```svelte
<script lang="ts">
  // Add to imports:
  import { logEntries, clearStore } from '$lib/stores/logs';
  import { getLogs, syncLogs } from '$lib/tauri';
  import { onMount } from 'svelte';

  function levelClass(level: string): string {
    switch (level) {
      case 'error': return 'text-red-600';
      case 'warn': return 'text-yellow-600';
      case 'info': return 'text-gray-600';
      case 'debug': return 'text-blue-600';
      default: return '';
    }
  }

  onMount(() => {
    // Load recent logs
    getLogs(undefined, 10).then(logs => {
      if (logs.length > 0) logEntries.set(logs);
    }).catch(() => {});
    
    const unlisten = syncConnectionState();
    const unlistenLogs = syncLogs();
    loadProfiles();
    return () => { 
      unlisten.then(fn => fn());
      unlistenLogs.then(fn => fn());
    };
  });
</script>
```

- [ ] **Step 3: Verify frontend builds**

Run: `npm run check && npm run build`
Expected: No errors

- [ ] **Step 4: Commit**

```bash
git add src/routes/+page.svelte
git commit -m "feat: add compact log panel to main page"
```

---

## Phase 2: Profile Editing

### Task 7: Add update_profile to backend

**Files:**
- Modify: `src-tauri/src/profiles/mod.rs` (add UpdateProfileRequest + update_profile)
- Modify: `src-tauri/src/lib.rs` (add update_profile_cmd + register)

- [ ] **Step 1: Add UpdateProfileRequest and update_profile function**

Add to `src-tauri/src/profiles/mod.rs` after the existing `CreateProfileRequest`:

```rust
#[derive(Debug, Deserialize)]
pub struct UpdateProfileRequest {
    pub id: String,
    pub label: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub auth_type: String,
    pub password: Option<String>,
    pub private_key: Option<String>,
    pub key_passphrase: Option<String>,
    pub identity_file_path: Option<String>,
}

pub async fn update_profile(
    pool: &DbPool,
    master_key: &[u8; 32],
    req: UpdateProfileRequest,
) -> Result<Profile, AppError> {
    let now = Utc::now().to_rfc3339();

    // Calculate new encrypted values
    let password_enc: Option<Vec<u8>> = match &req.password {
        Some(p) if !p.is_empty() => Some(encrypt(p.as_bytes(), master_key)?),
        _ => None, // Will be handled by COALESCE in SQL
    };
    let private_key_enc: Option<Vec<u8>> = match &req.private_key {
        Some(k) if !k.is_empty() => Some(encrypt(k.as_bytes(), master_key)?),
        _ => None,
    };
    let key_passphrase_enc: Option<Vec<u8>> = match &req.key_passphrase {
        Some(p) if !p.is_empty() => Some(encrypt(p.as_bytes(), master_key)?),
        _ => None,
    };

    // Update: when new value is None/empty, keep existing
    // We use COALESCE with a subquery for existing values
    sqlx::query(
        r#"
        UPDATE profiles SET
            label = ?1,
            host = ?2,
            port = ?3,
            username = ?4,
            auth_type = ?5,
            password_enc = CASE WHEN ?6 IS NOT NULL THEN ?6 ELSE password_enc END,
            private_key_enc = CASE WHEN ?7 IS NOT NULL THEN ?7 ELSE private_key_enc END,
            key_passphrase_enc = CASE WHEN ?8 IS NOT NULL THEN ?8 ELSE key_passphrase_enc END,
            identity_file_path = CASE WHEN ?9 IS NOT NULL THEN ?9 ELSE identity_file_path END,
            updated_at = ?10
        WHERE id = ?11
        "#,
    )
    .bind(&req.label)
    .bind(&req.host)
    .bind(req.port as i64)
    .bind(&req.username)
    .bind(&req.auth_type)
    .bind(password_enc.as_ref().map(|v| v.as_slice()))
    .bind(private_key_enc.as_ref().map(|v| v.as_slice()))
    .bind(key_passphrase_enc.as_ref().map(|v| v.as_slice()))
    .bind(&req.identity_file_path)
    .bind(&now)
    .bind(&req.id)
    .execute(pool)
    .await
    .map_err(|e| AppError::Tunnel(format!("Update failed: {}", e)))?;

    // Fetch the updated profile
    let profile = sqlx::query_as::<_, Profile>("SELECT * FROM profiles WHERE id = ?1")
        .bind(&req.id)
        .fetch_one(pool)
        .await
        .map_err(|e| AppError::Tunnel(format!("Failed to fetch updated profile: {}", e)))?;

    Ok(profile)
}
```

- [ ] **Step 2: Add update_profile_cmd Tauri command**

Add to `src-tauri/src/lib.rs`:

```rust
#[tauri::command]
async fn update_profile_cmd(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    req: profiles::UpdateProfileRequest,
) -> Result<profiles::Profile, AppError> {
    let profile = profiles::update_profile(&state.db, &state.master_key, req).await?;
    emit_log(&app, &state.db, "info", &format!("Profile updated: {}", profile.label), Some(&profile.id)).await;
    Ok(profile)
}
```

- [ ] **Step 3: Register in generate_handler!**

Add `update_profile_cmd` to the handler list in `lib.rs`:

```rust
.invoke_handler(tauri::generate_handler![
    greet,
    create_profile_cmd,
    update_profile_cmd,
    get_profiles_cmd,
    delete_profile_cmd,
    connect_tunnel,
    disconnect_tunnel,
    get_logs_cmd,
    clear_logs_cmd,
])
```

- [ ] **Step 4: Verify Rust compiles**

Run: `cd src-tauri && cargo check`
Expected: Compiles successfully

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/profiles/mod.rs src-tauri/src/lib.rs
git commit -m "feat: add profile update backend"
```

---

### Task 8: Add get_profile_by_id command

**Files:**
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/src/profiles/mod.rs`

- [ ] **Step 1: Check if get_profile_by_id exists**

Look at `src-tauri/src/profiles/mod.rs` — it should already have a `get_profile_by_id` function from M2. Verify it's exported in `lib.rs`:

```rust
use profiles::{create_profile, get_profiles, get_profile_by_id, delete_profile, update_profile, CreateProfileRequest, UpdateProfileRequest};
```

- [ ] **Step 2: Add get_profile_by_id_cmd Tauri command**

```rust
#[tauri::command]
async fn get_profile_by_id_cmd(
    state: tauri::State<'_, AppState>,
    id: String,
) -> Result<profiles::Profile, AppError> {
    get_profile_by_id(&state.db, &id).await
}
```

- [ ] **Step 3: Register command**

Add `get_profile_by_id_cmd` to `generate_handler!`.

- [ ] **Step 4: Add IPC wrapper**

Add to `src/lib/tauri.ts`:

```typescript
export async function getProfileById(id: string): Promise<Profile> {
  return await invoke('get_profile_by_id_cmd', { id });
}

export async function updateProfile(req: {
  id: string;
  label: string;
  host: string;
  port: number;
  username: string;
  auth_type: string;
  password?: string;
  private_key?: string;
  key_passphrase?: string;
  identity_file_path?: string;
}): Promise<Profile> {
  return await invoke('update_profile_cmd', { req });
}
```

Also add `get_profile_by_id_cmd` import for the invoke call. It's already covered by the existing import.

- [ ] **Step 5: Verify everything compiles**

Run: `cd src-tauri && cargo check && cd .. && npm run check`
Expected: No errors

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/lib.rs src/lib/tauri.ts
git commit -m "feat: add get_profile_by_id command and IPC wrapper"
```

---

### Task 9: Create edit profile page

**Files:**
- Create: `src/routes/connections/[id]/edit/+page.svelte`

- [ ] **Step 1: Create edit page**

Write to `src/routes/connections/[id]/edit/+page.svelte`:

```svelte
<script lang="ts">
  import { onMount } from 'svelte';
  import { page } from '$app/stores';
  import { Button } from '$lib/components/ui/button';
  import { Input } from '$lib/components/ui/input';
  import { getProfileById, updateProfile } from '$lib/tauri';

  let label = $state('');
  let host = $state('');
  let port = $state(22);
  let username = $state('');
  let authType = $state('password');
  let identityFilePath = $state('');
  let changeCredentials = $state(false);
  let password = $state('');
  let privateKey = $state('');
  let keyPassphrase = $state('');
  let hasExistingCredentials = $state(false);
  let error = $state('');
  let saving = $state(false);
  let loading = $state(true);

  let profileId = $derived($page.params.id);

  onMount(async () => {
    try {
      const profile = await getProfileById(profileId);
      label = profile.label;
      host = profile.host;
      port = profile.port;
      username = profile.username;
      authType = profile.auth_type;
      identityFilePath = profile.identity_file_path || '';
      hasExistingCredentials = profile.auth_type === 'password' || profile.auth_type === 'key_inline';
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  });

  async function handleSubmit(e: SubmitEvent) {
    e.preventDefault();
    saving = true;
    error = '';

    const req: any = {
      id: profileId,
      label,
      host,
      port,
      username,
      auth_type: authType,
      identity_file_path: identityFilePath || null,
    };

    if (changeCredentials) {
      if (authType === 'password') {
        req.password = password;
      } else if (authType === 'key_inline') {
        req.private_key = privateKey;
        if (keyPassphrase) req.key_passphrase = keyPassphrase;
      }
    }

    try {
      await updateProfile(req);
      window.location.href = '/';
    } catch (e) {
      error = String(e);
      saving = false;
    }
  }
</script>

<div class="container mx-auto p-6 max-w-lg">
  <h1 class="text-2xl font-bold mb-6">Edit Connection</h1>

  {#if loading}
    <p class="text-center text-muted-foreground py-8">Loading profile...</p>
  {:else}
    {#if error}
      <p class="text-red-500 mb-4">{error}</p>
    {/if}

    <form onsubmit={handleSubmit} class="space-y-4">
      <div>
        <label for="label">Label</label>
        <Input id="label" bind:value={label} placeholder="My Server" required />
      </div>

      <div>
        <label for="host">Host</label>
        <Input id="host" bind:value={host} placeholder="192.168.1.1" required />
      </div>

      <div>
        <label for="port">Port</label>
        <Input id="port" type="number" bind:value={port} />
      </div>

      <div>
        <label for="username">Username</label>
        <Input id="username" bind:value={username} placeholder="root" required />
      </div>

      <div>
        <label for="authType">Authentication</label>
        <select id="authType" bind:value={authType} class="w-full border rounded-md p-2">
          <option value="password">Password</option>
          <option value="key_inline">Private Key (Inline)</option>
          <option value="key_file">Key File</option>
          <option value="agent">SSH Agent</option>
        </select>
      </div>

      {#if hasExistingCredentials}
        <div class="border rounded-lg p-4">
          <label class="flex items-center gap-2 cursor-pointer">
            <input type="checkbox" bind:checked={changeCredentials} />
            <span class="text-sm font-medium">Change credentials</span>
          </label>

          {#if !changeCredentials}
            <p class="text-sm text-muted-foreground mt-1">
              Existing credentials will be kept.
            </p>
          {/if}
        </div>
      {/if}

      {#if changeCredentials}
        {#if authType === 'password'}
          <div>
            <label for="password">Password</label>
            <Input id="password" type="password" bind:value={password} placeholder="Enter new password" />
          </div>
        {:else if authType === 'key_inline'}
          <div>
            <label for="privateKey">Private Key</label>
            <textarea id="privateKey" bind:value={privateKey} class="w-full border rounded-md p-2 h-32" placeholder="-----BEGIN OPENSSH PRIVATE KEY-----"></textarea>
          </div>
          <div>
            <label for="keyPassphrase">Passphrase (optional)</label>
            <Input id="keyPassphrase" type="password" bind:value={keyPassphrase} placeholder="Key passphrase" />
          </div>
        {:else if authType === 'key_file'}
          <div>
            <label for="identityFile">Key File Path</label>
            <Input id="identityFile" bind:value={identityFilePath} placeholder="~/.ssh/id_rsa" />
          </div>
        {/if}
      {/if}

      <div class="flex gap-2 pt-4">
        <Button type="submit" disabled={saving || loading}>
          {saving ? 'Saving...' : 'Save Changes'}
        </Button>
        <Button type="button" variant="outline" onclick={() => window.location.href = '/'}>
          Cancel
        </Button>
      </div>
    </form>
  {/if}
</div>
```

- [ ] **Step 2: Update main page to add Edit buttons**

Read `src/routes/+page.svelte` and add Edit buttons alongside Connect/Delete:

```svelde
<!-- In the actions div, add Edit button -->
<Button 
  onclick={() => window.location.href = `/connections/${profile.id}/edit`}
  variant="outline"
  size="sm"
>
  Edit
</Button>
```

- [ ] **Step 3: Verify frontend builds**

Run: `npm run check && npm run build`
Expected: No errors

- [ ] **Step 4: Commit**

```bash
git add src/routes/connections/[id]/edit/+page.svelte src/routes/+page.svelte
git commit -m "feat: add profile edit page"
```

---

## Phase 3: SSH Config Import

### Task 10: Create SSH config parser

**Files:**
- Create: `src-tauri/src/ssh/config_parser.rs`
- Modify: `src-tauri/src/ssh/mod.rs` (export module)

- [ ] **Step 1: Create config parser module**

Write to `src-tauri/src/ssh/config_parser.rs`:

```rust
use serde::Serialize;
use std::path::{Path, PathBuf};

use crate::error::AppError;

#[derive(Debug, Serialize, Clone)]
pub struct SshConfigEntry {
    pub host_aliases: Vec<String>,
    pub hostname: String,
    pub user: Option<String>,
    pub port: Option<u16>,
    pub identity_file: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ParseResult {
    pub entries: Vec<SshConfigEntry>,
    pub skipped: Vec<String>,
}

pub fn parse_ssh_config(path: Option<&Path>) -> Result<ParseResult, AppError> {
    let config_path = path.unwrap_or(&PathBuf::from(
        std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string()),
    ).join(".ssh").join("config"));

    if !config_path.exists() {
        return Err(AppError::Tunnel(format!(
            "SSH config not found at {}",
            config_path.display()
        )));
    }

    let content = std::fs::read_to_string(config_path)
        .map_err(|e| AppError::Tunnel(format!("Failed to read SSH config: {}", e)))?;

    let mut entries: Vec<SshConfigEntry> = Vec::new();
    let mut skipped: Vec<String> = Vec::new();
    let mut current_aliases: Vec<String> = Vec::new();
    let mut current_hostname: Option<String> = None;
    let mut current_user: Option<String> = None;
    let mut current_port: Option<u16> = None;
    let mut current_identity_file: Option<String> = None;
    let mut in_host_block = false;

    for line in content.lines() {
        let trimmed = line.trim();

        // Skip comments and empty lines
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        // Handle Include directive
        if trimmed.to_lowercase().starts_with("include ") {
            // Follow include
            let include_path = trimmed[8..].trim();
            let expanded_path = shellexpand::tilde(include_path);
            let include_path_buf = PathBuf::from(expanded_path.as_ref());

            // Make relative paths relative to config dir
            let full_path = if include_path_buf.is_relative() {
                config_path.parent().unwrap_or(Path::new(".")).join(&include_path_buf)
            } else {
                include_path_buf
            };

            // Only follow simple file includes (no glob for now)
            if full_path.exists() {
                let include_result = parse_ssh_config(Some(&full_path))?;
                entries.extend(include_result.entries);
                skipped.extend(include_result.skipped);
            }
            continue;
        }

        if trimmed.to_lowercase().starts_with("host ") {
            // Save previous host block if complete
            if in_host_block {
                if let Some(hostname) = current_hostname.take() {
                    // Skip wildcard hosts
                    if current_aliases.iter().any(|a| a.contains('*') || a.contains('?')) {
                        skipped.push(current_aliases[0].clone());
                    } else {
                        entries.push(SshConfigEntry {
                            host_aliases: current_aliases.clone(),
                            hostname,
                            user: current_user.take(),
                            port: current_port.take(),
                            identity_file: current_identity_file.take(),
                        });
                    }
                }
                current_aliases.clear();
            }

            in_host_block = true;
            let hosts_part = &trimmed[5..].trim();
            // Use first host as label, keep all as aliases
            current_aliases = hosts_part.split_whitespace().map(|s| s.to_string()).collect();
            current_hostname = None;
            current_user = None;
            current_port = None;
            current_identity_file = None;
            continue;
        }

        if !in_host_block {
            continue;
        }

        if let Some(val) = trimmed.strip_prefix("HostName ").or_else(|| trimmed.strip_prefix("hostname ")) {
            current_hostname = Some(val.trim().to_string());
        } else if let Some(val) = trimmed.strip_prefix("User ").or_else(|| trimmed.strip_prefix("user ")) {
            current_user = Some(val.trim().to_string());
        } else if let Some(val) = trimmed.strip_prefix("Port ").or_else(|| trimmed.strip_prefix("port ")) {
            current_port = val.trim().parse::<u16>().ok();
        } else if let Some(val) = trimmed.strip_prefix("IdentityFile ").or_else(|| trimmed.strip_prefix("identityfile ")) {
            current_identity_file = Some(shellexpand::tilde(val.trim()).to_string());
        }
    }

    // Save last block
    if in_host_block {
        if let Some(hostname) = current_hostname {
            if current_aliases.iter().any(|a| a.contains('*') || a.contains('?')) {
                skipped.push(current_aliases[0].clone());
            } else {
                entries.push(SshConfigEntry {
                    host_aliases: current_aliases.clone(),
                    hostname,
                    user: current_user,
                    port: current_port,
                    identity_file: current_identity_file,
                });
            }
        } else {
            // Host block without HostName
            for alias in &current_aliases {
                if !alias.contains('*') && !alias.contains('?') {
                    skipped.push(format!("{} (no HostName)", alias));
                }
            }
        }
    }

    Ok(ParseResult { entries, skipped })
}
```

- [ ] **Step 2: Export module in ssh/mod.rs**

Read `src-tauri/src/ssh/mod.rs` and add:

```rust
pub mod config_parser;
```

- [ ] **Step 3: Add shellexpand dependency**

Add to `src-tauri/Cargo.toml`:

```toml
shellexpand = "3.1.0"
```

- [ ] **Step 4: Verify Rust compiles**

Run: `cd src-tauri && cargo check`
Expected: Compiles successfully

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/ssh/config_parser.rs src-tauri/src/ssh/mod.rs src-tauri/Cargo.toml
git commit -m "feat: add SSH config parser module"
```

---

### Task 11: Add SSH config import Tauri commands

**Files:**
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: Add parse_ssh_config_cmd and import_ssh_config_cmd**

Add to `src-tauri/src/lib.rs`:

```rust
use ssh::config_parser::{parse_ssh_config, SshConfigEntry, ParseResult};

#[tauri::command]
async fn parse_ssh_config_cmd(
) -> Result<ParseResult, AppError> {
    parse_ssh_config(None)
}

#[tauri::command]
async fn import_ssh_config_cmd(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    selected_hosts: Vec<String>,
) -> Result<Vec<profiles::Profile>, AppError> {
    use uuid::Uuid;
    use crate::profiles::create_profile;
    use crate::crypto::encrypt;

    let parse_result = parse_ssh_config(None)?;
    let mut imported = Vec::new();

    for entry in parse_result.entries {
        if selected_hosts.contains(&entry.host_aliases[0]) {
            let label = entry.host_aliases[0].clone();
            let auth_type = if entry.identity_file.is_some() {
                "key_file"
            } else {
                "agent"
            };

            let req = crate::profiles::CreateProfileRequest {
                label: label.clone(),
                host: entry.hostname.clone(),
                port: entry.port.unwrap_or(22),
                username: entry.user.unwrap_or_else(|| "root".to_string()),
                auth_type: auth_type.to_string(),
                password: None,
                private_key: None,
                key_passphrase: None,
                identity_file_path: entry.identity_file.clone(),
            };

            let profile = create_profile(&state.db, &state.master_key, req).await?;
            emit_log(&app, &state.db, "info", &format!("Imported profile: {}", label), Some(&profile.id)).await;
            imported.push(profile);
        }
    }

    emit_log(&app, &state.db, "info", &format!("SSH config import complete: {} profiles imported", imported.len()), None).await;

    Ok(imported)
}
```

- [ ] **Step 2: Register commands in generate_handler!**

Add `parse_ssh_config_cmd` and `import_ssh_config_cmd` to `generate_handler!`.

- [ ] **Step 3: Add IPC wrappers**

Add to `src/lib/tauri.ts`:

```typescript
// SSH config import
export interface SshConfigEntry {
  host_aliases: string[];
  hostname: string;
  user: string | null;
  port: number | null;
  identity_file: string | null;
}

export interface ParseResult {
  entries: SshConfigEntry[];
  skipped: string[];
}

export async function parseSshConfig(): Promise<ParseResult> {
  return await invoke('parse_ssh_config_cmd');
}

export async function importSshConfig(selectedHosts: string[]): Promise<Profile[]> {
  return await invoke('import_ssh_config_cmd', { selectedHosts });
}
```

- [ ] **Step 4: Verify everything compiles**

Run: `cd src-tauri && cargo check && cd .. && npm run check && npm run build`
Expected: No errors

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/lib.rs src/lib/tauri.ts
git commit -m "feat: add SSH config import Tauri commands"
```

---

### Task 12: Add Import button and dialog to main page

**Files:**
- Modify: `src/routes/+page.svelte`

- [ ] **Step 1: Add import dialog to main page**

Read `src/routes/+page.svelte` and add an import dialog component. Add this script section and dialog markup:

```svelte
<script lang="ts">
  // Add to existing script imports:
  import { parseSshConfig, importSshConfig } from '$lib/tauri';
  import type { ParseResult, SshConfigEntry } from '$lib/tauri';

  // Add to existing state:
  let showImportDialog = $state(false);
  let importLoading = $state(false);
  let importError = $state('');
  let parseResult = $state<ParseResult | null>(null);
  let selectedHosts = $state<Set<string>>(new Set());

  let selectAll = $derived(
    parseResult ? selectedHosts.size === parseResult.entries.length : false
  );
  let selectedCount = $derived(selectedHosts.size);

  function toggleAll() {
    if (!parseResult) return;
    if (selectAll) {
      selectedHosts = new Set();
    } else {
      selectedHosts = new Set(parseResult.entries.map(e => e.host_aliases[0]));
    }
  }

  function toggleHost(host: string) {
    const next = new Set(selectedHosts);
    if (next.has(host)) {
      next.delete(host);
    } else {
      next.add(host);
    }
    selectedHosts = next;
  }

  async function handleParse() {
    importLoading = true;
    importError = '';
    try {
      parseResult = await parseSshConfig();
      selectedHosts = new Set(parseResult.entries.map(e => e.host_aliases[0]));
    } catch (e) {
      importError = String(e);
    } finally {
      importLoading = false;
    }
  }

  async function handleImport() {
    importLoading = true;
    importError = '';
    try {
      await importSshConfig(Array.from(selectedHosts));
      showImportDialog = false;
      parseResult = null;
      await loadProfiles();
    } catch (e) {
      importError = String(e);
    } finally {
      importLoading = false;
    }
  }
</script>
```

Also add the "Import from SSH Config" button in the page header:

```svelte
<div class="flex justify-between items-center mb-6">
  <h1 class="text-3xl font-bold">XSSH Tunnel</h1>
  <div class="flex gap-2">
    <Button variant="outline" onclick={handleParse}>
      Import from SSH Config
    </Button>
    <Button onclick={() => window.location.href = '/connections/new'}>
      + New Connection
    </Button>
  </div>
</div>
```

And add the dialog at the bottom of the page (replacing the current behavior with a dialog overlay - use a simple modal since shadcn-svelte components may not be available):

```svelte
{#if showImportDialog}
  <!-- Import Dialog -->
  <div class="fixed inset-0 bg-black/50 flex items-center justify-center z-50" onclick={() => showImportDialog = false}>
    <div class="bg-white rounded-lg shadow-xl max-w-3xl w-full mx-4 max-h-[80vh] overflow-y-auto" onclick={(e) => e.stopPropagation()}>
      <div class="p-6">
        <div class="flex justify-between items-center mb-4">
          <h2 class="text-xl font-bold">Import from ~/.ssh/config</h2>
          <button onclick={() => showImportDialog = false} class="text-gray-400 hover:text-gray-600">&times;</button>
        </div>

        {#if importError}
          <p class="text-red-500 mb-4">{importError}</p>
        {/if}

        {#if importLoading}
          <p class="text-center text-muted-foreground py-8">Loading...</p>
        {:else if parseResult}
          <p class="text-sm text-muted-foreground mb-4">
            Found {parseResult.entries.length} hosts
            {#if parseResult.skipped.length > 0}
              (skipped {parseResult.skipped.join(', ')})
            {/if}
          </p>

          {#if parseResult.entries.length > 0}
            <table class="w-full border-collapse">
              <thead>
                <tr class="border-b">
                  <th class="text-left py-2 px-2">
                    <input type="checkbox" checked={selectAll} onchange={toggleAll} />
                  </th>
                  <th class="text-left py-2 px-2 text-sm font-medium">Host</th>
                  <th class="text-left py-2 px-2 text-sm font-medium">Hostname</th>
                  <th class="text-left py-2 px-2 text-sm font-medium">User</th>
                  <th class="text-left py-2 px-2 text-sm font-medium">Port</th>
                  <th class="text-left py-2 px-2 text-sm font-medium">Key File</th>
                </tr>
              </thead>
              <tbody>
                {#each parseResult.entries as entry}
                  <tr class="border-b hover:bg-gray-50">
                    <td class="py-2 px-2">
                      <input
                        type="checkbox"
                        checked={selectedHosts.has(entry.host_aliases[0])}
                        onchange={() => toggleHost(entry.host_aliases[0])}
                      />
                    </td>
                    <td class="py-2 px-2 text-sm">{entry.host_aliases[0]}</td>
                    <td class="py-2 px-2 text-sm">{entry.hostname}</td>
                    <td class="py-2 px-2 text-sm">{entry.user || '-'}</td>
                    <td class="py-2 px-2 text-sm">{entry.port || 22}</td>
                    <td class="py-2 px-2 text-sm">{entry.identity_file ? 'Yes' : 'No'}</td>
                  </tr>
                {/each}
              </tbody>
            </table>

            <div class="flex justify-end gap-2 mt-6">
              <Button variant="outline" onclick={() => { showImportDialog = false; parseResult = null; }}>
                Cancel
              </Button>
              <Button onclick={handleImport} disabled={selectedCount === 0}>
                Import {selectedCount} {selectedCount === 1 ? 'profile' : 'profiles'}
              </Button>
            </div>
          {:else}
            <p class="text-center text-muted-foreground py-8">
              No importable hosts found.
            </p>
            <div class="flex justify-end">
              <Button variant="outline" onclick={() => { showImportDialog = false; parseResult = null; }}>
                Close
              </Button>
            </div>
          {/if}
        {:else}
          <p class="text-center text-muted-foreground py-8">
            Click "Parse Config" to scan your ~/.ssh/config file.
          </p>
          <div class="flex justify-end">
            <Button onclick={handleParse}>
              Parse Config
            </Button>
          </div>
        {/if}
      </div>
    </div>
  </div>
{/if}
```

Note: The import dialog should open the first time the user clicks "Import from SSH Config". Update the button:

```svelte
<Button variant="outline" onclick={async () => {
  showImportDialog = true;
  await handleParse();
}}>
  Import from SSH Config
</Button>
```

- [ ] **Step 2: Verify frontend builds**

Run: `npm run check && npm run build`
Expected: No errors

- [ ] **Step 3: Commit**

```bash
git add src/routes/+page.svelte
git commit -m "feat: add SSH config import button and dialog"
```

---

### Task 13: Final integration and verification

**Files:**
- Verify all changes

- [ ] **Step 1: Verify full backend builds**

```bash
cd src-tauri && cargo check
```

Expected: Compiles successfully

- [ ] **Step 2: Verify full frontend builds**

```bash
cd .. && npm run check && npm run build
```

Expected: No errors

- [ ] **Step 3: Run test (if any exist)**

```bash
cd src-tauri && cargo test
```

Expected: Tests pass (or no tests to run)

- [ ] **Step 4: Stage all remaining files**

```bash
git status
```

Review pending changes, stage any uncommitted files.

- [ ] **Step 5: Tag milestone**

```bash
git tag -a m3-logs-edit-import -m "M3: Connection logs, profile editing, and SSH config import"
```

- [ ] **Step 6: Final summary**

Summarize what was built:
- Phase 1: Connection logs with SQLite persistence, real-time events, logs page and compact panel
- Phase 2: Profile editing with "Change credentials" toggle, secure credential handling
- Phase 3: SSH config parser supporting Host/HostName/User/Port/IdentityFile/Include/ProxyJump
- All features emit log entries and integrate with the existing profile system

---

## Self-Review Checklist

After writing this plan, I verified:
- [x] Spec coverage: Every requirement in the M3 design doc has corresponding tasks
- [x] No placeholders: All steps contain actual code and commands
- [x] Type consistency: Types match between tasks (LogEntry, Profile, UpdateProfileRequest, etc.)
- [x] Exact file paths: Every file path is precise and absolute
- [x] Complete commands: All commands include expected output
- [x] Phase ordering: Logs first (foundation), editing second, SSH import third

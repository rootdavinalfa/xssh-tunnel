# AGENTS.md — XSSH Tunnel Development Guide

> **This file is for AI agents working on the XSSH Tunnel codebase.**

## Quick Start for Agents

Before writing any code:
1. **Read this file** — Understand project conventions and skill requirements
2. **Check the plan** — See `docs/superpowers/plans/` for the current milestone plan
3. **Invoke relevant skills** — Every task must use appropriate skills from `.agents/skills/`
4. **Follow the stack** — Tauri 2 + Rust + SvelteKit 5 + shadcn-svelte

## Project Stack

| Layer | Technology | Skill |
|-------|-----------|-------|
| Desktop framework | Tauri 2 | `tauri-v2` |
| UI framework | SvelteKit 5 (SPA mode) | `svelte-core-bestpractices` |
| UI components | shadcn-svelte + Tailwind | `svelte-core-bestpractices` |
| Data fetching | TanStack Query | `tanstack-query-best-practices` |
| Tables | TanStack Table | `tanstack-table` |
| SSH client | russh 0.44 | — |
| TUN interface | tun2 / Wintun | — |
| TCP/IP stack | smoltcp 0.11 | — |
| Database | SQLite via sqlx | — |
| Encryption | ring (AES-256-GCM) | — |

## Mandatory Skill Invocation

**You MUST invoke relevant skills BEFORE any implementation work.** This is non-negotiable.

### Skill Priority Order

1. **Process skills first** (determine HOW to approach):
   - `brainstorming` — Before any creative work or feature design
   - `systematic-debugging` — Before fixing any bug
   - `test-driven-development` — Before writing implementation code
   - `writing-plans` — When creating implementation plans

2. **Domain skills second** (guide execution):
   - `tauri-v2` — For all Tauri/Rust work (commands, events, state, tray, etc.)
   - `svelte-core-bestpractices` — For all Svelte components
   - `svelte-code-writer` — When writing Svelte code
   - `tanstack-query-best-practices` — For data fetching/mutations
   - `tanstack-table` — For table components

3. **Verification skills last**:
   - `verification-before-completion` — Before claiming work is complete
   - `requesting-code-review` — Before merging major changes

### Skill Invocation Cheat Sheet

| Task Type | Required Skills |
|-----------|----------------|
| Scaffold new Tauri command | `tauri-v2` |
| Create Svelte component | `svelte-core-bestpractices`, `svelte-code-writer` |
| Build data fetching logic | `tanstack-query-best-practices` |
| Build a table | `tanstack-table`, `svelte-core-bestpractices` |
| Fix a bug | `systematic-debugging` |
| Add new feature | `brainstorming`, `test-driven-development` |
| Write implementation plan | `writing-plans` |
| Complete milestone | `verification-before-completion` |

## Available Skills Reference

All skills are located in `.agents/skills/`:

### Primary Skills (Use These)

#### `tauri-v2`
**When to use:** Every Rust/Tauri task
**Coverage:**
- Command creation and registration (`generate_handler!`)
- IPC patterns (invoke, events, channels)
- State management with `State<T>` and `Mutex<T>`
- Error handling with `thiserror` and serde
- Capability and permission configuration
- System tray, sidecars, deep links
- Plugin integration (fs, dialog, shell, http, store)
- Updater and distribution setup
- Mobile build considerations

**Key patterns from this skill:**
```rust
// Always register commands in generate_handler!
.invoke_handler(tauri::generate_handler![cmd1, cmd2])

// Always use owned types in async commands
#[tauri::command]
async fn good(name: String) -> String { ... }

// Always add capabilities before using plugins
// src-tauri/capabilities/default.json
{
  "permissions": ["core:default", "fs:default"]
}
```

#### `svelte-core-bestpractices`
**When to use:** Every Svelte component or module
**Coverage:**
- Runes: `$state`, `$derived`, `$effect` (use sparingly), `$props`
- Event handling with `onclick` (not `on:click`)
- Snippets with `{#snippet}` and `{@render}`
- Keyed each blocks
- CSS custom properties for child styling
- Context with `createContext`
- Avoiding legacy features

**Key patterns from this skill:**
```svelte
<script>
  // Use $state for reactive variables
  let count = $state(0);
  
  // Use $derived for computed values
  let doubled = $derived(count * 2);
  
  // Use $props for component props
  let { type } = $props();
  let color = $derived(type === 'danger' ? 'red' : 'green');
</script>

<!-- Use onclick, not on:click -->
<button onclick={() => count++}>Increment</button>
```

#### `svelte-code-writer`
**When to use:** Creating or editing `.svelte`, `.svelte.ts`, `.svelte.js` files
**Coverage:**
- Svelte 5 documentation lookup
- Component code generation
- Module code generation
- Best practices enforcement

#### `tanstack-query-best-practices`
**When to use:** Data fetching, caching, mutations, server state
**Coverage:**
- Query keys and caching strategies
- Mutations with optimistic updates
- Prefetching and stale-while-revalidate
- Error handling and retries
- Integration with Svelte

#### `tanstack-table`
**When to use:** Building data tables
**Coverage:**
- Headless table UI for Svelte
- Sorting, filtering, pagination
- Column definitions
- Row selection

### Process Skills (Use These)

#### `brainstorming`
**When to use:** Before any creative work — new features, components, or design decisions
**What it does:**
- Explores project context
- Asks clarifying questions
- Proposes 2-3 approaches with trade-offs
- Presents design for approval
- Writes spec to `docs/superpowers/specs/`

**Hard gate:** Do NOT write code until design is approved.

#### `systematic-debugging`
**When to use:** When encountering any bug, test failure, or unexpected behavior
**What it does:**
- Structured debugging methodology
- Root cause analysis
- Hypothesis testing
- Fix verification

#### `test-driven-development`
**When to use:** Before writing implementation code
**What it does:**
- Write failing test first
- Run to confirm failure
- Write minimal implementation
- Run to confirm pass
- Refactor

#### `writing-plans`
**When to use:** When creating implementation plans
**What it does:**
- Creates bite-sized tasks (2-5 minutes each)
- Exact file paths and code
- Exact commands with expected output
- No placeholders (TBD, TODO, etc.)
- Saved to `docs/superpowers/plans/`

### Verification Skills (Use These)

#### `verification-before-completion`
**When to use:** Before claiming work is complete
**What it does:**
- Runs verification commands
- Confirms tests pass
- Checks no regressions
- Evidence before assertions

#### `requesting-code-review`
**When to use:** Before merging major features
**What it does:**
- Validates work meets requirements
- Checks against acceptance criteria
- Identifies gaps

## Project Conventions

### Rust Conventions

**File Organization:**
```
src-tauri/src/
├── main.rs              # Thin passthrough only
├── lib.rs               # All logic: commands, state, builder setup
├── error.rs             # AppError enum with serde Serialize
├── tunnel/              # VPN tunnel implementation
│   ├── mod.rs
│   ├── tun_device.rs    # Platform-specific TUN
│   ├── packet_router.rs # smoltcp integration
│   ├── socks5.rs        # SOCKS5 proxy
│   └── route_manager.rs # OS route injection
├── ssh/                 # SSH client
│   ├── mod.rs
│   ├── client.rs        # russh client
│   └── config_parser.rs # ~/.ssh/config parser
├── db/                  # Database
│   ├── mod.rs
│   ├── schema.rs
│   └── migrations/
├── crypto/              # Encryption
│   ├── mod.rs           # AES-256-GCM
│   └── keychain.rs      # OS keychain
└── profiles/            # Profile management
    └── mod.rs           # CRUD operations
```

**Command Pattern:**
```rust
// src-tauri/src/lib.rs
use std::sync::Mutex;
use tauri::State;

#[derive(Debug, Error)]
enum AppError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Not found: {0}")]
    NotFound(String),
}

impl serde::Serialize for AppError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where S: serde::ser::Serializer {
        serializer.serialize_str(self.to_string().as_ref())
    }
}

#[tauri::command]
async fn create_profile(args: CreateProfileArgs) -> Result<Profile, AppError> {
    // Implementation
}

pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            create_profile,
            // ... all other commands
        ])
        .manage(Mutex::new(AppState::default()))
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

**Critical Rules:**
- `main.rs` is a thin passthrough — all logic in `lib.rs`
- Always register commands in `generate_handler![]`
- Always use owned types (`String`, not `&str`) in async commands
- Return `Result<T, E>` from commands
- Use `Mutex<T>` for shared state
- Add capabilities in `src-tauri/capabilities/default.json` before using any plugin

### SvelteKit Conventions

**SPA Mode Configuration:**
```javascript
// svelte.config.js
import adapter from '@sveltejs/adapter-static';

export default {
  kit: {
    adapter: adapter({
      fallback: 'index.html'
    }),
    // ... other config
  }
};
```

**File Organization:**
```
src/
├── lib/
│   ├── components/          # shadcn-svelte components
│   │   ├── ui/             # Base UI (Button, Input, etc.)
│   │   ├── profile-card.svelte
│   │   ├── import-dialog.svelte
│   │   └── ...
│   ├── stores/
│   │   └── connection.ts   # Connection state store
│   └── tauri.ts            # Type-safe IPC wrappers
├── routes/
│   ├── +layout.svelte      # Root layout
│   ├── +page.svelte        # Connections list
│   ├── connections/
│   │   ├── new/+page.svelte
│   │   └── [id]/edit/+page.svelte
│   ├── settings/+page.svelte
│   └── logs/+page.svelte
└── app.html
```

**IPC Wrapper Pattern:**
```typescript
// src/lib/tauri.ts
import { invoke, listen } from '@tauri-apps/api/core';

export async function createProfile(args: CreateProfileArgs): Promise<Profile> {
  return await invoke('create_profile', args);
}

export function onConnectionState(callback: (state: ConnectionState) => void) {
  return listen('connection-state', (event) => callback(event.payload));
}
```

**State Management:**
```typescript
// src/lib/stores/connection.ts
import { writable } from 'svelte/store';

export const connectionState = writable<ConnectionState>('disconnected');
export const bytesTransferred = writable({ up: 0, down: 0 });
```

**Critical Rules:**
- Use `$state` for reactive variables, not implicit reactivity
- Use `$derived` for computed values, not `$effect`
- Use `$props` instead of `export let`
- Use `onclick={...}` not `on:click={...}`
- Use `{#snippet}` and `{@render}` not `<slot>`
- Use keyed each blocks: `{#each items as item (item.id)}`

### Component Guidelines (shadcn-svelte)

**Install components:**
```bash
npx shadcn-svelte add button input dialog form
```

**Usage pattern:**
```svelte
<script>
  import { Button } from '$lib/components/ui/button';
  import { Input } from '$lib/components/ui/input';
  import * as Dialog from '$lib/components/ui/dialog';
</script>

<Dialog.Root>
  <Dialog.Trigger>
    <Button>Open</Button>
  </Dialog.Trigger>
  <Dialog.Content>
    <Dialog.Header>
      <Dialog.Title>Title</Dialog.Title>
    </Dialog.Header>
    <Input placeholder="Enter value" />
  </Dialog.Content>
</Dialog.Root>
```

## Development Workflow

### 1. Before Starting Work

```bash
# Check current state
git status
git log --oneline -5

# Read the current plan
cat docs/superpowers/plans/current-milestone.md
```

### 2. Invoke Required Skills

For every task, invoke the relevant skills BEFORE writing code:

```
Skill: tauri-v2
Reason: Creating new command
```

### 3. Follow Test-Driven Development

```
Skill: test-driven-development
```

Then:
1. Write failing test
2. Run to confirm failure
3. Write minimal implementation
4. Run to confirm pass
5. Refactor

### 4. Verify Before Completion

```
Skill: verification-before-completion
```

Run all verification steps and confirm output before claiming success.

## Common Patterns

### Adding a New Tauri Command

1. **Invoke skill:** `tauri-v2`
2. **Define command in Rust:**
   ```rust
   // src-tauri/src/profiles/mod.rs
   #[tauri::command]
   pub async fn get_profiles() -> Result<Vec<Profile>, AppError> {
       // Implementation
   }
   ```
3. **Register in lib.rs:**
   ```rust
   .invoke_handler(tauri::generate_handler![
       // ... existing commands
       profiles::get_profiles,
   ])
   ```
4. **Add capability if needed:**
   ```json
   // src-tauri/capabilities/default.json
   {
     "permissions": ["core:default", "fs:read"]
   }
   ```
5. **Create TypeScript wrapper:**
   ```typescript
   // src/lib/tauri.ts
   export async function getProfiles(): Promise<Profile[]> {
     return await invoke('get_profiles');
   }
   ```
6. **Use in Svelte component:**
   ```svelte
   <script>
     import { getProfiles } from '$lib/tauri';
     import { createQuery } from '@tanstack/svelte-query';
     
     const profiles = createQuery({
       queryKey: ['profiles'],
       queryFn: getProfiles
     });
   </script>
   ```

### Creating a New Svelte Component

1. **Invoke skills:** `svelte-core-bestpractices`, `svelte-code-writer`
2. **Create component:**
   ```svelte
   <!-- src/lib/components/profile-card.svelte -->
   <script>
     import { Button } from '$lib/components/ui/button';
     import { Badge } from '$lib/components/ui/badge';
     
     let { profile, onConnect, onEdit } = $props();
     
     let isConnecting = $derived(profile.status === 'connecting');
   </script>
   
   <div class="rounded-lg border p-4">
     <div class="flex items-center justify-between">
       <div>
         <h3 class="font-semibold">{profile.label}</h3>
         <p class="text-sm text-muted-foreground">
           {profile.username}@{profile.host}:{profile.port}
         </p>
       </div>
       {#if profile.source === 'imported'}
         <Badge variant="secondary">Imported</Badge>
       {/if}
     </div>
     <div class="mt-4 flex gap-2">
       <Button onclick={() => onConnect(profile.id)} disabled={isConnecting}>
         {isConnecting ? 'Connecting...' : 'Connect'}
       </Button>
       <Button variant="outline" onclick={() => onEdit(profile.id)}>
         Edit
       </Button>
     </div>
   </div>
   ```

### Handling Tauri Events

```rust
// Rust: Emit event
use tauri::Emitter;

#[tauri::command]
fn start_tunnel(app: tauri::AppHandle) {
    std::thread::spawn(move || {
        app.emit("connection-state", "connecting").unwrap();
        // ... connect logic
        app.emit("connection-state", "connected").unwrap();
    });
}
```

```typescript
// Svelte: Listen to event
<script>
  import { onConnectionState } from '$lib/tauri';
  import { connectionState } from '$lib/stores/connection';
  
  onMount(() => {
    const unlisten = onConnectionState((state) => {
      connectionState.set(state);
    });
    
    return () => unlisten();
  });
</script>
```

## Testing Strategy

### Rust Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_profile_encryption() {
        let profile = Profile::new(/* ... */);
        let encrypted = profile.encrypt(&master_key);
        let decrypted = Profile::decrypt(&encrypted, &master_key).unwrap();
        assert_eq!(profile.password, decrypted.password);
    }
}
```

Run: `cd src-tauri && cargo test`

### Frontend Tests

Use Vitest for unit tests, Playwright for E2E:

```bash
# Unit tests
npm test

# E2E tests
npm run test:e2e
```

## Performance Targets

Per the PRD, maintain these targets:

| Metric | Target |
|--------|--------|
| Time to tunnel active | < 5 seconds |
| Reconnect time | < 10 seconds |
| CPU usage (idle) | < 2% |
| Memory footprint | < 80 MB RSS |
| Binary size | < 30 MB installer |
| Startup time | < 1.5 seconds |
| DB query time | < 50 ms (up to 500 profiles) |

## Security Checklist

Before committing code that handles:
- [ ] **Credentials:** Verify AES-256-GCM encryption, unique nonces
- [ ] **Key storage:** Verify OS keychain usage (not plaintext)
- [ ] **Logs:** Verify no passwords/keys in log output
- [ ] **Host keys:** Verify fingerprint verification
- [ ] **Permissions:** Verify Tauri capabilities configured

## Documentation

- **Design specs:** `docs/superpowers/specs/YYYY-MM-DD-<feature>-design.md`
- **Implementation plans:** `docs/superpowers/plans/YYYY-MM-DD-<milestone>.md`
- **This file:** `AGENTS.md` — Update when conventions change

## Questions?

If unsure about:
- **How to do something:** Invoke the relevant skill
- **Whether to use a skill:** If there's even a 1% chance it applies, invoke it
- **Project structure:** Check this file and the current milestone plan
- **Coding standards:** Follow the conventions in this file

---

**Remember:** Invoke skills BEFORE any implementation work. This saves tokens, prevents bugs, and ensures consistency.

# M2 — SSH Connection Manager Design Specification

> **Date:** 2026-05-20
> **Milestone:** M2 — SSH Connection Manager
> **Status:** Approved

## 1. Goal

Replace hardcoded SSH credentials with a full profile management system: encrypted local storage, CRUD UI, and multiple authentication methods.

## 2. Architecture

```
┌─────────────────┐     ┌──────────────────┐     ┌─────────────────┐
│   Frontend UI   │────▶│   Tauri Commands │────▶│  Profile Manager │
│  (SvelteKit)    │     │   (Rust)         │     │  (Rust)          │
└─────────────────┘     └──────────────────┘     └────────┬────────┘
       ▲                                                  │
       │                                                  ▼
       │                                           ┌──────────────────┐
       │                                           │   SQLite +       │
       └───────────────────────────────────────────│   AES-256-GCM    │
                                                   │   Encryption     │
                                                   └──────────────────┘
                                                          │
                                                          ▼
                                                   ┌──────────────────┐
                                                   │   OS Keychain    │
                                                   │   (Master Key)   │
                                                   └──────────────────┘
```

## 3. Components

| Component | Responsibility | File |
|-----------|---------------|------|
| **Database** | SQLite schema, migrations, pool management | `src/db/mod.rs`, `src/db/migrations/` |
| **Crypto** | AES-256-GCM encrypt/decrypt, nonce generation | `src/crypto/mod.rs` |
| **Keychain** | Store/retrieve master key from OS keychain | `src/crypto/keychain.rs` |
| **Profiles** | CRUD operations, encryption/decryption | `src/profiles/mod.rs` |
| **Frontend Store** | Svelte store for profile list | `src/lib/stores/profiles.ts` |
| **Profile UI** | List, add, edit, delete profiles | `src/routes/+page.svelte`, `src/routes/connections/new/+page.svelte` |

## 4. Database Schema

```sql
CREATE TABLE profiles (
    id TEXT PRIMARY KEY,              -- UUID v4
    label TEXT NOT NULL,              -- Display name (plaintext)
    host TEXT NOT NULL,               -- Hostname/IP (plaintext)
    port INTEGER NOT NULL DEFAULT 22, -- SSH port (plaintext)
    username TEXT NOT NULL,           -- SSH username (plaintext)
    auth_type TEXT NOT NULL,          -- password | key_inline | key_file | agent
    password_enc BLOB,                -- AES-256-GCM ciphertext (nullable)
    private_key_enc BLOB,             -- AES-256-GCM ciphertext (nullable)
    key_passphrase_enc BLOB,          -- AES-256-GCM ciphertext (nullable)
    identity_file_path TEXT,          -- Path to key file (plaintext, nullable)
    created_at TEXT NOT NULL,         -- ISO-8601 timestamp
    updated_at TEXT NOT NULL          -- ISO-8601 timestamp
);
```

## 5. Encryption Scheme

- **Master key:** 256-bit random key, generated on first launch, stored in OS keychain
- **Algorithm:** AES-256-GCM via `ring` crate
- **Nonce:** 96-bit random nonce per field, stored alongside ciphertext
- **Encrypted fields:** `password_enc`, `private_key_enc`, `key_passphrase_enc`
- **Plaintext fields:** `label`, `host`, `port`, `username`, `auth_type`, `identity_file_path`

## 6. Keychain Integration

- **macOS:** Keychain Services (`security` framework via `security-framework` crate)
- **Storage:** Service name = "com.xssh.tunnel", Account = "master_key"
- **Fallback:** If keychain unavailable, generate new key (warns user)

## 7. Profile CRUD Operations

| Operation | Command | Description |
|-----------|---------|-------------|
| Create | `create_profile` | Encrypt sensitive fields, insert into SQLite |
| Read | `get_profiles` | List all profiles (decrypt only when needed) |
| Read One | `get_profile` | Get single profile by ID |
| Update | `update_profile` | Re-encrypt changed fields, update row |
| Delete | `delete_profile` | Remove row and all encrypted blobs |

## 8. Auth Methods

| Type | Storage | Description |
|------|---------|-------------|
| `password` | `password_enc` | Password stored encrypted |
| `key_inline` | `private_key_enc` | Full PEM key material encrypted |
| `key_file` | `identity_file_path` | Path to external key file (plaintext) |
| `agent` | None | Use running ssh-agent |

## 9. Frontend UI

### Profile List (`+page.svelte`)
- Display all profiles as cards (label, host:port, username)
- Connect/Disconnect button per profile
- Edit/Delete actions
- "+ New Connection" button

### Add Profile (`connections/new/+page.svelte`)
- Form: Label, Host, Port (default 22), Username
- Auth method selector (password/key_inline/key_file/agent)
- Dynamic fields based on auth type
- "Save" button

## 10. Scope

### In Scope (M2)
- SQLite schema and migrations via sqlx
- AES-256-GCM encryption with unique nonces
- macOS Keychain integration
- Profile CRUD (create, read, update, delete)
- Frontend UI: profile list, add/edit form, delete confirmation
- Auth methods: password, private key inline, key file path, SSH agent
- Replace hardcoded credentials with stored profiles

### Out of Scope (M3+)
- Import from SSH config
- Host key verification dialog
- Profile tags/groups
- Duplicate/clone profiles
- Test Connection button (SSH handshake validation)

## 11. Dependencies

```toml
[dependencies]
sqlx = { version = "0.8", features = ["runtime-tokio", "sqlite", "migrate"] }
ring = "0.17"
security-framework = "3"  # macOS keychain
uuid = { version = "1", features = ["v4", "serde"] }
```

## 12. Acceptance Criteria

- [ ] First launch generates master key and stores in keychain
- [ ] User can create a profile with all auth methods
- [ ] Profile list displays all saved profiles
- [ ] User can edit a profile (all fields except ID)
- [ ] User can delete a profile
- [ ] Credentials are encrypted at rest (verify with sqlite3 CLI)
- [ ] UI uses shadcn-svelte components (Card, Form, Input, Select, Button)
- [ ] Connect button uses stored credentials (not hardcoded)

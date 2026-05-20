CREATE TABLE IF NOT EXISTS profiles (
    id TEXT PRIMARY KEY,
    label TEXT NOT NULL,
    host TEXT NOT NULL,
    port INTEGER NOT NULL DEFAULT 22,
    username TEXT NOT NULL,
    auth_type TEXT NOT NULL CHECK(auth_type IN ('password', 'key_inline', 'key_file', 'agent')),
    password_enc BLOB,
    private_key_enc BLOB,
    key_passphrase_enc BLOB,
    identity_file_path TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_profiles_label ON profiles(label);
CREATE INDEX IF NOT EXISTS idx_profiles_host ON profiles(host);
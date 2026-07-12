pub(super) const MIGRATION_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS schema_migrations (
  version BIGINT PRIMARY KEY,
  name TEXT NOT NULL,
  applied_at BIGINT NOT NULL
)
"#;

pub(super) const BLOB_MUTATION_LOCK_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS blob_mutation_locks (
  hash TEXT PRIMARY KEY
)
"#;

pub(super) const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS settings (
  key TEXT PRIMARY KEY,
  value TEXT NOT NULL,
  updated_at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS users (
  id TEXT PRIMARY KEY,
  email TEXT NOT NULL UNIQUE,
  username TEXT NOT NULL UNIQUE,
  password_hash TEXT,
  role TEXT NOT NULL,
  is_disabled INTEGER NOT NULL DEFAULT 0,
  email_verified_at INTEGER,
  two_factor_enabled INTEGER NOT NULL DEFAULT 0,
  created_at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS sessions (
  id TEXT PRIMARY KEY,
  user_id TEXT NOT NULL,
  token_hash TEXT NOT NULL UNIQUE,
  expires_at INTEGER NOT NULL,
  created_at INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_sessions_token_hash ON sessions(token_hash);
CREATE TABLE IF NOT EXISTS api_tokens (
  id TEXT PRIMARY KEY,
  user_id TEXT NOT NULL,
  name TEXT NOT NULL,
  token_hash TEXT NOT NULL UNIQUE,
  scopes_json TEXT NOT NULL,
  expires_at INTEGER,
  last_used_at INTEGER,
  revoked_at INTEGER,
  created_at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS password_reset_tokens (
  id TEXT PRIMARY KEY,
  user_id TEXT NOT NULL,
  token_hash TEXT NOT NULL UNIQUE,
  expires_at INTEGER NOT NULL,
  used_at INTEGER,
  created_at INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_password_reset_token_hash ON password_reset_tokens(token_hash);
CREATE TABLE IF NOT EXISTS oidc_identities (
  id TEXT PRIMARY KEY,
  user_id TEXT NOT NULL,
  issuer TEXT NOT NULL,
  subject TEXT NOT NULL,
  email TEXT NOT NULL,
  created_at INTEGER NOT NULL,
  last_seen_at INTEGER NOT NULL,
  UNIQUE(issuer, subject)
);
CREATE INDEX IF NOT EXISTS idx_oidc_identities_user_id ON oidc_identities(user_id);
CREATE TABLE IF NOT EXISTS email_verification_tokens (
  id TEXT PRIMARY KEY,
  user_id TEXT NOT NULL,
  token_hash TEXT NOT NULL UNIQUE,
  expires_at INTEGER NOT NULL,
  used_at INTEGER,
  created_at INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_email_verification_token_hash ON email_verification_tokens(token_hash);
CREATE TABLE IF NOT EXISTS two_factor_challenges (
  id TEXT PRIMARY KEY,
  user_id TEXT NOT NULL,
  challenge_hash TEXT NOT NULL UNIQUE,
  code_hash TEXT NOT NULL,
  expires_at INTEGER NOT NULL,
  used_at INTEGER,
  created_at INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_two_factor_challenge_hash ON two_factor_challenges(challenge_hash);
CREATE TABLE IF NOT EXISTS invite_tokens (
  id TEXT PRIMARY KEY,
  token_hash TEXT NOT NULL UNIQUE,
  created_by_user_id TEXT NOT NULL,
  role TEXT NOT NULL,
  expires_at INTEGER,
  used_by_user_id TEXT,
  used_at INTEGER,
  revoked_at INTEGER,
  created_at INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_invite_tokens_token_hash ON invite_tokens(token_hash);
CREATE TABLE IF NOT EXISTS blobs (
  hash TEXT PRIMARY KEY,
  size_bytes INTEGER NOT NULL,
  content_type TEXT,
  ref_count INTEGER NOT NULL DEFAULT 0,
  created_at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS blob_mutation_locks (
  hash TEXT PRIMARY KEY
);
CREATE TABLE IF NOT EXISTS files (
  id TEXT PRIMARY KEY,
  public_id TEXT NOT NULL UNIQUE,
  blob_hash TEXT NOT NULL,
  original_filename TEXT,
  extension TEXT,
  content_type TEXT,
  size_bytes INTEGER NOT NULL,
  image_width INTEGER,
  image_height INTEGER,
  owner_user_id TEXT,
  delete_token_hash TEXT,
  expires_at INTEGER,
  visibility TEXT NOT NULL DEFAULT 'unlisted',
  metadata_json TEXT,
  thumbnail_hash TEXT,
  state TEXT NOT NULL,
  created_at INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_files_public_id ON files(public_id);
CREATE INDEX IF NOT EXISTS idx_files_owner ON files(owner_user_id);
CREATE TABLE IF NOT EXISTS pastes (
  id TEXT PRIMARY KEY,
  public_id TEXT NOT NULL UNIQUE,
  title TEXT,
  content TEXT NOT NULL,
  syntax TEXT,
  owner_user_id TEXT,
  delete_token_hash TEXT,
  expires_at INTEGER,
  visibility TEXT NOT NULL DEFAULT 'unlisted',
  state TEXT NOT NULL,
  created_at INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_pastes_public_id ON pastes(public_id);
CREATE TABLE IF NOT EXISTS paste_revisions (
  id TEXT PRIMARY KEY,
  paste_id TEXT NOT NULL,
  title TEXT,
  content TEXT NOT NULL,
  syntax TEXT,
  created_by_user_id TEXT,
  created_at INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_paste_revisions_paste ON paste_revisions(paste_id);
CREATE TABLE IF NOT EXISTS reports (
  id TEXT PRIMARY KEY,
  item_kind TEXT NOT NULL,
  item_public_id TEXT NOT NULL,
  reporter_user_id TEXT,
  reason TEXT NOT NULL,
  details TEXT NOT NULL,
  state TEXT NOT NULL,
  created_at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS scanner_results (
  id TEXT PRIMARY KEY,
  item_kind TEXT NOT NULL,
  item_public_id TEXT NOT NULL,
  adapter TEXT NOT NULL,
  decision TEXT NOT NULL,
  detail TEXT NOT NULL,
  created_at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS audit_events (
  id TEXT PRIMARY KEY,
  actor_user_id TEXT,
  action TEXT NOT NULL,
  target TEXT NOT NULL,
  detail TEXT NOT NULL,
  created_at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS rate_limit_buckets (
  key TEXT PRIMARY KEY,
  window_start INTEGER NOT NULL,
  count INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS moderation_notes (
  id TEXT PRIMARY KEY,
  item_kind TEXT NOT NULL,
  item_public_id TEXT NOT NULL,
  report_id TEXT,
  actor_user_id TEXT,
  note TEXT NOT NULL,
  created_at INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_moderation_notes_item ON moderation_notes(item_kind, item_public_id);
"#;

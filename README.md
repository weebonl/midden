# Midden

Midden is a self-hostable file and paste sharing service written in Rust. The default behavior is intentionally close to simple public file hosts: anonymous uploads are enabled, quotas are off, links are public but unlisted, and uploaded files get compact Catbox-style URLs.

The admin surface lets operators tighten the instance: disable files or pastes, require accounts for specific actions, turn off the REST API, edit branding, configure quotas, enable reports, and wire storage to local disk or S3-compatible object storage.

## Status

This repository is the initial v1 implementation. It includes:

- Axum web server with server-rendered Minijinja templates
- SQLite/Postgres-capable SQLx repository layer
- Local and S3-compatible blob storage abstraction
- Multipart file upload
- Plain text pastes with optional syntax highlighting
- Catbox-style file links
- File metadata including MIME, size, checksum, original filename, and cheap image dimensions
- Optional metadata extraction, supported image metadata stripping, and thumbnail references
- REST upload and paste endpoints
- Authenticated API item listing, anonymous item claiming, token list/revocation, and moderator/admin report/search/item endpoints
- Web and REST report submission
- tus-compatible resumable upload creation, head, patch flow, and small web upload page
- Local accounts, owner bootstrap CLI, sessions, email verification, optional email-code two-factor authentication, and scoped API token storage
- Runtime admin settings stored in the database
- Admin-created users, roles, disable/enable actions, and invite-only signup tokens
- Invite listing and revocation, password change, and account self-deactivation
- Anonymous delete-token deletion and later account claiming
- Public reports, moderator filters, bulk actions, item moderation pages, notes, and audit log writes
- Optional paste content search, paste editing with revisions, preview pages, homepage blocks, dark mode behavior, and configurable default expirations
- Optional public browse/search with per-item visibility, moderation controls, robots settings, and cursor pagination
- CDN/reverse-proxy cache headers, optional signed internal raw-file URLs, and storage export/import tooling
- Health, readiness, and metrics endpoints
- Single-process background jobs for expiration cleanup, scanner retries, metadata/thumbnail backfill, and storage verification

## Quick Start

```sh
cargo run -- migrate
cargo run -- owner create --email admin@example.test --username admin --password change-me
cargo run -- serve
```

Open `http://127.0.0.1:8080`.

By default, Midden uses `sqlite://midden.db?mode=rwc` and stores blobs under `data/blobs`.

Docker examples are available for SQLite/local storage and Postgres/MinIO:

```sh
docker compose -f docker-compose.sqlite.yml up --build
docker compose -f docker-compose.postgres-minio.yml up --build
```

## Configuration

Midden reads `midden.toml` by default, or a custom file with `--config`. Environment variables use the `MIDDEN__` prefix and `__` for nesting, for example:

```sh
MIDDEN__SERVER__BIND=0.0.0.0:8080
MIDDEN__DATABASE__URL=sqlite://midden.db?mode=rwc
```

See `midden.example.toml`.

## Commands

```sh
midden serve
midden migrate
midden config check
midden config print-defaults
midden owner create --email admin@example.test --username admin --password change-me
midden owner reset-password --email admin@example.test --password new-password
midden storage gc --dry-run
midden storage verify
midden storage export ./midden-export
midden storage import ./midden-export
midden jobs run-once
```

## Public API

Upload a file:

```sh
curl -F file=@example.txt http://127.0.0.1:8080/api/v1/files
```

Create a paste:

```sh
curl -H 'content-type: application/json' \
  -d '{"content":"hello","syntax":"text"}' \
  http://127.0.0.1:8080/api/v1/pastes
```

Authenticated requests use `Authorization: Bearer TOKEN`.

Expiry values are optional and accept `never`, hours such as `12h`, or days such as `7d`.

Create tokens from the account page after logging in. Scopes are comma separated, for example `files:write,pastes:write,files:delete`.
Use `files:read` and `pastes:read` to list your own account items at `/api/v1/me/files` and `/api/v1/me/pastes`.
Use `items:claim` to claim anonymous files or pastes at `/api/v1/claim/{kind}/{id}`. Use `tokens:read` and `tokens:write` for token listing and revocation. Moderator/admin API scopes include `admin:reports`, `admin:items`, and `admin:search`.
Anonymous web uploads and pastes show a one-time delete token. After logging in, use `/claim/file/ID` or `/claim/paste/ID` with that token to attach the item to your account when policy allows it.
Use `reports:write` for authenticated report submission, or omit auth when anonymous API use is allowed.
When `features.public_browse` is enabled, uploads and pastes can set `visibility` to `public`; the default remains `unlisted`.

Web forms use a same-site CSRF cookie and hidden form token. API clients do not use the CSRF cookie; keep using bearer tokens and scoped API credentials.

## Templates And Branding

Built-in templates are embedded. Set `server.template_dir` to override any template by filename while falling back to built-ins for missing files; see `docs/templates.md` for filenames and shared context. Runtime branding settings include instance name, tagline, logo, favicon, accent color, custom CSS, footer links, homepage notices, homepage blocks, abuse email, contact URL, and public takedown text.

Admin settings use structured forms for features, policy, quotas, default expirations, rate limits, scanners, branding, dark mode behavior, delivery/cache behavior, processing jobs, public discovery, URL-upload safety, MIME mismatch behavior, and public takedown text.

## Optional OIDC Login

Set `features.oidc_login = true` and configure `[oidc]` with `enabled`, `issuer_url`, `client_id`, and optionally `client_secret`/`redirect_url`. Midden uses provider discovery, authorization-code exchange, and the userinfo endpoint to provision OIDC users by issuer and subject. Existing local-password users must link OIDC from `/account` before OIDC login can use their email address. Operators can restrict sign-in with `allowed_domains` or `allowed_groups`, and map role/group claim values with `[oidc.role_mappings]`.

## Optional SMTP

Configure `[smtp]` to enable password reset emails, open-signup email verification, email-code two-factor authentication, and report notifications to the configured abuse contact. If SMTP is disabled, admins can still reset owner passwords from the CLI.

## Operations

See `docs/deployment.md` for binary, Docker Compose, systemd, reverse proxy, health, metrics, and storage verification notes. See `docs/backup-restore.md` for SQLite/local storage and Postgres/S3 backup procedures.

Run `cargo test` for SQLite/local-storage, real HTTP, tus, OIDC mock-provider, fixture upload, migration, and moderation regression coverage. Optional Postgres and MinIO/S3 smoke tests run when the `MIDDEN_TEST_POSTGRES_URL` or `MIDDEN_TEST_S3_*` environment variables documented in `docs/deployment.md` are set.

## License

AGPL-3.0-only.

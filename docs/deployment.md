# Deployment

Midden ships as one binary. The service can run with SQLite plus local blob storage, or with Postgres plus S3-compatible storage.

## Binary

```sh
cargo build --release
install -m 0755 target/release/midden /usr/local/bin/midden
install -m 0644 midden.example.toml /etc/midden.toml
midden --config /etc/midden.toml migrate
midden --config /etc/midden.toml owner create --email admin@example.test --username admin --password 'change-me'
midden --config /etc/midden.toml serve
```

Set `RUST_LOG=info` for normal logs. Use `RUST_LOG=midden=debug,tower_http=info` while debugging request behavior.

## Docker Compose

SQLite and local storage:

```sh
docker compose -f docker-compose.sqlite.yml up --build
docker compose -f docker-compose.sqlite.yml exec midden midden owner create --email admin@example.test --username admin --password 'change-me'
```

Postgres and MinIO:

```sh
docker compose -f docker-compose.postgres-minio.yml up --build
docker compose -f docker-compose.postgres-minio.yml exec midden midden owner create --email admin@example.test --username admin --password 'change-me'
```

For a public deployment, change `MIDDEN__SERVER__PUBLIC_BASE_URL`, all default passwords, and any S3 credentials before exposing the service.

## systemd

Create a dedicated user and data directory:

```sh
useradd --system --create-home --home-dir /var/lib/midden midden
install -d -o midden -g midden /var/lib/midden/blobs
```

Example unit:

```ini
[Unit]
Description=Midden file and paste sharing service
After=network-online.target
Wants=network-online.target

[Service]
User=midden
Group=midden
WorkingDirectory=/var/lib/midden
Environment=RUST_LOG=info
ExecStart=/usr/local/bin/midden --config /etc/midden.toml serve
Restart=on-failure
RestartSec=5s
NoNewPrivileges=true
PrivateTmp=true
ProtectSystem=strict
ReadWritePaths=/var/lib/midden

[Install]
WantedBy=multi-user.target
```

Run migrations before restarting after upgrades:

```sh
sudo -u midden /usr/local/bin/midden --config /etc/midden.toml migrate
systemctl restart midden
```

## Reverse Proxy

Put Midden behind HTTPS in production and set:

```toml
[server]
public_base_url = "https://files.example.com"
behind_proxy = true

[security]
secure_cookies = true
```

Nginx sketch:

```nginx
server {
    listen 443 ssl http2;
    server_name files.example.com;

    client_max_body_size 0;

    location / {
        proxy_pass http://127.0.0.1:8080;
        proxy_http_version 1.1;
        proxy_set_header Host $host;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
        proxy_request_buffering off;
        proxy_read_timeout 300s;
        proxy_send_timeout 300s;
    }
}
```

Caddy sketch:

```caddyfile
files.example.com {
    request_body {
        max_size 0
    }
    reverse_proxy 127.0.0.1:8080 {
        header_up X-Forwarded-Proto {scheme}
        transport http {
            read_timeout 300s
            write_timeout 300s
        }
    }
}
```

Traefik sketch:

```yaml
http:
  routers:
    midden:
      rule: Host(`files.example.com`)
      service: midden
      entryPoints: [websecure]
      tls: {}
  services:
    midden:
      loadBalancer:
        servers:
          - url: http://127.0.0.1:8080
```

Keep proxy body limits aligned with `limits.max_upload_bytes` and `limits.max_tus_upload_bytes`. Disable proxy request buffering for large uploads when your proxy supports it, and keep read/write timeouts long enough for slow clients.

## Health And Metrics

- `GET /healthz`: process can answer HTTP.
- `GET /readyz`: database and storage backend are reachable.
- `GET /metrics`: OpenMetrics text with upload, paste, byte, file serving, report, scanner, rate-limit, and request-latency metrics.

Run `midden storage verify` after deploys and restores to compare database blob rows with backend objects. Run `midden storage gc --dry-run` from cron before enabling destructive expiration cleanup; without `--dry-run` it also removes expired auth rows and stale tus temp files. The built-in job runner performs the same expiration cleanup, scanner retry, metadata backfill, thumbnail-reference backfill, and storage verification work inside the web process when `[jobs].enabled` is true. Use `midden jobs run-once` to run one pass manually during maintenance.

## Admin Operations

Use `/admin` for structured feature, policy, quota, default-expiry, rate-limit, scanner, URL-upload, delivery/cache, processing, background job, public discovery, branding, homepage block, dark mode, MIME, and takedown settings. Use `/admin/users` for user creation, role changes, invite listing, invite revocation, account disable/enable actions, email verification state, and 2FA state. Use `/admin/reports` for filtered report queues and bulk actions; each report links to an item moderation page with metadata, reports, scanner output, audit events, moderator notes, visibility controls, state transitions, and blocked-hash creation for files.

Optional paste content search and paste editing are disabled by default. Enable them from `/admin` when account owners should be able to search paste bodies or revise owned pastes. Preview pages are also disabled by default; when enabled, safe image previews and small text previews render on the file preview page.

Public browse/search is disabled by default. When enabled, uploads and pastes remain unlisted unless the creator or a moderator sets `visibility = public`. The `/browse` page uses cursor pagination and `robots.txt` only allows browse indexing when `discovery.robots_index` is enabled.

`delivery.public_cache_seconds` controls `Cache-Control` for raw files and `delivery.static_cache_seconds` controls embedded or overridden static assets. Deployments that serve objects through an internal proxy can enable `delivery.signed_internal_urls` with `delivery.internal_url_secret`; API file objects then include a short-lived `/internal/files/{id}/raw` URL.

Use `midden storage export DIR` and `midden storage import DIR` to mirror blob objects between backends. Export writes `DIR/manifest.json` plus `DIR/blobs/{sha256}` objects. Move the database with the backup procedure, point the config at the new backend, import the blobs, then run `midden storage verify`.

When SMTP is configured, open self-signup accounts receive a verification link before they can log in. Password resets also mark the account email verified because the user has proven control of the mailbox. Local-password users can enable email-code two-factor authentication from `/account` after verification; if SMTP is later disabled, accounts with 2FA enabled cannot complete a password login until SMTP is restored or an admin resets the account policy.

OIDC users are linked by provider issuer and subject, not by email alone. New OIDC-only users are provisioned automatically when policy allows OIDC, but an existing local-password user must start `/account/oidc/link` while signed in before OIDC login can use that email address. Use `oidc.allowed_domains` and `oidc.allowed_groups` to restrict sign-in, and `[oidc.role_mappings]` to map role or group claim values to `user`, `moderator`, `admin`, or `owner`.

Template override filenames and shared Minijinja context are documented in `docs/templates.md`. The built-in homepage supports configured notices and homepage blocks, so most operators can add policy or contact content without replacing templates.

## Release Packaging

For a tagged release, build the binary and container image from the same tag, publish the sample production config, and generate checksums for every archive:

```sh
cargo build --release
tar -C target/release -czf midden-x86_64-unknown-linux-gnu.tar.gz midden
sha256sum midden-x86_64-unknown-linux-gnu.tar.gz > midden-x86_64-unknown-linux-gnu.tar.gz.sha256
docker build -t ghcr.io/OWNER/midden:TAG .
```

## Release Validation

`cargo test` runs SQLite/local-storage unit and real HTTP server coverage, including multipart uploads, paste creation, delete-token deletion, claiming, report moderation, account search, scoped API tokens, public browse visibility, signed internal URLs, cache headers, tus offsets/completion/ownership, migration backfills, and moderation state regressions. Binary upload fixtures live under `tests/fixtures` as small hex payloads for PNG, GIF, JPEG, and unknown binary data.

Set `MIDDEN_TEST_POSTGRES_URL` to include a Postgres migration smoke test. Set `MIDDEN_TEST_S3_BUCKET` plus optional `MIDDEN_TEST_S3_ENDPOINT`, `MIDDEN_TEST_S3_REGION`, `MIDDEN_TEST_S3_ACCESS_KEY_ID`, and `MIDDEN_TEST_S3_SECRET_ACCESS_KEY` to include the MinIO/S3 storage round trip.

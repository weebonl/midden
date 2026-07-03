# Environment Variables

Environment variables use the prefix `MIDDEN`, double underscores for nesting, and names matching the TOML keys.

## Examples

```text
MIDDEN__SERVER__BIND=0.0.0.0:8080
MIDDEN__SERVER__PUBLIC_BASE_URL=https://files.example.test
MIDDEN__DATABASE__URL=postgres://midden:secret@postgres:5432/midden
MIDDEN__STORAGE__BACKEND=s3
MIDDEN__STORAGE__S3__BUCKET=midden
MIDDEN__SECURITY__SECURE_COOKIES=true
MIDDEN__METRICS__ENABLED=true
```

These map to:

```toml
[server]
bind = "0.0.0.0:8080"
public_base_url = "https://files.example.test"

[database]
url = "postgres://midden:secret@postgres:5432/midden"

[storage]
backend = "s3"

[storage.s3]
bucket = "midden"

[security]
secure_cookies = true

[metrics]
enabled = true
```

## Lists And Tables

The config loader parses environment values when possible. For complex structures such as arrays, role quota maps, homepage blocks, scanner adapters, and rate limits, prefer TOML files unless your deployment tooling has a proven way to pass the equivalent structure.

## Secrets

Keep these out of committed files:

- `MIDDEN__OIDC__CLIENT_SECRET`
- `MIDDEN__SMTP__PASSWORD`
- `MIDDEN__STORAGE__S3__SECRET_ACCESS_KEY`
- `MIDDEN__DELIVERY__INTERNAL_URL_SECRET`
- `MIDDEN__METRICS__BEARER_TOKEN`
- `MIDDEN__MODERATION__NOTIFY_WEBHOOK_SECRET`

Use your process manager, container orchestrator, or secret manager to inject them at runtime.

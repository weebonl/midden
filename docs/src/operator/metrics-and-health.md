# Metrics And Health

Midden exposes health endpoints and optional Prometheus/OpenMetrics metrics.

## Health

```text
GET /healthz
GET /readyz
```

`/healthz` returns `ok` when the HTTP server is alive.

`/readyz` checks database and storage health and returns:

```text
database=true
storage=true
```

If either dependency is unavailable, it returns HTTP 503 with the failed dependency state.

## Metrics

```toml
[metrics]
enabled = true
access = "admin"
```

Metrics are served at:

```text
GET /metrics
```

The response content type is OpenMetrics text.

## Access Modes

`metrics.access` accepts:

- `public`: no authentication.
- `admin`: current web session must be an admin or owner.
- `token`: request must include `Authorization: Bearer <metrics bearer token>`.
- `loopback`: request client IP must be loopback.

Token mode requires:

```toml
[metrics]
access = "token"
bearer_token = "change-me"
```

Loopback mode can respect reverse proxy headers only when `server.behind_proxy = true`.

## Metric Names

Registered metrics include:

- `uploads`
- `pastes`
- `upload_bytes`
- `served_files`
- `reports`
- `scanner_outcomes`
- `rate_limit_rejections`
- `request_latency_seconds`

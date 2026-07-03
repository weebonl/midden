# API Overview

Midden exposes JSON and multipart endpoints under `/api/v1`.

The runtime OpenAPI document is available at:

```text
GET /api/openapi.json
```

The OpenAPI output is the machine-readable reference for the running version. This guide provides practical usage notes and examples.

## Enable API

```toml
[features]
api = true

[policy]
use_api = "anonymous"
```

Individual endpoints still check feature flags, policies, scopes, roles, and rate limits.

## Common Response Fields

File item responses can include:

- `id`
- `url`
- `raw_url`
- `internal_url`
- `thumbnail_url`
- `filename`
- `content_type`
- `size_bytes`
- `image_width`
- `image_height`
- `visibility`
- `metadata`
- `expires_at`
- `state`
- `created_at`

Paste item responses can include:

- `id`
- `url`
- `raw_url`
- `title`
- `syntax`
- `size_bytes`
- `visibility`
- `expires_at`
- `state`
- `created_at`

## Errors

API routes use standard HTTP status codes such as 400, 401, 403, 404, 413, 429, and 500. Feature-disabled API routes return 403 or 404 depending on the surface.

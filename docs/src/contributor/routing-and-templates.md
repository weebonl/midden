# Routing And Templates

## Router

Routes are registered in `src/web.rs`. Public app routes, API routes, account routes, admin routes, system routes, and file routes are merged into one Axum router.

Important route groups:

- `/`, `/url-upload`, `/browse`
- `/files/{id}/raw`, `/files/{id}/thumbnail`, `/internal/files/{id}/raw`, `/{slug}`
- `/p/new`, `/p/{id}`, `/p/{id}/raw`, `/p/{id}/edit`
- `/auth/*`, `/register`, `/account`
- `/admin/*`
- `/api/docs`, `/api/openapi.json`, `/api/v1/*`
- `/healthz`, `/readyz`, `/metrics`, `/robots.txt`

## Middleware

The router layers:

- API error middleware.
- File origin middleware.
- Request context middleware.
- Request metrics middleware.
- CSRF cookie middleware.
- HTTP tracing.

Request context stores templates, runtime settings, current user, CSRF token, and HTMX state for rendering and error handling.

## Templates

Templates live in `templates/` and are registered in `src/templates.rs` with `include_str!`. If you add a built-in template, also register it there.

Operators can override templates with:

```toml
[server]
template_dir = "/etc/midden/templates"
```

Disk templates take precedence over built-ins when present.

## Static Assets

Built-in static assets live in `static/` and are served from `/static/{path}`. Operators can override static files with:

```toml
[server]
static_dir = "/etc/midden/static"
```

Disk static files take precedence over built-ins.

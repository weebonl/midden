# Configuration

Midden loads configuration from TOML and environment variables.

## Source Order

1. If `--config PATH` is provided, that TOML file must exist.
2. If no explicit path is provided, `midden.toml` is optional.
3. Environment variables with the `MIDDEN__` prefix override file values.
4. Runtime admin settings stored in the database override the relevant application settings after startup.

Use this command to inspect the compiled defaults:

```console
midden config print-defaults
```

Use this command before deploys:

```console
midden --config midden.toml config check
```

## Primary Sections

- `[server]`: bind address, public base URL, optional template/static directories, reverse proxy mode.
- `[database]`: SQL connection URL and pool size.
- `[storage]`: local or S3-compatible blob storage.
- `[features]`: feature switches for files, pastes, accounts, API, reports, URL upload, previews, browse, auth modes, and paste editing.
- `[limits]`: upload sizes, paste sizes, expiry defaults, expiry guardrails, anonymous quota, and role quotas.
- `[branding]`: instance name, tagline, colors, custom CSS, footer links, notices, Open Graph behavior, and takedown text.
- `[policy]`: signup mode and action requirements.
- `[security]`: cookies, content disposition, MIME policy, URL upload restrictions, and rate limits.
- `[delivery]`: cache behavior, file origin, isolated file origin, and signed internal URLs.
- `[smtp]` and `[oidc]`: email and OIDC login.
- `[scanning]`: command, webhook, or ClamAV upload scanners.
- `[processing]`: metadata extraction, metadata stripping, and thumbnails.
- `[jobs]`, `[metrics]`, `[tokens]`, `[moderation]`, and `[uploads]`: operational controls.

## Runtime Settings

The admin settings UI persists selected sections to the `settings` table as JSON. When a request runs, Midden merges those persisted settings over `AppConfig` defaults. File-only sections such as database and storage still come from startup configuration.

This means changing a TOML default does not automatically override a value already saved in the admin UI. Use the admin UI as the current source for runtime-adjustable settings on a live instance.

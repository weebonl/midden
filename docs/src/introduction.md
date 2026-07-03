# Midden

Midden is a self-hostable file and paste sharing service written in Rust. It provides browser workflows for quick uploads and pastes, account-owned items, delete tokens for anonymous items, optional public browsing, reports, moderation tools, API tokens, and operational controls for storage, scanning, jobs, metrics, and delivery.

The project is still in development. Treat each deployment as self-managed software: read the example configuration, keep backups, run upgrades intentionally, and validate your instance before trusting it with important data.

## Main Components

- The `midden` binary serves the web app and exposes maintenance commands.
- Axum handles HTTP routing, middleware, web UI pages, and JSON API endpoints.
- SQLx supports SQLite and PostgreSQL.
- `object_store` stores blobs on local disk or S3-compatible storage.
- Runtime settings are loaded from TOML and environment variables, then can be overridden through persisted admin settings.
- Background jobs expire old items, clean auth state, retry scanner decisions, process metadata and thumbnails, and verify storage drift.

## Documentation Map

- Use the getting started guide to run a local or Compose-based instance.
- Use the operator guide to configure production-like deployments.
- Use the user guide for browser workflows.
- Use the API guide for token-authenticated clients.
- Use the contributor guide for codebase structure and validation expectations.

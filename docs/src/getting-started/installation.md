# Installation

Midden is distributed as a Rust binary and a Docker image build target in this repository.

## Build The Binary

```console
cargo build --release --locked --bin midden
```

The binary will be available at:

```text
target/release/midden
```

## Configure

Create a starting configuration:

```console
midden config print-defaults > midden.toml
```

Edit at least:

```toml
[server]
bind = "0.0.0.0:8080"
public_base_url = "https://files.example.test"

[security]
secure_cookies = true
```

For local-only development, the defaults are enough.

## Run The Container Directly

The image defaults to listening on `0.0.0.0:8080`. Its default SQLite database and local blob paths are under the `/var/lib/midden` working directory, so mount that directory for persistence:

```console
docker build -t midden:local .
docker run --rm -p 8080:8080 \
  -v midden-data:/var/lib/midden \
  -e MIDDEN__SERVER__PUBLIC_BASE_URL=http://localhost:8080 \
  midden:local
```

The image health check requests `/healthz`. Use the Compose models for the PostgreSQL and S3-compatible layout.

## Migrate

Run migrations before serving, or let `midden serve` apply them at startup:

```console
midden --config midden.toml migrate
```

## Serve

```console
midden --config midden.toml serve
```

## Useful Commands

```console
midden --config midden.toml config check
midden --config midden.toml owner create --email owner@example.test --username owner
midden --config midden.toml owner reset-password --email owner@example.test --password new-password
midden --config midden.toml user set-role --email user@example.test --role moderator
midden --config midden.toml storage verify
midden --config midden.toml jobs run-once
```

`config check` constructs the application state and validates that database, storage, templates, SMTP configuration, and other startup surfaces can be initialized.

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

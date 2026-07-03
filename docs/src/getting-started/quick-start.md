# Quick Start

This quick start runs Midden from source with the default SQLite database and local blob storage.

## Prerequisites

- Rust stable with Cargo.
- A checkout of the Midden repository.
- A shell with network access for the first dependency build.

## Run From Source

From the repository root:

```console
cargo run -- config print-defaults > midden.toml
cargo run -- migrate
cargo run -- owner create --email owner@example.test --username owner --password change-me
cargo run -- serve
```

Open `http://127.0.0.1:8080`.

The default configuration listens on `127.0.0.1:8080`, stores SQLite data in `midden.db`, and stores file blobs under `data/blobs`.

## Upload A File

Open the home page and choose a file, or use the API:

```console
curl -F file=@example.txt http://127.0.0.1:8080/api/v1/files
```

By default, uploads can be anonymous. Anonymous items receive a delete token in the result; keep that token if you want to delete or later claim the item.

## Create A Paste

Open `/p/new`, or use the API:

```console
curl -H 'content-type: application/json' \
  -d '{"content":"hello","expires":"7d","visibility":"unlisted"}' \
  http://127.0.0.1:8080/api/v1/pastes
```

## Check Health

```console
curl http://127.0.0.1:8080/healthz
curl http://127.0.0.1:8080/readyz
```

`/healthz` confirms the HTTP process is alive. `/readyz` checks database and storage reachability.

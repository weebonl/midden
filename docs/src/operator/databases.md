# Databases

Midden supports SQLite and PostgreSQL through SQLx.

## SQLite

Default:

```toml
[database]
url = "sqlite://midden.db?mode=rwc"
max_connections = 8
```

SQLite is good for small self-hosted deployments and development. Keep the database file on durable storage and back it up with the blob directory.

In containers, use an absolute path inside the mounted volume:

```text
MIDDEN__DATABASE__URL=sqlite:///var/lib/midden/midden.db?mode=rwc
```

## PostgreSQL

```toml
[database]
url = "postgres://midden:secret@postgres:5432/midden"
max_connections = 8
```

Use PostgreSQL for larger deployments, managed backup tooling, and external database operations.

## Migrations

Apply migrations explicitly:

```console
midden --config midden.toml migrate
```

`midden serve` also runs migrations at startup. Explicit migrations are useful in deployment pipelines because failures happen before the HTTP listener starts.

## Runtime Settings Table

The `settings` table stores JSON runtime settings saved through the admin UI. Back up this table with the rest of the database, because it can contain important live policy and feature decisions that are not present in `midden.toml`.

# Backup And Restore

Back up the database and blob storage together. The database contains metadata, settings, sessions, API tokens, reports, audit events, scanner results, and blob reference counts. Storage contains the actual file bytes and generated derivatives.

## SQLite With Local Storage

Stop the service or take a consistent filesystem snapshot, then copy:

```text
midden.db
data/blobs/
```

If running in Docker Compose with the SQLite file under `/var/lib/midden`, back up the `midden-data` volume.

## PostgreSQL With S3

Use PostgreSQL-native backup tooling for the database and bucket-native backup or replication for object storage.

Confirm both systems are from a compatible point in time. A database backup without matching blobs can leave missing storage objects. Blob backups without matching database rows can leave orphaned objects.

## Blob Export And Import

Midden can export blobs referenced by the current database:

```console
midden --config midden.toml storage export ./midden-blob-export
```

The export contains:

```text
manifest.json
blobs/<hash>
```

Import blobs into the configured storage backend:

```console
midden --config midden.toml storage import ./midden-blob-export
```

This imports blob bytes only. It does not restore database rows. Use it with database backup and restore procedures.

## Verify After Restore

```console
midden --config midden.toml migrate
midden --config midden.toml storage verify
midden --config midden.toml config check
```

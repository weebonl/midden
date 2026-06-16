# Backup And Restore

Back up the database and blob storage together. Database rows contain the public IDs, ownership, delete token hashes, moderation state, and blob hashes. Blob storage contains the file bytes addressed by SHA-256.

## SQLite And Local Storage

Stop writes or briefly stop the service, then copy the SQLite database and blob directory:

```sh
systemctl stop midden
sqlite3 /var/lib/midden/midden.db ".backup '/var/backups/midden/midden.db'"
rsync -a --delete /var/lib/midden/blobs/ /var/backups/midden/blobs/
systemctl start midden
```

Restore in the opposite order:

```sh
systemctl stop midden
install -d -o midden -g midden /var/lib/midden/blobs
cp /var/backups/midden/midden.db /var/lib/midden/midden.db
rsync -a --delete /var/backups/midden/blobs/ /var/lib/midden/blobs/
chown -R midden:midden /var/lib/midden
midden --config /etc/midden.toml migrate
systemctl start midden
```

## Postgres And S3-Compatible Storage

Use native database and object-store tooling:

```sh
pg_dump --format=custom --file=midden.dump "$DATABASE_URL"
aws s3 sync s3://midden ./midden-blobs
```

For MinIO, use `mc mirror`:

```sh
mc mirror --overwrite local/midden ./midden-blobs
```

Restore:

```sh
createdb midden
pg_restore --dbname "$DATABASE_URL" midden.dump
aws s3 sync ./midden-blobs s3://midden
midden --config /etc/midden.toml migrate
```

## Storage Backend Moves

Use the built-in export/import commands when moving the same database to a new storage backend:

```sh
midden --config /etc/midden.old.toml storage export ./midden-export
midden --config /etc/midden.new.toml storage import ./midden-export
midden --config /etc/midden.new.toml storage verify
```

The export directory contains `manifest.json` and `blobs/{sha256}` files. Move the database with the matching backup procedure before starting the service against the imported backend.

## Verification

After restore:

```sh
midden --config /etc/midden.toml config check
midden --config /etc/midden.toml storage verify
midden --config /etc/midden.toml storage gc --dry-run
midden --config /etc/midden.toml jobs run-once
curl -fsS http://127.0.0.1:8080/readyz
```

Sample a few known public URLs and paste raw URLs before reopening the service to users.

## Restore Drills

Run a restore drill before relying on a backup plan:

1. Restore the latest SQLite/local or Postgres/S3 backup into an isolated host or throwaway bucket.
2. Run migrations, `config check`, `storage verify`, `storage gc --dry-run`, and `jobs run-once`.
3. Start the service bound to localhost and check `/readyz` for both `database=true` and `storage=true`.
4. Sample files, pastes, account login, delete-token deletion, and report submission.
5. Record the backup timestamp, restore duration, commands used, and any missing or orphaned objects found by verification.

# Storage

Midden stores metadata in the database and blob bytes in the configured object storage backend. File blobs are addressed by SHA-256 hash and stored using a two-level prefix layout.

## Local Storage

```toml
[storage]
backend = "local"

[storage.local]
path = "data/blobs"
```

The local backend creates the directory if it does not exist. Back up this directory together with the database.

## S3-Compatible Storage

```toml
[storage]
backend = "s3"

[storage.s3]
bucket = "midden"
region = "us-east-1"
endpoint = "https://s3.example.test"
access_key_id = "midden"
secret_access_key = "secret"
prefix = "production"
allow_http = false
virtual_hosted_style = false
```

`bucket` is required when `storage.backend = "s3"`. `endpoint`, credentials, and style flags are optional because the underlying AWS client can also use environment or platform credentials.

Set `allow_http = true` only for local S3-compatible systems such as MinIO on a private network.

## Verification

```console
midden --config midden.toml storage verify
```

This compares blob hashes referenced by the database with hashes listed by the storage backend and reports missing or orphaned objects.

## Garbage Collection

```console
midden --config midden.toml storage gc --dry-run
midden --config midden.toml storage gc
```

Garbage collection expires due files and pastes, decrements blob references, and deletes unreferenced blob objects.

Run non-dry-run garbage collection only while every Midden server and job process that uses the same database and storage backend is stopped. The CLI cannot share the in-process upload lock, so running it alongside uploads could delete a content-addressed object as that hash is being reused. The dry run and `storage verify` remain safe while the service is online.

Background jobs perform similar cleanup automatically when enabled.

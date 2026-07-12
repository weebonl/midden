# Docker Compose

The repository includes one common Compose model and two explicit storage/database overrides. Always combine the base file with exactly one override.

## SQLite And Local Storage

```console
docker compose -f docker-compose.yml -f docker-compose.sqlite.yml up --build
```

This starts only the Midden service. Data is persisted in the `midden-data` volume at `/var/lib/midden`.

Important environment variables from this file:

```text
MIDDEN__SERVER__BIND=0.0.0.0:8080
MIDDEN__SERVER__PUBLIC_BASE_URL=http://localhost:8080
MIDDEN__DATABASE__URL=sqlite:///var/lib/midden/midden.db?mode=rwc
MIDDEN__STORAGE__BACKEND=local
MIDDEN__STORAGE__LOCAL__PATH=/var/lib/midden/blobs
```

## PostgreSQL And MinIO

```console
docker compose -f docker-compose.yml -f docker-compose.postgres-minio.yml up --build
```

This starts Midden, PostgreSQL 17, MinIO, and a one-shot MinIO bucket initializer. It is useful for testing the PostgreSQL and S3-compatible paths without external services.

## Validate The Models

```console
docker compose -f docker-compose.yml -f docker-compose.sqlite.yml config --quiet
docker compose -f docker-compose.yml -f docker-compose.postgres-minio.yml config --quiet
```

The base file contains only settings shared by both deployments. The override selects the database, storage backend, dependencies, and durable volumes, so enabling an unrelated profile cannot leave Midden connected to SQLite while idle PostgreSQL or MinIO containers run beside it.

## Production Notes

- Set `MIDDEN_PUBLIC_BASE_URL` to the public HTTPS origin.
- Use durable volumes or managed services for the database and blob storage.
- Set `MIDDEN__SECURITY__SECURE_COOKIES=true` behind HTTPS.
- Set `MIDDEN__SERVER__BEHIND_PROXY=true` when a trusted reverse proxy supplies client IP headers.
- Do not use the example MinIO credentials outside local testing.

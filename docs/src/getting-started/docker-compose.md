# Docker Compose

The repository includes three Compose files for common local and self-hosted layouts.

## SQLite And Local Storage

```console
docker compose -f docker-compose.sqlite.yml up --build
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
docker compose -f docker-compose.postgres-minio.yml up --build
```

This starts Midden, PostgreSQL 17, MinIO, and a one-shot MinIO bucket initializer. It is useful for testing the PostgreSQL and S3-compatible paths without external services.

## Multi-Profile Compose File

```console
docker compose up --build
docker compose --profile postgres --profile s3 up --build
```

`docker-compose.yml` contains the base Midden service plus optional PostgreSQL and MinIO profiles.

## Production Notes

- Set `MIDDEN_PUBLIC_BASE_URL` to the public HTTPS origin.
- Use durable volumes or managed services for the database and blob storage.
- Set `MIDDEN__SECURITY__SECURE_COOKIES=true` behind HTTPS.
- Set `MIDDEN__SERVER__BEHIND_PROXY=true` when a trusted reverse proxy supplies client IP headers.
- Do not use the example MinIO credentials outside local testing.

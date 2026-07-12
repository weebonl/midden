# Tests And Release

## Common Checks

Run formatting:

```console
cargo fmt --all -- --check
```

Run clippy:

```console
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
```

Run tests:

```console
cargo test --locked --workspace --all-features
```

Run docs checks:

```console
mdbook build docs
mdbook test docs
```

Validate both Compose models:

```console
docker compose -f docker-compose.yml -f docker-compose.sqlite.yml config --quiet
docker compose -f docker-compose.yml -f docker-compose.postgres-minio.yml config --quiet
```

PostgreSQL and S3 integration tests are ignored by the default suite because they require external services. Supply every listed environment variable and invoke each test explicitly; the tests fail instead of silently skipping when their environment is incomplete:

```console
MIDDEN_TEST_POSTGRES_URL=postgres://midden:midden@localhost:5432/midden \
  cargo test --locked db::tests::postgres_migration_smoke_when_configured -- \
  --ignored --exact --nocapture

MIDDEN_TEST_S3_BUCKET=midden \
MIDDEN_TEST_S3_REGION=us-east-1 \
MIDDEN_TEST_S3_ENDPOINT=http://127.0.0.1:9000 \
MIDDEN_TEST_S3_ACCESS_KEY_ID=midden \
MIDDEN_TEST_S3_SECRET_ACCESS_KEY=midden-secret \
  cargo test --locked storage::tests::s3_storage_round_trip_when_configured -- \
  --ignored --exact --nocapture
```

Check whitespace before committing:

```console
git diff --check
```

## Focused Test Guidance

- Config changes: add unit tests in `src/config.rs` and route/admin tests if runtime settings are involved.
- Route behavior: add integration-style tests in `src/web/tests.rs`.
- Upload processing: add focused tests in `src/web/upload.rs` or `src/processing.rs`, then route tests when user-visible behavior changes.
- Storage: add local backend tests and gated S3 smoke tests when touching S3-specific behavior.
- Auth/config regressions: test rendered links and the admin save path.

## Release Basics

For a crate release:

1. Update `Cargo.toml`.
2. Update `Cargo.lock` through a build or metadata command.
3. Build with `cargo build`.
4. Commit the version bump.
5. Create an annotated tag such as `git tag -a v0.6.5 -m "Release v0.6.5"`.
6. Push the branch and tags.
7. Confirm `git status --short` is clean.

Docker image publishing is handled by the GitHub workflow on `main` pushes and version tags.

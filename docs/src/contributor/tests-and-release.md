# Tests And Release

## Common Checks

Run formatting:

```console
cargo fmt --all -- --check
```

Run clippy:

```console
cargo clippy --workspace --all-targets --all-features -- -D warnings
```

Run tests:

```console
cargo test --workspace --all-features
```

Run docs checks:

```console
mdbook build docs
mdbook test docs
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

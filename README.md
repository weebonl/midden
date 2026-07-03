# Midden

Midden is a self-hostable file and paste sharing service written in Rust.

Project is still in development; treat deployments as self-managed software and keep backups.

## Documentation

The project documentation is built with mdBook:

- Published docs: <https://projects.weeb.onl/midden/>
- Local source: [`docs/src`](docs/src)

Build or serve the book locally:

```sh
mdbook build docs
mdbook serve docs -n 127.0.0.1 -p 3000
```

## Quick Start

```sh
cargo run -- config print-defaults > midden.toml
cargo run -- migrate
cargo run -- owner create --email owner@example.test --username owner --password change-me
cargo run -- serve
```

Open <http://127.0.0.1:8080>.

See the [quick start](docs/src/getting-started/quick-start.md), [configuration guide](docs/src/operator/configuration.md), and [API guide](docs/src/api/overview.md) for details.

## License

AGPL-3.0-only.

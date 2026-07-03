# Runtime Settings

Startup configuration is represented by `AppConfig` in `src/config.rs`. Runtime request settings are represented by `RuntimeSettings`.

## Loading

`AppConfig::load` reads:

1. Explicit config file or optional `midden.toml`.
2. `MIDDEN__` environment variables.
3. Validation rules.

`AppState::settings()` asks the database for runtime settings and merges persisted JSON settings over the startup config.

## Persisted Sections

Admin settings can persist runtime sections such as:

- Features.
- Limits.
- Branding.
- Policy.
- Security.
- Delivery.
- Scanning.
- Processing.
- Discovery.
- Jobs.
- Uploads.
- Metrics.
- Tokens.
- Moderation.

Database and storage connection settings remain startup configuration because changing them at runtime would require rebuilding application state.

## Adding A Runtime Setting

When adding a runtime-adjustable option:

1. Add it to the appropriate config struct with serde defaults.
2. Include it in `RuntimeSettings`.
3. Merge it in `RuntimeSettings::from_config`.
4. Update admin form parsing and template fields if it should be user-editable.
5. Persist it with `set_json_setting`.
6. Add route-level or integration tests for the rendered admin form and save path when behavior can drift.

## Validation

Use `AppConfig::validate` for startup invariants that must fail before serving. Use admin save-path validation for runtime settings that could lock out users or create invalid live state.

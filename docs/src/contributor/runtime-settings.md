# Runtime Settings

Startup configuration is represented by `AppConfig` in `src/config.rs`. Runtime request settings are represented by `RuntimeSettings`.

## Loading

`AppConfig::load` reads:

1. Compiled defaults.
2. Explicit config file or optional `midden.toml`.
3. `MIDDEN__` environment variables, while recording the exact runtime field paths they control.
4. Validation rules.

`AppState::settings()` asks the database for runtime settings, merges persisted JSON settings over the pre-environment startup baseline, and then reapplies the recorded environment-controlled fields. The effective runtime precedence is defaults/file < persisted database settings < explicit `MIDDEN__` fields. Admin saves restore environment-owned fields from the raw persisted settings before writing, so a temporary environment override never becomes permanent as a side effect of another edit.

Environment locking is field-level rather than section-level. If `MIDDEN__SECURITY__SECURE_COOKIES` is present, only `security.secure_cookies` comes from the environment; sibling fields in the persisted `security` object remain active. If the environment variable is removed, the value already persisted for that field becomes active.

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
3. Merge it in `RuntimeSettings::from_config`. If it introduces a new top-level runtime group, add that group once to the `runtime_setting_groups!` descriptor; loading, atomic persistence, and environment-path ownership all use that descriptor.
4. Update admin form parsing and template fields if it should be user-editable.
5. Persist production admin changes through `replace_runtime_settings`; `set_json_setting` is a test-only fixture helper.
6. Validate both startup and live values through the shared `validate_runtime_settings` rules.
7. Add route-level or integration tests for the rendered admin form, atomic save path, and environment-precedence behavior when it can drift.

## Validation

Use `AppConfig::validate` for startup invariants that must fail before serving. Put runtime invariants in `validate_runtime_settings` so startup, database loading, and the admin save path enforce the same rules.

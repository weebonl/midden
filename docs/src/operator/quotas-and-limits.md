# Quotas And Limits

Midden applies upload size limits, paste size limits, expiry guardrails, and optional file storage quotas.

## Size Limits

```toml
[limits]
max_upload_bytes = 2147483648
max_paste_bytes = 1048576
```

`max_upload_bytes` applies to multipart and URL uploads. `max_paste_bytes` applies to paste creation and paste edits.

## Expiry Defaults

```toml
[limits]
default_file_expiry = "30d"
default_paste_expiry = "7d"
```

If defaults are omitted, users may create items without an expiry when allowed by guardrails.

Expiry values use compact durations such as `1h`, `12h`, `1d`, `7d`, and `30d`.

## Expiry Guardrails

```toml
[limits.expiry]
allow_never = true
anonymous_max_file_expiry = "7d"
user_max_file_expiry = "90d"
anonymous_max_paste_expiry = "7d"
user_max_paste_expiry = "90d"
allowed_presets = ["1h", "12h", "1d", "7d", "30d"]
```

Guardrails control which expiry choices are accepted and which presets are shown in forms.

## Anonymous Quotas

```toml
[limits.anonymous_quota]
storage_bytes = 10737418240
daily_upload_bytes = 1073741824
monthly_upload_bytes = 10737418240
item_count = 1000
```

Anonymous quota applies to anonymous file uploads. Paste content does not consume file storage quota.

## Role Quotas

```toml
[limits.role_quotas.user]
storage_bytes = 5368709120
daily_upload_bytes = 1073741824
monthly_upload_bytes = 10737418240
item_count = 500
```

Role quotas apply to account-owned file uploads. Configure only the limits you need; unset values are unlimited.

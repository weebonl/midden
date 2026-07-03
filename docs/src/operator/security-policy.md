# Security Policy

Midden exposes security controls through feature flags, action policies, content policy, URL upload restrictions, rate limits, delivery settings, and moderation states.

## Action Rules

Action rules accept:

```text
disabled
anonymous
authenticated
moderator
admin
owner
```

Example:

```toml
[policy]
upload_file = "anonymous"
create_paste = "anonymous"
use_api = "anonymous"
view_item = "anonymous"
delete_own_item = "authenticated"
delete_policy = "delete_tokens"
claim_anonymous_item = "authenticated"
create_account = "disabled"
```

## Delete Policy

`delete_policy` accepts:

- `disabled`: anonymous delete tokens cannot delete.
- `delete_tokens`: anonymous delete tokens can delete.
- `no_anonymous_delete`: only authorized account users can delete.
- `claim_later`: anonymous items can be claimed by an account with the token.

## Content Policy

```toml
[security.content_policy]
allowed_mime_types = []
forced_attachment_mime_types = ["image/svg+xml", "text/html", "application/javascript", "text/javascript"]
risky_mime_mode = "attachment"
max_filename_bytes = 180
```

If `allowed_mime_types` is empty, all MIME types are accepted unless blocked by scanner settings. Forced attachment types are served as downloads to reduce browser execution risk.

`risky_mime_mode` accepts:

- `attachment`: serve risky types as attachments.
- `inline_on_isolated_origin`: allow inline only on the isolated file origin.
- `plaintext`: serve risky types as text/plain.

## MIME Mismatch Rejection

```toml
[security]
reject_mime_mismatch = true
```

When enabled, Midden rejects uploads where sniffed content conflicts with the declared or filename-derived MIME type.

## URL Upload Restrictions

URL upload blocks private and local IPs by default and supports allow/block lists for ports and hosts. Keep `block_private_ips = true` unless the instance is strictly internal and you understand the SSRF risk.

## Rate Limits

Rate limits are disabled unless a named action is configured.

```toml
[security.rate_limits.login]
enabled = true
requests = 10
window_seconds = 300
```

Common action names include `upload_file`, `upload_by_url`, `create_paste`, `login`, `password_reset`, `report`, `api_upload_file`, `api_create_paste`, `api_delete_file`, `api_delete_paste`, `api_create_token`, `api_create_report`, `api_list_files`, and `api_list_pastes`.

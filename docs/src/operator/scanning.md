# Scanning

Midden can scan uploads before they are published. Scanners return one of three decisions:

- `allow`: publish normally.
- `quarantine`: store the file and scanner result, but do not serve it publicly.
- `reject`: reject the upload.

If multiple adapters run, Midden uses the most restrictive decision.

## Enable Scanning

```toml
[scanning]
enabled = true
blocked_hashes = []
blocked_mime_types = []
default_on_error = "allow"
```

`default_on_error` controls what happens when an adapter fails. For high-trust public upload systems, consider `quarantine` instead of `allow`.

## Block Lists

```toml
[scanning]
blocked_hashes = ["012345..."]
blocked_mime_types = ["application/x-msdownload"]
```

Blocked hashes and MIME types are checked before external adapters.

## Command Adapter

```toml
[[scanning.adapters]]
kind = "command"
program = "/usr/local/bin/midden-scan"
args = ["{path}", "{filename}", "{content_type}", "{sha256}"]
```

Command exit codes:

- `0`: allow.
- `10`: quarantine.
- `20`: reject.
- Any other status or execution error: use `default_on_error`.

Midden expands `{path}`, `{filename}`, `{content_type}`, `{sha256}`, `{public_id}`, and `{size_bytes}` in command arguments.

## Webhook Adapter

```toml
[[scanning.adapters]]
kind = "webhook"
url = "https://scanner.example.test/midden"
secret = "change-me"
```

Midden posts JSON metadata to the webhook. When `secret` is set, it sends it in the `x-midden-scanner-secret` header.

Expected response:

```json
{
  "decision": "allow",
  "detail": "clean"
}
```

## ClamAV Adapter

```toml
[[scanning.adapters]]
kind = "clam_av"
socket = "127.0.0.1:3310"
```

The socket can be TCP (`host:port`) or a Unix socket path on Unix platforms.

## Background Retries

Background jobs retry scanner decisions for candidate files when scanning is enabled. Configure retry batch size with:

```toml
[jobs]
scanner_retry_limit = 10
```

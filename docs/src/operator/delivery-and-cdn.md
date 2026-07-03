# Delivery And CDN

Delivery settings control public URLs, cache headers, isolated file serving, and signed internal raw URLs.

## Base URLs

```toml
[server]
public_base_url = "https://files.example.test"

[delivery]
public_file_base_url = "https://cdn-files.example.test"
```

`server.public_base_url` is the application origin. `delivery.public_file_base_url`, when set, is used for file URLs and can point at a separate file domain or CDN.

## Cache Settings

```toml
[delivery]
public_cache_seconds = 3600
static_cache_seconds = 31536000
```

Static assets can be cached longer than user files. Tune public file cache TTLs based on your moderation and takedown expectations.

## Isolated File Origin

```toml
[delivery]
isolated_file_origin = true
public_file_base_url = "https://files-cdn.example.test"
```

When isolated file origin is enabled, public file routes are only available through the configured file host. This reduces the risk of user-controlled file content sharing the main application origin.

## Signed Internal URLs

```toml
[delivery]
signed_internal_urls = true
internal_url_secret = "long-random-secret"
internal_url_ttl_seconds = 300
```

Signed internal raw URLs are included in API file responses when enabled. Use them for trusted reverse proxy or CDN fetches that need short-lived origin access.

Midden validates startup config so signed internal URLs require `internal_url_secret`, and isolated file origin requires `public_file_base_url`.

## Reverse Proxies

```toml
[server]
behind_proxy = true
```

When enabled, access checks that need the client IP can use `x-real-ip` or the first `x-forwarded-for` value. Only enable this behind a trusted proxy that strips untrusted incoming forwarding headers.

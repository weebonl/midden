# Authentication And Scopes

API tokens are bearer tokens:

```console
curl -H 'authorization: Bearer mdd_TOKEN' http://127.0.0.1:8080/api/v1/me/files
```

Some API routes can be anonymous when policy allows it. Account-specific routes and token management routes require a token.

## Scopes

Common scopes:

```text
files:read
files:write
files:delete
pastes:read
pastes:write
pastes:delete
reports:write
items:claim
tokens:read
tokens:write
admin:reports
admin:items
admin:search
*
```

`*` grants all scopes to the token holder. Token creation through the API can only request scopes already held by the caller unless the caller has `*`.

## Create A Token From The Account UI

Open `/account`, choose API Tokens, enter a name, scopes, and optional TTL seconds.

## Token Expiry

Operators can set default and maximum TTLs:

```toml
[tokens]
default_ttl_seconds = 2592000
max_ttl_seconds = 31536000
```

API token creation rejects non-positive TTLs and TTLs above the configured maximum.

# Tokens API

## List Tokens

```console
curl -H 'authorization: Bearer mdd_TOKEN' \
  http://127.0.0.1:8080/api/v1/tokens
```

Required scope:

```text
tokens:read
```

Response:

```json
{
  "items": []
}
```

## Create Token

```console
curl -H 'authorization: Bearer mdd_TOKEN' \
  -H 'content-type: application/json' \
  -d '{"name":"automation","scopes":["files:read","files:write"],"expires_in_seconds":2592000}' \
  http://127.0.0.1:8080/api/v1/tokens
```

Required scope:

```text
tokens:write
```

Response:

```json
{
  "token": "mdd_NEW_TOKEN",
  "expires_at": 1754093490
}
```

The token is only returned once. Store it before leaving the response.

The requested scopes must be a subset of the caller token scopes unless the caller token has `*`.

## Revoke Token

```console
curl -X DELETE \
  -H 'authorization: Bearer mdd_TOKEN' \
  http://127.0.0.1:8080/api/v1/tokens/TOKEN_ID
```

Required scope:

```text
tokens:write
```

Response:

```json
{
  "revoked": true
}
```

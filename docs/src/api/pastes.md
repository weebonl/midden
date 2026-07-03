# Pastes API

## Create A Paste

```console
curl -H 'content-type: application/json' \
  -d '{"title":"Example","syntax":"rust","content":"fn main() {}","expires":"7d","visibility":"unlisted"}' \
  http://127.0.0.1:8080/api/v1/pastes
```

Authenticated create:

```console
curl -H 'authorization: Bearer mdd_TOKEN' \
  -H 'content-type: application/json' \
  -d '{"content":"hello"}' \
  http://127.0.0.1:8080/api/v1/pastes
```

Required scope for authenticated callers:

```text
pastes:write
```

Response:

```json
{
  "id": "abc123",
  "url": "http://127.0.0.1:8080/p/abc123",
  "raw_url": "http://127.0.0.1:8080/p/abc123/raw",
  "delete_token": "token"
}
```

## List My Pastes

```console
curl -H 'authorization: Bearer mdd_TOKEN' \
  'http://127.0.0.1:8080/api/v1/me/pastes?q=rust'
```

Required scope:

```text
pastes:read
```

`features.paste_content_search` controls whether owned paste search includes paste body content.

## Delete A Paste

```console
curl -X DELETE \
  -H 'authorization: Bearer mdd_TOKEN' \
  http://127.0.0.1:8080/api/v1/pastes/abc123
```

Required scope:

```text
pastes:delete
```

Anonymous delete token:

```console
curl -X DELETE \
  -H 'x-delete-token: DELETE_TOKEN' \
  http://127.0.0.1:8080/api/v1/pastes/abc123
```

Response:

```json
{
  "deleted": true
}
```

## Claim A Paste

```console
curl -H 'authorization: Bearer mdd_TOKEN' \
  -H 'content-type: application/json' \
  -d '{"delete_token":"DELETE_TOKEN"}' \
  http://127.0.0.1:8080/api/v1/claim/paste/abc123
```

Required scope:

```text
items:claim
```

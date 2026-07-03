# Files API

## Upload A File

```console
curl -F file=@example.txt \
  -F expires=7d \
  -F visibility=unlisted \
  http://127.0.0.1:8080/api/v1/files
```

Authenticated upload:

```console
curl -H 'authorization: Bearer mdd_TOKEN' \
  -F file=@example.txt \
  http://127.0.0.1:8080/api/v1/files
```

Required scope for authenticated callers:

```text
files:write
```

Response:

```json
{
  "id": "abc123",
  "url": "http://127.0.0.1:8080/abc123.txt",
  "raw_url": "http://127.0.0.1:8080/files/abc123/raw",
  "internal_url": null,
  "delete_token": "token"
}
```

## List My Files

```console
curl -H 'authorization: Bearer mdd_TOKEN' \
  'http://127.0.0.1:8080/api/v1/me/files?q=example'
```

Required scope:

```text
files:read
```

Response:

```json
{
  "items": []
}
```

## Delete A File

```console
curl -X DELETE \
  -H 'authorization: Bearer mdd_TOKEN' \
  http://127.0.0.1:8080/api/v1/files/abc123
```

Required scope:

```text
files:delete
```

Anonymous delete token:

```console
curl -X DELETE \
  -H 'x-delete-token: DELETE_TOKEN' \
  http://127.0.0.1:8080/api/v1/files/abc123
```

Response:

```json
{
  "deleted": true
}
```

## Claim A File

```console
curl -H 'authorization: Bearer mdd_TOKEN' \
  -H 'content-type: application/json' \
  -d '{"delete_token":"DELETE_TOKEN"}' \
  http://127.0.0.1:8080/api/v1/claim/file/abc123
```

Required scope:

```text
items:claim
```

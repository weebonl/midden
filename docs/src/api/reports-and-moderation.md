# Reports And Moderation API

## Create A Report

```console
curl -H 'content-type: application/json' \
  -d '{"kind":"file","id":"abc123","reason":"abuse","details":"details"}' \
  http://127.0.0.1:8080/api/v1/reports
```

Authenticated report:

```console
curl -H 'authorization: Bearer mdd_TOKEN' \
  -H 'content-type: application/json' \
  -d '{"kind":"paste","id":"abc123","reason":"spam"}' \
  http://127.0.0.1:8080/api/v1/reports
```

Required scope for authenticated callers:

```text
reports:write
```

Response:

```json
{
  "reported": true
}
```

## List Reports

```console
curl -H 'authorization: Bearer mdd_TOKEN' \
  'http://127.0.0.1:8080/api/v1/admin/reports?state=open&kind=file&days=7'
```

Required scope and role:

```text
admin:reports
moderator or higher
```

## Update A Report

```console
curl -X PATCH \
  -H 'authorization: Bearer mdd_TOKEN' \
  -H 'content-type: application/json' \
  -d '{"action":"resolve","note":"handled"}' \
  http://127.0.0.1:8080/api/v1/admin/reports/REPORT_ID
```

The JSON body matches the admin report action form fields: `action` and optional `note`.

## Update An Item

```console
curl -X PATCH \
  -H 'authorization: Bearer mdd_TOKEN' \
  -H 'content-type: application/json' \
  -d '{"state":"takedown","visibility":"unlisted","note":"confirmed","block_hash":true}' \
  http://127.0.0.1:8080/api/v1/admin/items/file/abc123
```

Required scope and role:

```text
admin:items
moderator or higher
```

Valid item states:

```text
active
quarantined
takedown
legal_hold
deleted
```

`block_hash` only works for files.

## Admin Search

```console
curl -H 'authorization: Bearer mdd_TOKEN' \
  'http://127.0.0.1:8080/api/v1/admin/search?q=example&paste_content=true'
```

Required scope and role:

```text
admin:search
moderator or higher
```

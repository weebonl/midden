# Pastes

Pastes are text items with optional titles, syntax hints, expiry, visibility, ownership, and revisions.

## Create A Paste

Open:

```text
/p/new
```

The paste form accepts:

- `title`: optional display title.
- `syntax`: optional syntax hint.
- `content`: required paste body.
- `expires`: optional expiry duration.
- `visibility`: `unlisted`, `private`, or `public` when public browse is enabled.

## View A Paste

Paste pages are served at:

```text
/p/{id}
/p/{id}/raw
```

The normal page renders syntax-highlighted content when a syntax hint is available. The raw route serves plain paste content.

## Edit A Paste

Paste editing requires `features.paste_editing = true`. Owners can edit their own pastes. Admins can edit pastes as part of moderation.

Each edit creates a revision record.

## Limits

Paste creation and edit size are limited by:

```toml
[limits]
max_paste_bytes = 1048576
```

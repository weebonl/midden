# Files

The home page is the primary file upload surface.

## Upload

The upload form accepts:

- `file`: the file bytes.
- `expires`: optional expiry duration.
- `visibility`: `unlisted`, `private`, or `public` when public browse is enabled.

Anonymous uploads are allowed by default. If policy requires authentication, sign in first.

## Results

A successful upload returns:

- A page URL.
- A raw file URL.
- A delete token for anonymous uploads when the delete policy supports it.

Keep delete tokens private. They can delete or claim anonymous items depending on policy.

## Visibility

- `unlisted`: reachable by direct link.
- `private`: visible only to the owning account and moderators.
- `public`: visible in `/browse` when public browse is enabled.

## Preview Pages

When `features.preview_pages = true`, file links open a preview page first. Otherwise, file links serve the raw file directly.

## URL Upload

When `features.upload_by_url = true`, `/url-upload` lets users fetch a remote `http` or `https` URL into Midden. Operators can restrict hosts, ports, redirects, response size, and private IP access.

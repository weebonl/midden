# Template Overrides

Midden embeds its default Minijinja templates. Set `server.template_dir` to a
directory containing files with the same names to override individual templates
while falling back to embedded defaults for everything else.

## Shared Context

Every template receives:

- `settings`: runtime settings, including `features`, `limits`, `branding`,
  `policy`, `security`, `delivery`, `scanning`, `processing`, `discovery`, and
  `jobs`.
- `current_user`: the signed-in user or `null`.
- `page`: route-specific data for the current view.
- `human_bytes(value)`: helper for byte counts.

## Built-In Filenames

- `base.html`: page shell, nav, footer, CSS/JS includes, and the `head_extra`
  block used by preview/detail pages.
- `index.html`: normal upload form and homepage blocks.
- `browse.html`: optional public browse/search page.
- `resumable_upload.html`: tus-backed resumable upload form.
- `upload_result.html`: file upload result.
- `url_upload.html`: URL upload form.
- `paste_new.html`: paste creation form.
- `paste_show.html`: paste display.
- `paste_edit.html`: paste edit form.
- `paste_result.html`: paste creation result.
- `file_preview.html`: optional file preview page.
- `takedown.html`: unavailable item page.
- `login.html`: local/OIDC login form.
- `password_reset_request.html`: password reset request form.
- `password_reset_form.html`: password reset submit form.
- `email_verified.html`: email verification result.
- `two_factor.html`: email-code two-factor form.
- `register.html`: account registration form.
- `account.html`: account dashboard, tokens, 2FA, linked OIDC.
- `admin.html`: runtime settings form.
- `admin_search.html`: moderator/admin item search.
- `admin_users.html`: user and invite management.
- `reports.html`: report queue.
- `admin_item.html`: item moderation page.
- `report_form.html`: public report form.
- `delete_form.html`: delete-token form.
- `delete_result.html`: delete result.
- `claim_form.html`: claim anonymous item form.
- `error.html`: reserved error template.
- `docs.html`: API quick reference.

Route-specific `page` shapes are intentionally small and mirror the field names
rendered by the built-in templates. When overriding a template, keep form field
names and action URLs unchanged unless you also change the corresponding Rust
handler.

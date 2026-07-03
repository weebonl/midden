# SMTP And Email

SMTP enables password resets, email verification, report notifications, and two-factor challenge delivery.

## Configuration

```toml
[smtp]
enabled = true
host = "smtp.example.test"
port = 587
username = "midden"
password = "secret"
from = "Midden <midden@example.test>"
```

Midden considers mail enabled only when `enabled = true`, `host` is set, and `from` is set. Username and password are optional for SMTP servers that do not require authentication.

## Uses

- Password reset request emails.
- Email verification links.
- Two-factor challenge codes.
- Abuse or report notifications when `branding.abuse_email` is configured.

## Operator Notes

- Use a real sender address that your SMTP service allows.
- Prefer secret injection for `smtp.password`.
- Test password reset and two-factor flows after changing SMTP settings.
- If SMTP is not enabled, account flows that depend on mail cannot complete.

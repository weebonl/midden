# Moderation

Moderation features include reports, moderation roles, item states, notes, admin search, blocked hashes, and optional report webhooks.

## Feature Flag

```toml
[features]
reports = true
```

When reports are disabled, report forms and report APIs are unavailable.

## Roles

- `moderator`: can use moderation search, reports, and moderation item actions.
- `admin`: includes moderator access and user/settings management.
- `owner`: includes admin access and owner-only account mutation safeguards.

## Item States

Moderation can set files and pastes to:

```text
active
quarantined
takedown
legal_hold
deleted
```

Non-active items render the configured takedown page text instead of serving normal content.

## Reports

Users and anonymous visitors can submit reports when reports are enabled. Reports capture item kind, public ID, reason, details, optional reporter user, and state.

Admin and moderator surfaces support filtering by report state, kind, reason, and age.

## Moderation Webhook

```toml
[moderation]
notify_webhook_url = "https://moderation.example.test/midden"
notify_webhook_secret = "change-me"
```

When configured, Midden sends report notifications to the webhook. Keep the secret out of committed configuration.

## Abuse Email

```toml
[branding]
abuse_email = "abuse@example.test"
```

When SMTP is enabled, reports also send an email notification to this address.

## Block Hash From Item

Admin item actions can add a file blob hash to `scanning.blocked_hashes`. This only applies to files, not pastes.

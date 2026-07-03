# Authentication

Midden supports local accounts, OIDC login, invite-based or open signup, API tokens, and two-factor email challenges.

## Feature Switches

```toml
[features]
accounts = true
local_login = true
oidc_login = false

[policy]
signup = "disabled"
create_account = "disabled"
```

`accounts` controls account surfaces. `local_login` controls password login and registration affordances. `oidc_login` controls OIDC login routes, but OIDC must also be configured.

## Signup Modes

`policy.signup` accepts:

- `disabled`: no public signup.
- `open`: public registration is available.
- `invite_only`: users need invite tokens created by admins.
- `admin_created`: admins create users manually.

## Local Login

Local login uses password hashes stored on users. Owner password recovery is available from the CLI:

```console
midden --config midden.toml owner reset-password --email owner@example.test --password new-password
```

## OIDC Login

```toml
[features]
accounts = true
oidc_login = true

[oidc]
enabled = true
issuer_url = "https://accounts.example.test"
client_id = "midden"
client_secret = "secret"
redirect_url = "https://files.example.test/auth/oidc/callback"
allowed_domains = ["example.test"]
allowed_groups = ["midden-users"]
role_claim = "role"
groups_claim = "groups"

[oidc.role_mappings]
midden-moderators = "moderator"
midden-admins = "admin"
```

OIDC is considered usable only when accounts, the OIDC feature flag, provider config, client credentials, and redirect URL are present. The admin save path rejects settings that would disable local login without a usable OIDC login path.

## Two-Factor Challenges

Users can enable two-factor authentication from the account page. Midden sends a challenge code by email, so SMTP must be configured for the challenge flow to be usable.

## Roles

Roles are ordered:

```text
user < moderator < admin < owner
```

Use the CLI to assign roles:

```console
midden --config midden.toml user set-role --email user@example.test --role moderator
```

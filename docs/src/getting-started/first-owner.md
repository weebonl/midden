# First Owner Setup

Owner accounts can access the admin area and manage users, settings, moderation, and jobs. Create at least one owner before exposing an instance that requires account administration.

## Create An Owner

```console
midden --config midden.toml owner create \
  --email owner@example.test \
  --username owner \
  --password 'change-me'
```

If `--password` is omitted, the owner is created without a local password. That can be useful for OIDC-only deployments, but only if OIDC is already configured and usable.

## Reset An Owner Password

```console
midden --config midden.toml owner reset-password \
  --email owner@example.test \
  --password 'new-password'
```

## Promote An Existing User

```console
midden --config midden.toml user set-role \
  --email user@example.test \
  --role owner
```

Valid roles are `user`, `moderator`, `admin`, and `owner`.

## Avoid Lockouts

Midden has a server-side admin settings guard that rejects configurations where local login is disabled and OIDC is not actually enabled. Still, operators should stage authentication changes carefully:

- Keep one tested owner session open while changing auth settings.
- Confirm the configured OIDC issuer, client ID, client secret, redirect URL, and feature flags.
- Keep a recovery path using the CLI and direct configuration access.

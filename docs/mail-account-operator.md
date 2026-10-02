# Per-account Exchange mail access

Personal mail accounts retain their own provider endpoints and credentials. The
instance `CTO_EMAIL_*` configuration is not a fallback credential source for a
personal account. An instance sync failure does not skip the personal accounts.

The trusted-local operator CLI uses the same registry and encrypted secret store
as the authenticated Mail account configuration endpoint:

- `ctox channel email-account list` returns public configuration and `has_password`.
- `ctox channel email-account upsert --stdin` accepts a JSON object of at most
  16 KiB: `{"account": { ... }, "password": "..."}`. Supply it through a private
  in-memory pipe, never a shell literal, argument, log, or temporary secret file.
  Omit `password` to retain the existing account secret. The account object
  replaces that account's configuration; preserve existing fields when updating.
- `ctox channel email-account sync --address ADDRESS --limit 10` fetches only
  the registered account's INBOX, using its own stored credentials. Limits are
  1–100. Unknown accounts fail without falling back to the instance account.

The account object includes `address`, `display_name`, `provider`, `username`,
`owner_user_id`, `shared_user_ids`, IMAP/SMTP host/port fields, and `ews_url`, `owa_url`,
`ews_auth_type`, `ews_version`. For Exchange, `username` is the Windows login
if it differs from the email address. An OWA URL may supply the origin from which
the existing connector derives `/EWS/Exchange.asmx`; a successful OWA browser
login alone does not prove the EWS endpoint accepts the same credentials.
Set `owner_user_id` to the verified Business OS user ID before publishing a
personal account to non-admin users. An empty owner is not an implicit team
share. For a team mailbox, set `shared_user_ids` to an explicit list of verified,
active Business OS user IDs. The account endpoint accepts this field only from
an administrator. The trusted-local CLI accepts it through `upsert --stdin`.
Omitting the field preserves existing shares; `[]` explicitly revokes every
share. Duplicate, empty, inactive, and unknown IDs are rejected. Do not replace
an owner with a guessed ID or write the channel SQLite profile directly.
Preserve the current account configuration and secret when performing an
authorized owner or share update.
Account updates first remove native reader grants, then save the registry and
publish the new grants. If either persistence step fails, Mail can temporarily
hide that account; retry the supported upsert after correcting the error. This
ordering prevents a reported failed revocation from retaining the old native
share. Previously downloaded browser copies require a separate cache/reload
acceptance check.
Native send and self-test connector profiles omit Business OS owner/share fields;
their email-account upsert preserves the current native access binding. An
explicit empty owner or share list from the account configuration path still
revokes it. Neither operation requires sending a message to verify the
binding: the native profile regression covers both cases.
Deleting a configured account revokes its native owner/share grants before
removing the registry entry and secret. Historical communication records remain
for administrator review; a later connector profile refresh cannot restore the
deleted readers' access.

Nonsecret registry entries use the existing `CTO_EMAIL_ACCOUNTS` runtime setting.
Passwords use secret scope `email-account` and the normalized address as name.
The CLI returns no password. The existing authenticated GET/POST
`/api/business-os/mail/accounts` keeps its owner/admin checks and now carries the
Exchange fields too; this does not introduce an HTTP message-data interface.

Synchronization persists messages through the native communication store and
existing Business OS projection. It does not send a test email. Search/history
outputs can contain message bodies, codes and links: an automation consumer must
keep these private and report only the metadata required for its task.

Verification requires the native account regression tests, an actual successful
account sync, and Mail UI account/message/body checks after ordinary reload.
Source changes and `has_password` alone are not proof of usable mailbox access.

# Secret Rules

- Local admin credentials for a new local service are normally `generated`.
- Existing host config, mounted files, and environment variables may be `discovered`.
- Remote SaaS or remote service credentials are often `owner_supplied` or `external_reference`.
- Persist secret material under a local path such as `runtime/secrets/*.env`.
- Persist only the reference path and classification in normal operator-visible state.
- Persist secret metadata separately enough to answer:
  - what kind of secret it is
  - whether it is present, missing, rotated, or invalid
  - whether the owner may safely reply by email or must switch to TUI

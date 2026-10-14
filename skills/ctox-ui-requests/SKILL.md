---
name: ctox-ui-requests
description: Ask the user for a secret, an app connection or a confirmation via an in-session card instead of chat. Use when `ctox toolhub exec` exits 3 (credential_required) or before a risky action.
---

# In-session cards: `ctox ui request`

Never ask the user to paste passwords, tokens or API keys into chat. Raise a
card; the value goes from the card straight into the instance secret store and
never reaches you or the transcript.

```bash
# App connection (opens the provider's sign-in in the user's app)
ctox ui request connect --toolkit vercel

# Secret form (masked). Fields default to one `token` field; `:plain` = not masked
ctox ui request credential --toolkit slack
ctox ui request credential --toolkit acme --fields client_id:plain,client_secret

# Yes/no confirmation before a risky action
ctox ui request confirm --toolkit gmail --message "E-Mail an kunde@example.com senden?"
```

The command blocks until the user acts and prints `{"id","kind","status","toolkit"}`.

| exit | meaning | what to do |
|---|---|---|
| 0 | done (submitted / connected / approved) | rerun the original command |
| 4 | declined | stop, tell the user plainly, do not retry |
| 5 | expired (default 15 min, `--timeout <secs>`) | ask once in chat whether to try again |
| 2 | usage error | fix the command |

Options: `--session <id>` (or env `CTOX_SESSION_ID`) pins the card to one chat;
`--no-wait` returns at once (poll with `ctox ui status <id>`); `ctox ui list`.

`ctox toolhub exec` creates the matching card itself when it exits 3; its JSON
then contains `ui_request.id`. Wait with `ctox ui status <id>` or just ask the
user to use the card, then rerun. Pass `--no-ui-request` to suppress it.

## Only the supervisor asks the user / Nur der Supervisor fragt den Nutzer

EN: Cards (credential, connect, confirm) are raised **only by the CTOX supervisor**.
Workers never create ui requests and never wait on the user.

How the role is determined (`src/core/execution/toolhub_role.rs`):

1. The session (`--session` or `CTOX_SESSION_ID`, a CodeThread id) is looked up in
   the instance store: `workjet_supervisor_bindings` → **supervisor**,
   `workjet_persistent_worker_bindings` → **persistent worker**, any other session
   → **one-shot worker**.
2. `CTOX_TEAM_ROLE` (`supervisor` | `persistent_worker` | `one_shot_worker`), set by
   the spawner, can only *restrict*: it can turn a supervisor session into a worker,
   never a worker into a supervisor. Unknown values count as one-shot worker.
3. No session and no role = operator shell (human/admin) → allowed.
   Spawners of workers MUST export `CTOX_SESSION_ID` and/or `CTOX_TEAM_ROLE`.

Behaviour in a worker:

- `ctox ui request …` creates nothing, prints `{"needs_user":{kind,toolkit,reason,fields,role},"next":…}`
  and exits **6**.
- `ctox toolhub exec` with exit 3 (credential required) adds `needs_user` to its JSON
  instead of an auto-card.
- **One-shot worker:** stop, return the `needs_user` as your result. The supervisor
  dedupes, raises the card, and re-dispatches you after the user answered.
- **Persistent worker:** return the `needs_user` to the supervisor and park; resume when
  the supervisor reports the request resolved.

Credential cards also carry `params` (the toolkit's connection fields such as Jira site,
Zendesk subdomain, Salesforce instance URL, with DE+EN label/help/placeholder). They are
non-secret and are stored with the connection, not in the request file.

DE: Karten erzeugt ausschließlich der Supervisor. Worker liefern ein `needs_user`
(Exit 6 bei `ctox ui request`). One-Shot-Worker beenden sich damit und werden nach der
Antwort neu gestartet; nur persistente Worker dürfen parken und warten.

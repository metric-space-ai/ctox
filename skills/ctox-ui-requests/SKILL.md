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

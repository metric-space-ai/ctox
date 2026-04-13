# Zammad API Access

Preferred auth:
- `Authorization: Token token=<token>`

Fallback auth:
- HTTP basic auth with `ZAMMAD_USER` and `ZAMMAD_PASSWORD`

Useful endpoints:
- `GET /api/v1/getting_started`
  - setup state
- `GET /api/v1/user_access_token`
  - list current access tokens
- `POST /api/v1/user_access_token`
  - create a new token
- `GET /api/v1/tickets`
  - list tickets
- `POST /api/v1/tickets`
  - create a ticket
- `GET /api/v1/users/search?query=<query>`
  - search users
- `GET /api/v1/groups`
  - list groups

The running target instance for the current test host is:
- `http://100.96.1.7:8082`

Expected runtime secret location for CTOX on the target host:
- `/home/metricspace/ctox-validation-20260325/work/runtime/secrets/zammad-admin.env`

That file should hold only references or tightly scoped access material for the installed helpdesk instance.

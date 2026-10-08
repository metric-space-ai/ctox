# Workjet project configuration

`ctox.workjet.project.upsert` persists project configuration in native
`workjet_projects` and projects the same record through the authorized CTOX DB
plane. The owner identity comes from command admission, never the payload.
An existing project owned by another user cannot be changed.

| Native field | Value |
| --- | --- |
| `repo_url` | Absolute HTTP(S) URL, maximum 2048 characters, no embedded credentials |
| `public_url` | Same URL bounds |
| `info` | Object with optional `summary`, `goal` (4096 characters each), `phase` (128); legacy `description` (4096) and `status` (128) remain accepted |
| `jour_fixe` | Object with ISO `weekday` 1–7 (Monday–Sunday), `time` as HH:mm, IANA `timezone` |

The default timezone is `Europe/Berlin`. Project information and goals may
contain normal line breaks; other control characters are rejected. Unknown
fields, invalid times/zones and credential-bearing URLs fail before mutation.
For each optional metadata field, omission keeps its current value and explicit
JSON null clears it. `archived` omission preserves an existing archive state;
only explicit false reactivates a project. Renaming does not erase configuration.
The native command result includes the saved project. Equal retries reuse the
record without changing its revision or timestamps.

The shell's existing `workjetProjectControl` exposes `project.configure`:

```javascript
await workjetProjectControl({
  action: 'project.configure', commandId: 'unique-command-id',
  projectId: 'existing-project-id', title: 'CTOX',
  repoUrl: 'https://github.com/metric-space-ai/ctox', publicUrl: 'https://ctox.dev',
  info: { summary: 'CTOX project runtime', goal: 'All twelve projects usable', phase: 'delivery' },
  jourFixe: { weekday: 3, time: '09:30', timezone: 'Europe/Berlin' },
});
```

The bridge translates camelCase keys to the native fields and waits for a
terminal, correlated native receipt. Its response exposes `repoUrl`, `publicUrl`,
`info`, `jourFixe`. Existing `project.list` and `project.create` clients retain
exactly the legacy `id`, `title`, optional `createdAt`, `workingCopies` shape,
even when native configuration is present. A new client requests list metadata
explicitly with `{ action: 'project.list', includeConfiguration: true }`; the
flag must be boolean and is not forwarded as native authority or stored data.
No caller-supplied owner, archive flag,
credential or execution thread is accepted by this configuration action.
A replaced browser session or a receipt for another actor/project is rejected.
Business data remains on the command/WebRTC plane; this adds no HTTP data API.

The registered native supervisor binding reconciles a weekly report at the
configured appointment and a Jour-fixe preparation two hours before it. Both
use the same IANA calendar and retain a paused schedule. The report/preparation
paths recheck project policy and the persisted supervisor binding before
submitting a supervisor turn; a configuration alone cannot invent a supervisor.
The native meeting read command returns occurrence metadata, not a completed
deck. Speech, meeting artifacts and owner-confirmed goal updates remain separate
follow-ups. The confirmed todo list, not an unconfirmed meeting proposal, will
define the supervisor goal.

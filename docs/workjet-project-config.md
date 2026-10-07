# Workjet project configuration

`ctox.workjet.project.upsert` persists project configuration in native
`workjet_projects` and projects the same record through the authorized CTOX DB
plane. The owner identity comes from command admission, never the payload.
An existing project owned by another user cannot be changed.

| Native field | Value |
| --- | --- |
| `repo_url` | Absolute HTTP(S) URL, maximum 2048 characters, no embedded credentials |
| `public_url` | Same URL bounds |
| `info` | Object with optional `description`, `goal` (4096 characters each), `phase`, `status` (128 each) |
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
  info: { goal: 'All twelve projects usable', phase: 'delivery' },
  jourFixe: { weekday: 3, time: '09:30', timezone: 'Europe/Berlin' },
});
```

The bridge translates camelCase keys to the native fields and waits for a
terminal, correlated native receipt. Its response and `project.list` expose
`repoUrl`, `publicUrl`, `info`, `jourFixe`; no caller-supplied owner, archive flag,
credential or execution thread is accepted by this configuration action.
A replaced browser session or a receipt for another actor/project is rejected.
Business data remains on the command/WebRTC plane; this adds no HTTP data API.

This change supplies the configuration foundation. A configured appointment
alone does not start a report or meeting: the Jour fixe contract, preparation
schedule, supervisor delivery bridge, speech and owner-confirmed goal update are
separate native follow-ups. The confirmed todo list, not an unconfirmed meeting
proposal, will define the supervisor goal.

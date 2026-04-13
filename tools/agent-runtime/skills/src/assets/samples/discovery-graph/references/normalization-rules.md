# Discovery Graph Normalization Rules

Use these rules when translating raw collector output into normalized graph facts.

These rules are the canonical interpretation guidance for the agent.
Helper scripts may assist, but they do not replace these rules.

Keep the model conservative. Prefer a missing relation over an invented one.

## Listener, Process, And Unit Binding

Build these entities when the evidence exists:

- `listener`
  - from `ss -tulpnH`
  - include protocol, bind address, port, pid, and process name in `attrs`
- `process`
  - from `ps -eo ...`
  - natural key should be stable for the observed process, usually `process:<pid>`
- `systemd_unit`
  - from `systemctl list-units` and especially `systemctl show --type=service`
  - include `MainPID`, `FragmentPath`, `ActiveState`, `SubState`

Preferred relations:

- `process -> runs_on -> host`
- `systemd_unit -> runs_on -> host`
- `listener -> managed_by -> process`
  - only if `ss` exposes a PID and that PID matches a discovered process
- `process -> managed_by -> systemd_unit`
  - only if `systemctl show` exposes `MainPID` and it matches the process PID

If the PID cannot be matched, keep the listener as a standalone fact and do not invent a manager.

When available, `/proc/<pid>/cgroup` evidence is a valid stronger hint for `process -> managed_by -> systemd_unit` than unit-name similarity.

## Repo To Runtime Binding

Use `repo_inventory` hits to create `repo_file` entities.

Preferred relations:

- `repo_file -> contains -> repo`
- `systemd_unit -> defined_in -> repo_file`
  - only when the file path or matching lines explicitly reference the unit name, fragment basename, or a matching `ExecStart` target
- `process -> defined_in -> repo_file`
  - only when the repo file explicitly references the process command or service binary

Do not bind repo files to runtime objects from naming similarity alone.

## Timers

Build `timer` entities from `systemctl list-timers` and `systemctl show --type=timer`.

Preferred relations:

- `timer -> runs_on -> host`
- `systemd_unit -> scheduled_by -> timer`
  - only when `systemctl show` provides a concrete `Unit=` target

## Journals

Use `journalctl -p warning -n 200 --no-pager` to derive compact findings, not raw log entities.

Create `journal_finding` entities only when there is a concrete issue pattern such as:

- repeated restart failures
- permission denied
- bind/listen errors
- storage/full errors
- dependency start failures

Recommended attrs:

- `severity`
- `source_unit` when known
- `sample_count`
- `samples`

Preferred relation:

- `journal_finding -> about -> systemd_unit`
  - only when a unit can be identified from the log lines or nearby collector evidence

## Full Sweep Rule

When the run is a broad host/repo sweep, persist the normalized graph with `full_sweep=true`.
That allows unseen entities and relations to be marked inactive.

If a bootstrap normalizer produces a graph that conflicts with the raw evidence, prefer the raw evidence and adjust the graph yourself.

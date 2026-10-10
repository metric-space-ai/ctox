# Supervisor dispatch observation

`business_os.workjet_worker_dispatch` accepts `{action:"observe",limit:16}`
from a restricted, currently leased native project Supervisor. The default is
16 rows, with an explicit maximum of 32. The native command or confirmed-goal
lease, admitted Owner, current project/Supervisor binding and DataRead policy
are revalidated. Caller project or thread selectors are rejected.

The response contains `contract`, `projectId`, `supervisorThreadId`,
`observations` and `truncated`. Each observation retains the actual native
`intentId`, `executionKey`, `dispatchKey`, source revision/currentness, bounded
task preview, requested computer/profile, and an optional persisted source
`acknowledgement`. Entries are newest first. The metadata budget is 64 KiB;
truncation is explicit rather than a claim of completeness.

An acknowledgement with `status:"dispatched"` means the registered Workjet
source acknowledged startup. It is not a completed turn, review result or
merged PR. `execution` remains null until an actual execution-result contract
exists. Requested model metadata is deliberately absent: selection is not an
actual producer witness. A failed source acknowledgement preserves its typed
reason. Revoked or replaced registrations remain visible as historical records
with `registrationCurrent:false`; observation cannot reactivate them.

The Supervisor can read dispatches from prior turns of its own current project
and bound thread before commissioning further work, and cite these retained
references in the next deck. It must still obtain actual completion and PR
receipts before claiming an outcome. There is no new poller, executor, grant,
HTTP data path or automatic retry. Existing source controls and their mutation
fences remain unchanged. Read-only Core/Policy snapshots do not repair schema
or acquire writer reservations; an uninitialised dispatch store returns an
empty observation list.

Regression coverage uses real native Supervisor admission and signed sessions:
pending intent, acknowledged startup retained into a later turn, typed source
failure, revoked registration, foreign-owner exclusion, replaced execution
lease, request/response bounds, absent schema and concurrent Core writer.
These tests do not establish installed week-long autonomous operation.

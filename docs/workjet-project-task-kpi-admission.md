# Project task KPI admission source

The Core command ledger stores an audit-safe `client_context`; it intentionally
omits the actor ID. Project task recipes therefore read the private native
`business_commands` admission envelope for the canonical Owner, project and
registered Supervisor thread, then match the entire payload against the indexed
Core command receipt. Core determines queue execution phase, terminal status,
rolling-window time and watermark. Missing admission or changed payload cannot
count; explicit conversation turns remain excluded. No browser-supplied counter,
Owner rewrite, authority grant or execution behavior is introduced.

Calculation streams one admitted command and one indexed Core lookup at a time
under the caller's existing Core/Policy snapshots. It does not copy all task
prompts into a whitelist or attach another database. Missing admission tables
report `missing_source`, not an invented zero.

The regression submits real native Supervisor turns and verifies that their Core
actors are redacted while the admission envelopes retain the Owner. The native
recipes then count three tasks, two completed and one open, for the verified
same-person alias; a foreign user is rejected. Those terminal states are fixture
ledger observations, not model execution or installed-product acceptance. Separate
cases reject conversations, foreign project/thread/Owner, a Core-only row, a
changed private payload, and a forged actor in the audit intent.

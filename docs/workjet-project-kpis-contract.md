# Prompted project KPI contract v1

Canonical fixture: `src/core/rxdb/tests/fixtures/workjet-project-kpis-v1.json`.
Generate both native DTOs and the browser contract with
`node src/core/rxdb/tools/build_workjet_project_kpis_contract.mjs`; `--check`
rejects drift. The shared corpus tests limits and semantic invariants on both sides.

The owner configures up to three `{kpi_id, prompt}` sentences, never a manual
KPI type, label or value. An empty list clears the configuration. Configuration
uses an operation ID and expected native revision. A changed sentence advances
its prompt revision and invalidates previous results. Duplicate KPI IDs fail.

The bound project supervisor resolves each sentence through the native KPI tool.
`ResolveKpiRequest` names the existing KPI and prompt revision; it cannot carry
numbers, owner identity, SQL, code or an arbitrary data URL. Authorization derives
from the signed command and the persisted project/supervisor binding, never from
the request body. Project ownership, authority epoch and expected revision must
be rechecked in the effect transaction before storing a result.

A native snapshot preserves source connection/metric IDs, project scope, source
watermarks, evidence receipts and observation times. Its registered recipe uses
identity, sum, average or percentage of up to eight measured inputs; every input
must be consumed exactly once. The validator reproduces the numeric result,
rejects zero-denominator percentages, and binds results to the current KPI,
prompt revision and project. Registered resolvers must independently verify the
source receipts and metric scope; structurally valid references are not authority.
Raw queries or executable expressions are not recipes.

The native formatter produces `display_value`, `unit` and a short automatic
`label` (24 characters maximum). Main renders these fully, with no ellipsis or
pencil control. The browser does not compute or invent values.

States:

- `resolving`: no result yet.
- `ready`: an evidenced snapshot is required.
- `stale`: a previous snapshot remains visible, with an explicit reason/message.
- `missing_source`: an explicit reason/message, with **no snapshot or guessed value**.
- `failed`: an explicit failure, with no asserted value.

Freshness includes native calculation, next refresh and expiry timestamps.
Observation precedes calculation; refresh follows calculation and occurs no later
than expiry. Native reads must downgrade expired snapshots to `stale` using the
native clock, and schedule bounded refresh work. A missing connected source must
produce `missing_source`, e.g. `analytics_not_connected`, instead of estimating
visitor counts. Permission loss must hide the snapshot rather than preserve data
under a revoked authority.

This PR supplies the typed persistence/command contract and shared regression
corpus. Native storage, registered metric resolvers, refresh scheduling, command
handlers and the supervisor KPI tool are the following implementation slice.
The command metadata here does not register a callable tool or establish an
installed KPI workflow. Main owns card/configuration UI; Crew owns the native
implementation and installed evidence. Business data continues over WebRTC/RxDB.

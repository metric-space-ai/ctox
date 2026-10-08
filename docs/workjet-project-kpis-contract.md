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

The shared fixture generates the native/browser contract and regression corpus.
The configuration/read slice below supplies native prompt storage and handlers;
registered metric resolvers, refresh scheduling and the supervisor KPI tool
remain follow-ups. The resolver metadata alone does not register a callable
tool or establish an installed KPI workflow. Main owns card/configuration UI;
Crew owns native implementation and installed evidence. Business data continues
over WebRTC/RxDB.

## Native configuration/read slice

`ctox.workjet.project.kpis.configure` and `.kpis.read` run through the existing
signed command plane, workspace DataWrite/DataRead policy and current native
project owner check (including a verified same-person alias). Read uses the
shared `ReadKpisRequest {project_id}`. Both return `{ok:true,kpis:ProjectKpis}`.
An unconfigured project reads as revision 0/items []; saving three sentences
persists a new project KPI revision, and `prompts:[]` clears them. Unchanged
sentences retain their prompt revisions. Changed/re-added IDs advance a durable
watermark even across clear, so old resolutions cannot become current again.

Configuration atomically commits native state and its domain receipt. An
operation ID replays only its original intent; stale expected revisions and
conflicting reuses fail without changing state. The native table is not a
browser-writable collection. Project metadata, owner IDs and legacy local
card settings are untouched. Foreign, archived, deleted and revoked identities
cannot read or configure a project's KPI state.

New sentences explicitly have `missing_source/source_not_bound` and no numeric
snapshot. This slice does not register `.kpi.resolve`, publish source values or
claim refresh scheduling. The native supervisor resolver/tool, independently
verified source adapters and bounded refresh scheduling remain the next slice.
The Guest bridge must call these typed commands over the existing WebRTC route;
there is no HTTP data endpoint. Installed acceptance is separate from checks.

The shell's `workjetProjectControl` accepts `project.kpis.read` with
`{commandId, projectId}` and `project.kpis.configure` with those keys plus
`{operationId, expectedRevision, prompts:[{kpi_id,prompt}]}`. It maps the outer
camelCase identifiers to the native request and returns
`{action,commandId,projectId,contract,kpis}`. `kpis` is the shared `ProjectKpis`
wire value, including its snake_case keys. Both actions use only the typed
business command receipt, never a historical projection pull. Unknown authority
fields, a changed configuration intent, a foreign receipt, invalid wire values
and session/database replacement fail before exposing results.

# Prompted project KPI resolver

The Owner configures up to three prompt sentences through the existing native
KPI configuration command. The currently leased registered project Supervisor
uses `business_os.project_kpi` to read those prompts and the recipe catalogue,
then resolves each prompt with a matching recipe and a rolling window of 1–365
days. The tool accepts no measurements, URLs, SQL or another project's source.

`read` uses `{action:"read",request:{project_id}}`. `resolve` uses
`{action:"resolve",request:{operation_id,project_id,kpi_id,prompt_revision,
expected_revision,recipe,window_days}}`. The shared native/browser fixture
owns these strict DTOs. The native receipt returns the current `kpis` state.

Available native recipes count admitted queued Supervisor commands for the
current project, canonical Owner and registered thread. They report total,
completed, failed, open, or completed divided by completed plus failed. The
window selects command creation time. A completed count requires a native
terminal completed receipt; model prose, finished leases and merged PRs do not
substitute for it. An empty finished denominator is `missing_source`.

The selected definition survives restart. Its snapshot contains the scoped
source revision/hash, window, calculation and native observation time. Labels
are generated from the selected recipe and fit fourteen characters. A native
scheduler reconciliation refreshes due definitions hourly; JourFix preparation
forces a refresh before dispatching its Supervisor turn. Configuring a different
prompt retires the old recipe; its monotonic prompt revision rejects late
results. Reads mark expired snapshots stale without taking a writer lock.

GitHub merged PRs remain `missing_source` until a verified metric adapter is
connected. Analytics, revenue and other unregistered sources likewise have no
native recipe yet. The Supervisor must match the catalogue meaning to the
prompt and name missing sources rather than selecting a misleading substitute.
Owner/browser sessions cannot impersonate the leased Supervisor to bind a
recipe. Autonomous refresh revalidates active Owner, project, native Supervisor
binding and data policy; revocation, archive or rebinding stops the old recipe.

These native regressions prove source scope and persistence, not installed
Workjet card rendering or Monday's deck/audio acceptance. Delivery and real
product acceptance use the existing native and signed Shell writers.

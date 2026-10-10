# Reading the native KPI refresh schedule

The existing authenticated Owner command ctox.workjet.project.kpis.read and restricted Supervisor business_os.project_kpi action read accept optional request field include_refresh_schedule:true.

When explicitly requested, each result may contain next_refresh_ms from its current native definition. The read uses the same Owner/project snapshot, bound Supervisor and prompt revision. Missing, foreign or superseded definitions omit the field. It never derives a due time from snapshot freshness, copies a persisted DTO value or forces a refresh.

Existing requests omit the flag and retain their previous response shape, including for installed clients that reject unknown result fields. The read takes no writer lock, initializes no definitions and changes no schedule. Fixtures generate both native and browser contracts.

For automatic-refresh acceptance, read before/after through this authenticated product path and retain native revision, snapshot calculated_at_ms and next_refresh_ms while ordinary work is busy/backlogged. A manual forced refresh is not equivalent evidence.

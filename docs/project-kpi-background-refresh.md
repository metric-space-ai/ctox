# Hourly project KPI maintenance

Native KPI recipes derive values from admitted project task receipts. Their
hourly refresh runs before the channel router's active-turn, ready-prompt,
idle-preflight and queue-pressure returns. A running Supervisor or an ordinary
research backlog can therefore keep their existing dispatch limits without
preventing KPI metadata from becoming fresh.

This pass does not invoke a model, lease a task or change the project schedule,
configured prompt, recipe or authority. The existing resolver still checks the
current Owner, registered project/Supervisor binding and DataRead/DataWrite
policy. No due definition means no writer reservation. A refresh error is
reported as deferred without blocking communication routing.

The schedule dispatcher and pre-meeting preparation retain their existing
refresh calls. Installed verification must read the original project's native
snapshot after an hourly deadline and confirm freshness; a source test or a
manual recipe refresh does not substitute for that observation.

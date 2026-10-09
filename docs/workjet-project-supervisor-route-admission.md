# Selected Supervisor routes and holding execution

An explicit project `supervisor_luma_id` must not silently run with the native
instance default. The native service now checks that selection after creating
its actual restricted Supervisor command or confirmed-plan session and before
invoking a model. Unselected projects retain the existing route; their added
check takes no Core or Policy writer reservation and does not read Luma/account
configuration. Monday's ctox.dev default is unaffected.

The selected path holds the existing Core lease and Policy Owner/project fence.
It revalidates the registered Supervisor, native command envelope or confirmed
goal lease, current authority epoch, current instance configuration revision,
unique profile and route IDs, Owner-authorized profile/computer assignment, and
`llmRoutes[].nativeAccountReference`. Legacy Workjet `gatewayAccountId` is never
converted into a native account ID. The native account's current Owner, holder,
revision, selected/excluded models and fresh authenticated catalog must agree.

This increment is a fail-closed integration boundary, not a Claude Code executor.
Models' native catalog eligibility is explicitly not a holding execution permit.
There is currently no admitted adapter joining that exact native account to an
automatic Claude Code Supervisor ProviderSession. An eligible explicit selection
therefore returns `claude_code_holding_executor_unavailable` (other configured
harnesses: `project_supervisor_holding_executor_unavailable`), rather than
relabelling embedded Codex or substituting a Mac gateway account. A legacy route
without the native reference returns `missing_native_account_binding`.

The private Core `workjet_supervisor_route_attempts` retains requested project,
Supervisor, Luma/configuration revision, computer, harness, route and canonical
account/model facts for the exact signed native execution lease. Its
`actual_json` is NULL: no model was invoked. Repeating the same lease records one
row, while a replaced/expired lease cannot record another selection. Private
holder selectors, tokens and credentials are not included. Existing durable
worker error/finalization handling remains the terminal-state authority.

Remaining G3-B delivery is the real holding producer plus its durable actual
account/model/harness/session/turn receipt, followed by installed Molecularity
acceptance. It must revalidate the same native selection before dispatch and
publication and preserve the unselected default. Requested configuration and a
successful catalog observation must never be presented as execution evidence.
No successful Claude Code execution or full autonomous weekly cycle is claimed
by this increment.

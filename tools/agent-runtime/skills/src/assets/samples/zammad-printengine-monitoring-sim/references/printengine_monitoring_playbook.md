# Printengine Monitoring Playbook

Scope:
- Filialserver `133IG`
- Service `Prestige Printengine`

Rule:
- Read the current monitoring state before any Entwarnung or closure language is
  written into the ticket.

Execution:
1. Check whether the alert is still `critical`, already `recovered`, or
   currently `unknown`.
2. If the alert is still `critical`, do not write Entwarnung. Keep the ticket
   at the desk/execution boundary and escalate to Infrastructure Ops.
3. If the alert is `recovered`, require a fresh recovered timestamp before
   drafting an Entwarnung.

Verification:
- Current state is visible.
- Host, service, and checked timestamp match the expected alert target.
- A recovered signal is newer than the original alert if Entwarnung is drafted.

Writeback boundary:
- Default writeback is an internal note or suggested desk update.
- No public “resolved” statement is allowed from this source alone.

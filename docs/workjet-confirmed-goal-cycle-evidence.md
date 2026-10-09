# Confirmed project goals between Jour fixes

An explicitly Owner-confirmed todo list is an auto-advancing Core goal in the
project's bound Supervisor thread. The existing daemon plan tick emits one
durable plan-channel message per pending step. An outstanding step prevents a
second emission; completing a step still requires the existing reviewed
terminal-success proof. There is no second polling loop or weekly-report write
permission. Weekly reports remain read-only.

Preparation retains the exact previous confirmed goal reference. The
`previous_goal_definition` read resolves its current Core steps, including work
that finishes after preparation. Each step now includes `dispatch` with the
persisted `emission_attempts`, `message_key`, `updated_at`, and `completed_at`.
Null message/completion fields mean no dispatch/completion was persisted. These
are plan delivery observations, not claims of provider attempts, worker model,
merged PRs, or successful installed-product acceptance. Existing status and
bounded result excerpts are retained. The Owner/project/revision checks and
metadata budget apply to the complete response.

The native regression follows confirmation, automatic plan emission, repeated
tick, admitted plan-channel lease, restricted Supervisor MCP session, rejection
of unreviewed completion, reviewed completion, duplicate completion, and a new
read for the next deck. Only the review boundary is a test fixture. It also
checks that the completed lease cannot retain Supervisor authority and that a
foreign Owner cannot read the goal. It does not certify the installed
Molecularity cycle of 14–21 October: that requires actual commissioning,
network runs, PR outcomes, and the next generated deck in the product.

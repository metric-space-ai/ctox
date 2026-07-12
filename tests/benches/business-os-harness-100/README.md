# Business OS Harness 100

Live acceptance bench for the complete Business OS harness path. The catalog
contains exactly 100 small tasks: eight autonomous families (80 cases), ten
human approvals, and ten missing-input escalations. Modules rotate across
Customers, Invoices, Support, Calendar, Documents, Research, Reports,
Matching, Knowledge, and Threads.

Each family is crossed with every module using a domain-specific business
record (for example a customer file, invoice, support ticket, calendar entry,
contract, market research, monthly report, matching case, knowledge article,
or operational thread). They are deliberately small enough that failures can
be attributed to routing/review rather than task complexity.

The bench submits real `business_os.chat.task` documents through the native
Business OS command bus. It does not mock model answers or write terminal
state. The serial CTOX service remains the worker.

```sh
# Inspect all exact prompts and expected markers without writes.
ctox business-os harness-bench catalog
ctox business-os harness-bench run --dry-run

# Deliberately submit all 100 to a configured real instance.
ctox business-os harness-bench run --confirm-live \
  --run-id acceptance-2026-07-12 \
  --actor <business-os-user-id> \
  --reviewer <business-os-reviewer-id>

# Poll until settled; this exits red for failures or lost work.
ctox business-os harness-bench status --run-id acceptance-2026-07-12
ctox business-os harness-bench status --run-id acceptance-2026-07-12 --fail-on-inflight
```

Filters (`--case`, `--family`, `--limit`) allow a cheap canary before the full
run. A normal case passes only when answer markers, review, validation, and a
matching completed `business_chats` reply agree. A human case is accepted only when its
Approval/Escalation, Thread, and Notification are durable. A terminal task
without the required human route is `lost_between_chairs` and fails the suite.

Recommended live sequence:

1. Run `--case H001` to prove the answer-only review regression is fixed.
2. Run `--case H081`, open Threads, and confirm the pending approval is visible.
3. Run `--case H091`, open Threads, and confirm the missing-input escalation is
   visible without an invented action.
4. Submit the complete suite with a fresh run ID and poll with
   `--fail-on-inflight` until it settles.

The full green distribution is exactly `passed: 80` and
`awaiting_human: 20`, with zero `in_flight`, `failed`, and
`lost_between_chairs`. Preserve the status JSON as the release artifact.

The bench never auto-approves, auto-rejects, sends, or performs the protected
business action. Human decisions are exercised in the real Threads app.

## CTOX Runtime Contract

- Task spawning is allowed only for real bounded work steps that add mission progress, external waiting, recovery, or explicit decomposition. Do not spawn work merely because review feedback exists.
- The Review Gate is a quality checkpoint, not a control loop. After review feedback, continue the same main work item whenever possible and incorporate the feedback there.
- Do not create review-driven self-work cascades. If more work is needed, reuse or requeue the existing parent work item; create a new task only when it is a distinct bounded work step with a stable parent pointer.
- Every durable follow-up, queue item, plan emission, or self-work item must have a clear parent/anchor: message key, work id, thread key, ticket/case id, or plan step. Missing ancestry is a harness bug, not acceptable ambiguity.
- Rewording-only feedback means revise wording on the same artifact. Substantive feedback means add new evidence or implementation progress. Stale feedback means refresh or consolidate current runtime state before drafting again.
- Before adding follow-up work, check for existing matching self-work, queue, plan, or ticket state and consolidate rather than duplicating.
- Terminal work is valid only when the durable outcome exists in runtime state. Text like "sent", "done", or "closed" is not evidence. For outbound email, the required witness is an outbound email artifact with terminal state `accepted`.
- Outcome failures must stay agent-owned. The harness and reviewer explain the missing durable artifact; they do not send email, close tickets, or complete the work for the agent.


# Process Mining Diagnostics

Use this skill when CTOX needs to audit its own harness behavior, runtime state transitions, review gates, communication failures, queue drift, or stuck long-running operations.

## Procedure

1. Ensure instrumentation exists:
   ```bash
   ctox process-mining ensure
   ```

2. Verify the declarative state-machine model is internally consistent:
   ```bash
   ctox process-mining core-liveness
   ctox process-mining spawn-liveness
   ```

3. Check hard failure surfaces:
   ```bash
   ctox process-mining self-diagnose --limit 20000
   ctox process-mining deadlocks --limit 50
   ctox process-mining mapping-rules --limit 200
   ctox process-mining proofs --limit 50
   ctox process-mining spawn-edges --limit 50
   ctox process-mining state-scan --limit 20000
   ctox process-mining assert-clean --limit 20000
   ctox process-mining state-audit --limit 50
   ctox process-mining coverage --limit 50
   ctox process-mining scan-violations
   ctox process-mining violations --limit 50
   ```

3a. For deeper forensics use the harness-mining suite (Tier 1 + Tier 2):
   ```bash
   ctox harness-mining stuck-cases             # retry-loops & idle cases
   ctox harness-mining variants --cluster      # trace variant Pareto + clustering
   ctox harness-mining sojourn                 # state-holding-time distribution
   ctox harness-mining conformance             # threshold-gated conformance replay
   ctox harness-mining alignment               # alignment-based reparation hypotheses
   ctox harness-mining causal                  # predecessor lift per violation code
   ctox harness-mining drift                   # Page-Hinkley + chi-squared drift
   ctox harness-mining multiperspective        # data-aware constraint coverage
   ```

4. Inspect a suspicious case before responding:
   ```bash
   ctox process-mining explain-case <case-id> --limit 200
   ```

5. Inspect outcome-witness evidence before accepting any terminal work claim. In `ctox process-mining proofs`, check `evidence_summary.expected_artifact_refs`, `evidence_summary.delivered_artifact_refs`, and `report_summary.violation_codes`. Treat `WP-Outcome-Missing` and `WP-Outcome-Wrong-State` as critical because the agent claimed completion without a matching durable artifact in the expected terminal state.

6. Inspect spawn discipline for every stuck or duplicated case. Use `ctox process-mining spawn-edges --limit 50` to confirm that every spawned work item has a stable parent, budget key, checkpoint key, accepted spawn edge, and no rejected `spawn_*` violations. Use `ctox process-mining spawn-liveness` to verify that core spawners and harness subagents are bounded, leaf-only where required, and cannot create recursive worker trees.

7. Report only evidence-backed conclusions. Include model id, conformance run id, deadlock suspects, violation ids, proof ids, and spawn edge ids. If a communication reached `sent`, `done`, `completed`, or `delivered` without prior review evidence or without the required outcome witness, treat it as a critical harness violation and repair the queue before any further outbound communication.

8. For subsystem forensics, use `self-diagnose` first. It must cover at least process-mining coverage, core graph liveness, spawn liveness, outcome-witness health, knowledge growth/load, LCM continuity commits, queue throughput and slowest/fastest tasks, founder review gates, ticket/self-work backlog, and schedule/deadline backing.

## Guardrails

- Do not bypass `ctox process-mining` with ad-hoc SQLite queries unless the CLI lacks a required field.
- Treat `mapping_kind = unmapped` as a modeling gap: add or fix an explicit transition rule before claiming the harness is fully covered.
- Treat a terminal `WorkItem`, `QueueTask`, `FounderCommunication`, ticket, or schedule transition with expected artifacts but no delivered artifact refs as a hard kernel failure until the agent resumes and creates the artifact itself.
- Treat rejected or missing spawn edges as a process-model gap. Task spawners must be represented in the state machine with parent, child, checkpoint, budget, and intervention semantics.
- Treat zero durable knowledge entries, missing LCM commits, or compact commands without continuity mutation as harness degradation, not as harmless absence of data.
- Do not send Founder/customer communication while critical process violations are unresolved.
- Do not treat a reworded message as a valid fix when the process violation is missing research, missing review, missing recipient/CC validation, or missing deadline scheduling.

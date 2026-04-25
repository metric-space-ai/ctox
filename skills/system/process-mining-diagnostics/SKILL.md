# Process Mining Diagnostics

Use this skill when CTOX needs to audit its own harness behavior, SQLite state transitions, review gates, communication failures, queue drift, or stuck long-running operations.

## Procedure

1. Ensure instrumentation exists:
   ```bash
   ctox process-mining ensure
   ```

2. Build a current process model:
   ```bash
   ctox process-mining discover-petri --model-id current-harness
   ctox process-mining core-liveness
   ctox process-mining replay current-harness
   ```

3. Check hard failure surfaces:
   ```bash
   ctox process-mining self-diagnose --limit 20000
   ctox process-mining deadlocks --model-id current-harness
   ctox process-mining mapping-rules --limit 200
   ctox process-mining proofs --limit 50
   ctox process-mining state-scan --limit 20000
   ctox process-mining assert-clean --limit 20000
   ctox process-mining state-audit --limit 50
   ctox process-mining coverage --limit 50
   ctox process-mining scan-violations
   ctox process-mining violations --limit 50
   ```

4. Inspect a suspicious case before responding:
   ```bash
   ctox process-mining explain-case <case-id> --limit 200
   ```

5. Report only evidence-backed conclusions. Include model id, conformance run id, deadlock suspects, and violation ids. If a communication reached `sent`, `done`, `completed`, or `delivered` without prior review evidence, treat it as a critical harness violation and repair the queue before any further outbound communication.

6. For subsystem forensics, use `self-diagnose` first. It must cover at least process-mining coverage, core graph liveness, knowledge growth/load, LCM continuity commits, queue throughput and slowest/fastest tasks, founder review gates, ticket/self-work backlog, and schedule/deadline backing.

## Guardrails

- Do not bypass `ctox process-mining` with ad-hoc SQLite queries unless the CLI lacks a required field.
- Treat `mapping_kind = unmapped` as a modeling gap: add or fix an explicit transition rule before claiming the harness is fully covered.
- Treat zero durable knowledge entries, missing LCM commits, or compact commands without continuity mutation as harness degradation, not as harmless absence of data.
- Do not send Founder/customer communication while critical process violations are unresolved.
- Do not treat a reworded message as a valid fix when the process violation is missing research, missing review, missing recipient/CC validation, or missing deadline scheduling.

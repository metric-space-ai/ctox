---
name: acceptance-verification
description: Prove that a deployed service is actually usable, not merely running, by checking layered acceptance gates such as process, listener, HTTP, authenticated admin/API access, safe mutating smoke checks, and persistence. Use when CTOX must decide whether a deployment truly passed or still needs repair.
---

# Acceptance Verification

Use this skill after `service-deployment`, `change-lifecycle`, or any high-impact service repair.

Do not use it for:

- broad host health review: use `reliability_ops`
- initial secret generation by itself: use `secret-management`
- generic change planning without a service target: use `change_lifecycle`

## Operating Model

Treat this skill as a layered proof, not a shallow smoke test.

Preferred helper under `scripts/`:

- `verify_contract.py`

The helper is inspectable. Patch it when the service exposes unusual verification surfaces.
Use `--required-profile` and `--minimum-layer` to make the expected proof explicit instead of assuming that any highest observed layer is sufficient.

## Verification Layers

Check the highest relevant layer that the service safely supports:

1. `service_process`
2. `listener`
3. `http`
4. `authenticated_api`
5. `admin_identity`
6. `mutating_smoke`
7. `persistence`

Do not stop at a lower layer if a higher safe layer is available.

## Workflow

1. Name the service target and the expected operator outcome.
2. Enumerate the verification layers available for this service.
3. Prove them in order until either:
   - the deployment is truly usable
   - or the first failing layer is identified
4. Declare the expected minimum proof before summarizing:
   - `read_only_service`
   - `operator_managed`
   - `admin_managed`
   - `safe_mutation`
   - `durable_mutation`
   - or an explicit `--minimum-layer`
4. If a lower layer passes but a higher layer fails, return `needs_repair`.
5. Name the exact failed layer and likely cause.
6. Hand control back to `service-deployment` or the concrete service skill for repair.

## Operator Feedback Contract

Use these headings:

- `**Status**`
- `**State**`
- `**Verification Layers**`
- `**Passing Evidence**`
- `**Failed Layer**`
- `**Likely Cause**`
- `**Next Step**`

`State` must be one of:

- `executed`
- `blocked`
- `needs_repair`

## Guardrails

- A running service alone is never enough.
- A listening port alone is never enough.
- A web login page alone is never enough when CTOX is expected to administer the service.
- If authenticated admin or API access is expected and fails, the result is not complete.
- If a safe mutating smoke check is available and has not passed yet, do not claim full success.
- If the service is only proven to a lower layer than the declared minimum proof, return `needs_repair` with `cause=verification_incomplete`.

## Resources

- [references/verification-layers.md](references/verification-layers.md)
- [references/failure-classification.md](references/failure-classification.md)

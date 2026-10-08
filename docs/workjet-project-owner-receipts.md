# Workjet project configuration receipts

`ctox.workjet.project.upsert` resolves an authenticated, verified alias inside
the domain writer transaction. Its successful result includes the canonical
`owner_user_id` alongside the saved `project`. The scalar and the project's
owner come from that same transaction; the request cannot select either.

The Shell's `project.configure` bridge keeps its original authenticated actor,
session and database fence. It checks the terminal command ID, target project,
collection and result before returning the saved configuration, and requires
the project's owner to match the native result's owner. A verified alias can
therefore save its own project without a false failure after persistence.

An older native result without this scalar is accepted only when the project
owner exactly matches the authenticated actor. The Shell does not infer aliases
from email, profile text or caller-supplied fields. Foreign capability holders
cannot update the canonical owner's project by claiming the alias actor.

Regression coverage lives in `workjet-project-control.test.mjs` and
`workjet_identity_tests.rs`. The native alias update test uses real managed
capabilities through replicated-peer admission and verifies a foreign-token
attempt leaves the saved project unchanged.

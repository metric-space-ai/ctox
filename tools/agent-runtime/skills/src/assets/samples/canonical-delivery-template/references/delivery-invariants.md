# Delivery Family Invariants

The delivery family exists so CTOX can bring a service from "not present" or "misconfigured" to "installed and verified" without losing operator trust.

Delivery skills must:

- preserve the shared SQLite evidence kernel
- classify local install versus external integration before asking the owner for credentials
- classify credentials as:
  - `generated`
  - `discovered`
  - `owner_supplied`
  - `external_reference`
- persist only secret references and metadata in normal evidence state
- persist actual secret material in local secret files or another explicit secret backend
- verify the resulting service before declaring success

Delivery skills must not:

- ask the owner for credentials that CTOX can safely generate itself
- imply hidden manual steps
- leave an install half-finished without a durable next-work record
- conflate an external mount/integration requirement with a local installation requirement

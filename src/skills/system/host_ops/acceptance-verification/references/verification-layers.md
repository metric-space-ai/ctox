# Verification Layers

- `service_process`
  - process, unit, container, or snap service is running
- `listener`
  - expected TCP/UDP listener exists
- `http`
  - expected UI or health endpoint answers successfully
- `authenticated_api`
  - authenticated API request succeeds with the expected secret reference
- `admin_identity`
  - admin login or current-user identity endpoint confirms the expected operator identity
- `mutating_smoke`
  - a safe write action succeeds, for example creating a draft object, test key, or harmless temporary record
- `persistence`
  - the change or identity still exists in a fresh verification read

Rules:

- Use the highest safe layer available.
- When a service is supposed to be operator-managed by CTOX, `authenticated_api` or `admin_identity` is usually mandatory.
- If a mutating smoke check is too risky, say why and stop at the highest safe layer.
- Make the expected minimum proof explicit when summarizing:
  - `read_only_service` -> `http`
  - `operator_managed` -> `authenticated_api`
  - `admin_managed` -> `admin_identity`
  - `safe_mutation` -> `mutating_smoke`
  - `durable_mutation` -> `persistence`

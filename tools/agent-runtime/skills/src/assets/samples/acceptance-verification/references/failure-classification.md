# Failure Classification

When verification fails, classify the highest failing layer into one of:

- `secret_invalid`
  - supplied or generated credential does not authenticate
- `auth_path_unknown`
  - the service is up but the correct login or API path is not yet established
- `bootstrap_incomplete`
  - migrations, setup wizard, seed data, or initialization steps are unfinished
- `binding_or_url_mismatch`
  - service answers on the wrong host, port, or trusted URL
- `service_not_ready`
  - process exists but is not yet ready to answer correctly
- `unsupported_verifier`
  - CTOX lacks a verifier for this service shape and must build or refine one

Rules:

- Report the most specific cause supported by evidence.
- `401`, `403`, or invalid-admin responses are not a passed deployment.
- `unsupported_verifier` means CTOX must improve the generic deployment family instead of pretending the install succeeded.

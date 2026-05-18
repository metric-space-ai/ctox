# Helper Surface

- Prefer the built-in `ctox secret` CLI as the canonical interface.
- Use `ctox secret put` and `ctox secret intake` to persist values in the encrypted SQLite secret store.
- Use `ctox secret list` and `ctox secret show` when you only need metadata.
- Use `ctox secret get` only for the narrow execution step that truly requires the raw value.

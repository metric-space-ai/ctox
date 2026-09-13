# Password generation in the existing CTOX Secret Store

`ctox secret generate --scope credentials --name PROVIDER_PASSWORD`
creates a password directly in the existing encrypted Secret Store. The default
length is 24; `--length` accepts 16 through 128. Generated passwords use a uniform
64-character alphabet (ASCII upper/lowercase, digits, `-` and `_`) and contain all
four classes. This deliberately bounded policy is not a promise of compatibility
with every provider's password requirements.

Only the generation receipt and secret metadata are returned. Password bytes
are kept in zeroizing native memory, encrypted with the existing AES-256-GCM
implementation, and never added to the request, command output, model context,
environment, or a plaintext password file. Encryption's working buffer is also
zeroized on failure. This is not a claim that process memory or the destination
password field can remain encrypted while the password is being used.

Generation is create-only. A repeated request for the same scope/name returns
`created: false` and the existing metadata, without replacing the value or
changing its timestamps/policy metadata. A SQLite immediate transaction protects
this rule across concurrent writers. Generation must not be used for rotation;
an existing value is not proof of a working provider account.

## Native Business OS command

The existing command dispatcher accepts `ctox.secret.generate` with:

```json
{"name":"PROVIDER_PASSWORD","length":24}
```

`length` may be omitted. The command requires the existing server-authoritative
`SecretsManage` workspace permission, fixes the scope to `credentials`, validates
the existing UPPER_SNAKE_CASE key rules, and rejects unknown fields. The result
contains `created`, `name`, `secret_ref: {scope, name, secret_id}`, and
`secret_value_revealed: false`. No browser credential value is required or
returned. Command requests/receipts use the existing native dispatcher and
RxDB/WebRTC path; no HTTP data bridge or new secret database was added.

Malformed generation payloads are replaced with an invalid null payload at
both replicated-document intake and native command intake, before claim,
projection or failure receipts persist. This avoids retaining an accidentally
supplied `value`, nested content or other invalid fields. The strict handler
still rejects that request; sanitization never converts it into a valid request.
Valid name/length requests are unchanged. This guard applies to payloads, not
arbitrary user text placed in unrelated command metadata.

## Registration integration boundary

This change provides generation and an authorized native command, **not a
completed autonomous-registration feature**. It does not add a new model tool,
MCP action, Credentials UI button, provider adapter, or deployment to THESEN.
Consumers must pass a secret reference to an authorized native destination
adapter. They must not call `secret get` into a shell/tool transcript, insert the
plaintext into model-authored source, or export it to browser storage. Existing
authorization, destination-origin checks, terms approval, and interactive
handoffs remain in force.

The existing web-stack signup helper resolves a secret reference natively but
interpolates its value into generated automation source. Its full runtime/error/
artifact path still needs verification before using it for the requested
no-plaintext-registration workflow. The password generator alone does not certify
that path or account activation through email.

## Verification

Added native regressions cover length/classes, sample uniqueness (not an entropy
proof), encrypted storage and metadata-only receipts, reopening/retry behavior,
preservation of imported credentials, concurrent create-once behavior, invalid
arguments, and Business OS permission denial/receipt persistence.
Additional regressions cover invalid-payload sanitization and full native
intake with synthetic canaries for authorized and denied actors, both directly
and after the replicated-document sanitizer writes to the real fixture store.
They check failed receipts and all SQLite stores for the canary, and ensure no
credential was generated. These added regressions have not yet executed.

Run on the authorized native verification host, with at most two workers:

```sh
cargo test --locked --bin ctox generated_password -- --test-threads=2
cargo test --locked --bin ctox ctox_secret_generate -- --test-threads=2
```

These tests are not recorded as passing until an actual native run completes.
Repository-wide native/CTOX DB checks remain separate from this focused proof.

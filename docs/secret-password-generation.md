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

The Credentials app offers a create-only **Generate password** action. It sends
only the validated name and length 24 through the existing command bus, validates
the native metadata receipt before reporting success, and refreshes metadata
after generation (including commands from another client). Existing stored
credentials are not rotated. Unknown/failed outcomes are not shown as success;
raw errors and response values are not rendered. The action does not expose the
generated password or certify provider registration.

This change provides generation and an authorized native command, **not a
completed autonomous-registration feature**. It does not add a new model tool,
MCP action, provider adapter, or deployment to THESEN.
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

## Authorized display and clipboard

The Credentials app also offers explicit **Show / Hide / Copy** for an existing
credential, masked by default. This does not rotate the credential. JSON login
bundles using the native username/email/login/login_hint and
password/credential/secret aliases show individually copyable username/password
fields; scalar API keys and unknown formats retain their exact raw value.

`ctox.credentials.reveal.v1` is a transient auxiliary WebRTC method with one
exact `{name}` parameter, not a business command or an MCP action. Native policy
requires current `SecretsManage` workspace authority and read access to the
`business_commands` channel. Scope is fixed to `credentials`; invalid, stale,
revoked or unprivileged capabilities fail closed. Authorization is rechecked
after the encrypted-store read. Responses and errors are not persisted as
commands, projections, audit payloads, or application logs. Metadata lists and
exports still exclude values.

The browser forbids this method on the cross-tab proxy, both before a follower
sends and in the leader relay handler. Use the directly connected tab; there is
no BroadcastChannel, HTTP or storage fallback. Values live only in transient
response/UI memory and an explicitly requested clipboard write. Display clears
after 30 seconds, on Hide, blur/pagehide, hidden document, selection/re-render,
and disposal. Late responses are discarded. DOM insertion uses text, not HTML.
JavaScript garbage collection and OS clipboard/history are not a zeroization
guarantee; the app does not clear or overwrite the user's clipboard afterward.

The new native `credential_reveal_*` cases cover unchanged stored metadata/value,
synthetic-canary absence across fixture files, exact request shape, missing
values, invalid/unprivileged/stale capabilities, and generic errors. They must
execute before native acceptance. Controller tests use DOM/clipboard doubles;
they are not a live THESEN or real WebRTC permission test.

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

The Credentials UI also has dependency-free module/event-handler regressions:

```sh
node --experimental-vm-modules --test --test-concurrency=1 src/apps/business-os/modules/credentials/generation.test.mjs
node --test --test-concurrency=1 src/apps/business-os/modules/credentials/reveal.test.mjs
cargo test --locked --bin ctox credential_reveal -- --test-threads=2
```

They exercise selector-only dispatch, receipt validation, create-only existing
records, duplicate-click protection, permission denial, redacted failure and
metadata refresh using host/DOM doubles. Passing these does not establish a
real browser registration, encrypted-store integration or live THESEN readiness.

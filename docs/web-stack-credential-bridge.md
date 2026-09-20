# Native web-stack credential bridge

This source candidate supplies a request-borrowed resolver to native web search
and person research. It is not runtime acceptance or a complete Leadfeeder
migration; the Leadfeeder adapter must separately override the new source hook.

`ctox_web_stack::credentials` defines `CredentialReference { scope, name }`,
`CredentialResolver::resolve`, a non-serializable `SecretValue` with redacted
Debug, and sanitized error categories. `resolve_credential(None, ...)` returns
Unavailable. Only an absent authorized row returns `Ok(None)`.

Core owns `web_stack::credentials::NativeCredentialResolver`. It binds the
trusted operation root and permits exactly these case-sensitive references:

| Scope | Name |
| --- | --- |
| credentials | LEADFEEDER_API_KEY |
| credentials | LEADFEEDER_LEGACY_API_TOKEN |

The resolver calls the native encrypted store's optional getter directly; it
never launches `ctox secret get` or reads plaintext runtime configuration.
Denied references fail before opening the store. Store/key failures,
decryption failures, and invalid UTF-8 remain distinct from absence. Public
errors carry no underlying store error text.

For an existing ciphertext row, reads load an existing key only. A missing key
returns Unavailable without generating or persisting a replacement. Supported
embedded/runtime legacy keys may migrate through the existing guarded conflict
checks; only write-side key initialization may generate a new key.
Account and authentication mode remain separate non-secret configuration.
This is an internal capability for
native research operations, not an external arbitrary-secret lookup tool.

`SourceCtx` is unchanged. `SourceModule::fetch_direct_with_resolver` receives
an optional borrowed resolver and defaults to the old `fetch_direct` for
existing modules. Credential-aware adapters must override it, use the resolver
only for the authorization header, and reject an absent resolver or lookup
error without selecting a browser authentication mode. The compatibility
hook does not retrofit encrypted lookup into legacy adapters.

Native CLI search/person-research, the native person-research command/service
and importer, and the native Responses search facade lend this resolver.
Standalone web-stack entrypoints preserve their old signatures and pass None;
explicit `*_with_resolver` entrypoints permit host injection. Deep research's
internal generic searches retain the standalone path.

Each successful direct API hit passes through `extract_from_hits` separately,
with its original query and row. The native serializer emits
`results[].extracted_fields` as an array of
`{ field, value, confidence, source_url, note }`. These fields do not promote
transport/content verification, evidence eligibility, or citations. Generic
search-cache serialization omits the new request-local field array. Pinned
source searches do not read or write the generic result cache; cached-only
pinned requests fail before source/credential access. Successful repeated
requests fetch and extract again. Direct API rows are merged after generic
page fetching, keeping them out of the page-fetch/cache path. A new generic
cache-key namespace excludes older entries that could contain authenticated
source hits. Generic queries must warm that new cache before cached-only use.

## Verification and remaining integration

Authored regressions cover a real encrypted native-store fixture, exact scope
and name denial, absence versus corrupt ciphertext/unavailable store, redacted
Debug/errors, and a synthetic API source passed through the production direct
conversion and serializer with owned/foreign rows. The synthetic source is
not a Leadfeeder API response fixture. Rustfmt and diff checks are lightweight
source checks; compilation, test execution, and clippy remain required through
the shared admission gate before readiness or preflight replay.

Remaining acceptance includes the corrected Leadfeeder adapter integration,
real adapter-to-native serializer fixtures, and encrypted values traversing
both complete native search and person-research entrypoints. Neither this
source candidate nor the independent credential-presence metadata interface
establishes configured provider readiness or authorizes a production replay.

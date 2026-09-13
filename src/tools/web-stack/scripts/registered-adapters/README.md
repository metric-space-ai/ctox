# Registered adapter sources

`bundesanzeiger-de.cjs` is the versioned successor to the previously registered native
Bundesanzeiger revision 21 (trimmed UTF-8 SHA-256
`fe2fb95cd97780d1296f4f37394bb7272f0fd9bddc1eb452e55215e30211a2b8`).
It retains public result-table extraction and exact publisher matching. It does
not open protected publication documents or solve access challenges.

The adapter implements [query completion receipt v1](../../../../../docs/scrape-query-completion.md).
An explicit empty result page or inspected nonmatching publishers can produce
an empty completion receipt only after a successful response to the actual
submitted query. Home-page status, old queries, errors and unknown selectors
do not establish completion. The receipt does not claim exhaustive pagination
or certify absence of a company from the provider.

Query completion requires a matching full-document navigation, awaited from
before submission through DOMContentLoaded. A Chromium CDP observer binds the
actual network request and redirect chain to the response, loading completion
and current main-frame document loader, checked again after extracting rows.
Patchright's synthetic init-script response is not provider HTTP evidence.
AJAX-only responses, old or replaced documents, cached/service-worker responses
and unobservable network metadata are rejected. A future AJAX implementation
must establish current-result DOM binding before admitting a receipt.

Run the bounded, network-free adapter checks with temporary files on the
designated disposable volume:

```sh
TMPDIR=/Volumes/tmp/dev-artifacts/ctox/bundes-emitter \
  greppy bash-smart -- node --test --test-concurrency=1 \
  src/tools/web-stack/scripts/registered-adapters/bundesanzeiger-de.test.cjs
```

This file is not registered or activated automatically. After the native
receipt implementation and this adapter are reviewed and validated, use the
existing supported `ctox scrape register-script` operation, with a separate
regular staging file inside the target's allowed workspace. Preserve the
prior revision and verify the registered hash before one bounded current-query
run. Do not register it against a runtime that lacks the receipt contract.
Read back the current native result and persisted receipt; historical successful
records are not the result of an empty query. A `completed_empty` result proves
query completion, not extraction of all expected fields or all-adapter research.

# Grok client identity metadata: owned-source proposal

Status: proposed, not published or activated. The native CTOX host uses the
reviewed client version shipped with its release. It starts no background
metadata request.

## Runtime boundary

CLIProxyAPI upstream polls an npm registry for the Grok client version. CTOX
deliberately excludes that runtime dependency. Account authentication, live
model discovery, and inference still use the provider's actual API; this
proposal concerns only the client identity header.

The optional portable SDK capability targets this CTOX-owned public endpoint:

`GET https://llm.ctox.dev/v1/provider-client-metadata/xai`

The endpoint must serve a reviewed release-owned record, without fetching npm,
GitHub, a CDN, or another third-party metadata service while serving requests.
Upstream comparisons belong to development and release maintenance. An update
must preserve its immutable upstream source reference and review evidence.

The response contains no account identity, token, model selection, prompt, or
Business OS collection data:

```json
{
  "schema": "ctox.provider-client-metadata.v1",
  "provider": "xai",
  "version": "1.0.46"
}
```

The version shown is the reviewed release fallback, not an LLM model ID. The
handler should return `Content-Type: application/json`, an `ETag`, and
`Cache-Control: public, max-age=10800`. A source revision and publication time
may be included as public provenance, but are not substitutes for release
review.

## Consumer and failure behavior

The SDK binding remains absent by default. The native host does not bind the
fetcher until the owned endpoint is published and verified. There is no new
environment switch.

The optional fetcher requires the exact schema and provider envelope and a
strict numeric client version at or above the accepted server floor. A raw
registry response is rejected. Each fetch retains its ten-second deadline and
one-MiB body limit. Cancellation, replacement-owner protection, and the
three-hour refresh cadence remain guarded by the existing tests.

Absent a binding, or after a rejected response, the shared version reader
keeps the release fallback or last accepted value. Catalog and inference
headers read the same identity. API-key/custom-origin requests and explicit
account header precedence retain their existing behavior.

## Activation evidence required

A follow-up may bind the native capability only after the owned service is
actually available. Record the deployment revision, public GET result and
headers, exact reviewed metadata, malformed-response behavior, and installed
catalog/inference header checks. The published handler must not introduce a
runtime third-party fetch.

This proposal publishes no endpoint, changes no credentials or account
defaults, and proves no WELSCH login or installed model check. The accepted
CLIProxyAPI production pin and incomplete full-upstream parity remain
documented in `src/core/execution/cliproxyapi/PORTING.md`.

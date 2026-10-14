# Native API-key holder parity — implementation in progress

The private runtime topology supports API-key accounts for Z.ai platform/coding, MiniMax, xAI API, Kimi Code and Moonshot global/China. Credentials are references in `provider-api-keys`; the configuration never contains a key. Existing OAuth/default routes retain precedence. Accounts reuse the persistent session/quota selector, and an empty model list creates no wildcard route. Account and encrypted credential generations are revalidated around buffered/streamed publication.

## Live discovery and private installation seam

`cliproxyapi_integration::install_api_key_account_authorized` prepares an account through fixed provider `GET /models` endpoints and commits its selected endpoint plus observed IDs to the private runtime configuration. Kimi probes both regional Code bases (`api.kimi.com/coding/v1`, `api.kimi.ai/coding/v1`) and Moonshot platform bases (`api.moonshot.ai/v1`, `api.moonshot.cn/v1`), without a key-prefix guess or user plan choice. All candidates are checked; successful Code discovery takes precedence. Inference subsequently uses the stored endpoint, with no silent endpoint fallback.

The transport uses CTOX's real client identity, refuses redirects and ambient proxy fallback, and bounds discovery to eight seconds per endpoint. HTTP failures retain only received status and classified error; transport, timeout and retired-authority failures do not invent upstream credential rejection. Failure receipts identify the checked endpoints without echoing keys or upstream error bodies. Catalogs are bounded and validated; duplicate IDs are removed in order, and a key echoed as a model ID is rejected. Catalog success is not an inference or quota check.

The caller must supply its retained native Owner/Admin authority. No Policy transaction or lifecycle mutex may span provider I/O. Authority is checked before and after discovery, watched while requests run, and checked again at private credential/configuration commit. A server-generated account/secret identity never overwrites an existing key. A rejected configuration commit removes only that new, exact encrypted credential generation. Existing accounts, sessions and the default model are preserved.

This is a private seam, not a renderer RPC or public account projection. The actual authenticated Owner/Admin ingress adapter, native registry adoption/public account metadata, enable/remove and model checks still need integration in this PR. No user-visible Workjet account-management delivery or installed acceptance is claimed. The retained authority callback is not itself proof that those missing adapters exist.

## Validation and delivery

Isolated regressions cover all endpoint probes, Code/platform precedence, received error provenance, key-echo rejection, revocation, encrypted commit and selective rollback. Model names reuse existing port fixtures; no static production model, suggestion or default is introduced. Source qualification is recorded per head through the gpu3 lane. Supervisor alone merges and the existing release writer installs; real Workjet/provider acceptance follows delivery.

Endpoint reference: [official Kimi Code API and platform comparison](https://www.kimi.com/code/docs/en/#api-access). No new runtime environment switch, listener, model identifier or third-party metadata service is added.

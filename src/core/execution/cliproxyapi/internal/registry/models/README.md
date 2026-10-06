# Pinned static model catalog

`models.json` is the byte-for-byte source snapshot of
`internal/registry/models/models.json` from CLIProxyAPI v8.0.14 commit
`16d98881d4bb37adaa827599e4be8f5154e81646` (MIT).

The upstream file has SHA-256
`3a97eea65c1df3ea8ad4edac838b37f7714868d1e784b3723d0650b6e848aa9a`
and Git blob `228ef1c7319210501eea759572404daf7fe6a9ee`.
The Rust guard hashes `trim_end()`; its normalized SHA-256 is
`8fad5ea79ca3a61dc4dfca72338ec4cd966218c3f9f62c6b074233d4af9579e6`.

The v8.0.14 delta restores two Antigravity Claude4.6 records with 200,000-token
contexts, 64,000-token completions and 1,024–64,000 thinking budgets. The existing
Claude5.5 records and all other provider/tier catalogs are unchanged.
This snapshot is candidate source, not a promotion of the accepted production pin.

When advancing the snapshot, replace the asset from the exact new commit and
update the full-catalog hash/channel and provider-capability guards in
`model_definitions_test.rs`. Run the management-models differential probe against
the same candidate Go commit; historical accepted-pin results do not prove
current snapshot or code-defined Codex/xAI built-in parity.

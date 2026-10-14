# Candidate: preserve plugin configuration defaults

Upstream v8.0.29 (`5fc580b1d72d87a7bcfe5cfb6f479c1aea5625b3`,
`sdk/pluginapi/types.go:65-66`) adds `Default any` to `ConfigField`.
Previously Rust discarded that property during metadata decoding, so consumers
could not read plugin-declared defaults.

The Rust DTO now preserves the JSON value as `default_value`. Its wire name is
`Default`, matching the untagged Go field. The property is always serialized:
an omitted input or explicit null becomes `Default:null`, and false, zero,
empty strings, arrays and objects remain actual default values.

Eleven shared fixture cases exercise metadata round-trips. The qualification
also executes those cases against the actual pinned Go v8.0.29 type using a
Go overlay over the existing dependency checkout; the overlaid source is
byte-compared against the immutable upstream Git object before compilation.
This tests the changed wire contract, not a reimplementation of it in Go.

This slice changes SDK metadata only. It adds no model IDs, runtime fetches,
environment settings or renderer code. It does not promote the accepted
upstream pin or establish complete v8.0.29 compatibility.

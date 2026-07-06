# CTOX Communication Adapter Developer Guide

Status: Draft for the native chat-adapter expansion.

This guide defines the implementation contract for native CTOX communication
adapters. It complements the operator-focused runbook in
`docs/communication-adapter-operator-runbook.md`.

## Scope

Use this guide when adding or changing a native adapter for channels such as
Slack, Discord, Telegram, Matrix, Mattermost, Zulip, Google Chat, Signal,
Rocket.Chat, XMPP or IRC.

Existing first-party adapters are:

- `src/core/communication/email_native.rs`
- `src/core/communication/whatsapp_native.rs`
- `src/core/communication/teams_native.rs`
- `src/core/communication/jami_native.rs`
- `src/core/communication/meeting_native.rs`

The new bot-chat adapters currently share code in
`src/core/communication/chat_native.rs`. A provider-specific split is allowed
later only if it reduces real complexity and preserves the same contract.

## Required Contract

Every native adapter must provide these typed operations:

- `test`: validate authentication and minimum provider reachability.
- `sync`: fetch inbound messages or provider events and persist CTOX records.
- `send`: send a reviewed outbound message to the provider.

Adapters must normalize inbound data into the existing communication tables via
`communication_accounts`, `communication_threads`, `communication_messages`,
sync-run evidence and routing rows. Browser-facing state is projected through
the CTOX Sync Engine / RxDB / WebRTC data plane.

## Data Boundary

Adapter code must not introduce a browser HTTP data bridge for Business OS
records. HTTP may serve static shell assets, bootstrap configuration, status,
auth and explicit control-plane endpoints, but not browser data replication.

Provider tokens, signing secrets, app tokens, bot tokens and API keys must stay
server-side. Browser code may display redacted status and remediation text, but
must not store provider secrets in replicated collections.

Production runtime behavior must not depend on new process-environment feature
toggles. Runtime settings belong in typed config, the SQLite runtime store or
the CTOX secret/runtime paths already used by `communication::gateway`.

## Message Identity

Use stable provider IDs for `remote_id` and deterministic `message_key` values.
The key should include the account and provider message identifier. Thread keys
must encode enough provider context to route replies:

- Slack: channel plus root `thread_ts`.
- Discord: channel, and message reference when available.
- Telegram: chat ID, and reply message ID for replies.
- Matrix: room ID and event ID.
- Mattermost: channel plus root post ID.
- Zulip: stream plus topic, with stable message IDs across topic moves.
- Google Chat: space plus thread name.

Never use display names as the only stable identity.

## Status And Errors

Provider-specific failures must become durable `adapterStatus` fields on the
account profile. At minimum, classify:

- `deauthorized`
- `missing_scope`
- `missing_permission`
- `missing_intent`
- `rate_limited`
- `failed`

When a provider returns Retry-After or equivalent rate-limit data, persist
`rate_limited_until_ms`. When a real-time transport is planned or present,
expose `realtime_transport`, `realtime_config_state`,
`realtime_supervision_state`, `realtime_cursor_state_key`,
`realtime_last_cursor` and `realtime_backoff_until_ms`.

## Sync And Realtime

Pull-sync adapters must persist a high-water cursor per account or per
destination. WebSocket, gateway or long-poll adapters must be bounded by CTOX
service supervision and expose reconnect/backoff state.

Slack Socket Mode is the reference for a private-instance realtime adapter
without an unbounded daemon: service sync opens the provider connection, runs a
short capped WebSocket cycle, acknowledges provider envelopes, persists only
allowlisted message events, records cursor/backoff/supervisor state, and then
closes the socket.

If an event API delivers update-only events, do not force them through the
normal message upsert if that would clear existing fields. Apply targeted row
updates instead. Zulip `update_message` events are the reference pattern:
content edits update only `message_id`, while topic/channel moves update all
`message_ids` without overwriting existing message bodies.

## Outbound Safety

External sends must go through the existing review/approval gate before the
provider call. Unreviewed sends must fail before any provider request. After a
successful provider send, persist provider response IDs in the outbound
communication record and mark the approval evidence as consumed.

Bot-chat adapters are text-only in v1. Attachments must be rejected with a clear
reason until provider-specific upload, MIME, size, persistence and security
review paths are implemented.

## Business OS Integration

Settings, pairing, test, disconnect and send actions must use server-side
Business OS commands and policy checks. Do not add UI-only permission checks for
actions that mutate server state.

Browser UI may mirror status and remediation hints from `adapterStatus`, but
server-side policy and persistence remain authoritative.

## Tests

For a new adapter, add the narrowest useful tests for:

- runtime settings and missing-config behavior;
- `test` success and provider probe failure classification;
- inbound normalization, stable message keys and stable thread keys;
- cursor or event sequence persistence;
- reviewed send success and unreviewed send block;
- provider response ID/audit evidence persistence;
- rate-limit and deauthorization classification;
- attachment rejection for v1 bot-chat adapters;
- Business OS status projection when relevant.

Fake-provider tests must not require real tokens. Real-provider smoke reports
belong in release evidence with secrets redacted.

## Verification

Choose checks by blast radius. Common narrow checks:

```sh
rustfmt src/core/communication/chat_native.rs
CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox fake_provider_smoke_covers_all_bot_chat_adapters
CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox provider_error_classification_maps_common_auth_scope_and_rate_states
node --check src/apps/business-os/shared/react-settings.js
```

For Business OS / RxDB data-plane changes, also run the relevant RxDB and core
RxDB checks from `AGENTS.md`.

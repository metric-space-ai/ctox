# Business OS Support

Support is a CTOX-native support desk module over the existing Business OS data
plane. It uses RxDB/WebRTC collections, `ctx.commandBus.dispatch`, and native
Rust `support.*` command handlers; it does not proxy support data over HTTP.

The module reads communication threads/messages, customer records, ticket
cases, Business Chat, queue tasks, and Support-owned projections. Mutations are
typed commands and are validated by the native Business OS backend before
records are projected back into RxDB.

Implementation details and phase status live in
`docs/business-os-support-app-implementation-plan.md`.

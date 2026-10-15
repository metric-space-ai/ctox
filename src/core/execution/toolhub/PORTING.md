# Porting Activepieces actions to CTOX Toolhub

DE: Leitfaden für die Portierung von Activepieces-Pieces (MIT) nach Rust.
EN: Guide for porting Activepieces pieces (MIT) to Rust.

## Pipeline
1. `tools/toolhub-catalog/extract.js <activepieces>/packages/pieces/community out.json`
   reads every piece mechanically (TypeScript AST): auth (OAuth2 authUrl/tokenUrl/scope,
   API key, basic, custom props), actions/triggers with props -> JSON Schema, logo, categories.
2. `store-catalog.json` (slim, piece level) feeds the Workjet store; only toolkits that
   are registered in `toolkits::builtin()` are shown as connectable (`implemented: true`).
3. Per service one file `src/toolkits/<slug>.rs` implementing `ToolkitAdapter`
   (pure `build_request` / `parse_response`, no I/O, no credentials; the hub injects auth).
4. Per service one test file `tests/port_<slug>.rs` using `tests/support`:
   - one recorded `Case` per tool (request expectations derived from the TS `run()`),
   - `assert_full_coverage` so no tool ships untested,
   - an `#[ignore]` live test calling read-only tools against the real API with
     `CTOX_TOOLHUB_LIVE_<SLUG>_TOKEN` (never printed). Run:
     `CTOX_TOOLHUB_LIVE_GMAIL_TOKEN=… cargo test --test port_gmail -- --ignored`.

## Rules
- Tool slugs: `<SERVICE>_<VERB>_<OBJECT>` upper snake case, Composio style.
- Mutating tools set `mutating: true`; the CLI requires `--confirm` for them.
- Path parameters go through `path_segment`; query values are URL-encoded.
- Dynamic dropdowns from Activepieces become plain id strings (+ a list tool).
- AI-assisted pieces actions and `custom_api_call` are not ported.
- Definition of done for an action: recorded test passes AND one live call succeeded.
  Mock-only actions are reported as "not done".

# Pi Runtime Upstream Pin

The CTOX pi-code coding sidecar embeds the real Pi coding-agent core as a
pinned dependency, not as visual inspiration. It adapts the pinned Pi packages
to CTOX's server-side authorization, the Business OS app-source store, and the
P0 commit-based review model.

The Pi CLI and TUI are out of scope: only the headless coding-agent core is
embedded, driven programmatically over LocalTransport by the native Rust owner
in `src/core/business_os` / `src/core/coding_agents`.

- Upstream: `https://github.com/earendil-works/pi`
- CTOX fork target: consumed as pinned npm packages (no source fork)
- Audited upstream tag: `v0.80.2`
- Audited upstream commit: `0201806adfa825ab3d7957a4267d46e5030fd357`
- Runtime dependency: `@earendil-works/pi-agent-core@0.80.2`
- AI protocol dependency: `@earendil-works/pi-ai@0.80.2`
- Coding-agent dependency: `@earendil-works/pi-coding-agent@0.80.2`

Ported core surface:

- Threaded agent runs through `runAgentLoop` (`@earendil-works/pi-agent-core`)
- Pi message / event / stream protocol (`@earendil-works/pi-ai`)
- Coding-agent tools through the upstream factories: `read`, `bash`, `edit`,
  `write`, `grep`, `find`, `ls`
- A single-turn API modelled on `runVercelPiCodingAgentTurn`

Explicitly excluded:

- `pi-tui` and the CLI product shell
- Host shell / process / filesystem access and generic network tools
- Pi's own provider fan-out (CTOX routes the stream through its model gateway)

Provenance: derived from `MRP-learn-buddy/packages/agent-runtime` (same pinned
Pi packages, same headless approach), re-targeted from the Vercel virtual
filesystem to the CTOX Business OS app-source store.

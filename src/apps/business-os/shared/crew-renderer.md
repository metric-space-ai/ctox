# Shared crew renderer

`crew-renderer.js` extracts the existing Business OS creature presentation from `business-chat.js`. It has no imports, DOM access, timers, network or persistence. It is browser ESM; do not import the chat runtime just to display a creature.

- `normalizeCrewAppearance(appearance)` preserves explicit name, six-digit hex colour and one of the existing round/blob/square/triangle shapes. Missing or invalid values retain the existing neutral Crew fallback.
- `renderCrewCreature({ appearance, animationKey, taskState, mode, placement, progressPercent, activity })` returns the existing decorative SVG wrapper. The caller supplies the resolved state and telemetry (`total`, `lastKind`, `updatedAt`). No actor, permission or identity is inferred from them.
- `CREW_CREATURE_CSS` supplies the original base styles and reduced-motion behavior for a standalone host. Load it once in that host; give the wrapper its layout dimensions. `CREW_CREATURE_BASE_CSS` is exposed for the existing chat adapter, which inserts those exact bytes at their previous cascade position and retains its existing broader reduced-motion rules.

The chat adapter still owns `crewIdentityKey`, task/progress/mode resolution, activity telemetry, presence, procedural activity animation and all application state. Its `crewCreatureHtml` export remains compatible. The extracted local rendering functions are removed from that adapter.

`animationKey` is only an input to the existing motion hash. In the legacy chat it may come from a chat, command or task reference. It is **not** a WorkerProfileId mapping, an appearance allocator or an authority token. Workjet must supply the confirmed profile/appearance association through its existing authoritative model. This extraction does not establish that missing contract or publish a Workjet package.

The fixture records SHA-256 references for 360 pre-extraction outputs from CTOX commit `e00ecbeb131808797aafb04fad5862a8ea17aa4c`: four shapes, nine task/expression states, five placements and work/review progress. `crew-renderer.test.mjs` checks both the pure renderer and the legacy adapter against those references, plus the original CSS bytes, neutral fallbacks and escaped caller values. Existing procedural-motion guards continue to check the composed chat stylesheet.

Focused validation: `node --test --test-concurrency=1 --test-name-pattern='crew|creature' src/apps/business-os/shared/crew-renderer.test.mjs src/apps/business-os/shared/business-chat.test.mjs` (17 passed). This is not a live Desktop/Mobile, animation-performance or full Sync/Command acceptance. No shell slot or tenant has been deployed from this extraction branch.

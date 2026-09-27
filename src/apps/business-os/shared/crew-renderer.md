# Crew creatures: genome renderer and motion engine

Two browser ESM files, no imports from the chat runtime:

- `crew-renderer.js` — pure. No DOM access, timers, network, persistence or randomness.
- `crew-motion.js` — the one page-wide motion engine (browser only).

## Appearance is generated, not drawn

A member's look comes from a **genome** (`crewGenome(appearance)`), seeded by the member id (without an id: its name). The owner sets two genes — archetype (`round | blob | square | triangle`, the `crew_members.shape` column) and colour — everything else is derived deterministically:

- outline: superellipse with low-frequency wobble (round, square), lobed cloud (blob), rounded polygon with per-corner radii, side bulges and apex offset (triangle); width, height, underside flattening and lean
- face: eye line height, eye spacing from the body width at that line, eye size, stroke, slant, gaze offset, slight asymmetry
- tone: the owner colour with an individual hue/saturation/lightness shift (grey stays grey), a darker shade and a cheek tint
- extras: shine, optional cheeks, optional tuft (not on triangles)
- motion temperament: `tempo`, `amplitude`, `irregularity`, `phase` (written to `data-crew-motion`)

The same member therefore looks identical in the chat bar, chat windows, the CTOX map, tickets and app badges, and two members of the same archetype and colour are never twins. No schema change: the genome is a pure function of the authoritative `id`, `shape` and `color`.

Without a member (`appearance` missing or unnamed) the creature is the **neutral crew ghost** (`is-neutral`: dashed outline, translucent body) — never mistaken for a member, not even for a grey one.

`renderCrewCreature({ appearance, animationKey, taskState, mode, placement, progressPercent, activity })` returns

```
span.ctox-crew-creature[data-crew-mode, data-crew-key, data-crew-motion, data-activity-*]
  span.ctox-crew-ground          soft coloured ground shadow (no filter)
  svg.ctox-crew-figure           tuft, body, shine, cheeks, eyes
```

`CREW_CREATURE_BASE_CSS` is the only creature stylesheet (hosts size the wrapper and nothing else); `CREW_CREATURE_CSS` adds the reduced-motion rules for standalone hosts. The stylesheet has no keyframe loops and no per-frame filters.

## Motion is procedural and alive

`crew-motion.js` observes the whole document (MutationObserver) and animates every creature from three layers:

1. **base pose** — continuous, per state, shaped by the temperament: sleeping breathes, working bobs and sways, review peers and scans, reading scans lines, learning floats, failed slumps, anything else waits (breathes, looks around)
2. **impulse** — a finite gesture triggered only by durable telemetry: a hop for a tool turn, a tilt for a thinking turn, a sway in review (`data-activity-turns` increases, or a fresh first event ≤ 8 s); plus a stretch when waking, a shake when failing, a cheer when learning
3. **face** — blinks and glances on the eyes group

Mode changes blend from the last pose (480 ms). Only creatures that are on screen in a visible tab are animated (IntersectionObserver, `document.hidden`); `prefers-reduced-motion` clears every transform and stops the loop. The body moves as one element (`svg.ctox-crew-figure`, `will-change` only while visible), so per-frame work stays on the compositor; eye moves are quantised so a held glance costs nothing. One engine per page (`window.__ctoxCrewMotionEngine`), even if the module is loaded under several URLs.

`syncCrewMotion(root)` makes pickup immediate after an in-place render; `business-chat.js` keeps `syncCrewProceduralMotion` as a compatible wrapper. `crewMotionSnapshot()` is the acceptance view (`scripts/assert-ctox-crew-map.mjs`).

## Validation

- `node --test shared/crew-renderer.test.mjs shared/business-chat.test.mjs` — determinism, no twins, frame/face bounds, archetype and colour fidelity, neutral ghost, faces per mode, escaping, motion contract.
- `node scripts/assert-ctox-crew-map.mjs` — browser: durable tool turn → finite gesture, waiting creature gets none, gesture settles back into the working pose, reduced motion stops everything.
- `scripts/crew-gallery.html` — visual gallery (archetypes × individuals × states × sizes, turn buttons); serve the repo root statically and open it.

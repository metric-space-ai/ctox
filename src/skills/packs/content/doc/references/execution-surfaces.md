# Documents: operation-to-surface map and gating status

Source of truth for feature status:
`src/apps/business-os/office-engine/features.json` (statuses: discovered →
oracle_captured → frontend_ported → rust_ported → differential_passed →
shipped). A task class is usable when its feature group is `shipped`;
`differential_passed` allows use behind the same rollout flag as the editor
itself. Planned engine ops are usable when the op exists in
`ctox-office-engine` (CLI) / `business_commands`.

Update this file when features.json statuses change or ops land; do not let
the skill promise operations the build cannot perform.

## Editor-flow surface (layout-affecting)

| Operation class | Feature group | Status (2026-07-11, re-baselined) |
|---|---|---|
| Open, render pages, zoom (visual QA) | `document.open-render-zoom` | oracle_captured |
| Author/edit text, save | `document.edit-save` | oracle_captured |
| Undo, clipboard, keyboard | `document.undo-clipboard-keyboard` | oracle_captured |
| Character/paragraph formatting | `document.character-paragraph-formatting` | oracle_captured |
| Styles, lists, real numbering | `document.styles-lists-numbering` | oracle_captured |
| Tables (create, geometry, layout) | `document.tables` | oracle_captured |
| Images and positioning | `document.images-positioning` | oracle_captured |
| Sections, headers, footers | `document.sections-headers-footers` | oracle_captured |
| Links, bookmarks, fields, TOC | `document.links-bookmarks-fields` | oracle_captured |
| Tracked changes / comments in context | `document.comments-track-changes` | oracle_captured |
| Drawings and charts | `document.drawings-charts` | oracle_captured |

Verify with `node src/scripts/check-office-skill-gating.mjs` after edits.

## Native batch operations (deterministic OOXML, planned ctox-office-engine ops)

| Operation | Op (planned name) | Status (2026-07-11) |
|---|---|---|
| Inspect package (manifest, parts, structure) | `inspect` | available |
| Export (byte-preserving round-trip/merge) | `export` | available |
| Accept all tracked changes | `tracked-changes-accept` | **available** |
| Reject all tracked changes (refuses `*PrChange`) | `tracked-changes-reject` | **available** |
| Insert tracked replacements | `tracked-changes replace` | planned |
| Extract comments (text, author, resolved state) | `comments-extract` | **available** |
| Add a comment (anchored by paragraph text) | `comments-add` | **available** |
| Resolve comments (one or all) | `comments-resolve` | **available** |
| Strip all comments (parts, rels, anchors) | `comments-strip` | **available** |
| Privacy scrub (authors, rsid, custom props) | `privacy-scrub` | **available** |
| Layout-preserving redaction (terms, emails, phones) | `redact` | **available** (single-run matches) |
| Edit protection (readonly/comments/forms/none) | `protection-set` | **available** |
| Merge/append documents | `merge` | planned |
| Accessibility audit (alt text, heading ladder, table headers) | `a11y-audit` | **available** (safe fixes planned) |
| Style lint (fake bullets, fake headings) | `style-lint` | **available** (normalize planned) |
| Fields report (instructions + cached results) | `fields-report` | **available** (materialize planned) |
| Watermark add / audit / remove | `watermark add\|audit\|remove` | planned |
| Table → CSV export | `table-export` | **available** (import planned) |

## Known coverage gaps fed back to the office port

Operations used by document workflows with no feature group and no planned op
decision yet (candidates for new feature groups or explicit Ebene-B
decisions): content controls / forms (SDTs), footnotes/endnotes, TOC/field
materialization interplay with deterministic rendering.

See `docs/ctox-office-skills-adaptation-plan.md` for the adaptation rationale.

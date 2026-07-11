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

| Operation class | Feature group | Status (2026-07-11) |
|---|---|---|
| Open, render pages, zoom (visual QA) | `document.open-render-zoom` | differential_passed |
| Author/edit text, save | `document.edit-save` | differential_passed |
| Character/paragraph formatting | `document.character-paragraph-formatting` | differential_passed |
| Styles, lists, real numbering | `document.styles-lists-numbering` | differential_passed |
| Tables (create, geometry, layout) | `document.tables` | differential_passed |
| Images and positioning | `document.images-positioning` | differential_passed |
| Sections, headers, footers | `document.sections-headers-footers` | differential_passed |
| Links, bookmarks, fields, TOC | `document.links-bookmarks-fields` | differential_passed |
| Tracked changes / comments in context | `document.comments-track-changes` | differential_passed |
| Drawings and charts | `document.drawings-charts` | differential_passed |

## Native batch operations (deterministic OOXML, planned ctox-office-engine ops)

| Operation | Op (planned name) | Status (2026-07-11) |
|---|---|---|
| Inspect package (manifest, parts, structure) | `inspect` | available |
| Export (byte-preserving round-trip/merge) | `export` | available |
| Accept/reject all tracked changes | `tracked-changes accept\|reject` | planned |
| Insert tracked replacements | `tracked-changes replace` | planned |
| Add / extract / resolve / strip comments | `comments add\|extract\|resolve\|strip` | planned |
| Privacy scrub (authors, rsid, custom props) | `privacy-scrub` | planned |
| Layout-preserving redaction | `redact` | planned |
| Edit protection (read-only/comments/forms) | `protection set\|clear` | planned |
| Merge/append documents | `merge` | planned |
| Accessibility audit (+ safe fixes) | `a11y-audit` | planned |
| Style lint / conservative normalize | `style lint\|normalize` | planned |
| Structure audits (headings, sections, images, footnotes, fields) | `audit <kind>` | planned |
| Field materialization (SEQ/REF display text) | `fields materialize` | planned |
| Watermark add / audit / remove | `watermark add\|audit\|remove` | planned |
| Table ↔ data conversion (docx table ↔ csv, xlsx → docx table) | `table export\|import` | planned |

## Known coverage gaps fed back to the office port

Operations used by document workflows with no feature group and no planned op
decision yet (candidates for new feature groups or explicit Ebene-B
decisions): content controls / forms (SDTs), footnotes/endnotes, TOC/field
materialization interplay with deterministic rendering.

See `docs/ctox-office-skills-adaptation-plan.md` for the adaptation rationale.

# Documents: operation-to-surface map and gating status

Source of truth for feature status:
`src/apps/business-os/office-engine/features.json` (statuses: discovered →
oracle_captured → frontend_ported → rust_ported → differential_passed →
shipped). A task class is usable when its feature group is `shipped`;
`differential_passed` allows use behind the same rollout flag as the editor
itself. Planned engine ops are usable when the op exists in
`ctox-office-engine` (CLI). The browser editor uses the separate typed
`office.document.prepare|commit|export` lifecycle commands. Native batch ops
must not be described as Business OS commands until they are registered in
the server-authoritative command inventory and policy.

Update this file when features.json statuses change or ops land; do not let
the skill promise operations the build cannot perform.

## Editor-flow surface (layout-affecting)

| Operation class | Feature group | Status |
|---|---|---|
| Open, render pages, zoom (visual QA) | `document.open-render-zoom` | differential_passed |
| Author/edit text, save | `document.edit-save` | differential_passed |
| Undo, clipboard, keyboard | `document.undo-clipboard-keyboard` | differential_passed |
| Character/paragraph formatting | `document.character-paragraph-formatting` | differential_passed |
| Styles, lists, real numbering | `document.styles-lists-numbering` | differential_passed |
| Tables (create, geometry, layout) | `document.tables` | differential_passed |
| Images and positioning | `document.images-positioning` | differential_passed |
| Sections, headers, footers | `document.sections-headers-footers` | differential_passed |
| Links, bookmarks, fields, TOC | `document.links-bookmarks-fields` | differential_passed |
| Tracked changes / comments in context | `document.comments-track-changes` | differential_passed |
| Drawings and charts | `document.drawings-charts` | differential_passed |
| Full DOCX round-trip corpus (release gate) | `document.docx-roundtrip-corpus` | differential_passed |

Verify with `node src/scripts/check-office-skill-gating.mjs` after edits.

## Native batch operations (deterministic OOXML)

| Operation | Op | Status |
|---|---|---|
| Inspect package (manifest, parts, structure) | `inspect` | available |
| Prepare source package as editor payload | `prepare-editor` | available |
| Inspect a prepared editor payload | `inspect-editor` | available |
| Export (byte-preserving round-trip/merge) | `export` | available |
| Accept all tracked changes | `tracked-changes-accept` | **available** |
| Reject all tracked changes (refuses `*PrChange`) | `tracked-changes-reject` | **available** |
| Insert tracked replacements (simple runs; complex reported) | `tracked-changes-replace` | **available** |
| Extract comments (text, author, resolved state) | `comments-extract` | **available** |
| Add a comment (anchored by paragraph text) | `comments-add` | **available** |
| Resolve comments (one or all) | `comments-resolve` | **available** |
| Strip all comments (parts, rels, anchors) | `comments-strip` | **available** |
| Privacy scrub (authors, rsid, custom props) | `privacy-scrub` | **available** |
| Layout-preserving redaction (terms, emails, phones) | `redact` | **available** (single-run matches) |
| Edit protection (readonly/comments/forms/none) | `protection-set` | **available** |
| Merge/append documents (refuses media/hyperlink/comment rels) | `merge-append` | **available** |
| Accessibility audit (alt text, heading ladder, table headers) | `a11y-audit` | **available** |
| Accessibility safe fixes (alt from name, header rows) | `a11y-fix` | **available** |
| Style lint (fake bullets, fake headings) | `style-lint` | **available** |
| Style normalize (clear heading-style overrides) | `style-normalize` | **available** |
| Fields report (instructions + cached results) | `fields-report` | **available** |
| Field materialization (REF/PAGEREF/SEQ; PAGE stays live) | `fields-materialize` | **available** |
| Watermark audit / add / remove (VML header objects) | `watermark-audit\|add\|remove` | **available** |
| Table ↔ CSV (export nth table, import with geometry + header row) | `table-export\|import` | **available** |

## Known coverage gaps fed back to the office port

Operations used by document workflows with no feature group and no planned op
decision yet (candidates for new feature groups or explicit Ebene-B
decisions): content controls / forms (SDTs), footnotes/endnotes, TOC/field
materialization interplay with deterministic rendering.

See `docs/ctox-office-skills-adaptation-plan.md` for the adaptation rationale.

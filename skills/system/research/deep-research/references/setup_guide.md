# Deep Research Skill — Setup Guide

This guide walks an operator from "I have a topic" to "I have a delivered
DOCX" for any of the seven supported report types. The deep-research skill
is a manager-loop architecture: a single LLM manager orchestrates one
workspace tool, one asset tool, one user-rescue tool, one mandatory
public-research tool, three sub-skills (Block Writer, Revision, Flow
Review), and four loop-end gates. The manager never writes prose itself.
The host (Rust side) downgrades any premature `decision: "finished"` from
the manager when one of the four gates is unsatisfied, so a clean run
literally cannot ship a half-baked report. Read `SKILL.md` and
`references/manager_path.md` once before running your first study; the
table-of-contents below is the operator surface, not the architecture
surface.

## Prerequisites

CTOX itself must be installed and `ctox doctor` must report healthy. The
skill ships its own asset pack and reference markdown, but the host needs
a few things on the OS that CTOX cannot provision for you:

- **Python 3.11 or newer** with the following packages reachable on the
  same interpreter the renderer is invoked from:
  - `python-docx >= 1.1` — DOCX writing.
  - `pdf2image >= 1.17` — DOCX visual review (drives `pdftoppm`).
  - `Pillow >= 10` — pulled in by `pdf2image`.

  Install (pick one form, not both):

  ```bash
  python3 -m pip install --user "python-docx>=1.1" "pdf2image>=1.17" "Pillow>=10"
  # or, if you manage Python with uv:
  uv pip install "python-docx>=1.1" "pdf2image>=1.17" "Pillow>=10"
  ```

  CTOX will not install these for you. The bundled scripts detect missing
  modules and emit an explicit install hint instead of crashing — see
  `Troubleshooting > DOCX rendering fails`.

- **LibreOffice** providing the `soffice` binary. The renderer lint
  scripts call it as `soffice --headless --convert-to pdf` to produce a
  PDF for visual review. On macOS:

  ```bash
  brew install libreoffice
  ```

  On Ubuntu / Yoda (production Linux host):

  ```bash
  sudo apt-get install -y libreoffice
  ```

- **Poppler** providing `pdftoppm` for PDF → PNG rendering during visual
  review:

  ```bash
  # macOS
  brew install poppler
  # Ubuntu
  sudo apt-get install -y poppler-utils
  ```

- **Outbound HTTPS** to the public-research backends. CTOX's research tool
  fans out to Crossref first, then OpenAlex, then arXiv, then a generic
  web fetch — all served by the CTOX web stack:
  - `api.crossref.org`
  - `api.openalex.org`
  - `export.arxiv.org`
  - `doi.org`

  If your environment blocks any of these (e.g. a corporate proxy), the
  research tool will return `research_empty` and the run will block. The
  bundled `scripts/doi_resolve.py` helper is the fastest way to verify
  that DOI resolution works end-to-end from the host you are sitting on.

- **A real corpus topic.** The resolver does not invent results. If the
  literature on your topic is genuinely thin, you will hit
  `research_empty` repeatedly and the manager will end the run with
  `decision: "blocked"`. In that case: broaden the topic, supply seed
  DOIs via `--seed-doi`, or accept that the report cannot be evidence-
  graded with the current public corpus. See
  `Troubleshooting > Manager keeps calling public_research`.

## Quickstart for a feasibility study

The feasibility study is the only report type that ships with a real
goldreferenz (the RASCON archetype, `domain_profile_id=ndt_aerospace`).
This is the smoothest first run. End-to-end command sequence:

```bash
# 1. Create a new run. The CLI prints a run_id; you use it for every later step.
ctox report new feasibility_study \
    --domain ndt_aerospace \
    --depth standard \
    --language de \
    --topic "Kontaktlose Pruefung des LSP-Kupfergitters in CFRP-Strukturen"
# -> Run created: r_2026_05_08_a1b2c3

# 2. Run the manager. This is the long step (5-25 minutes depending on depth).
ctox report run r_2026_05_08_a1b2c3
```

`ctox report run` streams the manager's tool calls and gate results to
stdout. The phases you will see are:

1. `bootstrap` — `workspace_snapshot` and `asset_lookup` resolve the
   `report_type`, `domain_profile`, `style_profile` triple and bind the
   block library for the run.
2. `evidence` — one or more `public_research` calls until the workspace
   carries at least `depth_profile.min_evidence_count` excerpts. For
   `standard` depth that is 12 sources; for `decision_grade` it is 20.
3. `drafting` — `write_with_skill` packets of up to 6 instance_ids each,
   followed by `apply_block_patch`.
4. `iteration` — `completeness_check`, `character_budget_check`,
   `release_guard_check`, `narrative_flow_check`. Any gate that returns
   `needs_revision=true` triggers a focused `revise_with_skill` call.
5. `finalisation` — gates re-run after the last patch. When all four
   report ready, the manager emits `decision: "finished"`.

The run will pause and end with `decision: "needs_user_input"` when the
sub-skill returns blocking_questions that the manager cannot answer
autonomously. Inspect the open questions and answer them:

```bash
ctox report status r_2026_05_08_a1b2c3 --json | jq '.open_questions'
# -> [
#      {"id": "Q1", "section": "component_layout",
#       "question": "Liegt zusätzlich eine geschlossene Metallfolie über dem Kupfergitter vor?",
#       "allow_fallback": false}
#    ]

ctox report answer r_2026_05_08_a1b2c3 \
    --question-id Q1 \
    --answer "Nein, nur das Kupfergitter; keine zusätzliche Folie laut Vorgabe."

ctox report continue r_2026_05_08_a1b2c3
```

`ctox report continue` resumes the manager loop with the freshly answered
question carried into the next `write_with_skill` brief. Repeat the
answer-and-continue cycle until the manager finishes.

When the run reports `decision: "finished"`, render the deliverable:

```bash
ctox report render r_2026_05_08_a1b2c3 --format docx --out report.docx
ctox report render r_2026_05_08_a1b2c3 --format md   --out report.md
```

Open the DOCX in Word or LibreOffice and skim it before forwarding. If
the table of contents reads "Inhaltsverzeichnis" but is empty, right-click
the field and choose "Feld aktualisieren" — the renderer emits the TOC
field but Word fills it on first open.

## Quickstart for the other six report types

Every report type uses the same CLI pattern; only the `report_type` first
positional argument and the recommended `--domain` / `--depth` / `--language`
defaults change. The table below is the operator's at-a-glance map.

| `report_type_id`         | Recommended domain                 | Default depth     | Language | Typical chars | Min sections | Real-evidence hint                                                                                                |
| ------------------------ | ---------------------------------- | ----------------- | -------- | ------------- | ------------ | ----------------------------------------------------------------------------------------------------------------- |
| `feasibility_study`      | `ndt_aerospace` (or pick another)  | `standard`        | `de`     | ~30 000       | 9            | Layup details, defect classes, access geometry. Hand the manager any internal datasheet that scoping requires.    |
| `market_research`        | `materials_method_assessment`      | `standard`        | `de`/`en`| ~28 000       | 8            | Hand-supplied DOIs of analyst reports help. Specify target geography in the topic; otherwise the run hedges.       |
| `competitive_analysis`   | `manufacturing_process` or custom  | `standard`        | `de`/`en`| ~22 000       | 7            | Name the named competitors in the topic. The manager does not invent competitors; it scores those you anchor.      |
| `technology_screening`   | `materials_method_assessment`      | `orienting`       | `de`     | ~14 000       | 6            | Spell out the screening question. The manager runs a longlist → matrix → shortlist arc; vague questions blur it.   |
| `whitepaper`             | matches the topic                  | `orienting`       | `de`/`en`| ~18 000       | 5            | A clear thesis is required. Without one, the writer blocks on `Q: Was ist die These dieses Whitepapers?`.          |
| `literature_review`      | matches the topic                  | `decision_grade`  | `de`/`en`| ~22 000       | 6            | Wide is good. State the year window in the topic ("seit 2018") so `public_research` filters the corpus correctly. |
| `decision_brief`         | matches the topic                  | `orienting`       | `de`/`en`| ~8 000        | 5            | Criteria + weights must be supplied. Otherwise the manager invents a generic option grid that misses the decision. |

The `--depth` flag picks one of `orienting`, `standard`, `decision_grade`.
The depth profile shapes:
- `min_sources` — research floor before any writer call (6 / 12 / 20).
- `min_methods_screened` — only relevant for `feasibility_study` and
  `technology_screening`.
- `scenario_branches_required` — only relevant for `feasibility_study`.
- Generation tone: `orienting` may stay qualitative; `decision_grade`
  requires semi-quantitative axes wherever sources allow.

Pick `decision_grade` only when the deliverable has to support a real
investment / certification / regulatory decision. The depth multiplier on
research budget is large (20 sources vs. 12) and the run takes
proportionally longer.

## Supplying auftrag context

Beyond the topic string, the operator can hand the manager structured
context. Each is optional, but every piece you supply moves the manager
out of "guess from the topic" mode and into "constrain the run".

- **Reference documents.** DOCX/PDF documents the manager should read
  verbatim into the evidence packet. The host treats the extracted text
  as `kind: "operator_reference"` evidence — it never appears in the
  references list as a public source, but it does anchor the writer's
  facts.

  ```bash
  ctox report new feasibility_study \
      --topic "..." \
      --reference-doc /path/to/internal_layup_spec.pdf \
      --reference-doc /path/to/test_protocol.docx
  ```

- **Seed DOIs / URLs.** A list of canonical references the manager
  should resolve and prioritise during the evidence pre-phase. The
  Crossref-first resolver normalises every DOI before adding it to the
  research provenance.

  ```bash
  ctox report new market_research \
      --topic "EU mid-market cloud DLP 2025" \
      --seed-doi 10.1109/ACCESS.2024.3480112 \
      --seed-doi 10.1145/3633472 \
      --seed-url https://www.gartner.com/en/documents/4012345
  ```

  Status: `--seed-doi` and `--seed-url` are wired in the CLI surface.
  Verify locally with `ctox report new --help` before scripting them into
  a pipeline.

- **User notes.** Free-text scoping notes that the writer reads on every
  call:

  ```bash
  ctox report note r_xxxx "Fokus: nur 1-seitiger Zugang. CFK-Bauteile mit Surfacer-Lack-Aufbau."
  ```

- **Review feedback.** A reviewed DOCX with comments. The host extracts
  the comments, matches them to instance_ids, and feeds them into the
  next run as `review_feedback`. This is how iteration on a previous run
  works — see `Iterating on a finished run` below.

  ```bash
  ctox report review r_xxxx --review-doc /path/to/reviewed_report.docx
  ```

  Status: `--review-doc` is intended; the operator must verify locally
  whether comment extraction is wired in your CTOX build. If
  `ctox report review --help` shows the flag, it is wired. If not, fall
  back to file-attaching the DOCX as a `--reference-doc` and writing the
  reviewer's points into `ctox report note`.

## Reading the manager output

The manager always emits a final JSON envelope. `ctox report status RUN_ID --json`
returns it. The five fields the operator actually reads:

- `decision` — one of `finished`, `needs_user_input`, `blocked`. This
  is the headline. The host loop-end gate may downgrade `finished` to
  `blocked` after the LLM has emitted it; the downgrade is what makes
  the four checks binding.
- `summary` — one-paragraph human description of the run state. When
  `decision == "blocked"` after `finished`, the summary names the
  failing check qualified by the run's `report_type_id`, e.g.
  `release_guard_check (LINT-DUPLICATE-MATRIX-RATIONALE) on competitive_analysis`.
- `changed_blocks[]` — the instance_ids the most recent
  `apply_block_patch` committed. Useful for iterating: you can re-render
  only the changed blocks against a previous version.
- `open_questions[]` — present only when `decision == "needs_user_input"`.
  Each entry has `id`, `section`, `question`, `allow_fallback`. Answer
  via `ctox report answer`.
- `reason` — diagnostic field set by the host loop-end gate when it
  downgrades. Always equal to `summary` after a downgrade; differs only
  in error paths.

The four-check status is at `.checks` in `--json` output:

```bash
ctox report status r_xxxx --json | jq '.checks'
# -> {
#      "completeness":     {"ready_to_finish": true,  "missing_required": [],     "thin_required": []},
#      "character_budget": {"within_tolerance": true, "severely_off_target": false, "status": "within"},
#      "release_guard":    {"ready_to_finish": true,  "needs_revision": false, "reasons": []},
#      "narrative_flow":   {"ready_to_finish": true,  "needs_revision": false, "reasons": []}
#    }
```

A `decision: "blocked"` immediately after the LLM emitted `decision: "finished"`
means the host downgraded. The first check whose `ready_to_finish` is
false (or whose `severely_off_target` is true, in the case of
`character_budget`) is the blocker. Inspect that check's `reasons[]` for
the lint code or block id, then either re-run with revisions
(`ctox report revise`) or abort if the failure is structural.

## Rendering the deliverable

Two formats:

```bash
ctox report render RUN_ID --format docx --out report.docx
ctox report render RUN_ID --format md   --out report.md
```

DOCX rendering goes through the bundled `scripts/render_manuscript.py`
helper, which reads a manuscript JSON on stdin and writes a DOCX. The
manuscript JSON is also accessible:

```bash
ctox report render RUN_ID --format json --out manuscript.json
```

Use it to debug rendering issues, hand the same JSON to a different
renderer, or re-render after a style-pack update.

After rendering, run the visual review:

```bash
python3 /Users/michaelwelsch/Documents/ctox/skills/system/research/deep-research/scripts/render_check.py \
    --docx report.docx \
    --out-dir /tmp/render_check
# -> prints one PNG path per page on stdout
```

Open the PNGs in your image viewer (Preview on macOS, eog on Linux). The
typical things to look for: TOC populated after first open, table column
widths sane (no overflow), heading numbering consistent, no Unicode
hyphens (only ASCII), evidence list in the appendix non-empty.

## Iterating on a finished run

A finished run is not the end. Two iteration paths exist:

1. **Targeted block revision.** When the operator reads the rendered
   report and wants a specific block tightened (or expanded, or
   re-anchored on a named competitor), the revision sub-skill is the
   right tool:

   ```bash
   ctox report revise r_xxxx \
       --instance-id detail_assessment_per_option#02 \
       --goal "Spezifische Sensitivitaet in mm angeben, basierend auf evidence_id e_017 und e_022." \
       --goal "Verdikt-Zeile beibehalten; Detailbewertung um drei Saetze erweitern."
   ```

   The revisor honours `goals[]` verbatim. Goals must be specific,
   actionable, falsifiable. `--goal "make better"` is rejected by the
   revisor schema. See `references/manager_path.md > How to call
   revise_with_skill` for the goal-phrasing rules.

   Form-only revisions (length, ordering, transitions, sentence rhythm —
   no new facts) take an explicit flag:

   ```bash
   ctox report revise r_xxxx \
       --instance-id management_summary#01 \
       --form-only \
       --goal "Auf 8 Saetze straffen; Verdikt-Zeile am Ende fettsetzen."
   ```

2. **Re-run on review feedback.** Hand back a reviewed DOCX with
   comments; the host extracts each comment, matches it to the nearest
   instance_id, and the next run consumes the comments as
   `review_feedback`. This is the workflow for iterative collaboration
   between the operator and a domain reviewer:

   ```bash
   ctox report review r_xxxx --review-doc /path/to/reviewed_report.docx
   ctox report continue r_xxxx
   ```

Targeted revision is targeted block surgery — it does not re-write the
whole document. Treat the revision tool as a scalpel, not a re-roll.

## Adding archetypes for non-feasibility report types

Today only `feasibility_study` × `ndt_aerospace` has a real goldreferenz
(the RASCON archetype). The other six report types currently run without
a structural archetype: the writer falls back to the
`document_blueprint` and the `style_guidance.dossier_story_model[]`
arc. Output is solid, but a real archetype lifts it from "good
template-following" to "calibrated against a known-good real-world
example".

To add an archetype, the operator (or the skill maintainer) does:

1. **Place a real reference dossier somewhere reachable.** A DOCX or
   PDF that the operator agrees represents the "we wish the manager
   wrote like this" exemplar for the target type × domain pair.

2. **Run the (TBD) one-off extraction tool.** The intent is a CLI like:

   ```bash
   ctox skill deep-research extract-archetype \
       --report-type market_research \
       --domain-profile ndt_aerospace \
       --source /path/to/reference_market_study.docx \
       --archetype-id rascon_market_archetype
   ```

   The extractor walks the DOCX, parses the heading hierarchy into
   block stanzas, and emits a YAML stub the maintainer hand-edits.
   Status: not yet wired — track the parallel asset-pack agent's
   progress.

3. **Update the asset pack.** Add an entry under
   `reference_archetypes[]`:

   ```json
   {
     "id": "rascon_market_archetype",
     "report_type_id": "market_research",
     "source_doc": "/path/to/reference_market_study.docx",
     "domain_profile_id": "ndt_aerospace",
     "structural_summary": "12-chapter market study on contactless NDT vendors in EU aerospace.",
     "uses_resource_ids": [
       "rascon_market_intro_pattern",
       "rascon_market_segment_pattern",
       "..."
     ]
   }
   ```

   And add the matching `reference_resources[]` entries (one per
   `uses_resource_ids[]` element). The `id` of each
   `reference_resources` entry must match the string in
   `uses_resource_ids[]`. Each resource holds the verbatim excerpt
   plus the metadata the writer reads to calibrate verdict shape,
   risk vocabulary, and section bridging.

4. **Bump the asset pack `manifest.version` and reload.** The host
   reads the asset pack at run-creation time. A version bump
   invalidates any in-memory caches.

5. **Run a fresh report of that type and inspect the output for
   calibration drift.** The first run after archetype addition is the
   one where the writer either internalises the calibration or quietly
   ignores it. If the output does not match the archetype's section
   bridging or verdict shape, the archetype's `uses_resource_ids[]`
   probably lacks the right resource entries — iterate.

The shape of a future archetype-extraction tool is intentionally not
specified here in stone. Today, hand-edit the asset pack directly; when
the extractor lands the workflow tightens up.

## Summary checklist

Before declaring a run done:

- [ ] `ctox report status RUN_ID --json | jq '.decision'` is `"finished"` (not `"blocked"`, not `"needs_user_input"`).
- [ ] All four checks are `ready_to_finish: true` or `check_applicable: false`.
- [ ] `ctox report render RUN_ID --format docx --out report.docx` produced a DOCX without error.
- [ ] `python3 scripts/render_check.py --docx report.docx --out-dir /tmp/rc` produced PNGs with a populated TOC and no broken tables.
- [ ] The evidence list in the appendix has at least `depth_profile.min_sources` non-trivial entries.
- [ ] No verdict line says "vielversprechend" or "transformatorisch" (the consultant phrases live in `style_guidance.consultant_phrases_to_soften[]`; the lint suite catches them but eyes-on confirms).

Anything failing on this list goes back through `Troubleshooting`.

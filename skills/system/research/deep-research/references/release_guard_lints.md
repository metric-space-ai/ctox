# Release Guard Lints — Catalogue for `release_guard_check`

This file is the deterministic-check catalogue used by the `release_guard_check`
tool of the CTOX deep-research skill. Every lint listed here MUST be
implementable as a deterministic Rust function — no LLM calls, no fuzzy
network calls. The tool is the deterministic counterpart to the LLM-based
`narrative_flow_check`; together they decide whether a research package can
be released.

The structural template for this catalogue is the `buildReleaseGuardPayload()`
function in
`/Users/michaelwelsch/Documents/Hypoport/Fördervorhaben-Agent/Foerdervorhaben-Agent.html`
(approx. lines 5745–5984). The substance examples are anchored against the
RASCON Machbarkeitsstudie (see `rascon_archetype.md`) and against the
`style_guidance` asset block (lines 2403–2581 of the same HTML).

## Output contract

`release_guard_check` returns a single JSON object with the fixed shape:

```jsonc
{
  "summary": "<string, short German sentence>",
  "check_applicable": <bool>,
  "ready_to_finish": <bool>,
  "needs_revision": <bool>,
  "candidate_instance_ids": [<string>, ...],   // max 6
  "goals":  [<string>, ...],                   // max 8
  "reasons":[<string>, ...]                    // max 6
}
```

Semantics (mirroring `buildReleaseGuardPayload()`):

- `check_applicable = false` only when no resolved package or no populated
  blocks exist. In that case `ready_to_finish = true`, `needs_revision =
  false`, and the three id/goal/reason arrays are empty.
- `ready_to_finish = (issues.length === 0)`.
- `needs_revision = (issues.length > 0)`.
- `candidate_instance_ids` is the deduplicated union of all
  `instance_ids` across triggered lints, capped at 6.
- `goals` is the deduplicated list of `goal` strings emitted by lints,
  capped at 8.
- `reasons` is the deduplicated list of `reason` strings, capped at 6.
- `summary` is the German one-liner: either
  `"<n> Freigabe- bzw. Stilrisiken erkannt."` or
  `"Keine harten Freigabe- oder Stilrisiken gefunden."`.

A lint that fires emits one `{instance_ids[], reason, goal}` issue. The same
lint may fire on several blocks; each firing is one issue. Severity rules
(see end of file) decide which issues actually flip `ready_to_finish` and
`needs_revision`.

## Lint applicability matrix

This catalogue serves nine report types defined in the asset-pack
`report_types[]` array (see `references/asset_pack.json`):

- `feasibility_study` — qualitative, axis-and-matrix scored, scenario-
  conditional verdicts, RASCON archetype (see `rascon_archetype.md`).
- `project_description` — company and project narrative for innovation,
  funding or financing contexts: problem, innovation leap, implementation,
  scope/budget/timeline and economic benefit.
- `source_review` — systematic source and data-source discovery:
  documented search strategy, source groups, source catalogue, extracted
  data, coverage assessment and gaps.
- `market_research` — TAM/SAM/SOM, segments, competitor landscape,
  growth/method-bound numerics.
- `competitive_analysis` — competitor profiles, capability matrix,
  positioning, differentiators.
- `technology_screening` — broad scan over candidate technologies, often
  matrix-bearing, lighter on scenarios than feasibility.
- `whitepaper` — single-thesis position paper, argument/counter-argument,
  evidence-anchored claims.
- `literature_review` — themed synthesis of an existing body of sources,
  thematic blocks plus an explicit gaps section.
- `decision_brief` — short, recommendation-first memo, criteria with
  weights, binary recommend/not-recommend close-out.

Every lint listed below has an `applicability` clause (rendered as
`applies when: report_type ∈ {…}` inside its `Triggers when` predicate)
that the host-side dispatcher reads before running the check. Lints
without an explicit clause fire universally. The matrix table below is
the single source of truth for which lint runs against which report
type. `✓` means the lint is in scope; an empty cell means the lint is
out of scope for that report type and the dispatcher MUST skip it.
`conditional` means the lint runs but with a relaxed predicate (used
where a structural element is optional rather than required).

| Lint id | feasibility_study | market_research | competitive_analysis | technology_screening | whitepaper | literature_review | decision_brief |
|---------|:----:|:----:|:----:|:----:|:----:|:----:|:----:|
| LINT-FAB-DOI | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| LINT-FAB-ARXIV | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| LINT-FAB-AUTHOR | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| LINT-UNCITED-CLAIM | ✓ | ✓ | ✓ | ✓ | conditional | ✓ | ✓ |
| LINT-CITED-BUT-MISSING | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| LINT-DOI-NOT-RESOLVED | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| LINT-EVIDENCE-FLOOR | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | conditional |
| LINT-EVIDENCE-CONCENTRATION | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| LINT-DEAD-PHRASE | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| LINT-META-PHRASE | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| LINT-CONSULTANT-OVERUSE | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| LINT-UNANCHORED-HEDGE | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| LINT-FILLER-OPENING | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| LINT-INVERTED-PERSPECTIVE | ✓ | ✓ | ✓ | ✓ | conditional | ✓ | ✓ |
| LINT-DUPLICATE-RATIONALE | ✓ | ✓ | ✓ | ✓ |  |  |  |
| LINT-VERDICT-MISMATCH | ✓ |  |  | ✓ |  |  | ✓ |
| LINT-AXIS-COMPLETENESS | ✓ | ✓ | ✓ | ✓ |  |  |  |
| LINT-RUBRIC-MISMATCH | ✓ | ✓ | ✓ | ✓ |  |  |  |
| LINT-MIN-CHARS | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| LINT-MAX-CHARS | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| LINT-MISSING-DISCLAIMER | ✓ | ✓ |  | ✓ |  |  |  |
| LINT-DUPLICATE-SECTION-OPENING | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| LINT-MR-UNQUANTIFIED-MARKET |  | ✓ | ✓ |  |  |  | conditional |
| LINT-MR-METHOD-MISSING |  | ✓ | conditional |  |  |  | conditional |
| LINT-MR-COMPETITOR-NAMELESS |  | ✓ | ✓ |  |  |  | conditional |
| LINT-MR-SEGMENT-WITHOUT-SIZE |  | ✓ | conditional |  |  |  |  |
| LINT-WP-THESIS-DRIFT |  |  |  |  | ✓ |  |  |
| LINT-WP-EVIDENCE-MISSING-FOR-CLAIM |  |  |  |  | ✓ |  |  |
| LINT-WP-FILLER-OPENING |  |  |  |  | ✓ |  |  |
| LINT-DB-RECOMMENDATION-BURIED |  |  |  |  |  |  | ✓ |
| LINT-DB-HEDGE-RECOMMENDATION |  |  |  |  |  |  | ✓ |
| LINT-DB-CRITERIA-WITHOUT-WEIGHTS |  |  | conditional |  |  |  | ✓ |
| LINT-LR-THEME-IMBALANCE |  |  |  |  |  | ✓ |  |
| LINT-LR-NO-GAPS-SECTION |  |  |  |  |  | ✓ |  |

The `conditional` cells mean: the lint runs only when the indicated
structural element actually appears in the resolved package, but is not
required by that report type's blueprint.

## Lint format

Every lint below follows this exact structure:

- **Triggers when** — the predicate, expressed precisely enough to port
  one-to-one into Rust.
- **Reason emitted** — German sentence template with `{placeholders}`.
- **Goal emitted** — German sentence template with `{placeholders}`.
- **Affected instance_ids** — how the lint identifies the affected blocks.
- **Implementation note** — one paragraph describing the deterministic Rust
  implementation.
- **Example trigger** — verbatim slop excerpt that would trigger.
- **Example clean** — verbatim RASCON or asset-pack excerpt that would not
  trigger.

---

## Evidence integrity (8 lints)

### LINT-FAB-DOI: hallucinated DOI

**Triggers when**: A markdown block contains a substring matching
`10\.\d{4,9}/[-._;()/:A-Za-z0-9]+` and that DOI does not appear (case-
insensitive, after stripping a leading `https?://(dx\.)?doi\.org/`) in any
`evidence_register[*].doi` field of the run.

**Reason emitted**:
`Block {title} zitiert eine DOI ({doi}), die nicht im Evidence-Register dieses Runs auftaucht.`

**Goal emitted**:
`Entferne in {title} die DOI {doi} oder belege sie zuerst über public_research und ergänze den Eintrag im Evidence-Register.`

**Affected instance_ids**: every block that contains at least one orphan
DOI. Each orphan DOI counts as a separate issue (so a block with three
fabricated DOIs emits three reasons, but contributes its `instance_id`
exactly once to `candidate_instance_ids`).

**Implementation note**: regex-extract DOIs from the block text, normalise
to lower-case, build a `HashSet<&str>` of registered DOIs from the run's
`evidence_register` (also lower-cased and normalised), then `set_diff` to
get orphans. No allocations beyond a small Vec; deterministic; runtime
O(blocks·doi_per_block).

**Example trigger**:
`Eddy-Current-Arrays erreichen laut Müller et al. eine POD von 87% (DOI: 10.9999/fake.2025.0001).`

**Example clean** (RASCON Anhang A, entry [4]):
`Feng, B.; Pasadas, D.J.; Ribeiro, A.L.; Ramos, H.G. Eddy Current Testing of the Lightning Strike Protection Layer in Aerospace Composite Structures. […] https://doi.org/10.3233/SAEM200004`

---

### LINT-FAB-ARXIV: hallucinated arXiv ID

**Triggers when**: A markdown block contains a substring matching
`arXiv:?\s*\d{4}\.\d{4,5}(v\d+)?` (case-insensitive) and that arXiv ID is
not present in the run's `evidence_register[*].arxiv_id` field.

**Reason emitted**:
`Block {title} verweist auf arXiv {arxiv_id}, ohne dass dieser Eintrag im Evidence-Register existiert.`

**Goal emitted**:
`Belege {arxiv_id} über public_research bevor er in {title} stehenbleibt; ansonsten entfernen.`

**Affected instance_ids**: blocks containing at least one orphan arXiv ID.

**Implementation note**: same pattern as LINT-FAB-DOI but on the arXiv
identifier regex. Normalise by stripping a leading `arXiv:` and any
trailing version suffix `vN` before set lookup.

**Example trigger**:
`Vergleichsweise robuste Ergebnisse berichten arXiv:2403.99999 für THz-Imaging unter Lack.`

**Example clean** (none of RASCON's references use arXiv; a clean run
either contains no arXiv references or every arXiv ID resolves through
the register).

---

### LINT-FAB-AUTHOR: orphan author-year cite

**Triggers when**: A markdown block contains a substring matching
`[A-ZÄÖÜ][a-zäöüß]+(?:\s+(?:et\s+al\.|und|and|&)\s+[A-ZÄÖÜ][a-zäöüß]+)?\s*(?:\(|,\s*)?(19|20)\d{2}\b`
and no `evidence_register[*]` entry has both (a) a matching first-author
family name (case-insensitive substring of `authors[0].family`) and (b) a
matching `year`.

**Reason emitted**:
`In {title} wirkt {citation} wie ein Autor-Jahr-Verweis, hat aber keinen Treffer im Evidence-Register.`

**Goal emitted**:
`Hinterlege {citation} im Evidence-Register oder entferne den Verweis aus {title}.`

**Affected instance_ids**: blocks containing at least one orphan author-
year cite. Cap at 5 unique cites per block in the issue list to avoid
flooding `reasons`.

**Implementation note**: extract candidates with the regex, build a
canonical `(family_lower, year)` tuple, set-diff against the register.
Whitelist common false positives (`Tabelle 2024`, `Stand: 2026`, document-
date phrases) using a small fixed deny-list of leading tokens.

**Example trigger**:
`Schmidt et al. 2023 zeigen eine 99-prozentige Detektionsrate von Wirbelstromsystemen auf CFK-Strukturen.`

**Example clean** (RASCON Kap. 6.2 caption, with all three names
resolvable in Anhang A):
`Abbildung 4: Prinzip der Induktions-Thermografie (eigene Darstellung). In Anlehnung an grundlegende ECPT/Induktions-Thermografie-Prinzipien, u.a. Oswald‑Tranta (2025) sowie Feng et al. (2020).`

---

### LINT-UNCITED-CLAIM: quantitative or method-specific claim without `used_reference_ids[]`

**Triggers when**: A block whose `template_id` is one of
`management_summary`, `detail_assessment`, or `recommendation` contains a
sentence that

- includes a quantitative pattern: a number with unit
  (`\d+(?:[.,]\d+)?\s*(?:%|°C|K|GHz|THz|MHz|kHz|µm|um|nm|mm|cm|m|kV|kW|kA|A|V|s|ms|µs|us|min|h|m²/h|m\^2/h)`)
  OR
- mentions a named method with quantitative qualifier (regex
  `(?:POD|Auflösung|Aufloesung|Sensitivität|Sensitivitaet|Durchsatz|Frequenz|Wellenlänge|Wellenlaenge|TRL)\s*(?:von|bei|≥|>=|>|<|≤|<=|=|:)\s*[\w\d.,]+`)

…and the block's metadata array `used_reference_ids[]` is empty for that
sentence's containing paragraph.

**Reason emitted**:
`Quantitative Aussage in {title} („{snippet}…“) ist nicht über used_reference_ids[] belegt.`

**Goal emitted**:
`Verknüpfe in {title} den Satz „{snippet}…“ mit der zugehörigen Quelle aus dem Evidence-Register oder ersetze die Zahl durch eine qualitative Einordnung.`

**Affected instance_ids**: each block that has at least one such
unsupported quantitative sentence.

**Implementation note**: split the markdown block into paragraphs (blank-
line separator), and for each paragraph match the two regex sets. If a
match exists and the paragraph's `used_reference_ids[]` is empty, fire.
The asset pack stores `used_reference_ids[]` per paragraph anchor, so the
mapping is structural, not heuristic. `snippet` = first 60 characters of
the offending sentence.

**Example trigger**:
`Induktions-Thermografie erreicht im Produktionseinsatz typischerweise einen Durchsatz von 38 m²/h bei einer minimalen detektierbaren Defektgröße von 0,8 mm.`
(no `used_reference_ids[]`)

**Example clean** (RASCON Kap. 7.2 Phase 1, list of measurands —
qualitative, no numbers, so no obligation):
`Messgrößen (Beispiele): Detektionswahrscheinlichkeit (POD), Falschalarme, Lokalisierungsfehler, minimale detektierbare Defektgröße, Durchsatz (m²/h) und Robustheit gegenüber Abstand/Neigung.`

---

### LINT-CITED-BUT-MISSING: dangling reference id

**Triggers when**: a block's `used_reference_ids[]` array contains an id
that does not resolve to any entry of `evidence_register`.

**Reason emitted**:
`Block {title} verweist auf Reference-ID {ref_id}, die nicht im Evidence-Register vorhanden ist.`

**Goal emitted**:
`Lege {ref_id} im Evidence-Register an oder entferne die Verknüpfung aus {title}.`

**Affected instance_ids**: blocks with at least one dangling id.

**Implementation note**: `HashMap<&str, &EvidenceEntry>` keyed by id; for
each block, iterate `used_reference_ids[]` and look up. Cheap.

**Example trigger**: block metadata contains
`used_reference_ids: ["ref-99-ghost"]` and the register has only
`ref-1` … `ref-16`.

**Example clean** (RASCON-style: every Detail-block's
`used_reference_ids[]` lands inside the 16-entry Anhang A index).

---

### LINT-DOI-NOT-RESOLVED: register entry never resolved by Crossref

**Triggers when**: an entry of `evidence_register` has a `doi` field set
and `crossref_status ∈ {"not_found","timeout","error","unresolved"}`.

**Reason emitted**:
`Quelle {ref_id} (DOI {doi}) wurde von Crossref mit Status {crossref_status} zurückgewiesen.`

**Goal emitted**:
`Ersetze {ref_id} durch eine Quelle mit auflösbarer DOI oder hinterlege eine bestätigte Alternative.`

**Affected instance_ids**: every block whose `used_reference_ids[]`
contains the unresolved `ref_id`. If the entry is referenced from no block
at all, the `instance_ids` array is empty (the lint still fires; the
reason is added regardless).

**Implementation note**: simple iteration over `evidence_register`
followed by reverse lookup. The smoke test for this lint is the 16
RASCON entries in `rascon_archetype.md` — all of them must resolve, with
`crossref_status = "ok"`.

**Example trigger**:
`{ "ref_id": "ref-7", "doi": "10.3390/s16060843", "crossref_status": "timeout" }`

**Example clean**:
`{ "ref_id": "ref-7", "doi": "10.3390/s16060843", "crossref_status": "ok" }`

---

### LINT-EVIDENCE-FLOOR: register too small for depth profile

**Triggers when**:
`evidence_register.length < depth_profile.min_evidence_count`.

**Reason emitted**:
`Das Evidence-Register hält {actual} Quellen bereit; das Tiefenprofil verlangt mindestens {required}.`

**Goal emitted**:
`Erweitere das Evidence-Register mit public_research auf mindestens {required} valide Quellen, bevor das Paket freigegeben wird.`

**Affected instance_ids**: empty (this lint is package-level, not block-
level).

**Implementation note**: scalar comparison against the resolved
`depth_profile`. The depth profile for a feasibility-study run typically
asks for 14–18 entries, anchored against RASCON's 16-entry Anhang A.

**Example trigger**: register has 7 entries, depth profile demands 14.

**Example clean**: register has 16 entries (RASCON-equivalent).

---

### LINT-EVIDENCE-CONCENTRATION: single source dominates

**Triggers when**: more than 60% of `used_reference_ids[]` occurrences
across all populated blocks point at the same `ref_id`.

**Reason emitted**:
`Die Belegkette stützt sich zu {percent}% auf {ref_id} ({short_title}); das wirkt wie eine Monoquelle.`

**Goal emitted**:
`Streue die Belege in den Detailkapiteln über mehrere Quellen aus dem Register; ergänze ggf. weitere via public_research.`

**Affected instance_ids**: every block where the dominant `ref_id` appears
(deduplicated, capped at 6).

**Implementation note**: count `ref_id` occurrences across blocks,
compute share. Two thresholds (see Severity rules below): >60%
→ soft warn, >80% → effectively a hard signal because LINT-EVIDENCE-
CONCENTRATION at >80% is paired with a user-visible reason that the
package is single-source-bound.

**Example trigger**: 9 of 12 cited paragraphs reference `ref-3 (Towsyfyan
et al. 2020)`.

**Example clean** (RASCON management summary plus six detail chapters
each cite different subsets of refs [1]–[16] with no single source above
~25% of citations).

---

## Anti-slop language (6 lints)

### LINT-DEAD-PHRASE: dead Förderdeutsch

**Triggers when**: a block's lower-cased markdown contains any string from
`style_guide.style_guidance.dead_phrases_to_avoid[]`.

**Reason emitted**:
`Block {title} enthält tote Wendung „{phrase}“.`

**Goal emitted**:
`Ersetze in {title} „{phrase}“ durch eine konkrete Aussage über Mechanik, Wirkung oder Beleg.`

**Affected instance_ids**: every block with at least one dead phrase. Up
to 3 phrases per block surface in the reason.

**Implementation note**: the asset-pack list is fixed at build time. A
single `aho_corasick::AhoCorasick` automaton over all dead phrases scans
all blocks in O(text); no per-phrase loop. The current asset-pack list
contains: `vor diesem Hintergrund`, `im Rahmen des Vorhabens`,
`weist eine hohe Relevanz auf`, `konnte festgestellt werden`,
`sollte ermöglicht werden`, `stellt einen wesentlichen Baustein dar`,
`zielt darauf ab`, `um den Anforderungen der Zukunft gerecht zu werden`.

**Example trigger**:
`Vor diesem Hintergrund zielt das Vorhaben darauf ab, eine signifikante Verbesserung der Prozesslandschaft zu erzielen.`

**Example clean** (asset-pack `micro_examples.example_dead_language.prefer`):
`Die neue Loesung verkuerzt Rueckkopplungen zwischen Lager, Fertigung und Disposition erheblich und macht Engpaesse frueher sichtbar.`

---

### LINT-META-PHRASE: forbidden meta-/Akten-Sprache

**Triggers when**: a block's lower-cased markdown contains any string
from `style_guide.style_guidance.forbidden_meta_phrases[]`.

**Reason emitted**:
`Verbotene Meta-Formel in {title}: „{phrase}“.`

**Goal emitted**:
`Formuliere {title} aus interner Feststellungsperspektive neu und entferne Gutachter- oder Aktenformeln.`

**Affected instance_ids**: every block with at least one hit.

**Implementation note**: identical mechanic to LINT-DEAD-PHRASE, second
Aho-Corasick automaton. Phrase list (asset pack):
`nach dem vorliegenden Kontext`, `oeffentlich belastbare Hinweise`,
`öffentlich belastbare Hinweise`, `soweit beigefuegt`, `soweit beigefügt`,
`nicht gesondert belegt`, `liegt derzeit nicht vor`, `falls vorhanden`,
`sofern beigefuegt`, `sofern beigefügt`. The reason text mirrors the
template used in `buildReleaseGuardPayload()` lines 5790–5795 verbatim.

**Example trigger**:
`Nach dem vorliegenden Kontext wird das Unternehmen als Spezialdienstleister beschrieben.`

**Example clean** (asset-pack `micro_examples.example_internal_voice.prefer`):
`Die Gesellschaft fuehrt die Schwer- und Spezialtransportlogik, die Kranleistungen und die Industriemontage am Standort in einer durchgaengigen Leistungskette zusammen.`

---

### LINT-CONSULTANT-OVERUSE: consultant words overused in one block

**Triggers when**: any string from
`style_guide.style_guidance.consultant_phrases_to_soften[]` appears more
than 2 times in a single block (case-insensitive whole-word boundary).

**Reason emitted**:
`Beraterhaft glatte Formulierungen in {title}: {phrase} (×{count}).`

**Goal emitted**:
`Ersetze in {title} glatte Beraterwörter durch konkretere fachliche Sprache.`

**Affected instance_ids**: blocks with at least one phrase whose count
≥ 3.

**Implementation note**: regex for each phrase with `\b` boundaries,
count occurrences, threshold > 2. Phrase list (asset pack):
`belastbare operative Groesse`, `unternehmerisch folgerichtig`,
`gewachsene operative Tiefe`, `naechster Entwicklungsschritt`, `Hebel`,
`Logik`, `durchgaengig`. Soft severity (see end of file).

**Example trigger**:
`Der nächste Entwicklungsschritt schafft die belastbare operative Größe, die als nächster Entwicklungsschritt jeden weiteren Hebel öffnet — ein wirklich folgerichtiger Hebel.`

**Example clean** (RASCON Kap. 6.1, no asset-pack consultant terms):
`Induktive Verfahren regen Wirbelströme in leitfähigen Strukturen an und messen die resultierende Änderung des Magnetfelds bzw. der Sondenimpedanz.`

---

### LINT-UNANCHORED-HEDGE: hedge without confidence anchor

**Triggers when**: a sentence contains a hedge token from the fixed list
{ `möglicherweise`, `moeglicherweise`, `in bestimmten Fällen`,
`in bestimmten Faellen`, `tendenziell`, `vielleicht`,
`could potentially`, `in some cases`, `may be able to` } AND that
sentence does not also contain an anchor token from the fixed list
{ `unter der Randbedingung`, `unter der Annahme`, `bei`,
`vorausgesetzt`, `wenn`, `sofern`, `Quelle`, `[`, `Abbildung`, `Tabelle`,
`Szenario`, `provided that`, `under the assumption`, `assumption` }.

**Reason emitted**:
`Hedging in {title} ohne erkennbaren Anker: „{snippet}…“.`

**Goal emitted**:
`Binde {title} den Hedge an Annahme, Szenario oder Quelle, oder streiche ihn.`

**Affected instance_ids**: every block with at least one unanchored hedge
sentence.

**Implementation note**: split on `[.!?]\s` to get sentences; each
sentence is a single Aho-Corasick scan against both lists, then the
predicate `has_hedge AND NOT has_anchor`. `snippet` = first 60 chars of
the offending sentence.

**Example trigger**:
`Möglicherweise eignet sich Terahertz-Imaging für die Aufgabe.`

**Example clean** (RASCON Kap. 5.3 footnote, hedge anchored to scenario):
`Auch bei dominanter Folie kann die thermische bzw. magnetische Signatur von Leiteranomalien detektierbar bleiben, weil Unterbrechungen/Engstellen die Stromverteilung und Joule-Erwärmung beeinflussen.`

---

### LINT-FILLER-OPENING: filler paragraph opener

**Triggers when**: any paragraph (separated by `\n\n`) of a block opens
with one of the fixed openers (case-insensitive, possibly preceded by
markdown formatting): `Im Folgenden`, `Im Rahmen dieser`,
`Vor diesem Hintergrund`, `Es ist anzumerken`,
`Die folgenden Abschnitte`.

**Reason emitted**:
`Absatz in {title} beginnt mit Füllformel „{opener}“.`

**Goal emitted**:
`Eröffne den Absatz in {title} mit einem konkreten Anker (Verfahren, Schichtaufbau, Defekt) statt mit „{opener}“.`

**Affected instance_ids**: every block with at least one filler-opening
paragraph.

**Implementation note**: per paragraph, strip leading whitespace and
markdown markers (`-\s+`, `\*\s+`, `>\s+`, `\d+\.\s+`), then check
`starts_with` against the opener list. Note: the same automaton MUST NOT
fire when the opener appears mid-paragraph as part of LINT-DEAD-PHRASE —
filler-opening is structural, dead-phrase is lexical. Both can fire
together.

**Example trigger**:
`Im Folgenden werden die kontaktlosen Verfahren bewertet.`

**Example clean** (RASCON Kap. 6.1, opens with the physical mechanism):
`Induktive Verfahren regen Wirbelströme in leitfähigen Strukturen an und messen die resultierende Änderung des Magnetfelds bzw. der Sondenimpedanz.`

---

### LINT-INVERTED-PERSPECTIVE: first-person plural in third-person register

**Triggers when**: a block whose `template_id` is one of
`management_summary`, `detail_assessment`, `risk_register`,
`scope_disclaimer`, `recommendation` contains a token matching
`\b(?:Wir|wir|Unser|unser|unsere|unserem|unseren|unserer|unseres|we|our|We|Our)\b`.

**Reason emitted**:
`Block {title} schreibt in Wir-Form, obwohl der Block in dritter Person geführt wird.`

**Goal emitted**:
`Stelle {title} auf dritte Person bzw. Sachregister um; halte die Perspektive im gesamten Paket einheitlich.`

**Affected instance_ids**: every block in the listed templates with at
least one Wir-/our-token.

**Implementation note**: per block, regex search with whole-word
boundaries, ignore matches inside fenced code or quoted strings (cheap
heuristic: skip lines starting with ` ``` ` or `>` followed by `"`).
Hard severity: `internal_perspective_rules` in the asset pack mandates a
single perspective dossier-wide.

**Example trigger** (in a `detail_assessment` block):
`Wir empfehlen Eddy-Current-Arrays, weil unser Versuchsstand sie bereits unterstützt.`

**Example clean** (RASCON Kap. 9, third person throughout):
`Unter der Randbedingung „kontaktlos“ sind elektrische/magnetische Verfahren (Eddy Current/Induktion) und insbesondere Induktions-Thermografie die aussichtsreichsten Kandidaten für eine robuste Detektion von Gitteranomalien unter Beschichtung.`

---

## Matrix integrity (4 lints)

### LINT-DUPLICATE-RATIONALE: matrix cells with copy-paste rationales

**Triggers when**: two matrix cells of the same option (i.e. same row in
the option×axis matrix) carry rationale strings whose Levenshtein
distance over word-shingles (3-grams) is below 0.3 (i.e. ≥70% shingle
overlap). Applies when: `package_context.report_type` carries a
matrix in its `block_library_keys[]` (currently `feasibility_study`,
`market_research`, `competitive_analysis`, `technology_screening`).

**Reason emitted**:
`Bewertungsmatrix für {option}: Begründungen in den Achsen {axis_a} und {axis_b} sind nahezu identisch.`

**Goal emitted**:
`Schreibe die Achsen-Begründungen für {option} so, dass jede Achse ihre eigene fachliche Logik trägt (z.B. Single-Shot vs. Defektsensitivität vs. Reifegrad).`

**Affected instance_ids**: the matrix block instance plus, where stored
separately, the per-cell instance ids of the duplicated cells.

**Implementation note**: build word-3-gram sets per cell rationale,
compute Jaccard similarity (`|A∩B| / |A∪B|`); fire when similarity
≥ 0.7 (i.e. distance ≤ 0.3). Critical severity: this is the single
strongest tell for AI slop in feasibility studies and flips
`needs_revision = true` even when other checks pass.

**Example trigger**:
| Verfahren | Fläche | Gitterdefekt | Reifegrad |
|---|---|---|---|
| Eddy Current | hoch — sehr robust und industrieerprobt | hoch — sehr robust und industrieerprobt | hoch — sehr robust und industrieerprobt |

**Example clean** (RASCON Kap. 5.2 row "Eddy Current (ECT) / Arrays"):
each axis carries its own value (`mittel`, `sehr hoch`, `sehr hoch`,
`mittel`, `hoch`); Kap. 6.1 explains *why* the gitter axis is
`sehr hoch` (Wirbelstrom-Kopplung), not by repeating the same sentence.

---

### LINT-VERDICT-MISMATCH: detail verdict ≠ matrix cell

**Triggers when**: a `detail_assessment` block's terminal verdict line
matches the regex
`Erfolgsaussichten\s*\(qualitativ\)\s*:?\s*(sehr\s+hoch|hoch|mittel(?:[\s\-–—]*hoch)?|niedrig(?:[\s\-–—]*mittel)?|niedrig)`
and the extracted level does not equal the matrix cell value for the
same option on the most-relevant axis (the axis whose `axis_id` is set
on the detail-block as `primary_axis_id`). Applies when:
`package_context.report_type` declares a `verdict_line_pattern` in the
asset pack (currently `feasibility_study`, `technology_screening`,
`decision_brief`). Other report types either have no verdict line or
phrase it differently and the lint MUST be skipped.

**Reason emitted**:
`Verdict in {title} („{verdict}“) passt nicht zur Matrixzelle {option}/{axis} („{matrix_value}“).`

**Goal emitted**:
`Synchronisiere Detail-Verdict und Matrixzelle für {option}/{axis} und passe ggf. die Matrix an, falls die Detailbegründung tragfähiger ist.`

**Affected instance_ids**: the detail block and the matrix block.

**Implementation note**: lookup table mapping verdict-level strings to
matrix-cell values (`sehr hoch == sehr hoch`,
`mittel-hoch == mittel-hoch`, etc.). Use the `primary_axis_id` declared
in the asset pack so the lint knows which axis the detail block is
arguing about.

**Example trigger**: detail block 6.1 ends with
`Erfolgsaussichten (qualitativ): mittel`, but the matrix cell
`Eddy Current / Gitterdefekt` reads `sehr hoch`.

**Example clean** (RASCON Kap. 6.1, last sentence aligned with matrix):
`Erfolgsaussichten (qualitativ): sehr hoch für Gitterabbildung und Gitterdefekte; mittel für reine Delamination ohne elektrische Auswirkung.`

---

### LINT-AXIS-COMPLETENESS: matrix cell empty for a required option

**Triggers when**: the resolved matrix has at least one cell with
empty `value_label` (or value `null`/`""`/`-`/`tbd`) for a `(option,
axis)` pair that is marked `required: true` in the matrix asset.
Applies when: `package_context.report_type` carries a matrix in its
`block_library_keys[]` (currently `feasibility_study`,
`market_research`, `competitive_analysis`, `technology_screening`).

**Reason emitted**:
`Bewertungsmatrix: Zelle {option}/{axis} ist nicht ausgefüllt.`

**Goal emitted**:
`Trage in {option}/{axis} eine qualitative Bewertung im erlaubten Rubrik-Vokabular ein oder begründe ihren Ausschluss explizit.`

**Affected instance_ids**: matrix block instance.

**Implementation note**: iterate the matrix structure, check each
required cell, emit one issue per empty cell. Hard severity.

**Example trigger**:
| Verfahren | Fläche | Gitterbild | Gitterdefekt | Delamination | Reifegrad |
|---|---|---|---|---|---|
| THz | mittel | hoch | – | mittel | mittel |

**Example clean** (RASCON Kap. 5.2, every cell of every required option
filled with one of `niedrig`/`mittel`/`hoch`/`sehr hoch`/`niedrig–mittel`/
`mittel–hoch`).

---

### LINT-RUBRIC-MISMATCH: matrix cell value not in axis rubric

**Triggers when**: a matrix cell's `value_label` is not present in the
asset-pack `axes[axis_id].rubric[]` list for that axis. Applies when:
`package_context.report_type` carries a matrix in its
`block_library_keys[]` (currently `feasibility_study`,
`market_research`, `competitive_analysis`, `technology_screening`).

**Reason emitted**:
`Bewertungsmatrix: {option}/{axis} = „{value}“ entspricht keiner Stufe der Rubrik ({allowed}).`

**Goal emitted**:
`Wähle in {option}/{axis} eine zulässige Rubrikstufe ({allowed}) und passe ggf. die Begründung an.`

**Affected instance_ids**: matrix block instance.

**Implementation note**: per-axis `HashSet<String>` of allowed rubric
labels (lower-cased and Unicode-NFC). Cell value gets the same
normalisation, then `contains`. Critical: this protects the rubric
vocabulary RASCON uses (`niedrig`/`mittel`/`hoch`/`sehr hoch` plus the
hyphenated bridges `niedrig–mittel`/`mittel–hoch`).

**Example trigger**: cell value `outstanding` for axis `Reifegrad`.

**Example clean** (RASCON Kap. 5.2 row "Induktions-Thermografie (ECT +
IR)" with values `hoch / hoch / hoch / hoch / hoch` — all from the rubric).

---

## Structural integrity (4 lints)

### LINT-MIN-CHARS: required block too short

**Triggers when**: a required block (`required: true` in asset pack)
has `trim_norm(markdown).len() < min_chars * 0.65`, where `min_chars` is
the per-block target from the asset pack and `trim_norm` collapses
whitespace and strips leading/trailing blanks.

**Reason emitted**:
`Block {title} ist mit {actual} Zeichen deutlich kürzer als das Sollmaß ({min_chars}).`

**Goal emitted**:
`Verdichte {title} auf mindestens das Sollmaß; orientiere dich am Detailgrad eines vergleichbaren RASCON-Kapitels.`

**Affected instance_ids**: every required block under the floor.

**Implementation note**: scalar comparison. The 0.65 factor is the same
soft floor used by `buildReleaseGuardPayload()` for length asymmetry. Hard
severity for required blocks; LINT-MAX-CHARS handles the upper bound.

**Example trigger**: `management_summary` with 480 characters (target
1500).

**Example clean** (RASCON Management Summary measures 1938 characters,
above target).

---

### LINT-MAX-CHARS: block over-stuffed

**Triggers when**: any block has `trim_norm(markdown).len() > min_chars
* 2.0`.

**Reason emitted**:
`Block {title} ist mit {actual} Zeichen über doppelt so lang wie das Sollmaß ({min_chars}).`

**Goal emitted**:
`Kürze {title} auf das Sollmaß; verschiebe Zusatzdetails in die zugehörigen Detailkapitel oder einen Anhang.`

**Affected instance_ids**: every block over the ceiling.

**Implementation note**: scalar comparison; soft severity (warns, does
not block release). Long blocks are usually a sign of over-stuffing
rather than fabrication.

**Example trigger**: `detail_assessment_eddy_current` at 4 200 chars
when the target is 1 200.

**Example clean** (RASCON Kap. 6.1 at 816 chars — under target, dense
without being over-stuffed).

---

### LINT-MISSING-DISCLAIMER: scope_disclaimer lacks required substrings

**Triggers when**: the `scope_disclaimer` block of the package does not
contain at least one substring from each of the three required clusters
(matched case-insensitive against the document language):

- assumption cluster: `Annahme`, `Plausibilitätsannahme`, `Annahmen`,
  `assumption`, `assumptions`
- validation cluster: `Validierung`, `validiert`, `repräsentative Proben`,
  `validation`, `validated`
- limitation cluster: `Grenze`, `Einschränkung`, `nicht`, `Limit`,
  `limitation`

Applies when: `package_context.report_type_id ∈ { "feasibility_study",
"technology_screening", "market_research" }`. Other report types either
do not require a scope disclaimer or carry a structurally different
limitations block, and the lint MUST be skipped for them.

**Reason emitted**:
`Scope-Disclaimer fehlt eine erforderliche Klausel ({missing_cluster}).`

**Goal emitted**:
`Ergänze im Scope-Disclaimer eine Aussage zu {missing_cluster}; orientiere dich an dem Hinweisblock einer RASCON-Studie.`

**Affected instance_ids**: scope-disclaimer block.

**Implementation note**: three Aho-Corasick automata, one per cluster.
The block must hit at least one substring from each. Hard severity.

**Example trigger**:
`Diese Studie betrachtet kontaktlose Prüfverfahren.` (no assumption /
validation / limitation language)

**Example clean** (RASCON title-block disclaimer):
`Hinweis: Diese Studie basiert auf öffentlich zugänglichen Informationen, Plausibilitätsannahmen und typischen Material-/Schichtsystemen. Eine belastbare Aussage zur Detektierbarkeit erfordert Validierung an repräsentativen Proben (Schichtaufbau, Gittergeometrie, Defektkatalog).`

---

### LINT-DUPLICATE-SECTION-OPENING: re-introduction instead of bridge

**Triggers when**: two blocks' first sentences share more than 70%
word overlap (case-insensitive, after stop-word removal). Stop words:
fixed German+English list bundled in the asset pack
(`der`, `die`, `das`, `und`, `oder`, `ist`, `sind`, `mit`, `für`,
`the`, `is`, `are`, `with`, `for`, `of`, `to`, …).

**Reason emitted**:
`Die Blöcke {title_a} und {title_b} steigen mit fast identischer Einleitung ein.`

**Goal emitted**:
`Bau in {title_b} eine Brücke zum vorherigen Gedanken statt das Vorhaben erneut bei null vorzustellen.`

**Affected instance_ids**: both blocks.

**Implementation note**: sentence-tokenise (`[.!?]\s`), take first
sentence per block, compute stop-word-filtered word sets, Jaccard ≥ 0.7
fires. Soft severity. The `section_bridging` rules in the asset pack
explicitly demand bridges, not re-introductions.

**Example trigger**: Kap. 6.1 opens
`Kontaktlose Prüfverfahren erlauben die Inspektion von CFK-Bauteilen.`
and Kap. 6.2 opens
`Kontaktlose Prüfverfahren erlauben die Inspektion von CFK-Bauteilen.`

**Example clean** (RASCON Kap. 6.1 opens with Wirbelstrom-Mechanik;
Kap. 6.2 opens with the explicit pivot
`Bei der Induktions-Thermografie wird die Blitzschutzlage induktiv
erwärmt; eine IR-Kamera erfasst die Temperaturentwicklung flächig.` —
shared lemmas are method-specific, not boilerplate.)

---

## New lints (per report-type)

The following twelve lints extend the catalogue beyond the universal
twenty-two. Each carries an explicit `Applies when:` clause that the
host-side dispatcher reads before running the check. The matrix at the
top of this file is the binding source of truth — the predicates below
mirror that matrix.

## Market-research integrity (4 lints)

### LINT-MR-UNQUANTIFIED-MARKET: market-growth claim without a sourced number

**Applies when**: `report_type_id ∈ {market_research}`. Conditional on
`competitive_analysis` and `decision_brief` (only if the run produces a
`market_overview` / `demand_drivers` block).

**Triggers when**: A `market_overview`, `market_sizing`, `demand_drivers`,
or `segments` block contains a regex-detectable growth claim
(`(?i)\b(wächst|hochdynamisch|deutlicher\s+wachstumstrend|rapidly\s+growing|double[-\s]digit\s+growth|expanding\s+fast|stark\s+wachsend)\b`)
and EITHER no number with units (`\d+(?:[.,]\d+)?\s*(%|Mio\.?|Mrd\.?|bn|m\b|million|billion)`)
appears within 200 characters of the claim, OR no `used_reference_ids[]`
links the claim to the run's evidence_register.

**Reason emitted**:
`Wachstumsbehauptung in {title} ohne belegte Zahl oder Quelle (gefundene Phrase: "{phrase}").`

**Goal emitted**:
`Belege die Wachstumsaussage in {title} mit einer datierten Marktzahl und einer registrierten Quelle, oder streiche sie.`

**Affected instance_ids**: every market-side block whose markdown contains
a growth phrase failing the predicate.

**Implementation note**: regex scan; for every match, scan ±200 chars
window for a number-with-unit and check `used_reference_ids[]` non-empty
on the block. Two cheap passes, no external calls.

**Example trigger**:
`Der Markt für kontaktlose NDT-Verfahren wächst stark und wird in den
nächsten Jahren weiter zulegen.`

**Example clean**:
`Der Markt für kontaktlose NDT-Verfahren ist laut Smithers (2024) zwischen
2020 und 2024 mit einer CAGR von 6,4 % auf rund 1,2 Mrd. EUR gewachsen
[ev:smithers_ndt_2024].`

---

### LINT-MR-METHOD-MISSING: TAM/SAM/SOM without method note

**Applies when**: `report_type_id == "market_research"`. Conditional on
`competitive_analysis` and `decision_brief` (only if a `market_sizing`
block exists).

**Triggers when**: Any block contains the regex `\b(TAM|SAM|SOM)\b` and
no paragraph within 200 characters contains one of: `top-down`,
`bottom-up`, `Methode`, `method`, `Annahmen`, `assumption`.

**Reason emitted**:
`{title} nennt TAM/SAM/SOM ohne Hinweis auf die Berechnungsmethode.`

**Goal emitted**:
`Ergänze in {title} eine kurze Methodenangabe (top-down vs bottom-up,
Bezugsjahr, geographischer Geltungsbereich) für die TAM/SAM/SOM-Werte.`

**Affected instance_ids**: every block where a TAM/SAM/SOM mention is
not accompanied by a method note within 200 chars.

**Implementation note**: regex match position, then substring window
check. Linear scan.

**Example trigger**:
`Der adressierbare Markt liegt bei einem TAM von 8,4 Mrd. EUR.`

**Example clean**:
`Der adressierbare Markt liegt bei einem TAM von 8,4 Mrd. EUR
(top-down, Bezugsjahr 2024, EU27 + UK), abgeleitet aus
[ev:eurostat_2024] und [ev:smithers_2024].`

---

### LINT-MR-COMPETITOR-NAMELESS: competitor mentions without named competitors

**Applies when**: `report_type_id ∈ {market_research, competitive_analysis}`.
Conditional on `decision_brief` (only when an `options_summary` block
discusses competitors).

**Triggers when**: A `competitor_landscape`, `competitor_set`,
`channel_overlap`, or `gap_to_close` block contains the lemma
`(?i)\b(wettbewerber|competitors?|players?|anbieter|marktteilnehmer|various\s+(?:players|firms))\b`
and the block does NOT contain at least three distinct named-competitor
candidates. A named-competitor candidate is a token sequence of 1–4
capitalised words (≥ 3 chars each) that is NOT in a Stop-list of
generic capitalised nouns (`Wettbewerber`, `Markt`, `Industry`, etc.).

**Reason emitted**:
`{title} spricht über Wettbewerber, ohne mindestens drei konkret zu benennen.`

**Goal emitted**:
`Nenne in {title} mindestens drei Wettbewerber namentlich (z. B. Marktführer, Herausforderer, Nischenanbieter) und verlinke sie zu Evidenz-Einträgen.`

**Affected instance_ids**: blocks failing the named-competitor count.

**Implementation note**: tokenise capitalised n-grams (n=1..4),
deduplicate, subtract stop-list, count. Reject the block if the
generic-mention regex matches AND the count is < 3.

**Example trigger**:
`Das Segment ist von einer Reihe etablierter Wettbewerber besetzt;
verschiedene Anbieter teilen sich den Markt.`

**Example clean**:
`Das Segment teilen sich GE Inspection Technologies, Olympus IMS
und Eddyfi Technologies; daneben hat Tecnatom in Europa eine starke
Stellung [ev:gartner_ndt_2024, ev:eddyfi_ar_2023].`

---

### LINT-MR-SEGMENT-WITHOUT-SIZE: segment listed without addressable size

**Applies when**: `report_type_id == "market_research"`. Conditional on
`competitive_analysis` only when a `segments` block is present.

**Triggers when**: A `segments` block lists segments (bullet list with at
least 3 items) and does NOT contain at least one addressable-size
estimate (`\d+(?:[.,]\d+)?\s*(%|Mio\.?|Mrd\.?|bn|M\s*units|EUR|USD)`)
nor any of the lemmas `Methode|method|Annahmen|assumption`.

**Reason emitted**:
`Segmentliste in {title} ohne adressierbare Grösse oder Methodenangabe.`

**Goal emitted**:
`Ergänze in {title} pro Segment entweder eine Grösseneinschätzung mit
Bezugsjahr oder eine Sammelmethodenangabe.`

**Affected instance_ids**: the offending `segments` block.

**Implementation note**: bullet-list detection (`(?m)^\s*[-*]\s+` lines
≥ 3) plus regex search for the size pattern within the block.

**Example trigger**:
- KMU-Industriebetriebe
- Großserienfertiger Aerospace
- Forschungseinrichtungen

**Example clean**:
- KMU-Industriebetriebe (≈ 320 Mio. EUR adressierbar, top-down 2024)
- Großserienfertiger Aerospace (≈ 540 Mio. EUR, bottom-up aus [ev:asd_2024])
- Forschungseinrichtungen (≈ 80 Mio. EUR, kalibriert an [ev:bmbf_2023])

---

## Whitepaper integrity (3 lints)

### LINT-WP-THESIS-DRIFT: thesis lacks a single position or counter-arguments do not actually counter it

**Applies when**: `report_type_id == "whitepaper"`.

**Triggers when**: ONE of:
1. The `thesis` block contains more than one declarative position
   sentence whose noun-phrase head sets disagree (no overlap of head
   nouns), so the thesis is structurally split.
2. The `counter_arguments` block does NOT share at least 30% of the
   noun phrases extracted from the `thesis` block.

**Reason emitted (case 1)**:
`Thesisblock {title} enthält mehrere konkurrierende Positionen ohne erkennbaren Hauptanker.`

**Reason emitted (case 2)**:
`Gegenargumente {title} adressieren die These offenbar nicht — Begriffsüberschneidung unter 30 %.`

**Goal emitted (case 1)**:
`Verdichte {title} auf eine einzige Hauptposition; verlagere konkurrierende Aussagen in argument_section.`

**Goal emitted (case 2)**:
`Schreibe {title} so um, dass mindestens drei Schlüsselbegriffe aus dem Thesisblock direkt aufgegriffen und entkräftet bzw. abgegrenzt werden.`

**Affected instance_ids**: the thesis block (case 1) or counter_arguments
block (case 2).

**Implementation note**: simple noun-phrase extraction by capitalised
N-grams + verb-deletion; Jaccard overlap of head-noun sets. Pure-Rust,
no NLP library.

**Example trigger (case 2)**:
- thesis: "Kontaktlose Verfahren werden Ultraschall in der CFK-Inspektion verdrängen."
- counter_arguments: "Ausbildungskosten für neues Prüfpersonal sind hoch."
(Counter-arguments do not address kontaktlos vs Ultraschall.)

**Example clean**:
- thesis: "Kontaktlose Verfahren werden Ultraschall in der CFK-Inspektion verdrängen."
- counter_arguments: "Ultraschall behält in foliengeschützten Layups einen technischen Vorteil; kontaktlose Verfahren stoßen genau hier an Grenzen [ev:rascon_ch5]."

---

### LINT-WP-EVIDENCE-MISSING-FOR-CLAIM: argument block has quantitative claim without source

**Applies when**: `report_type_id == "whitepaper"`.

**Triggers when**: An `argument_section` block contains a quantitative
or method-specific claim matched by
`\d+(?:[.,]\d+)?\s*(%|Mio\.?|Mrd\.?|bn|x|fold|×)|\b(POD|TRL|signal-to-noise|SNR)\b\s*\d`
AND `used_reference_ids[]` is empty for the block.

**Reason emitted**:
`{title} stellt eine quantitative oder methodenspezifische Behauptung auf, ohne {evidence_id_count} registrierte Quelle.`

**Goal emitted**:
`Verlinke die Aussage in {title} mit einem Evidenz-Eintrag (oder streiche die Quantifizierung).`

**Affected instance_ids**: the offending argument block.

**Implementation note**: regex scan of block markdown; evidence-array
length check. Linear cost.

**Example trigger**:
`Induktionsthermografie liefert eine POD von 92 % auf typischen LSP-Coupons.`
(no `used_reference_ids[]`)

**Example clean**:
`Induktionsthermografie liefert eine POD von 92 % auf typischen LSP-Coupons [ev:liu_2025].`

---

### LINT-WP-FILLER-OPENING: whitepaper-typical filler opening

**Applies when**: `report_type_id == "whitepaper"`.

**Triggers when**: Any block starts (after stripping leading whitespace,
blank lines, or one heading line) with one of the regexes:
- `(?i)^in\s+today's\b`
- `(?i)^in\s+der\s+heutigen\b`
- `(?i)^paradigm\s+shift\b`
- `(?i)^(next|cutting)[-\s]edge\b`
- `(?i)^state[-\s]of[-\s]the[-\s]art\b`
- `(?i)^im\s+zeitalter\s+der\b`
- `(?i)^wir\s+leben\s+in\b`

**Reason emitted**:
`{title} öffnet mit einer Whitepaper-Floskel ("{phrase}").`

**Goal emitted**:
`Beginne {title} mit einer konkreten Aussage zum Untersuchungsgegenstand statt mit einer Zeitgeist-Floskel.`

**Affected instance_ids**: blocks whose first non-blank line matches.

**Implementation note**: trim + first-line regex test. Cheap.

**Example trigger**:
`In der heutigen, hochvernetzten Fertigungslandschaft …`

**Example clean**:
`Kontaktlose Prüfverfahren stehen vor zwei strukturellen Hürden: dem
Lift-off-Effekt induktiver Sonden und der Reflektionsbarriere
geschlossener Metallfolien für THz-Imaging.`

---

## Decision-brief integrity (3 lints)

### LINT-DB-RECOMMENDATION-BURIED: recommendation appears late in the document

**Applies when**: `report_type_id == "decision_brief"`.

**Triggers when**: The `recommendation_brief` block's order index is
beyond the front-third of the document. Concretely: with N total
populated blocks in document order, the `recommendation_brief` block
must satisfy `order_index < ceil(N / 3)`.

**Reason emitted**:
`recommendation_brief steht erst an Position {position}/{total} — nicht im Vorderteil des Dokuments.`

**Goal emitted**:
`Verschiebe recommendation_brief in das vordere Drittel; führe situation, options_summary und criteria nach der Empfehlung als Begründung an.`

**Affected instance_ids**: the offending `recommendation_brief` block.

**Implementation note**: read `document_blueprints[<id>_single].sequence`
order, find position of `recommendation_brief`, compare to
`ceil(N / 3)`. Pure scalar.

**Example trigger**:
A decision_brief whose document order is `decision_at_stake → situation
→ options_summary → criteria → evaluation → recommendation_brief →
caveats_and_next_steps` (recommendation at index 5 of 7 → buried).

**Example clean**:
`decision_at_stake → recommendation_brief → situation → options_summary
→ criteria → evaluation → caveats_and_next_steps`
(recommendation at index 1 of 7 → front-loaded).

---

### LINT-DB-HEDGE-RECOMMENDATION: recommendation hedges instead of taking position

**Applies when**: `report_type_id == "decision_brief"`.

**Triggers when**: The `recommendation_brief` block contains the regex
`(?i)(should\s+consider|may\s+want\s+to|recommend\s+exploring|empfehle\s+weiter\s+zu\s+prüfen|sollte\s+erwogen\s+werden|könnte\s+sinnvoll\s+sein|wäre\s+zu\s+prüfen)`
AND does NOT contain at least one binary decision token from
`(?i)\b(recommend|not\s+recommended|recommend\s+with\s+caveats|empfohlen|nicht\s+empfohlen|empfohlen\s+mit\s+Auflagen)\b`.

**Reason emitted**:
`recommendation_brief hedgt ("{phrase}") ohne klare Empfehlungsentscheidung.`

**Goal emitted**:
`Formuliere {title} als binäre Empfehlung ("empfohlen", "nicht empfohlen", "empfohlen mit Auflagen") und liste Auflagen separat.`

**Affected instance_ids**: the offending `recommendation_brief` block.

**Implementation note**: two regex matches; clear logical AND/NOT.

**Example trigger**:
`Wir empfehlen, die Option weiter zu prüfen und ggf. eine Pilotphase
in Erwägung zu ziehen.`

**Example clean**:
`Empfohlen mit Auflagen: Option B wird umgesetzt, sofern die
Pilotergebnisse aus Q3 die POD-Schwelle von 85 % erreichen
[ev:pilot_specs_2025].`

---

### LINT-DB-CRITERIA-WITHOUT-WEIGHTS: criteria block lists criteria without weights

**Applies when**: `report_type_id == "decision_brief"`. Conditional on
`competitive_analysis` (only when a `capability_axes` block lists
weighted axes).

**Triggers when**: A `criteria` block contains a bullet list of at least
three items AND does NOT contain a weighting indicator: regex
`\d+\s*%|\bweight(s|ing)?\b|\bGewicht(ung)?\b|\bpriorit(y|ät)\b`
in the block, AND no explicit ordering language (`zuerst`, `vor allem`,
`primär`, `priorisierend`).

**Reason emitted**:
`Kriterienliste in {title} ohne Gewichtung oder explizite Priorisierung.`

**Goal emitted**:
`Ergänze in {title} entweder Prozent-Gewichte (Summe 100 %) oder eine
sichtbare Reihenfolge mit Begründung.`

**Affected instance_ids**: the offending `criteria` block.

**Implementation note**: bullet-count + dual regex match.

**Example trigger**:
- Wirtschaftlichkeit
- Technische Reife
- Integrationsaufwand
- Risiko

**Example clean**:
- Wirtschaftlichkeit (40 %)
- Technische Reife (30 %)
- Integrationsaufwand (20 %)
- Risiko (10 %)

---

## Literature-review integrity (2 lints)

### LINT-LR-THEME-IMBALANCE: one theme block holds disproportionate citation share

**Applies when**: `report_type_id == "literature_review"`.

**Triggers when**: Across the run's `theme_section` repeatable blocks,
one theme block holds more than 60 % of the run's distinct
`used_reference_ids` AND at least one other theme block holds less
than 10 %. Computed over distinct ids per theme, not raw counts.

**Reason emitted**:
`Themenverteilung unausgewogen: {theme_title} hält {share}% der Quellen, andere unter 10%.`

**Goal emitted**:
`Bringe Quellen zwischen den Themenblöcken in Balance; verschiebe
Belege oder spalte das dominante Thema bei Bedarf in zwei.`

**Affected instance_ids**: every theme block above 60 % AND every theme
block below 10 %. Capped at 6 (3 high, 3 low).

**Implementation note**: per-theme set of evidence_ids (parsed from
`used_reference_ids[]` of the block); compute share over union of all
theme sets; flag.

**Example trigger**: 6 theme blocks, theme_3 cites 18 of 24 distinct
sources, theme_5 cites 1, theme_6 cites 1 → trigger.

**Example clean**: 6 theme blocks, each citing 4–6 distinct sources,
overlap at most 2 sources between any pair.

---

### LINT-LR-NO-GAPS-SECTION: literature_review lacks a substantial gaps_and_open_questions block

**Applies when**: `report_type_id == "literature_review"`.

**Triggers when**: The blueprint defines `gaps_and_open_questions` as a
required block AND that block is missing from `committed_blocks` OR
its character count is below `min_chars * 0.65`.

**Reason emitted**:
`literature_review-Lauf ohne ausgearbeiteten gaps_and_open_questions-Block ({chars} Zeichen, Soll mindestens {min_chars}).`

**Goal emitted**:
`Schreibe einen substantiellen gaps_and_open_questions-Block (mindestens {min_chars} Zeichen), der pro Thema offene Fragen explizit benennt.`

**Affected instance_ids**: the missing or thin `gaps_and_open_questions`
block (synthetic instance_id if absent so the manager knows what to
add).

**Implementation note**: blueprint lookup + character count.

**Example trigger**:
A literature_review with thematische Synthese but only a 200-Zeichen
Schluss "Offene Fragen bleiben."

**Example clean**:
A literature_review whose `gaps_and_open_questions` block lists at
least one open question per theme, each with a short rationale and an
evidence-anchored boundary of current knowledge.

---

## Severity rules

`release_guard_check` MUST classify every fired lint into one of three
severities and combine them as follows when computing the final
`ready_to_finish` and `needs_revision` flags:

- **Hard lints** — `ready_to_finish = false` if any of these fire,
  `needs_revision = true`:
  - LINT-FAB-DOI
  - LINT-FAB-ARXIV
  - LINT-FAB-AUTHOR
  - LINT-UNCITED-CLAIM
  - LINT-CITED-BUT-MISSING
  - LINT-DOI-NOT-RESOLVED
  - LINT-EVIDENCE-FLOOR
  - LINT-DUPLICATE-RATIONALE
  - LINT-VERDICT-MISMATCH
  - LINT-AXIS-COMPLETENESS
  - LINT-RUBRIC-MISMATCH
  - LINT-MIN-CHARS (only on required blocks)
  - LINT-MISSING-DISCLAIMER
  - LINT-INVERTED-PERSPECTIVE
  - LINT-DEAD-PHRASE
  - LINT-META-PHRASE
  - LINT-FILLER-OPENING
  - LINT-UNANCHORED-HEDGE

- **Soft lints** — `needs_revision = false` for these alone; they
  surface in `goals`/`reasons` as warnings and do not block release:
  - LINT-MAX-CHARS
  - LINT-CONSULTANT-OVERUSE
  - LINT-DUPLICATE-SECTION-OPENING
  - LINT-EVIDENCE-CONCENTRATION when the share is in the band
    `(60%, 80%]`

- **Hard lints — conditional on report_type** (run only when the
  applicability matrix at the top of this file marks `✓`; skipped
  silently when blank):
  - LINT-MISSING-DISCLAIMER (feasibility, technology_screening, market_research)
  - LINT-DUPLICATE-RATIONALE (matrix-bearing types only)
  - LINT-VERDICT-MISMATCH (verdict-bearing types only)
  - LINT-AXIS-COMPLETENESS (matrix-bearing types only)
  - LINT-RUBRIC-MISMATCH (matrix-bearing types only)
  - LINT-MR-COMPETITOR-NAMELESS (market_research, competitive_analysis)
  - LINT-WP-THESIS-DRIFT (whitepaper)
  - LINT-WP-EVIDENCE-MISSING-FOR-CLAIM (whitepaper)
  - LINT-DB-RECOMMENDATION-BURIED (decision_brief)
  - LINT-DB-HEDGE-RECOMMENDATION (decision_brief)
  - LINT-LR-NO-GAPS-SECTION (literature_review)

- **Soft lints — multi-report extensions** (warning only):
  - LINT-MR-UNQUANTIFIED-MARKET
  - LINT-MR-METHOD-MISSING
  - LINT-MR-SEGMENT-WITHOUT-SIZE
  - LINT-WP-FILLER-OPENING
  - LINT-DB-CRITERIA-WITHOUT-WEIGHTS
  - LINT-LR-THEME-IMBALANCE

- **Critical lints** — flip `needs_revision = true` even when no other
  hard lint fires:
  - LINT-FAB-DOI
  - LINT-FAB-AUTHOR
  - LINT-UNCITED-CLAIM
  - LINT-DUPLICATE-RATIONALE
  - LINT-VERDICT-MISMATCH
  - LINT-DB-HEDGE-RECOMMENDATION
  - LINT-EVIDENCE-CONCENTRATION when the share is `> 80%`

Resolution order when assembling the output payload:

1. Run all lints, collect `{lint_id, severity, instance_ids, reason,
   goal}` issues.
2. `ready_to_finish` = no hard or critical lints fired.
3. `needs_revision` = any hard or critical lint fired.
4. `candidate_instance_ids` = unique union, capped at 6, with hard +
   critical lints prioritised over soft when capping.
5. `goals` = unique, capped at 8.
6. `reasons` = unique, capped at 6.
7. `summary` follows the German one-liner template specified in the
   output contract above.

This catalogue is exhaustive for the current asset pack. New lints may be
added when the asset pack grows new structural elements; existing lints
must remain id-stable so that downstream tooling (turn logs, mission
review entries) can reference issues by `lint_id`.

## How `release_guard_check` reads applicability

`release_guard_check` is dispatched by the manager after every
`apply_block_patch` and once before every `finished` decision. It is
deterministic; no LLM call. The dispatcher uses the applicability matrix
at the top of this file together with the run's `package_context` to
decide which lints actually run.

```python
def run_release_guard(workspace_state, package_context):
    issues = []
    rt_id = package_context["report_type_id"]
    rt    = package_context["report_type"]   # resolved object from asset_pack
    for lint in LINT_CATALOGUE:
        if not applies(lint, rt_id, rt):
            continue
        for hit in lint.check(workspace_state, package_context):
            issues.append({
                "lint_id":      lint.id,
                "severity":     lint.severity,        # hard | soft | critical
                "instance_ids": hit.instance_ids,
                "reason":       hit.reason,
                "goal":         hit.goal,
            })
    return assemble_payload(issues)


def applies(lint, rt_id, rt):
    """Mirrors the matrix table at the top of this file."""
    cell = APPLICABILITY[lint.id].get(rt_id)
    if cell is None or cell == "":
        return False
    if cell == "conditional":
        return lint.conditional_predicate(rt)   # type-defined, e.g.
                                                # "block X exists in
                                                # block_library_keys"
    return True   # cell == "✓"
```

The supported predicate vocabulary inside `applies_when` clauses (and the
conditional-cell predicates) is intentionally small so the dispatcher
stays scalar:

| predicate | semantics |
|-----------|-----------|
| `report_type_id == "X"` | exact match |
| `report_type_id in {X, Y, Z}` | enum membership |
| `report_type has matrix in block_library_keys` | true if any of `screening_matrix`, `scenario_matrix`, `screening_matrix_short`, `competitor_matrix` is in `report_type.block_library_keys[]` |
| `report_type.verdict_line_pattern != null` | the type emits a quantified verdict line |
| `<block_id> in report_type.block_library_keys` | the block is part of this type's blueprint |

Anything more complex belongs in the lint's own `check(...)` body, not in
the applicability gate. This separation keeps the matrix readable and
the per-lint Rust implementation thin.

`assemble_payload(issues)` follows the resolution order specified above
(hard + critical → not ready, soft alone → ready but with warnings;
caps at 6 instance_ids, 8 goals, 6 reasons; German one-liner summary).
The dispatcher MUST emit issues in deterministic order
(lint_id ascending, then instance_id ascending) so that the same workspace
state always produces the same payload — turn ledger replays depend on
this.

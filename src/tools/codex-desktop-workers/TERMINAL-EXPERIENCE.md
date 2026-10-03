# Terminal PR learning report

The report uses the canonical `~/.codex/proxy-workers/` registry. It does not create
proxy jobs for native actors or change existing selection, approval or merge records.

```sh
python3 scripts/terminal_report.py collect
python3 scripts/terminal_report.py build
python3 scripts/terminal_report.py assess /absolute/path/assessment.json
```

Outputs: `MODEL-EXPERIENCE.html` and identical embedded `MODEL-EXPERIENCE.json`.
The HTML is self-contained, with separate parent/worker leaderboards and a
paginated terminal PR list. No server, npm installation or external assets.

Collection requests only MERGED/CLOSED CTOX, Workjet and ctox-dev PRs, plus existing
external registry PRs. GitHub reviews, changed-file summaries and exact-head
check snapshots remain distinct from installed acceptance. Large connections
report truncation. Content-addressed snapshots and assessment revisions are
durable. STOP architecture PRs are inventory-only.

Unified assessments use five separately justified 0–10 criteria: correctness
30%, assignment fulfillment 25%, evidence quality 20%, closure quality 15%,
efficiency 10%. Essential defects/missing core requirements cap the result at 4.
Unknown evidence needs a specific reason; no zero substitution. Legacy ratings
retain their original methodology and cannot supply invented component values.

Each assessment identifies the terminal PR, actor, role, package, independently
validated scope, reviewer, immutable first/corrected heads, evidence, rework and
cause. Model attribution requires the actual authoring turn, model and rollout
reference. Parent self-assessments are rejected. Revisions are append-only, read
back after writing, and contribute only their latest actor/package observation.
Unknown models are excluded from model means. First/corrected results and parent
phase observations cannot inflate independent sample counts.

`terminal_provenance.py` recovers candidate actor evidence through Greppy and
read-only Codex metadata. Head references and PR mentions are leads; assessors
must verify authorship and obligations before assigning credit. Native leaf
histories remain native evidence and never fabricate registry selections.

At an actual terminal PR disposition or a genuine corrected assessment, refresh
the existing evidence and rebuild the report as part of the owning lifecycle
handoff. Reuse the existing supervisory review; do not add a watcher, automation,
polling ledger or acknowledgment gate. Report builds are local and network-free;
collection is explicit. Live PRs are never scored by this interface.

Narrow verification:
```sh
greppy bash-smart -- python3 -B -m unittest test_terminal_report
```
Run from `scripts/`, with disposable fixtures routed to the designated tmp volume.
Broad provenance scans, indexing and broad suites require the shared admission gate.


The current HTML has a four-column parent leaderboard (harness/model, PR count,
one PR-completion score, correction count), a five-column worker leaderboard
(harness/model, PR count, first score, final score, correction count), and a
seven-column PR list (PR link/description, state, combined parent/worker identity,
parent score, worker first score, worker final score, correction count).
All columns sort on click; numeric values sort numerically and unknowns stay last
in either direction. PR sorting applies to the entire filtered list before pagination.
Scores use bars on the common 0–10 scale.

Parent assessments store a separate parent_completion result for the actual
review/integration/checks/merge or justified closure responsibility. Historical
first/corrected broad-outcome ratings remain immutable in the assessment records.
Completion models come from the actual closing/action turn. Missing completion
evidence cannot silently reuse a broader product-outcome score.

Correction counts are evidenced absolute iterations after PR publication,
including Draft corrections, through merge/close. They are not percentages,
commit counts, number of test attempts, or number of assessment revisions.
Actor counts are attributed to actual evidenced models; a whole-PR count requires
deduplicated correction evidence or an explicit verified PR-scoped count.
Partial counts cannot pretend to be a complete total.

The SVG scatter plot uses the parent completion score on X and worker score on Y.
First/final points and first-to-final arrows are switchable. Each actual
parent/worker harness/model combination has an editable color and visibility.
The JSON stores the same computed pairs, record IDs and historical stage models.
Pairs require a proved parent-child edge on the same terminal PR; missing
endpoints never become zero or an invented arrow.

Lightweight UI verification executes the real generated script with a static DOM
harness, checking clickable headers, both sort directions, null ordering, full-list
pagination, exact column counts, score bars, true pairing, point/arrow modes,
color and visibility controls. It does not claim real-browser visual acceptance:
greppy bash-smart -- node scripts/test_terminal_report_ui.cjs /absolute/path/MODEL-EXPERIENCE.html

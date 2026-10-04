# Terminal PR learning report

The canonical local registry/reviews live in ~/.codex/proxy-workers/. The offline
MODEL-EXPERIENCE.html embeds the identical MODEL-EXPERIENCE.json. No server,
external assets or npm installation are required.

Run scripts/terminal_report.py collect to collect MERGED/CLOSED CTOX, Workjet,
ctox-dev and existing external-registry PR evidence; build recalculates the
offline report; assess /absolute/path/assessment.json appends an independent
review. Build is network-free. STOP is inventory-only; OPEN/DRAFT is not scored.

All actors use five justified0–10 criteria: correctness30%, fulfillment25%,
evidence20%, closure15%, efficiency10%. Essential defects/missing core
requirements cap the result at4. Missing evidence needs a reason and is never0.
Legacy totals cannot supply invented criterion ratings.

Each record identifies PR, actor/role/package/scope, independent reviewer,
immutable source head, actual historical turn/model, concrete review evidence,
lifecycle states, failure cause and proved corrections. Self-grading is rejected.
Revisions are append-only/read back; only the latest logical actor/package
record is pooled. Historical first/final results remain immutable.

A review-ready uncommitted delivery uses source_snapshot with its actual base
commit, complete retained Git patch, SHA256 and historical delivery evidence.
Its head stays null: the base or a later corrected commit is not the original
authored head. Recording verifies the patch hash before appending the rating.

Parents get one parent_completion score for the actual PR review/integration/
checks/merge or justified closure, attributed to the actual closing/action turn.
Broader product-acceptance grades stay historical; completion cannot silently
reuse them. Parent leaderboard: model/harness, PRs, score, average corrections per PR.
Worker leaderboard: model/harness, PRs, first, final, average corrections per PR.
PR list: link/description, status, combined parent/worker, parent score,
worker first, worker final, corrections.

Display identities use model (@codex). Observed harness metadata stays in the
original records. All headers are clickable/keyboard accessible. Numeric sort
is numeric; unknowns stay last both ways. Sort the full filtered list before
pagination. Score bars use the0–10scale. Worker first/final means and PR count
use the identical paired PR cohort and actual model. Unpaired evidence stays
in JSON/PR list with explicit comparison_excluded counts.

Corrections are evidenced absolute iterations after PR publication (including
Draft) through merge/close, not percentages or inferred commit/test/rerating/
score-difference counts. Leaderboards show the mean absolute iteration count over the same PR cohort
used for scores (11 iterations across 2 PRs = 5.5); the PR list retains each
individual absolute count. JSON preserves both mean and raw iteration total.
Unknown is not zero. Whole-PR totals require explicit
PR scope or deduplicated complete history; parent/worker counts can overlap.
A sole actor count does not establish a whole-PR total. Actor event union is
used only when the review explicitly attests complete PR correction history.

Scatter X is parent completion, Y is worker. Switch first/final points/arrows.
The global filter bar is removed. One compact combination dropdown and color field
replace a repeated legend. All combinations displays all points equally;
selecting a combination highlights its points and fades the others.
Absent first/final model endpoints normalize to the same visible combination.
The JSON contains computed pairs/record IDs/stage models. Require a proved
parent-child edge on the same PR; never fabricate missing points/arrows.

terminal_provenance.py reads candidates through Greppy/read-only Codex metadata.
A PR/head mention or registry assignment is not authorship proof. Native actors
do not become invented proxy jobs. Refresh at actual terminal disposition/review
using the existing lifecycle; no watcher/automation/extra acknowledgment gate.

Run narrow checks through greppy bash-smart. Python unittest checks rubric,
immutable revisions, terminal boundaries, weighting, cohort identity and JSON
integrity; route temporary fixtures to the designated tmp volume. UI command:
node scripts/test_terminal_report_ui.cjs /absolute/path/MODEL-EXPERIENCE.html
It executes the generated script in a static DOM harness to verify sorting,
pagination, columns, actual chart edges/modes, unique model/harness selection
and color changes. It does not claim real-browser visual acceptance.
Broad suites/indexing require the shared admission gate.

Parent leaderboards use the full proved correction count for each compared PR,
including corrections authored by workers on other models. Worker comparisons
retain author-specific counts. Unknown whole-PR history is never zero.

Explicit user-requested terminal URLs in terminal-evidence/explicit-terminal-prs.json
are retained by collection alongside the configured repositories and existing
external registry cases. Merge target branches are captured as baseRefName;
an integration-branch merge does not imply delivery to main.

Proved pre-inference failures are retained separately as non_delivery_attempts
and cannot contribute numeric assessments or delivery statistics. Numeric
assessments cannot be hidden with this classification. Native Grok deliveries
retain the actual session/turn-number identity and raw authoring model/harness;
the compact UI displays the native harness as @grok.

Native Claude assessments retain the actual entrypoint from each exact
assessed assistant event alongside session, turn UUID, model and timestamp.
The compact display normalizes proved claude-desktop to @claude while raw
client metadata remains in JSON and immutable assessment history. Unknown
clients are preserved rather than inferred from a model name.

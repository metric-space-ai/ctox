# CTOX Context Build Contract

The live model input is rebuilt for every turn. “Rebuilt” does not mean that
every available document is appended. The builder selects the smallest set of
authoritative signals that can change the model's next action.

## Physical input lanes

1. Base/system instructions define stable identity, operating rules, and tool
   behavior. They are installed when the harness thread starts and changing
   them rebuilds the process-local session contract before the next turn.
2. The current user request is sent once as the actual user message, without a
   generated wrapper.
3. The dynamic CTOX context is sent as one marked developer section. It is
   rebuilt each turn and replaces the older marked section in the actual model
   request.
4. Prior user, assistant, reasoning, and tool-call history remains harness
   history and is compacted by the existing harness compactor.
5. LCM is the durable source for selected summaries, referenced context items,
   and the fresh conversation tail. Full history is retrieval/audit data, not a
   default live-prompt payload.

## Dynamic build order and contribution

| Component | Source | Unique contribution | Inclusion rule |
| --- | --- | --- | --- |
| Skill dispatch | resolved skill binding | Tells the agent which specialized operating instructions to load | Only when a skill is actually bound |
| Verified evidence | mission assurance | Supplies facts already validated by durable mechanisms | Only with evidence payload |
| Strategy | active strategy snapshot | Constrains the current action by an explicitly active strategy | Only with configured strategy content |
| Anchors | continuity anchor head | Keeps stable decisions, identifiers, and non-negotiable facts available | Only with anchor payload |
| Focus | continuity focus head | States the active mission, current step, blocker, next step, and finish rule | Always; it is the short operational state |
| Execution contract | focus, workspace, workflow | Converts finish conditions and workspace scope into a concise exit gate | Always; generated from authoritative fields |
| Workflow state | queue, plan, ticket state | Identifies open durable work and prevents prose from replacing state transitions | Only for open, blocked, failed, or unavailable state |
| Narrative | continuity narrative head | Provides the minimum causal bridge needed to understand how the current state arose | Only with narrative payload |
| Governance | recent governance events | Surfaces a recent policy decision or recovery action that changes what may happen next | Only when recent events exist |
| Autonomy override | typed autonomy setting | Changes the default decision latitude | Only when not `Balanced` |
| Context health | deterministic health assessment | Warns about stale, contradictory, or incomplete context and names repair needs | Only when unhealthy, warning, or repair is present |
| Conversation evidence | ranked LCM working set | Provides task-relevant prior facts and recent exchanges not represented above | Only when selected entries or an omission marker exist |

The `PromptContextBreakdown` reports the characters that were actually
included, not the size of omitted candidate documents. A zero contribution is
therefore visible and intentional. Optional components must earn prompt budget
by carrying a signal. `LivePromptArtifact` exposes `user_input` and
`runtime_context` separately; its combined review view mirrors the physical
system/developer/user lanes rather than only showing the legacy merged preview.

## Selection and ordering

The builder first takes the bounded LCM working set. Conversation evidence is
ranked by term overlap with the current user request and then by recency. It is
still bounded by item and character limits. Omitted material is represented by
an omission marker so the agent can use `context_retrieve` deliberately rather
than receiving a silent or unbounded history dump.

The dynamic blocks are ordered from constraints and verified state toward
explanatory history. This keeps current action and completion conditions ahead
of narrative material.

## Cross-module invariants

- Exactly one current user request is present in the wire request.
- At most one marked CTOX runtime context is present in the wire request.
- A marked runtime context survives rollout/resume through the latest
  `TurnContextItem`.
- Old marked runtime contexts may remain in the append-only rollout for audit,
  but are not model-visible.
- Empty/default optional blocks consume no live prompt budget.
- Focus and the execution contract remain present even when every optional
  component is empty.
- The token preflight measures stable instructions plus the same dynamic
  context and user text sent to the model, and accounts for accumulated thread
  history using model token events.
- Changing the workspace updates the typed turn cwd; it is not merely a prompt
  hint.

## Audit procedure

The deterministic checks live beside their owners:

- `context::live_context` tests conditional inclusion, relevant-evidence
  ranking, current-request rendering, and bounded selection.
- `context_manager::history_tests` verifies that only the latest marked runtime
  context reaches the model while unrelated developer and conversation history
  survives.
- app-server/core checks verify the typed developer-instruction override and
  durable `TurnContextItem` path.
- direct-session and turn-loop checks verify exact/heuristic preflight behavior,
  thread reuse, cwd propagation, and compaction thresholds.

Release validation must inspect the actual request projection, not only the
diagnostic combined prompt artifact.

## Module audit result

| Area | Result | Evidence |
| --- | --- | --- |
| Stable system/base instructions | serves its purpose | default/override renderer tests plus per-slice session-contract comparison |
| Current request lane | serves its purpose | app-server protocol round-trip and `runtime_context_excludes_generated_current_request_wrapper` |
| Skill dispatch | serves its purpose, conditional | named/path dispatch tests |
| Verified evidence and strategy | serves their purpose, conditional | empty/signal payload tests; omitted candidates report zero characters |
| Anchors and narrative | serve their purpose, conditional | entry-aware clipping test and payload omission guards |
| Focus | serves its purpose, mandatory | mission-state fallback test |
| Execution contract | serves its purpose, mandatory | full-document workspace-root preservation test |
| Workflow state | serves its purpose, conditional | open ticket and relevance-scope tests |
| Governance, autonomy, health | serve exception handling, conditional | they render only for recent events, non-default autonomy, or unhealthy context |
| Conversation evidence | serves recall without bulk history | relevance-before-recency, mission-floor fallback, omission counts, and 50k-message LCM bound |
| Harness history projection | serves long-session hygiene | latest-marker-only request test; normal and compaction model requests share the projection |
| Rollout/resume | serves restart continuity | non-ephemeral named worker thread, persisted `TurnContextItem`, typed resume path |
| Token protection | serves bounded execution | physical-lane exact preflight plus model-usage-based history projection |

No additional live context block is justified by this audit. New blocks must
identify a decision they change, an authoritative source, an omission rule, and
a deterministic contribution check before being added.

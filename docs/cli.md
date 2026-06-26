# CTOX CLI Reference

This page is the command reference for CTOX. The root `README.md` intentionally does not duplicate this material.

## Service and Everyday Commands

Manage the persistent CTOX daemon loop and perform everyday interactions:

```sh
ctox                             # Open the TUI
ctox tui                         # Open the TUI
ctox start                       # Start the persistent mission loop daemon
ctox stop                        # Stop the daemon
ctox status                      # Show service status (JSON)
ctox version                     # Print the version and build info (JSON)
ctox doctor                      # Run environment health check
```

Submit prompts directly to the running daemon loop:

```sh
ctox chat "Run a git pull, cargo build, and tell me if it succeeded"
ctox chat "Analyze logs" --wait  # Block until the slice completes
ctox chat "Send update" --to boss@example.com --subject "Daily Report"
```

Configure allowed working-hours for the daemon to start active slices:

```sh
ctox work-hours set 08:00 18:00
ctox work-hours off
```

Inspect API model token and cost accounting:

```sh
ctox cost today
ctox cost daily
ctox cost week
ctox cost month
```

Inspect or end the current CLI turn ledger (tracking locking state):

```sh
ctox turn status
ctox turn end
```


## Updates

Inspect the current install layout and staged update state:

```sh
ctox update status
ctox update check
```

Configure the GitHub release channel that later `check` and remote `apply` calls will use:

```sh
ctox update channel set-github --repo metric-space-ai/CTOX
ctox update channel show
```

Adopt a legacy repo-root install into the managed `current` / `releases` layout:

```sh
ctox update adopt --install-root ~/.local/lib/ctox --state-root ~/.local/state/ctox
```

Stage and switch to a new source checkout without resetting the persistent state root:

```sh
ctox update apply --source /path/to/new/CTOX-checkout
ctox update apply --latest
ctox update apply --version v0.4.0
ctox update rollback
```

## Runtime / Boost

Switch the active chat model and runtime profile preset:

```sh
ctox runtime switch Qwen/Qwen3.6-27B quality --context 128k
ctox runtime switch openai/gpt-5.4 quality --context 128k --timeout 1800
```

Inspect or start a temporary chat runtime model boost lease:

```sh
ctox boost status
ctox boost start --minutes 60 --model Qwen/Qwen3.6-27B --reason "complex code migration"
ctox boost stop
```

Check local hardware acceleration capabilities, embedding, speech-to-text (STT), and text-to-speech (TTS) engines:

```sh
ctox doctor
ctox runtime embedding-doctor
ctox runtime embedding-smoke [--token-id 123]
ctox runtime stt-doctor
ctox runtime stt-smoke /path/to/audio.wav
ctox runtime stt-realtime-smoke /path/to/audio.wav
ctox runtime tts-doctor
ctox runtime tts-smoke --text "hello world"
ctox runtime openrouter-tool-smoke [--model model-id]
```

The legacy local proxy command and `chat-runtime-apply` have been removed. Runtime model access now goes through the managed internal gateway, Candle/integrated runtimes, and IPC runtime paths.

## Channels

Initialize and inspect the shared communication store:

```sh
ctox channel init
ctox channel list --limit 20
```

Sync inbound messages:

```sh
ctox channel sync --channel email --email you@example.com
ctox channel sync --channel jami --account-id jami:youraccount
```

Lease, acknowledge, and send messages:

```sh
ctox channel take --limit 5 --lease-owner codex
ctox channel ack email-import::example
ctox channel send --channel tui --account-key tui:local --thread-key local/test --body "hello"
```

## Plans

Create, mutate, and inspect persistent multi-step execution plans:

```sh
ctox plan init                                         # Initialize plan storage
ctox plan draft --title <title> --prompt <text>        # Draft a new multi-step plan
ctox plan ingest --title <title> --prompt <text>       # Ingest/create an active multi-step plan
ctox plan list                                         # List all plans
ctox plan show --goal-id <goal-id> [--json]            # Show details of a specific plan
ctox plan emit-next --goal-id <goal-id> [--json]       # Show the next executable step
ctox plan tick                                         # Progress all active plans by executing due steps
ctox plan complete-step --goal-id <id> --step-id <id> [--note <text>] # Mark a step as successfully completed
ctox plan fail-step --goal-id <id> --step-id <id> [--note <text>] # Mark a step as failed
ctox plan retry-step --goal-id <id> --step-id <id> [--note <text>] # Queue a step for retry
ctox plan block-step --goal-id <id> --step-id <id> --reason <text> # Mark a step as blocked
ctox plan unblock-step --goal-id <id> --step-id <id>   # Unblock a blocked step
```

## Queue

Inspect, manage, and audit the execution queue:

```sh
ctox queue add --title <label> --prompt <text> [--thread-key <key>] [--workspace-root <path>] [--skill <name>] [--priority <urgent|high|normal|low>] [--parent-message-key <key>] # Submit a task
ctox queue list [--status <pending|leased|blocked|failed|handled|cancelled>]... [--limit <n>] # List queued items
ctox queue show --message-key <key>                    # Show details for a queued item
ctox queue edit --message-key <key> [--title <label>] [--prompt <text>] [--thread-key <key>] [--workspace-root <path>] [--clear-workspace-root] [--skill <name>] [--clear-skill] [--priority <urgent|high|normal|low>] # Edit a task
ctox queue reprioritize --message-key <key> --priority <urgent|high|normal|low> # Update task priority
ctox queue block --message-key <key> --reason <text>   # Block execution of a task
ctox queue release --message-key <key> [--priority <urgent|high|normal|low>] [--clear-note] [--note <text>] # Release/unblock a task
ctox queue complete --message-key <key> [--note <text>] # Mark task successfully completed
ctox queue fail --message-key <key> --reason <text>    # Mark task as failed
ctox queue cancel --message-key <key> [--reason <text>] # Cancel task execution
ctox queue spill --message-key <key> [--ticket-system <name>] [--reason <text>] [--skill <name>] [--publish] # Spill task to external ticket tracker
ctox queue spill-candidates [--limit <n>]              # List tasks ready for external spilling
ctox queue spills [--state <spilled|restored>] [--limit <n>] # List spilled queue tasks
ctox queue restore --message-key <key> [--priority <priority>] [--note <text>] # Restore a spilled task back to queue
ctox queue cleanup-scope [--all-open] [--match-run-id <id>] [--match-thread-prefix <prefix>] [--dry-run] # Bulk clean/prune queue scope
ctox queue assert-clean-scope [--all-open] [--match-thread-prefix <prefix>] [--empty] # Assert queue scope matches conditions
ctox queue repair [--dry-run] [--mechanical]            # Run automated queue cleanup & diagnostic repair pass
```

## Schedule

Work with recurring cron-based or time-scheduled execution tasks:

```sh
ctox schedule init                                     # Initialize the schedule storage
ctox schedule add --name <label> --cron '<expr>' --prompt <text> [--thread-key <key>] [--skill <name>] # Add recurring task
ctox schedule list                                     # List scheduled tasks
ctox schedule pause --task-id <id>                     # Temporarily disable a scheduled task
ctox schedule resume --task-id <id>                    # Re-enable a paused scheduled task
ctox schedule remove --task-id <id>                    # Delete a scheduled task
ctox schedule run-now --task-id <id>                   # Force-trigger a scheduled task immediately
ctox schedule tick                                     # Progress queue scheduling and emit due tasks
```

## Follow-Up

Evaluate whether a slice is done, blocked, or needs continuation:

```sh
ctox follow-up evaluate \
  --goal "stabilize remote rollout" \
  --result "deploy script patched; smoke test still pending" \
  --step-title "patch deploy script" \
  --open-item "run remote smoke test"
```

## Scraping

Operate the durable scrape subsystem:

```sh
ctox scrape init
ctox scrape upsert-target --input /path/to/target.json
ctox scrape execute --target-key acme-jobs --allow-heal
ctox scrape show-api --target-key acme-jobs
ctox scrape query-records --target-key acme-jobs --where classification.category=job --limit 20
ctox scrape semantic-search --target-key acme-jobs --query "remote rust jobs"
```

## Documents

Index and read a local document corpus under explicit roots:

```sh
ctox doc corpus add-root --path ~/Documents
ctox doc corpus list
ctox doc formats
ctox doc index
ctox doc search --query "quarterly roadmap" --mode hybrid
ctox doc read --path ~/Documents/roadmap.pdf --query "milestones and owners"
ctox doc read --path ~/Documents/roadmap.pdf --find "budget" --find "deadline"
```

## Continuity and Memory (LCM)

Inspect and maintain the long-context memory (LCM) substrate:

```sh
# Long-Context Memory (LCM) CLI commands
ctox lcm-init runtime/ctox.sqlite3
ctox lcm-add-message runtime/ctox.sqlite3 <conversation-id> <role> <content>
ctox lcm-compact runtime/ctox.sqlite3 <conversation-id> [token-budget] [--force]
ctox lcm-grep runtime/ctox.sqlite3 <conversation-id|all> <scope> <mode> <query> [limit]
ctox lcm-describe runtime/ctox.sqlite3 <summary-id>
ctox lcm-expand runtime/ctox.sqlite3 <summary-id> [depth] [--messages] [token-cap]
ctox lcm-dump runtime/ctox.sqlite3 <conversation-id>
ctox lcm-refresh-continuity runtime/ctox.sqlite3 <conversation-id>
ctox lcm-show-continuity runtime/ctox.sqlite3 <conversation-id>
ctox lcm-run-fixture runtime/ctox.sqlite3 <fixture-path>

# Continuity substrate
ctox continuity-init runtime/ctox.sqlite3 <conversation-id>
ctox continuity-show runtime/ctox.sqlite3 <conversation-id> [narrative|anchors|focus]
ctox continuity-apply runtime/ctox.sqlite3 <conversation-id>
ctox continuity-log runtime/ctox.sqlite3 <conversation-id>
ctox continuity-forgotten runtime/ctox.sqlite3 <conversation-id>
ctox continuity-build-prompt runtime/ctox.sqlite3 <conversation-id>
ctox continuity-rebuild runtime/ctox.sqlite3 <conversation-id>

# Context health and retrieval
ctox context-health --db runtime/ctox.sqlite3 --conversation-id 1
ctox context-retrieve --db runtime/ctox.sqlite3 --conversation-id 1 --mode current
ctox context-stress --db runtime/ctox.sqlite3 --conversation-id 1
ctox chat-prompt-export
```

## Verification

Inspect verification runs and mission assurance evidence:

```sh
ctox verification init                                 # Initialize verification tables in the database
ctox verification assurance [--conversation-id <id>]    # Snapshot of the overall mission assurance status
ctox verification runs [--conversation-id <id>] [--limit <n>] # List recent slice verification runs
ctox verification claims [--conversation-id <id>] [--limit <n>] [--all] # List active or all mission claims
ctox verification claim-set --conversation-id <id> --kind <kind> --status <status> --subject <text> --summary <text> --evidence <text> [--blocks-closure] [--recheck-policy <always|on_change|never>] [--expires-at <epoch-ms>] [--last-run-id <id>] [--claim-key <id>] # Manually upsert a verification claim
```

## Browser Reference Workspace

Prepare the local Playwright reference workspace:

```sh
ctox browser install-reference
ctox browser doctor
ctox browser install-reference --install-browser
```

## Business OS

Manage the bundled WebRTC pairing room and runtime-installed apps for the Business OS frontend shell:

```sh
ctox business-os status                               # Show native/bundled status
ctox business-os peer status                          # Show active pairing configuration
ctox business-os peer rotate                          # Rotate signaling room and password
ctox business-os serve [--addr 127.0.0.1:8765]        # Serve Business OS static web app
ctox business-os app create --instruction "<text>"   # Queue runtime app creation
ctox business-os app modify <module-id> --instruction "<text>"
ctox business-os modules list|enable|disable          # Manage skill-app module options
ctox business-os skills list|enable|disable           # Manage packed skills
```

## Secrets and Credentials

Securely store and retrieve API keys, credentials, and configuration scopes:

```sh
ctox secret put --scope <scope> --name <name> --value "<value>"
ctox secret get --scope <scope> --name <name>
ctox secret list
```

## Skills Catalog

Manage modular system skills:

```sh
ctox skills list
ctox skills enable <skill-name>
ctox skills disable <skill-name>
```

## Strategic Directives

Inspect and mutate the high-level goals and mission directives for the daemon loop:

```sh
ctox strategy show [--conversation-id <id>|--thread-key <key>] # Show the active strategic snapshot
ctox strategy history [--conversation-id <id>|--thread-key <key>] [--kind <kind>] [--limit <n>] # View directives history
ctox strategy set --kind <kind> --title <text> (--body <text>|--body-file <path>) [--conversation-id <id>|--thread-key <key>] [--author <name>] [--reason <text>] [--status <active|proposed>] [--triggered-by-inbound <message_key>] # Create/upsert a strategic directive
ctox strategy propose --kind <kind> --title <text> (--body <text>|--body-file <path>) [--conversation-id <id>|--thread-key <key>] [--author <name>] [--reason <text>] [--triggered-by-inbound <message_key>] # Propose a strategic directive
ctox strategy activate --directive-id <id> [--decided-by <name>] [--reason <text>] [--triggered-by-inbound <message_key>] # Activate a proposed directive
```

## Governance

Inspect governance decisions, safety overrides, active block/gate state, and events:

```sh
ctox governance init                                  # Initialize the governance tables
ctox governance snapshot [--conversation-id <id>]      # Show the current active block/gate snapshot (alias: ctox governance status)
ctox governance inventory                             # List all registered safety and authority gates
ctox governance events [--conversation-id <id>] [--limit <n>] # List recent safety and authority gate event logs
```

## Mailserver

Manage integrated mailserver accounts for daemon intake/outbound reviewed send:

```sh
ctox mailserver list-domains                          # List registered mailserver domains
ctox mailserver add-domain <domain> [--selector <selector>] [--private-key <key>] # Add a new domain
ctox mailserver list-users                            # List registered users
ctox mailserver add-user <email> <password>           # Add a mail user account
ctox mailserver send-email --from <email> --to <email> --subject <subject> --body <body> # Send a test/outbound email
```

## Harness Flow

Render visual representation of active and support processes for a message or task chain:

```sh
ctox harness-flow [--latest] [--message-key <key>] [--work-id <id>] [--width <n>] [--json] # Render the flowchart (ASCII or JSON)
ctox harness-flow init                                # Initialize the harness flow event ledger
ctox harness-flow events [--message-key <key>] [--work-id <id>] [--ticket-key <key>] [--limit <n>] # List durable harness ledger events
```

## Forensic Process Mining

Investigate daemon liveness, mutation event logs, and structural conformance:

```sh
ctox process-mining ensure                             # Reinstall triggers and schema if needed
ctox process-mining schema                             # Inspect tables and trigger schema details
ctox process-mining inventory                          # List registered table triggers and versions
ctox process-mining events [--limit <n>]               # View raw database mutation events
ctox process-mining cases [--limit <n>]                # List case-id aggregate summaries
ctox process-mining case <case-id> [--limit <n>]       # Fetch raw events for a specific case
ctox process-mining explain-case <case-id> [--limit <n>] # Render directly-follows edges for a case
ctox process-mining objects [--limit <n>]              # View multi-perspective object frequencies
ctox process-mining transitions [--limit <n>]          # View transition matrix count distribution
ctox process-mining dfg [--limit <n>]                  # View directly-follows graph frequencies
ctox process-mining core-liveness                      # Run structural check on the core state machine
ctox process-mining spawn-liveness                     # Verify worker spawn models & liveness
ctox process-mining spawn-edges [--limit <n>]          # List parent-to-child subagent spawn trees
ctox process-mining deadlocks [--limit <n>]            # Detect activities that represent terminal dead ends
ctox process-mining violations [--limit <n>]          # Inspect raw protocol and transition violations
ctox process-mining scan-violations                    # Force-execute a protocol violation audit
ctox process-mining prune [--sqlite-access-window <n>] # Prune high-volume SQLite statement events only
ctox process-mining compact-payloads [--limit <n>] [--vacuum] # Compact oversized row snapshots; optional VACUUM reclaims disk
```

## Harness Mining

Forensic and conformance audit of the autonomous-agent harness retry loops and triggers against the core state machine:

```sh
ctox harness-mining brief [--stuck-min-attempts <n>] [--conformance-threshold <0..1>] [--drift-threshold <f>] # Cluster health brief
ctox harness-mining stuck-cases [--min-attempts <n>] [--idle-seconds <s>] [--limit <n>] # Detect stuck agent runs
ctox harness-mining variants [--entity-type <t>] [--limit <n>] [--cluster] # Trace variant clustering
ctox harness-mining sojourn [--entity-type <t>] [--limit <n>] # State holding time distribution
ctox harness-mining conformance [--lane <lane>] [--since <iso8601>] [--window <n>] [--fitness-threshold <0.0..1.0>] # Replay conformance replay fit
ctox harness-mining alignment [--entity-type <t>] [--limit <n>] # A* alignment synchronous check
ctox harness-mining causal [--violation-code <code>] [--lookback <n>] [--limit <n>] # Predecessor analysis for violations
ctox harness-mining drift [--window <n>] [--threshold <f>] # Page-Hinkley concept drift
ctox harness-mining multiperspective [--entity-type <t>] [--limit <n>] # Data-aware constraint check
ctox harness-mining audit-tick [--stuck-min-attempts <n>] [--conformance-threshold <0..1>] [--drift-threshold <f>] # Execute audit tick
ctox harness-mining findings [--status <status>] [--kind <k>] [--limit <n>] # List findings
ctox harness-mining finding-ack --finding-id <id> [--note <text>] # Acknowledge finding
ctox harness-mining finding-mitigate --finding-id <id> --by <by> [--note <text>] # Mitigate finding
ctox harness-mining finding-verify --finding-id <id> [--note <text>] # Verify finding mitigation
```

## Audit Trail Reset and Recovery

CTOX instruments every runtime table with SQLite triggers that record each mutation into the process-mining event log. This trail is what makes a mission auditable after the fact. Because the instrumentation sits in the write path, a corrupted event log or a bad trigger can amplify a bug — failing the very writes it only means to observe. `ctox reset` is the stable, scoped recovery for that situation. It clears or rebuilds the audit trail only; it never touches business data.

```sh
ctox reset process-mining                  # Dry-run: report what a soft reset would delete
ctox reset process-mining --confirm        # Soft reset: empty the recorded-data tables
ctox reset process-mining --hard --confirm # Hard reset: drop + rebuild triggers and schema
ctox reset harness-mining --confirm        # Clear harness-mining findings + audit-run log
ctox reset all --confirm                   # Soft-reset process-mining + harness-mining
ctox reset all --hard --confirm            # Hard-reset process-mining + clear harness-mining
```

- **Dry-run by default.** Without `--confirm`, `reset` only reports the row counts it would delete (as JSON) and changes nothing. Destructive runs require `--confirm` and execute inside a single transaction.
- **soft** (default) empties the recorded-data tables (`ctox_process_events`, `ctox_process_context`, the `ctox_pm_*` analysis tables, and the `ctox_core_transition_proofs` / `ctox_core_spawn_edges` evidence) while leaving the schema, mutation triggers, and transition-rule configuration in place.
- **`--hard`** (process-mining only) first drops every process-mining trigger so a broken instrumentation layer can no longer block live writes, then drops the process-mining tables and rebuilds a clean schema — reinstalling fresh triggers and re-seeding the default transition rules. This is the recovery path when the instrumentation itself is the problem.
- **Self-recording.** Every `ctox` invocation is itself recorded in the audit trail, so a confirmed reset is immediately followed by a small number of rows describing the reset command run. This is expected: the log faithfully records that a reset happened.
- For routine retention, use `ctox process-mining prune` to trim only the high-volume `sqlite-access:` events rather than wiping the trail. Use `ctox process-mining compact-payloads --vacuum` when old row snapshots contain oversized JSON/BLOB payloads and disk space must be reclaimed.

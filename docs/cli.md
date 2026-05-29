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
ctox runtime switch Qwen/Qwen3.5-27B quality --context 128k
ctox runtime switch openai/gpt-5.4 quality --context 128k --timeout 1800
```

Inspect or start a temporary chat runtime model boost lease:

```sh
ctox boost status
ctox boost start --minutes 60 --model Qwen/Qwen3.5-27B --reason "complex code migration"
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

Create and inspect persistent multi-step plans:

```sh
ctox plan init
ctox plan draft --title "remote rollout" --prompt "inspect host, patch deploy script, run smoke check"
ctox plan ingest --title "remote rollout" --prompt "inspect host, patch deploy script, run smoke check"
ctox plan list
ctox plan show --goal-id <goal-id>
ctox plan emit-next --goal-id <goal-id>
```

## Queue

Inspect and manage the execution queue:

```sh
ctox queue list
ctox queue add --title "run smoke check" --prompt "Run the remote smoke check on host X and summarize failures." --skill "follow-up-orchestrator" --priority high
ctox queue show --message-key <message-key>
ctox queue reprioritize --message-key <message-key> --priority urgent
ctox queue block --message-key <message-key> --reason "waiting for owner approval"
ctox queue release --message-key <message-key>
ctox queue complete --message-key <message-key> --note "smoke check passed"
```

## Schedule

Work with recurring or time-based work:

```sh
ctox schedule init
ctox schedule add --name "blocked review" --cron "0 * * * *" --prompt "review blocked tasks"
ctox schedule list
ctox schedule tick
ctox schedule run-now --task-id <task-id>
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
ctox verification assurance
ctox verification runs
ctox verification claims
```

## Browser Reference Workspace

Prepare the local Playwright reference workspace:

```sh
ctox browser install-reference
ctox browser doctor
ctox browser install-reference --install-browser
```

## Business OS

Manage the bundled WebRTC pairing room and customer repositories for the Business OS frontend shell:

```sh
ctox business-os status                               # Show native/bundled status
ctox business-os peer status                          # Show active pairing configuration
ctox business-os peer rotate                          # Rotate signaling room and password
ctox business-os serve [--addr 127.0.0.1:8765]        # Serve Business OS static web app
ctox business-os install --target <empty-dir>         # Install standalone Business OS repo
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

Inspect the high-level goals and mission directives for the daemon loop:

```sh
ctox strategy show
```

## Governance

Inspect governance decisions, safety overrides, and active block/gate state:

```sh
ctox governance audit
ctox governance list
```

## Mailserver

Manage integrated mailserver accounts for daemon intake/outbound reviewed send:

```sh
ctox mailserver list
ctox mailserver domain add <domain>
ctox mailserver user add <email> --password <pwd>
ctox mailserver test-send --to <recipient> --subject <subj> --body <body>
```

## Forensic Process Mining

Investigate daemon liveness, harness executions, mutation event logs, and conformance drift:

```sh
ctox harness-flow                                     # Render visual harness ASCII flowchart
ctox process-mining spawn-liveness                     # Verify worker liveness & contracts
ctox harness-mining stuck-cases                       # Detect stuck agent runs
ctox harness-mining variants                          # Analyze executed harness variants
ctox harness-mining multiperspective                   # Deep multiperspective conformance audit
```

## Audit Trail Reset and Recovery

CTOX instruments every runtime table with SQLite triggers that record each
mutation into the process-mining event log. This trail is what makes a mission
auditable after the fact. Because the instrumentation sits in the write path,
a corrupted event log or a bad trigger can amplify a bug — failing the very
writes it only means to observe. `ctox reset` is the stable, scoped recovery
for that situation. It clears or rebuilds the audit trail only; it never touches
business data.

```sh
ctox reset process-mining                  # Dry-run: report what a soft reset would delete
ctox reset process-mining --confirm        # Soft reset: empty the recorded-data tables
ctox reset process-mining --hard --confirm # Hard reset: drop + rebuild triggers and schema
ctox reset harness-mining --confirm        # Clear harness-mining findings + audit-run log
ctox reset all --confirm                   # Soft-reset process-mining + harness-mining
ctox reset all --hard --confirm            # Hard-reset process-mining + clear harness-mining
```

- **Dry-run by default.** Without `--confirm`, `reset` only reports the row
  counts it would delete (as JSON) and changes nothing. Destructive runs require
  `--confirm` and execute inside a single transaction.
- **soft** (default) empties the recorded-data tables (`ctox_process_events`,
  `ctox_process_context`, the `ctox_pm_*` analysis tables, and the
  `ctox_core_transition_proofs` / `ctox_core_spawn_edges` evidence) while leaving
  the schema, mutation triggers, and transition-rule configuration in place.
- **`--hard`** (process-mining only) first drops every process-mining trigger so
  a broken instrumentation layer can no longer block live writes, then drops the
  process-mining tables and rebuilds a clean schema — reinstalling fresh triggers
  and re-seeding the default transition rules. This is the recovery path when the
  instrumentation itself is the problem.
- **Self-recording.** Every `ctox` invocation is itself recorded in the audit
  trail, so a confirmed reset is immediately followed by a small number of rows
  describing the reset command run. This is expected: the log faithfully records
  that a reset happened.
- For routine retention (trimming only the high-volume `sqlite-access:` events
  rather than wiping the trail), use `ctox process-mining prune` instead.

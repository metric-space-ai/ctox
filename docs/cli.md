# CTOX CLI Reference

This page is the command reference for CTOX. The root `README.md` intentionally does not duplicate this material.

## Service

Manage the persistent CTOX loop:

```sh
ctox version
ctox start
ctox stop
ctox status
ctox
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

## Runtime

Apply local chat runtime presets:

```sh
ctox chat-runtime-apply openai/gpt-oss-20b quality
ctox chat-runtime-apply Qwen/Qwen3.5-35B-A3B performance
```

The legacy local proxy command has been removed. Runtime model access now goes through the managed internal gateway and IPC runtime paths.

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

## Continuity And Memory

Inspect the local continuity substrate:

```sh
ctox lcm-init runtime/ctox_lcm.db
ctox lcm-compact runtime/ctox_lcm.db 1
ctox lcm-show-continuity runtime/ctox_lcm.db 1
ctox context-retrieve --db runtime/ctox_lcm.db --conversation-id 1 --mode current
```

## Verification

Inspect verification runs and mission assurance:

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

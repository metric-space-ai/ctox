# Ops Insight Reporting Shapes

Use these patterns to summarize operational state from the existing CTOX substrate.

## Useful Inputs

```sh
ctox queue list --limit 50
ctox plan list
ctox schedule list
ctox status
```

Add host evidence only when it changes the conclusion:

```sh
uptime
df -h
free -h
ss -s
journalctl -p err -n 100 --no-pager
```

## Compact Scorecard Shape

- scope
- time window
- services reviewed
- incidents observed
- blocked work
- recurring pressure signals
- top three next actions

## Priority Backlog Shape

Rank items by:

1. user-visible risk
2. chance of repeat failure
3. operational drag
4. ease of mitigation

## Decision Brief Shape

- what changed
- what is currently risky
- what can wait
- what should be executed next

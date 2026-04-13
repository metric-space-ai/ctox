# Automation Patterns For CTOX

Use these patterns to automate repeated admin work without building a second control plane.

## Good Shapes

- repo script plus explicit invocation
- `ctox schedule add` for recurring checks or maintenance
- `ctox queue add` for deferred execution slices
- `ctox plan draft` for multi-step automation rollouts

## Commands

```sh
ctox schedule add --name "<name>" --cron "<expr>" --prompt "<prompt>" --skill "automation-engineering"
ctox queue add --title "<title>" --prompt "<prompt>" --skill "automation-engineering"
ctox plan draft --title "<title>" --prompt "<prompt>" --skill "automation-engineering"
ctox schedule list
ctox queue list
```

## Script Guidance

- keep inputs explicit
- exit non-zero on failure
- print enough state for a later operator to understand the run
- support dry-run flags when possible
- keep output parseable if the result will be consumed again

## Avoid

- background loops outside CTOX
- hidden cron edits that bypass `ctox schedule`
- scripts that mutate production with no precheck and no rollback path
- automations that only exist in prose and cannot be rerun consistently

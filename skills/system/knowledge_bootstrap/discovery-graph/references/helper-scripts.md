# Discovery Graph Helper Scripts

The scripts under `scripts/` are helper resources, not hidden authorities.

Use them because they save time and reduce repetition. Do not treat them as mandatory black boxes.

## Roles

- `linux_collect.py`
  - raw command capture helper
  - good default for single collectors or direct inspection
- `capture_run.py`
  - helper script for a full raw sweep
  - good default when you want one persisted `run_id` quickly
- `discovery_store.py`
  - CTOX persistence helper
  - use for schema init, raw capture storage, and normalized graph storage
- `discovery_query.py`
  - summary and export helper
- `normalize_minimum.py`
  - conservative bootstrap normalizer
  - use for a first graph or regression-safe fallback

## Expected Agent Behavior

In simple cases:

- run the helper
- inspect the output
- persist the result

In difficult cases:

- open the helper script
- inspect its assumptions
- patch it if needed
- or bypass it and work from raw commands plus the schema resources

## What Not To Do

- Do not assume a helper script is always correct.
- Do not skip raw evidence review just because a helper returned JSON.
- Do not let `normalize_minimum.py` become the only interpretation path.

## Rule Of Thumb

Use the helpers by default.  
Trust evidence more than helpers.  
Patch helpers when the same edge case would otherwise recur.

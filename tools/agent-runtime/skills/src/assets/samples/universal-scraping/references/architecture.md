# Universal Scraping Architecture

CTOX should not model scraping as "write one temporary file and hope we remember it later".

Use four layers:

1. Skill layer
   - stable instructions and helper tools
   - lives in `skills/system/universal-scraping/`
2. Registry layer
   - mutable metadata for targets, revisions, templates, runs, and artifacts
   - lives in `runtime/ctox_scraping.db`
3. Runtime workspace layer
   - mutable target-specific files
   - lives in `runtime/scraping/targets/<target_key>/`
4. Control-plane layer
   - queue and schedule state
   - lives in CTOX core commands such as `ctox queue` and `ctox schedule`

## Why Not Store Generated Scripts In The Skill Folder

Skill folders are reusable operating knowledge. They should stay small, inspectable, and portable.

Generated target scripts are operational state:

- they change frequently
- they may be target-specific or secret-adjacent
- they need revision history and run linkage
- they may be rotated or superseded without changing the skill itself

Putting them into the skill folder would blur stable capability and mutable runtime state.

## Hybrid Revision Model

For every target:

- the executable script exists as a real file under the runtime workspace
- the registry keeps the revision number, hash, reason, and body snapshot

That gives both:

- easy execution from a concrete file path
- stable metadata and history for later comparison and scheduling

For multi-source targets, this applies twice:

- one root script coordinates the scrape target as a whole
- source-local extractor modules can be revised independently per `source_key`

That keeps cross-source orchestration separate from source-specific extraction drift.

## Template Promotion Model

Provider-family logic should not be promoted from one lucky success.

Use three levels:

1. target revision
   - specific to one target
2. template example
   - a nominated reusable pattern for a provider family
3. promoted template
   - active reusable template backed by multiple examples or explicit override reasoning

This mirrors the useful part of the Ninja workflow while fitting CTOX's more explicit runtime model.

## Scheduling Model

Recurring scrapes should use normal CTOX scheduling:

- `ctox schedule add --skill "universal-scraping" ...`

The schedule record should name the `target_key`, not embed brittle extraction logic. The current target registry and script revision resolve the implementation at run time.

If the target aggregates several upstreams, the runtime source graph still stays under one scheduled `target_key`; CTOX should not create one shadow scheduler per source.

## Evidence Model

Use `runtime/ctox_scraping.db` for:

- operational scrape metadata
- script revisions
- promoted templates
- run and artifact bookkeeping
- materialized latest records
- semantic embedding cache for target records

Use the shared evidence kernel only for compact cross-skill evidence when that helps later discovery, reliability, or automation reasoning.

## Default API Model

Each target should ship with a default runtime API surface, not just raw run artifacts.

Default endpoints:

- `/ctox/scrape/targets/<target_key>/api`
- `/ctox/scrape/targets/<target_key>/records`
- `/ctox/scrape/targets/<target_key>/semantic`
- `/ctox/scrape/targets/<target_key>/latest`

This mirrors the useful Ninja pattern:

- exact filters over structured or classified fields
- semantic retrieval over selected text fields through embeddings
- an editable postprocessing template for optional LLM enrichment

## Enrichment Model

Do not hardcode one postprocessing behavior into CTOX.

Instead, each target workspace should carry editable templates for:

- exact-filter classifications
- structured extraction
- summaries or semantic synopses

Those templates are the default path the agent should reuse and adapt. If a target needs something special, the agent can diverge from the template instead of inventing everything from zero.

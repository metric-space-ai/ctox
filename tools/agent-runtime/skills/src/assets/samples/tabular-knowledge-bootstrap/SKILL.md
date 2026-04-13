---
name: tabular-knowledge-bootstrap
description: Build reusable knowledge from tabular or record-shaped source systems such as ticket exports, CMDB tables, monitoring inventories, CSV/XLSX sheets, SQL query results, or JSON arrays by first normalizing them into the shared SQLite discovery kernel and only then projecting them into domain-specific knowledge planes.
metadata:
  short-description: Normalize tabular source systems into reusable discovery knowledge
---

# Tabular Knowledge Bootstrap

Use this skill when CTOX is dealing with a source whose reality is primarily exposed as rows, records, worksheets, exports, query results, or list endpoints.

Examples:

- ticket system exports
- CMDB and asset tables
- monitoring inventories
- user/role directories
- service catalog spreadsheets
- SQL query results
- CSV, TSV, XLSX, or JSON array datasets

This skill exists so CTOX does **not** reinvent one discovery method per system.

Read these references when the problem moves beyond file parsing:

- [contracts/tabular-knowledge-taxonomy.md](../../../contracts/tabular-knowledge-taxonomy.md)
- [references/method.md](references/method.md)
- [references/artifacts.md](references/artifacts.md)

The rule is:

1. capture the tabular source as evidence
2. interpret it into the shared discovery graph
3. only then project the relevant subset into a domain-specific knowledge plane such as tickets, monitoring, access, or service mapping

Do not build a ticket-only discovery path when the real source is a generic table-like system.

## When To Use This Skill

Use this skill when the main problem is:

- understanding a foreign system whose important facts arrive as rows or records
- turning exported tables into services, assets, teams, labels, queues, or ownership facts
- building a reusable knowledge base from structured data instead of prose
- avoiding system-specific onboarding logic for every vendor

Do not use this skill when the source of truth is primarily:

- host commands and runtime state: use `discovery_graph`
- live service health and saturation: use `reliability_ops`
- a live outage: use `incident_response`
- routine ticket execution after knowledge already exists: use `ticket-knowledge` or `ticket-knowledge-maintenance`

## Core Architecture

This skill reuses the existing shared discovery SQLite kernel:

- `discovery_run`
- `discovery_capture`
- `discovery_entity`
- `discovery_relation`
- `discovery_evidence`

Use `skill_key=tabular_knowledge_bootstrap`.

The consumer system is never the authority.
The canonical path is:

1. raw tabular evidence
2. source profile
3. taxonomy candidates
4. promoted taxonomy buckets with examples
5. downstream projection

## Source Shapes

Supported shapes:

- `.csv`
- `.tsv`
- `.xlsx`
- `.json` arrays of records
- SQL result exports
- API list responses that are naturally record-shaped

If it behaves like rows and columns, this skill applies.

## Helper Composition

Useful helpers:

- `spreadsheet` for `.xlsx` and sheet-safe inspection
- `discovery_graph` storage model and persistence expectations
- downstream consumers such as `ticket-knowledge-maintenance` only after taxonomy promotion

Helpers are not the authority.
Raw evidence is the authority.
The agent owns interpretation.

## Mandatory Phases

Every serious run must move through these phases in order:

1. `structure`
   - identify tables, headings, row families, likely keys, and enum-like columns
2. `source-profile`
   - classify each important column by role
3. `taxonomy-candidates`
   - propose plausible classification dimensions
4. `bucket-refinement`
   - cluster rows into defensible buckets and reject noise
5. `example-selection`
   - pick canonical examples, common examples, and edge cases
6. `promotion`
   - only stable dimensions and buckets become reusable knowledge
7. `projection`
   - project promoted results into ticket, monitoring, access, or service knowledge

Do not skip from `structure` to `projection`.

## Minimum Durable Artifacts

By the end of a successful run, CTOX should be able to point to:

- one `source profile`
- at least one `taxonomy dimension`
- at least one `bucket` under that dimension
- representative `examples` for that bucket
- a declared downstream `projection target`, if projection was needed

See [references/artifacts.md](references/artifacts.md) for the exact artifact set.

## Completion Gate

Do not report success unless all of the following are true:

- the source scope is explicit
- the source profile is explicit
- at least one promoted taxonomy dimension exists
- each promoted bucket has representative examples
- ambiguous rows are either excluded or explicitly marked as unresolved
- any downstream projection names the upstream taxonomy it depends on

If you only have raw rows, cardinalities, or a prose summary, the work is not complete.

## Output Rule

The main reusable output is not a prose memo.
It is a promoted taxonomy view with examples that other CTOX systems can consume.

## Operator Feedback Contract

Answer for the operator first.

Use these exact headings:

- `**Status**`
- `**State**`
- `**Source Scope**`
- `**Autonomous Actions**`
- `**Escalation**`
- `**Current Findings**`
- `**Next Step**`

`State` must be one of:

- `proposed`
- `prepared`
- `executed`
- `blocked`

The operator-facing answer must not begin with database table names, raw row counts without context, or parser implementation details.

## Guardrails

- Do not create a vendor-specific discovery ontology unless the generic one clearly fails.
- Do not push raw table dumps into ticket notes or operator surfaces.
- Do not confuse a column heading with a confirmed semantic meaning.
- Do not bypass the shared discovery graph when the source is structurally tabular.
- Do not promote a taxonomy without representative examples.
- Do not let downstream projections invent their own categories independently of the promoted taxonomy layer.
- Do not treat one noisy export as final truth; prefer incremental refinement.

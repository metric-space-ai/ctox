---
name: ticket-monitoring-ingest
description: Use when CTOX can learn service, process, or infrastructure reality from monitoring systems and should project that evidence into the ticket knowledge plane.
metadata:
  short-description: Ingest monitoring evidence into ticket knowledge
cluster: ticket_integration
---

# Ticket Monitoring Ingest

Use this skill when monitoring systems such as Prometheus, Grafana, uptime checks, or service dashboards can improve ticket understanding.

## Core Rule

Monitoring data is not ticket prose. Ingest it into the SQLite-backed knowledge plane so later ticket work can load it through the normal knowledge path.

Monitoring snapshots alone do not mean the full ticket+knowledge pipeline is healthy. They are one SQLite-backed knowledge domain, not proof that mirrored tickets, source bindings, runbooks, or desk skills exist.

## Command

```sh
ctox ticket monitoring-ingest --system "<system>" --snapshot-json '<json>' [--key "<name>"] [--title "<text>"] [--summary "<text>"] [--status "<value>"]
```

## Recommended Snapshot Shape

```json
{
  "sources": [{"name": "prometheus"}],
  "services": [{"name": "vpn"}],
  "alerts": [{"name": "vpn-down", "severity": "critical"}],
  "assets": [{"name": "vpn-gateway-01"}]
}
```

## Operating Pattern

1. Pull the monitoring facts you actually need.
2. Ingest them into `monitoring_landscape`.
3. Re-run the normal ticket knowledge load before classifying or executing ticket work.
4. If monitoring is the only populated domain, say so explicitly. Do not let monitoring snapshots masquerade as a complete ticket knowledge plane.

## Important Boundaries

- Do not leave critical monitoring evidence only in temporary notes.
- Do not invent monitoring state when no snapshot is available.

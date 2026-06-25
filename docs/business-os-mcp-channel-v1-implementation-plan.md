# Business OS MCP Channel v1: Implementation Plan

Status: Planungsstand 2026-05-27

Ziel: `Business OS MCP Channel v1` wird ein offizieller CTOX
Kommunikationskanal fuer ChatGPT und andere MCP Clients. Der Kanal erlaubt
Business-OS-Interaktion ueber typisierte Tools fuer Module, Records, Commands,
Runs, Artefakte, Freigaben und Handoffs, ohne Business OS als Browser-Shell zu
duplizieren und ohne den CTOX DB / RxDB / WebRTC Datenvertrag zu umgehen.

## Readiness Statement

Die Implementierung ist sinnvoll, aber nur wenn MCP als eigener CTOX Channel
behandelt wird. Ein generisches `run_cli` oder eine HTTP-RxDB-Proxy-Schicht waere
kein akzeptabler Produkt- oder Sicherheitsvertrag.

Der Kanal ist:

- ein offizieller Inbound-/Outbound-Kommunikationskanal fuer delegierbare Arbeit,
  Statusabfragen, Ergebnisabruf und Freigaben
- ein typisierter Business-OS-Zugriff ueber Module, Entities, Records,
  Commands, Runs, Artifacts, Approvals, Activity und Deep Links
- ein ChatGPT-kompatibler MCP Server mit optionalem spaeterem Widget

Der Kanal ist nicht:

- ein Remote Desktop fuer Business OS
- ein allgemeiner CLI- oder Shell-Zugang
- ein HTTP-Datenproxy fuer Business-OS-Collections
- ein Ersatz fuer CTOX DB / RxDB / WebRTC
- ein Weg, Authoritative Commands ohne CTOX Validierung auszufuehren

Parallel zum MCP Server entsteht ein extern installierbarer Agent Skill. Dieser
Skill ist nicht fuer den internen CTOX Worker-Skill-Store gedacht, sondern als
GitHub-distributierbares Integrationsartefakt fuer andere Agent-Umgebungen. Er
erklaert einem externen Agent, wann und wie der CTOX Business OS MCP Server zu
verwenden ist, welche Tool-Grenzen gelten, welche Aktionen approval-gated sind
und was zu tun ist, wenn kein MCP Server verbunden ist.

## Architecture Thesis

Business OS bleibt local-first und P2P:

```text
Business OS Browser <-> CTOX DB / RxDB / WebRTC <-> CTOX Instance
```

Der MCP Channel ist ein separater Kommunikationskanal:

```text
ChatGPT / MCP Client
  -> Business OS MCP Server
    -> CTOX Channel Adapter
      -> CTOX Runtime / Business OS Store / Command Validation
```

Fuer private CTOX Instanzen braucht ChatGPT einen oeffentlichen HTTPS
Touchpoint. Dieser Touchpoint ist kein Business-OS-Datenproxy, sondern ein MCP
Gateway/Rendezvous:

```text
ChatGPT
  -> https://mcp.ctox.dev/mcp
    -> Auth, Routing, Rate Limits, Audit
      -> outbound session from private CTOX instance
        -> local CTOX runtime
```

Der empfohlene Managed-Mode nutzt `ctox.dev` auf Cloudflare. Dev-Mode kann mit
lokalem Server plus `cloudflared tunnel` oder `ngrok` starten. Enterprise-Mode
laesst Kunden einen eigenen MCP Endpoint hosten.

## Channel Identity

Jeder MCP Tool Call muss als eigener CTOX Channel-Eintrag nachvollziehbar sein.

Canonical channel metadata:

```json
{
  "channel": "chatgpt_mcp",
  "surface": "business_os_mcp",
  "actor": "chatgpt:<subject>",
  "workspace": "<ctox-instance-or-workspace>",
  "tool": "<tool-name>",
  "request_id": "<stable-request-id>",
  "confirmation_state": "not_required|required|approved|rejected",
  "created_at_ms": 0
}
```

Business-OS-Command-Metadaten:

```json
{
  "inbound_channel": "chatgpt_mcp",
  "client_context": {
    "channel": "chatgpt_mcp",
    "surface": "business_os_mcp",
    "actor": "chatgpt:<subject>",
    "requires_confirmation": true
  }
}
```

## Core Object Model

MCP v1 bildet Business OS ueber diese stabilen Objekte ab:

| Objekt | Bedeutung |
| --- | --- |
| `Module` | Installierte Business-OS-App oder Capability-Bereich |
| `Entity` | Modul-spezifischer Datentyp, z.B. Customer, Ticket, Campaign |
| `Record` | Einzelnes Business-OS-Objekt mit ID, Summary, Fields und Links |
| `Action` | Typisierte moegliche Aktion fuer Modul, Record oder Selection |
| `Command` | Authoritative Business-OS- oder CTOX-Command mit Status |
| `Run` | Ausfuehrung, Agent Slice, Import, Matching, Research oder Workflow |
| `Artifact` | Ergebnisdokument, Draft, Report, Evidence, Export oder Link |
| `Approval` | Menschliche Freigabe, Ablehnung oder Aenderungsanforderung |
| `Activity` | Auditierbares Ereignis, Statuswechsel, Kommentar oder Ergebnis |
| `DeepLink` | Ruecksprung in Business OS auf Modul/Record/Run/Artifact |

## Tool Contract v1

### Generic Tools

| Tool | Klasse | Zweck |
| --- | --- | --- |
| `business_os.status` | read | CTOX/Business-OS/MCP Channel Health |
| `business_os.list_modules` | read | Installierte und erlaubte Module listen |
| `business_os.get_module` | read | Modulmanifest, Entities, Actions und Limits |
| `business_os.list_entities` | read | Entity-Kontrakte eines Moduls listen |
| `business_os.search_records` | read | Moduluebergreifend oder modulbezogen suchen |
| `business_os.query_records` | read | Records mit Filter, Sortierung, Limit abrufen |
| `business_os.get_record` | read | Einzelnen Record abrufen |
| `business_os.get_record_context` | read | Verknuepfte Records, Activity, Runs, Artifacts |
| `business_os.list_record_activity` | read | Timeline/Audit fuer Record |
| `business_os.list_mcp_activity` | read | Audit Events des MCP Channels lesen |
| `business_os.list_module_actions` | read | Erlaubte Actions fuer Modul/Record/Selection |
| `business_os.propose_action` | write-prep | Command vorbereiten, noch nicht ausfuehren |
| `business_os.create_app` | write | Runtime-installierte Business-OS-App erstellen |
| `business_os.modify_app` | write | Bestehende Business-OS-App aendern |
| `business_os.execute_action` | write | Validierten Command erzeugen oder starten |
| `business_os.get_command_status` | read | Command-Status und Ergebnisreferenzen abrufen |
| `business_os.list_runs` | read | Laufende/abgeschlossene/blockierte Runs listen |
| `business_os.get_run` | read | Run-Details, Harness Flow, Evidence |
| `business_os.list_artifacts` | read | Ergebnisartefakte suchen/listen |
| `business_os.get_artifact` | read | Artefakt-Summary, Link oder sichere Preview |
| `business_os.list_approvals` | read | Offene Freigaben listen |
| `business_os.approve` | external-effect | Freigabe erteilen |
| `business_os.reject` | write | Freigabe ablehnen |
| `business_os.request_changes` | write | Aenderungsanforderung an Run/Command |
| `business_os.open_link` | read | Business-OS-Deep-Link erzeugen |

### Explicit Non-Tools

Diese Tools duerfen in v1 nicht existieren:

```text
run_cli
run_shell
write_sql
push_rxdb_record
open_arbitrary_file
remote_control_browser
execute_raw_business_command
```

Interne Implementierungen duerfen einzelne CLI- oder Store-Pfade nutzen, aber
nur hinter festen, schema-validierten Tool-Funktionen.

## Module Adapter Scope v1

v1 startet mit fuenf Business-OS-Modulen, weil sie den ChatGPT-Nutzen am
staerksten tragen:

| Modul | v1 Capability |
| --- | --- |
| `tickets` | Tickets finden, zusammenfassen, naechste Schritte vorschlagen, Follow-up erzeugen |
| `knowledge` | Knowledge suchen, Dokumente/Runbooks abrufen, Notiz/Runbook-Kandidat vorbereiten |
| `customers` | Kunden/Kontakte suchen, Activity abrufen, Follow-up oder Update vorschlagen |
| `matching` | Anforderungen/Kandidaten/Jobs abfragen, Match Run starten, Match erklaeren |
| `outbound` | Kampagnenstatus lesen, Drafts vorbereiten, Sendefreigaben behandeln |

Weitere Module bleiben ueber generische Read-Tools sichtbar, erhalten aber keine
modulspezifischen Actions, bis ihr Adaptervertrag definiert ist.

## External Agent Skill Contract

Der MCP Server alleine reicht nicht. Externe Agents brauchen parallel einen
installierbaren Skill, damit sie CTOX ueber MCP konsistent verwenden und nicht
auf freie CLI-/Shell-Muster zurueckfallen.

Skill-Artefakt:

```text
skills/ctox-business-os-mcp/SKILL.md
```

Der Skill muss knapp und strikt bleiben:

- triggert bei Business-OS-MCP-, ChatGPT-App-, MCP-Connector-, Codex-,
  ChatGPT- und sonstigen Agent-Integration-Aufgaben
- beschreibt MCP als offiziellen CTOX Kommunikationskanal
- erzwingt read-first, propose-before-execute und approval-gated external
  effects
- verbietet `run_cli`, `run_shell`, SQL, raw RxDB writes und Browser Remote
  Control als Tool-Abstraktion
- verweist auf diesen Implementierungsplan fuer Bauarbeiten am Server
- erklaert den ehrlichen Fallback, wenn der MCP Server noch nicht erreichbar ist

Der Skill wird versioniert und zusammen mit dem MCP Tool Contract getestet. Wenn
ein Tool umbenannt, entfernt oder in seiner Risikoklasse geaendert wird, muss
der Skill in derselben Phase angepasst werden. Der Skill muss direkt aus dem
GitHub-Repo installierbar sein, ohne dass eine CTOX-Installation den internen
Skill-Pack zuerst registriert.

## Implementation Phases

Jede Phase muss einzeln testbar sein. Eine Phase gilt erst als abgeschlossen,
wenn alle Tests und Exit-Kriterien erfuellt sind.

### Phase 0: Baseline, Decision Record, Dirty-Tree Guard

Ziel: Vor Implementierung die Produktentscheidung, Scope-Grenzen und den
aktuellen Arbeitsbaum festhalten.

Artefakte:

- `docs/business-os-mcp-channel-v1-implementation-plan.md`
- optional spaeter: `docs/rfcs/0010_business_os_mcp_channel_v1.md`
- Baseline-Notiz im Plan oder RFC mit aktueller Business-OS-/CTOX-Architektur

Implementierung:

- MCP als offizieller Kommunikationskanal festlegen
- Non-goals dokumentieren
- P2P/RxDB-Vertrag gegen HTTP-Datenproxy abgrenzen
- Cloud/Managed/Self-hosted Betriebsmodi skizzieren
- bestehenden Dirty Worktree dokumentieren, aber nicht veraendern

Tests:

- `git diff -- docs/business-os-mcp-channel-v1-implementation-plan.md`
- Markdown-Lint oder manuelle Strukturpruefung
- Review-Check: keine bestehenden Business-OS-Dateien geaendert

Exit-Kriterien:

- Produktrolle und Nicht-Ziele sind eindeutig
- jede spaetere Phase hat testbare Deliverables
- kein Codepfad wurde vor Vertragsklaerung angefasst

### Phase 1: Core Contract and Type Definitions

Ziel: Das stabile Objekt- und Fehler-Modell fuer den MCP Channel definieren.

Artefakte:

- `src/core/business_os/mcp_channel.rs`
- `src/core/business_os/mcp_channel_contract.rs` oder gleichwertige Module
- JSON-Schema oder serde-Typen fuer:
  - `McpChannelRequestContext`
  - `BusinessOsModuleDescriptor`
  - `BusinessOsEntityDescriptor`
  - `BusinessOsRecordSummary`
  - `BusinessOsActionDescriptor`
  - `BusinessOsCommandEnvelope`
  - `BusinessOsRunSummary`
  - `BusinessOsArtifactSummary`
  - `BusinessOsApprovalSummary`
  - `BusinessOsActivityEvent`
  - `BusinessOsDeepLink`
  - `BusinessOsMcpError`
- External Agent Skill Baseline:
  - `skills/ctox-business-os-mcp/SKILL.md`

Implementierung:

- Actor, channel, surface, request id und confirmation state als Pflichtfelder
  definieren
- Fehlerklassen festlegen:
  - `not_authenticated`
  - `not_authorized`
  - `module_not_found`
  - `entity_not_found`
  - `record_not_found`
  - `action_not_allowed`
  - `confirmation_required`
  - `sync_not_ready`
  - `runtime_unavailable`
  - `validation_failed`
  - `external_effect_blocked`
- Response-Konvention festlegen:
  - knappe `summary`
  - strukturierte `data`
  - optionale `links`
  - optionale `evidence_refs`
  - niemals unbounded dumps
- externen Skill-Inhalt auf den Core Contract ausrichten:
  - erlaubte Tool-Klassen
  - verbotene Tool-Muster
  - Fallback-Regel, wenn MCP nicht verbunden ist

Tests:

- Rust unit tests fuer serde roundtrips
- snapshot tests fuer Beispiel-JSON
- compile check: `cargo check`
- negative tests fuer fehlende Pflichtfelder
- Skill-Review: Skill nennt keine Tools, die im Contract nicht vorgesehen sind
- Skill-Review: Skill verbietet generische CLI/Shell/SQL/RxDB Tooling-Muster
- Skill-Review: Skill ist als eigenstaendiger GitHub-Skill installierbar und
  setzt keine interne CTOX-Skill-Registry voraus

Exit-Kriterien:

- alle Core Types serialisieren stabil
- Fehler sind maschinenlesbar
- Channel-Metadaten koennen nicht versehentlich fehlen

### Phase 2: Read-Only Runtime Adapter

Ziel: Generische read-only Business-OS-Abfragen aus dem CTOX Core anbieten,
ohne MCP Server und ohne Schreibpfade.

Artefakte:

- `src/core/business_os/mcp_channel.rs`
- Funktionen:
  - `mcp_status`
  - `list_modules`
  - `get_module`
  - `list_entities`
  - `search_records`
  - `query_records`
  - `get_record`
  - `get_record_context`
  - `list_record_activity`
  - `open_link`

Implementierung:

- vorhandene Business-OS-Manifest-/Module-APIs wiederverwenden
- vorhandene Store-/Knowledge-/Record-Projektionen lesen
- Records begrenzt ausgeben: `limit`, `cursor`, `fields`, `summary_level`
- Deep Links deterministisch erzeugen
- keine HTTP-RxDB-Fallbacks einfuehren

Tests:

- Unit tests mit temp runtime root
- read-only invariants: vor/nach Aufruf keine neuen Commands/Records
- Modul-Listing gegen Test-Manifeste
- Record query tests mit Limit und Filter
- Deep-Link snapshot tests
- `cargo test business_os::mcp_channel`

Exit-Kriterien:

- Read Tools liefern echte Business-OS-Daten oder klare `sync_not_ready` Fehler
- alle Ergebnisse sind begrenzt und ChatGPT-tauglich
- kein Schreibpfad ist erreichbar

### Phase 3: Channel Audit and Permission Gate

Ziel: MCP Tool Calls als auditierbaren CTOX Channel einfuehren.

Artefakte:

- neue oder erweiterte Runtime-Tabellen fuer MCP Channel Events
- Permission-Funktionen fuer:
  - read
  - write-prep
  - write
  - external-effect
- Channel-Settings in CTOX Runtime oder Business-OS Store

Implementierung:

- Channel standardmaessig deaktiviert
- Admin/Chef kann Channel aktivieren
- Tool Calls schreiben Audit Events
- Tool Calls erhalten actor/workspace/session Kontext
- sensible Read Tools koennen pro Modul/Entity deaktiviert werden
- externe Wirkung ist standardmaessig bestaetigungspflichtig

Tests:

- migration/schema tests fuer neue Tabellen
- disabled-channel tests: alle Tools werden sauber abgelehnt
- permission tests pro Tool-Klasse
- audit tests: jeder erlaubte Call erzeugt ein Event
- no-secret tests fuer Audit-Metadaten

Exit-Kriterien:

- MCP ist sichtbar als eigener Channel
- Zugriff ist abschaltbar
- Audit-Evidence existiert fuer jeden Tool Call

### Phase 4: Command and Approval Adapter

Ziel: Schreibende Business-OS-Aktionen koennen sicher vorbereitet, ausgefuehrt,
abgelehnt und freigegeben werden.

Artefakte:

- Funktionen:
  - `list_module_actions`
  - `propose_action`
  - `execute_action`
  - `get_command_status`
  - `list_approvals`
  - `approve`
  - `reject`
  - `request_changes`
- Mapping auf `business_commands`, Queue Tasks, Tickets oder Approval Records

Implementierung:

- Action Descriptor mit:
  - input schema
  - risk class
  - confirmation policy
  - idempotence key
  - expected artifact/result type
- `propose_action` erzeugt noch keine externe Wirkung
- `execute_action` erzeugt validierte Commands oder Work Items
- External-effect Actions brauchen Approval
- Status wird aus persistiertem Command-/Run-/Approval-State gelesen

Tests:

- propose-action erzeugt validen Vorschlag ohne Ausfuehrung
- execute-action erzeugt genau einen Command bei gleichem idempotence key
- external-effect ohne Approval wird abgelehnt
- approve/reject schreibt auditierbaren Status
- command-status testet pending/running/completed/failed/blocked

Exit-Kriterien:

- Schreibpfade laufen nie direkt ueber freie Mutation
- alle Commands tragen `chatgpt_mcp` Herkunft
- Freigabezwang ist technisch erzwungen

### Phase 5: Module Adapter v1a: Tickets and Knowledge

Ziel: Die ersten zwei produktiv nuetzlichen Module tief anbinden.

Artefakte:

- `tickets` Adapter:
  - Entities: ticket, self_work_item, verification, writeback
  - Actions: summarize, propose_next_action, create_followup, request_review
- `knowledge` Adapter:
  - Entities: document, skillbook, runbook, data_table, data_row
  - Actions: create_note_candidate, create_runbook_candidate

Implementierung:

- `tickets` nutzt vorhandene Ticket-/Self-Work-/Harness-Evidence
- `knowledge` nutzt vorhandene Knowledge Index-/Document-/DataFrame Pfade
- Summaries bleiben kurz und referenzieren Evidence/Deep Links
- Record Context verbindet Tickets mit Runs, Verifications und Artifacts

Tests:

- adapter contract tests pro Entity
- search/get/context tests fuer tickets und knowledge
- action descriptor snapshot tests
- follow-up command test fuer ticket action
- knowledge fetch test fuer document/runbook/data table

Exit-Kriterien:

- ChatGPT kann echte Tickets und Knowledge finden, verstehen und verlinken
- mindestens eine sichere mutating Action pro Modul funktioniert

### Phase 6: Module Adapter v1b: Customers, Matching, Outbound

Ziel: Business-OS-Workflows mit CRM, Matching und Outbound sinnvoll ueber MCP
zugreifbar machen.

Artefakte:

- `customers` Adapter:
  - Entities: account, contact, opportunity, task, note, activity
  - Actions: create_followup, propose_update, link_artifact
- `matching` Adapter:
  - Entities: requirement, candidate, job, match, shortlist
  - Actions: run_match, explain_match, create_shortlist
- `outbound` Adapter:
  - Entities: campaign, company, contact, draft, send_approval
  - Actions: draft_message, request_send_approval, record_reply_context

Implementierung:

- Modul-Schemas aus Business-OS-Contracts lesen
- modulinterne Actions auf `business_commands` mappen
- externe Kommunikation in `outbound` nie ohne Approval erlauben
- Matching Runs und Outbound Drafts als Runs/Artifacts sichtbar machen

Tests:

- fixture-backed record query tests fuer alle drei Module
- action validation tests fuer required inputs
- external-effect block tests fuer outbound send
- matching run creates command/run reference
- customers follow-up creates command/activity reference

Exit-Kriterien:

- die fuenf v1 Module haben stabile Adapter
- ChatGPT kann nicht nur Status, sondern echte Business-Workflows anstossen

### Phase 7: Local MCP Server

Ziel: Einen lokalen MCP Server fuer Developer Mode und lokale Clients
bereitstellen und den externen Agent Skill gegen die realen Tool Descriptors
validieren.

Artefakte:

- `src/apps/business-os-mcp/` oder `src/tools/business-os-mcp/`
- CLI:
  - `ctox business-os mcp serve --addr 127.0.0.1:8788`
  - `ctox business-os mcp status`
- HTTP endpoint:
  - `/mcp`
  - health endpoint fuer lokale Diagnose
- Tool descriptors mit korrekten read/write annotations
- aktualisierter `ctox-business-os-mcp` Skill, falls Tool-Namen,
  Risikoklassen oder Fallbacks vom Phase-1-Contract abweichen

Implementierung:

- bevorzugt schneller v1 Server als TypeScript/Node MCP Server oder Rust HTTP
  Adapter, abhaengig von Repo-Konvention nach Phase 1
- Server ruft Core Adapter auf, nicht direkt Shell/CLI frei
- Tool schemas aus Contract ableiten
- strukturierte MCP Results mit knappen Summaries und Daten
- lokale Auth fuer Developer Mode klaeren
- Skill so formulieren, dass Agents zuerst MCP Tools verwenden und nur mit
  expliziter User-Freigabe auf lokale CTOX CLI-Fallbacks ausweichen

Tests:

- server start/stop smoke
- tool list snapshot
- MCP initialize/listTools/callTool integration tests
- callTool read-only tests
- callTool mutating/approval tests
- invalid input schema tests
- Skill/tool-descriptor consistency test:
  - alle im Skill genannten positiven Tool-Klassen existieren als Descriptor
  - alle im Skill verbotenen Muster existieren nicht als Descriptor

Exit-Kriterien:

- lokaler MCP Server ist mit einem MCP Inspector oder Testclient nutzbar
- alle v1 Tools sind gelistet und validieren Input
- kein Non-Tool ist erreichbar
- External Agent Skill stimmt mit der tatsaechlichen MCP Server Oberflaeche
  ueberein

### Phase 8: ChatGPT Developer Mode Integration

Ziel: Den lokalen MCP Server als ChatGPT App/Connector im Developer Mode testen.

Artefakte:

- Dev Setup Doku:
  - `ctox business-os mcp serve`
  - `cloudflared tunnel` oder `ngrok`
  - ChatGPT Developer Mode Verbindung
- Test-Skript oder manuelle Testmatrix fuer ChatGPT
- Prompt-/Tool-Behavior-Beispiele

Implementierung:

- Tool descriptions fuer Modellwahl schaerfen
- Status strings und Tool-Ausgaben fuer ChatGPT optimieren
- Output begrenzen, damit ChatGPT keine Massendumps bekommt
- Deep Links in Tool Results pruefen

Tests:

- ChatGPT kann `business_os.status` aufrufen
- ChatGPT kann Records suchen und Details abrufen
- ChatGPT kann Action vorschlagen
- externe Wirkung fordert Approval
- Approval erzeugt persistierten Status
- Deep Link oeffnet passende Business-OS-Oberflaeche

Exit-Kriterien:

- realer ChatGPT Developer Mode Flow funktioniert end-to-end
- alle kritischen Pfade sind reproduzierbar dokumentiert

### Phase 9: ctox.dev Managed MCP Gateway

Ziel: Einen optionalen managed Gateway bauen, damit private CTOX Instanzen ohne
Inbound Ports von ChatGPT erreichbar sind.

Artefakte:

- `mcp.ctox.dev` Gateway Design
- HTTP-Relay Startpunkt: `integrations/cloudflare/business-os-mcp-gateway`
- Cloudflare Worker fuer HTTP/MCP Entry
- Durable Object oder gleichwertiger Router pro CTOX Instance/Session
- outbound session client in CTOX
- Gateway Auth und Instance Binding

Implementierung:

- Phase 9a: Worker Health, Auth und explizites HTTP-Relay zu `UPSTREAM_MCP_URL`
- Phase 9b: Managed Routes `POST /mcp/<instance-id>` und
  `GET /connect/<instance-id>` mit Durable Object Session; Betriebsstatus ueber
  `GET /status/<instance-id>`
- Phase 9c: lokaler CTOX Connector `ctox business-os mcp connect --url ...`
  verarbeitet Gateway-Envelopes gegen den lokalen MCP-Dispatcher und verbindet
  sich bei Gateway-/Netzwerkabbruechen mit begrenztem Backoff neu
- Phase 9d: Connector sendet `ctox_hello` mit CTOX-Version, MCP-Protokoll und
  Faehigkeiten; Gateway-Status zeigt diese Session-Metadaten
- Phase 9e: optionales `ALLOWED_INSTANCE_IDS` begrenzt Managed Routes auf
  explizit erlaubte CTOX Instanzen
- Phase 9f: `/status/<instance-id>` zeigt nur betriebliche Session-Metriken
  ohne MCP Payloads oder Business-OS-Records
- Phase 9g: CTOX CLI kann Gateway-Status per
  `ctox business-os mcp gateway-status ...` abfragen
- Phase 9h: Production Deploy Contract fuer `ctox.dev`
  - `wrangler.jsonc` enthaelt Durable Object Binding, Domain Route und
    nicht-geheime Runtime Defaults
  - Secrets: `MCP_GATEWAY_TOKEN`, `INSTANCE_CONNECT_TOKENS`,
    optional `UPSTREAM_AUTHORIZATION`
  - `/health` liefert nur nicht-geheime Betriebs-Posture und Limits
  - alle JSON Gateway Responses setzen `cache-control: no-store`
  - `npm run smoke` prueft Health, Status und MCP managed route gegen
    `mcp.ctox.dev`
- CTOX Instance verbindet outbound zu `mcp.ctox.dev`
- Gateway routet ChatGPT MCP Calls zur gebundenen Instanz
- keine Business-OS-Collections werden zentral gespiegelt
- Gateway speichert nur Routing-, Session-, Rate-Limit- und Audit-Metadaten
- Timeouts, Body-/Response-Limits, backpressure und offline status sauber
  modellieren
- `MAX_PENDING_REQUESTS` begrenzt parallele MCP Calls pro verbundener CTOX
  Instanz

Tests:

- local fake instance connects outbound
- Gateway routes request to correct instance
- wrong actor/workspace is rejected
- instance offline returns `runtime_unavailable`
- large response limits are enforced
- no collection dump storage in Gateway tests
- production smoke test prueft managed rendezvous ohne Business-OS-Daten-Dump

Exit-Kriterien:

- private CTOX Instanz ist ueber ChatGPT erreichbar, ohne inbound Port
- Gateway verletzt den P2P/RxDB-Datenvertrag nicht
- Gateway kann CTOX Connects per Instance-ID scoped token binden
- Gateway blockt stale/replayed CTOX Connect Attempts per Timestamp/Nonce
- Gateway hat einen dokumentierten Production-Deploy- und Smoke-Test-Vertrag

### Phase 10: Security, Privacy, and Admin Controls

Ziel: Den Channel fuer echte Nutzung absichern.

Artefakte:

- Admin Settings fuer Channel:
  - enabled/disabled
  - read/write/approval/external-effect policy
  - deny-list fuer einzelne Tools
  - allowed actors/workspaces
  - allowed modules/entities
  - retention policy
- Privacy notice fuer ChatGPT/MCP Datenfluss
- Security review checklist
- Security/Admin Guide:
  - `docs/business-os-mcp-channel-v1-security-admin-guide.md`

Implementierung:

- Phase 10a: Runtime Policy Gate im MCP Dispatcher
  - `CTOX_BUSINESS_OS_MCP_ENABLED`
  - `CTOX_BUSINESS_OS_MCP_ALLOW_READS`
  - `CTOX_BUSINESS_OS_MCP_ALLOW_WRITES`
  - `CTOX_BUSINESS_OS_MCP_ALLOW_APPROVALS`
  - `CTOX_BUSINESS_OS_MCP_ALLOW_EXTERNAL_EFFECTS`
  - `CTOX_BUSINESS_OS_MCP_RATE_LIMIT_PER_MINUTE`
  - `CTOX_BUSINESS_OS_MCP_AUDIT_RETENTION_DAYS`
  - `CTOX_BUSINESS_OS_MCP_ALLOWED_ACTORS`
  - `CTOX_BUSINESS_OS_MCP_ALLOWED_WORKSPACES`
  - `CTOX_BUSINESS_OS_MCP_ALLOWED_MODULES`
  - `CTOX_BUSINESS_OS_MCP_ALLOWED_COLLECTIONS`
  - `CTOX_BUSINESS_OS_MCP_DENY_TOOLS`
  - Fehler: `channel_disabled`, `permission_denied`, `rate_limited`
- Phase 10b: CLI Admin Surface
  - `ctox business-os mcp policy`
  - `ctox business-os mcp policy keys`
  - `ctox business-os mcp policy set --enabled true|false ...`
  - validierte Boolean-Werte
  - Deny-Tools muessen `business_os.*` Namen sein
- Phase 10c: Response Redaction
  - MCP Tool Responses werden rekursiv vor Ausgabe redacted
  - gilt fuer `structuredContent` und Text-Content
  - obvious secret fields wie API keys, tokens, passwords, credentials,
    authorization headers, cookies und private/access keys werden maskiert
  - Redaction ist Server-Vertrag und darf vom Agent Skill nicht umgangen werden
- Phase 10d: Response Size Limits
  - MCP Tool Responses werden zentral nach Redaction gemessen
  - zu grosse Antworten schlagen mit `response_too_large` fehl
  - Clients muessen dann Query/Limit/Record-Kontext enger schneiden
- Phase 10e: Local Rate Limits
  - MCP Tool Calls werden pro `actor + workspace` ueber ein 60s Fenster gezaehlt
  - Zaehler basiert auf `business_os_mcp_events`
  - `0` deaktiviert das lokale Limit fuer kontrollierte Test-/Enterprise-Setups
- Phase 10f: Audit Export
  - `ctox business-os mcp audit --format json|jsonl --output <path>`
  - JSONL dient fuer SIEM-/Review-Pipelines
  - Export nutzt dieselbe canonical MCP Audit Event Form wie
    `business_os.list_mcp_activity`
- Phase 10g: Permission Matrix / Allowlists
  - Actor- und Workspace-Allowlist am Tool-Call-Eingang
  - Module-Allowlist fuer Module und Actions
  - Collection-Allowlist fuer Records, Entities und abgeleitete Context-Tools
  - Default bleibt offen, leere Listen bedeuten keine Einschraenkung
- Phase 10h: JSON-RPC Error Contract
  - typed Business-OS-MCP-Fehler erscheinen in `error.data.code`
  - `error.data.field` enthaelt den betroffenen Policy-/Input-Key, falls vorhanden
  - JSON-RPC Codes werden stabil gemappt, z.B. invalid params, permission denied,
    rate limit, response too large und not found
- Phase 10i: Audit Retention
  - lokale MCP Audit Events werden per Retention Policy begrenzt
  - Default: 90 Tage, `0` deaktiviert Pruning fuer kontrollierte Archive
  - `ctox business-os mcp audit --prune` fuehrt Pruning manuell aus
- Phase 10j: Security/Admin Documentation
  - dokumentiert Datenfluss, Privacy Notice, Policy Defaults, Emergency Disable,
    Gateway Secrets, Audit Export und Release Gate
- Least-privilege defaults
- per-tool/module allowlist
- audit export
- emergency disable switch

Tests:

- Phase 10a:
  - disabled-channel emergency test
  - read-policy test
  - deny-list de-duplication test
  - external-effect policy test
- Phase 10b:
  - CLI Boolean parser tests
  - Deny-Tool scope/dedup tests
  - Policy-Key projection ohne Secrets
- Phase 10c:
  - Canary-Secret Tests fuer Record-Felder
  - MCP Envelope Tests fuer `structuredContent` und Text-Content
- Phase 10d:
  - oversized response test mit typisiertem `response_too_large`
- Phase 10e:
  - actor/workspace rate-limit test mit typisiertem `rate_limited`
- Phase 10f:
  - JSONL Export Test mit validen Audit Events
- Phase 10g:
  - actor/workspace/module/collection permission matrix tests
- Phase 10h:
  - JSON-RPC Tests fuer typed permission und validation errors
- Phase 10i:
  - Retention-Test entfernt abgelaufene MCP Events und behaelt aktuelle Events
- Phase 10j:
  - Doku verweist auf konkrete pruefbare Commands und Gateway-Checks
- disabled-channel emergency tests
- external-effect policy tests

Exit-Kriterien:

- Channel kann sicher fuer interne Nutzer aktiviert werden
- Admin kann Daten- und Aktionsumfang nachvollziehbar begrenzen

### Phase 11: Optional ChatGPT Widget

Ziel: Nur wenn der headless MCP Channel Nutzen zeigt, ein kleines Widget fuer
kompakte Status- und Approval-Flows bauen.

Artefakte:

- Widget resource fuer Apps SDK
- Views:
  - offene Approvals
  - laufende Runs
  - Record Summary
  - Artifact Card
  - Open in Business OS

Implementierung:

- kein vollstaendiges Business OS iframe
- keine Shell-Duplikation
- Widget rendert nur Tool Results und ruft erlaubte MCP Tools
- CSP und resource domains eng setzen

Tests:

- Widget render smoke
- Tool result update test
- approve/reject interaction test
- mobile/narrow viewport visual check
- CSP allowlist test

Exit-Kriterien:

- Widget verbessert Approval/Status ohne Business OS zu ersetzen

### Phase 12: Release Gate and Documentation

Ziel: Business OS MCP Channel v1 als installierbare und dokumentierte Funktion
abschliessen.

Artefakte:

- User docs:
  - setup local
  - setup ChatGPT Developer Mode
  - setup managed ctox.dev
  - security/admin guide
- Developer docs:
  - adapter authoring guide
  - module action contract
  - test fixtures
- Agent docs:
  - Skill usage examples
  - MCP setup examples fuer ChatGPT, Codex/Agents und lokale MCP Clients
  - GitHub install path fuer den externen Skill
- Release notes
  - `docs/business-os-mcp-channel-v1-release-notes.md`
- Live rollout report
  - `docs/business-os-mcp-channel-v1-live-rollout-report.md`

Tests:

- full local integration test
- managed gateway integration test
- live `mcp.ctox.dev` route verification
- module adapter regression suite
- docs command examples verified
- External Agent Skill validation against final tool descriptors
- Release notes / known limits document
- clean install smoke
- upgrade/migration smoke

Exit-Kriterien:

- v1 ist lokal testbar
- v1 ist ueber managed Gateway testbar
- `mcp.ctox.dev` routet auf den Business OS MCP Gateway Worker
- Admin-/Privacy-Dokumentation ist vorhanden
- alle v1 Module haben Adaptertests
- der externe Agent Skill ist aus dem GitHub-Repo installierbar, discoverable
  und gegen v1 Tooling validiert
- keine bekannten Release-Blocker offen

## Suggested Progress Model

| Phase | Gewicht |
| --- | ---: |
| 0. Baseline and decision record | 4% |
| 1. Core contract, type definitions, agent skill baseline | 8% |
| 2. Read-only runtime adapter | 10% |
| 3. Channel audit and permission gate | 9% |
| 4. Command and approval adapter | 10% |
| 5. Tickets and Knowledge adapters | 10% |
| 6. Customers, Matching, Outbound adapters | 12% |
| 7. Local MCP server and skill/tool validation | 10% |
| 8. ChatGPT Developer Mode integration | 7% |
| 9. ctox.dev managed MCP gateway | 10% |
| 10. Security, privacy, admin controls | 5% |
| 11. Optional widget | 2% |
| 12. Release gate and documentation | 3% |
| **Gesamt** | **100%** |

## Implementation Notes

- Start headless. Das Widget ist nicht v1-kritisch.
- Prefer Core Adapter first, MCP Server second. Sonst entsteht ein Server, der
  versehentlich lokale Implementation Details exponiert.
- Business-OS-Module duerfen nicht ueber direkte Browser-UI-Automation
  gesteuert werden.
- Schreibende Aktionen muessen immer ueber Business-OS-/CTOX-Commands oder
  Work Items laufen.
- Externe Wirkung, insbesondere Outbound Send, bleibt approval-gated.
- Managed Gateway darf keine vollstaendige Business-OS-Datenkopie werden.
- Jede Tool Response braucht Limits, Cursor und Summary-Level.
- Deep Links sind Pflicht, weil ChatGPT nicht die dichte Business-OS-Oberflaeche
  ersetzen soll.

## Open Decisions

1. Wird der lokale MCP Server in v1 als Rust-Subcommand oder als TypeScript
   Server gebaut?
2. Welche Auth-Quelle gilt fuer ChatGPT Actor Binding: OpenAI subject,
   CTOX-issued token oder ctox.dev account binding?
3. Wird `chatgpt_mcp` als separater Channel in bestehenden Channel-Tabellen
   modelliert oder bekommt MCP eigene Runtime-Tabellen mit Bridge ins
   Channel-System?
4. Welche Module sind fuer v1 read-only sichtbar, aber ohne Actions?
5. Welche Daten duerfen in Managed Gateway Logs nie erscheinen?
6. Welche Deep-Link-Form ist canonical: custom scheme, HTTPS hash route oder
   beide?

## First Implementation Slice

Der erste Code-Slice nach diesem Plan sollte Phase 1 und Phase 2 nur fuer
read-only Tools liefern und den externen Agent Skill als Baseline enthalten:

```text
business_os.status
business_os.list_modules
business_os.get_module
business_os.list_entities
business_os.search_records
business_os.get_record
business_os.open_link
```

Dieser Slice ist wertvoll, weil er das Objektmodell und die Datenbegrenzung
beweist, ohne Schreib- oder Cloud-Risiko einzufuehren. Danach folgen Audit,
Permissions und Commands.

Parallel muss der GitHub-installierbare Skill bereits sagen koennen: MCP ist der
bevorzugte Channel, freie CLI/Shell ist kein MCP Ersatz, und fehlende
MCP-Verbindung muss ehrlich als nicht verbunden gemeldet werden.

# interviews – v4-Handler (Strang H8, Shell v4)

Stand 15.10.2026. Quelle der Einstufung: `~/.local/state/shell-v4/analysis/E-befehlstypen.md` (§2 Tabelle, §3 Capabilities),
Trennregel Architektur §16.13, Handler-Vertrag §16.16. Rust nur gelesen: `~/.local/state/shell-v4-src/4794af03d/src/core/business_os/` `store_ats_commands.rs`, `ats_gates.rs`.
Handler-Vertrag: `handlers/<command_type>.js`, `export default async function handle(command, ctx)`; Fehler `throw {code, message}`
(`invalid|forbidden|not_found|conflict|revoked|expired|quota|unavailable`). Tests: `node --test handlers/tests/*.test.mjs`
(In-Memory-ctx `handlers/tests/_ctx.mjs`: eigene Collections = v4-Manifest `collections`, Fremdzugriff = Manifest
`foreign_collections` + hier beantragte Ergänzungen; Capabilities protokolliert und je Test beantwortbar).

**Stand: 1 von 1 Befehlstypen als JS-Handler, 0 über `ctx.cap.legacy.<command_type>`, 0 PLATTFORM (nur dokumentiert). Tests: 2 grün.**

## Befehlstypen

| # | Befehlstyp | Einstufung | Rust-Ort | Ziel | Stand |
|---|---|---|---|---|---|
| 1 | `ats.interview.transcribe` | GEMISCHT | `store_ats_commands.rs:1151-1237` | `handlers/ats.interview.transcribe.js` + `cap.speech.stt` | portiert |

**Manifest-Wunsch:** `foreign_collections.desktop_files: {owner_app:'desktop', ops:['read']}` (Größenprüfung ≤ 256 MiB).
Wie Rust (`.ok()`) zählt ein Lesefehler als Größe 0 – auch ein fehlendes Recht; das verdeckt eine fehlende Deklaration.

**Abweichung:** Rust trennte fatale Materialisierungsfehler (`?`) von nicht-fatalen STT-Fehlern (`ok:false`). Über eine
einzige Capability ist das nicht unterscheidbar; v4 behandelt jeden Wurf der Capability als nicht-fatal (`ok:false`,
nichts geschrieben). Vorschlag: Capability wirft `unavailable` nur für fehlende Modellgewichte, sonst `invalid`; dann
kann der Handler wieder trennen.

| Capability | Signatur (Vorschlag) | Beispielaufruf | Herkunft |
|---|---|---|---|
| `ctx.cap.speech.stt.transcribe` | `speech.stt.transcribe({source:{kind:'desktop_file', file_id, generation_id}, command_id}) → {text, model}` | `await ctx.cap.speech.stt.transcribe({source:{kind:'desktop_file', file_id, generation_id}, command_id: command.id})` | E §3 `cap.speech.stt`; Quelle statt `blob_id`, weil Rust aus desktop_file_chunks materialisiert und Rohpfade verbietet |

## Gemeinsame ATS-Grundlage

`handlers/_ats.js` ist in consent, esign, intake, interviews, nachweise, placements, submissions **byte-gleich** kopiert
(App-Artefakte importieren nicht über App-Grenzen, §16.15). Inhalt: 1:1-Port von `ats_gates.rs` (credential_status,
evaluate_deployment_readiness, consent_valid, evaluate_consent_gate, retention_due, find_double_submission,
signature_request_status, compute_nachweis_billing, evaluate_billing_gate) und die Helfer aus `store_ats_commands.rs:1-385`
(ATS_WRITABLE_COLLECTIONS, ats_actor_value, record_ats_governance_event, ats_invoice_line, ats_canonical_invoice,
upsert_business_record, first_string_field, checked_scaled). Gate-Tests: `consent/handlers/tests/ats_gates.test.mjs`.
Änderung an einer Kopie ⇒ alle sieben angleichen (Prüfung: `shasum modules/*/handlers/_ats.js`).

**Abweichungen vom Rust-Verhalten (bewusst, begründet):**
1. *Fehlercodes*: Rust lieferte für jeden Fehler einen generischen `anyhow`-Fehlschlag. v4: Pflichtfeld → `invalid`,
   unbekannter Datensatz → `not_found`, Fremd-Collection außerhalb der Allowlist bzw. anonymer Handelnder → `forbidden`.
   Meldungstexte sind wörtlich übernommen.
2. *Handelnder*: `ats_actor_value(session)` wird aus `command.actor {id, role}` gebildet (`display_name` nur, wenn der Kern
   es mitliefert). „Authentifiziert“ = `actor.id` nicht leer und ≠ `rxdb-command`. Die Rollenprüfung chef/admin
   (heute `is_ats_mutating_command` → `rxdb_command_session`) bleibt **Kern-Policy** und ist nicht Teil des Handlers.
3. *Atomarität*: Mehrere Schreibvorgänge eines Befehls laufen in **einer** `ctx.transaction` (Rust: einzelne Upserts
   ohne Transaktion). Das Prüfspur-Ereignis ist externe Wirkung und läuft **nach** der Transaktion.
4. *Löschen (DSGVO)*: Rust überschrieb den Payload vor `deleted=1` mit einem PII-freien Grabstein (`redacted`,
   `redacted_at_ms`, `erasure_basis`/`retention_days`). v4 nutzt `ctx.remove`. **Laufzeitanforderung an „ctox sync v4“:**
   der Lösch-Grabstein des Sync-Stores darf keinen Payload (PII) behalten; die Metadaten stehen im Prüfspur-Ereignis.
5. *IDs*: `appl_/plac_/subm_/sig_/transcript_{now}` wie Rust (Kollision bei gleicher Millisekunde unverändert).
6. *Konfiguration*: `env_or_config(root, KEY)` (SQLite-Laufzeitstore) → Capability-Wunsch `ctx.cap.config.get`.

**Capabilities (alle ATS-Apps):**

| Capability | Signatur (Vorschlag) | Beispielaufruf | Herkunft |
|---|---|---|---|
| `ctx.cap.audit.event` | `audit.event({collection, record_id, event_type, payload, observed_at_ms}) → void` | `await ctx.cap.audit.event({collection:'business_commands', record_id:'', event_type:'business_os.ats.candidate_presented', payload, observed_at_ms: now})` | Wunsch aus Strang H3 (outbound), ersetzt `insert_business_event` → `business_events` |
| `ctx.cap.config.get` | `config.get({key}) → {value: string|null}` | `await ctx.cap.config.get({key:'CTOX_BUSINESS_OS_AUE_REQUIRED_CREDENTIALS'})` | **neu** (nicht in E §3); ersetzt `runtime_env::env_or_config`, Schlüssel je App im Manifest freizugeben |


## Kernform der Capabilities (Strang CAPSIG, 15.10.2026, §16.30/§16.32)

| Capability | Aufruf (Kernform) |
|---|---|
| `speech.stt.transcribe` | `({blob_id:{ref}, model})` → `{text, segments}` (language nativ nicht unterstützt, wird nicht gesendet) |

Quelle: `source_file_ref` oder `source_file_id` (files-Datensatz). Fehlt die Datei in files → nicht-fataler Ausgang
(`ok:false`, Fehlertext nennt den Kern-Import).

### Kern-Import
- **desktop_files → files** (einmalig, gleiche id; siehe cv-print-builder). Fremdzugriff `desktop_files` gestrichen,
  `files` (ctox-platform, read) neu.

# creator – v4-Handler (Strang H8, Shell v4)

Stand 15.10.2026. Quelle der Einstufung: `~/.local/state/shell-v4/analysis/E-befehlstypen.md` (§2 Tabelle, §3 Capabilities),
Trennregel Architektur §16.13, Handler-Vertrag §16.16. Rust nur gelesen: `~/.local/state/shell-v4-src/4794af03d/src/core/business_os/`.
Handler-Vertrag: `handlers/<command_type>.js`, `export default async function handle(command, ctx)`; Fehler `throw {code, message}`
(`invalid|forbidden|not_found|conflict|revoked|expired|quota|unavailable`). Tests: `node --test handlers/tests/*.test.mjs`
(In-Memory-ctx `handlers/tests/_ctx.mjs`: eigene Collections = v4-Manifest `collections`, Fremdzugriff = Manifest
`foreign_collections` + hier beantragte Ergänzungen; Capabilities protokolliert und je Test beantwortbar).

**Stand: 0 von 3 Befehlstypen als JS-Handler, 0 über `ctx.cap.legacy.<command_type>`, 3 PLATTFORM (nur dokumentiert). Tests: – grün.**

## Befehlstypen

| # | Befehlstyp | Einstufung | Rust-Ort | Ziel | Stand |
|---|---|---|---|---|---|
| 1 | `ctox.business_os.app.create` | PLATTFORM | `store.rs:6753`, `:10984` ensure_app_create_actor_lifecycle_assignment, `:26636` retry_failed_app_create_command | Kern (Queue) | nur dokumentiert |
| 2 | `ctox.business_os.app.modify` | PLATTFORM | `store.rs:6753` | Kern (Queue) | nur dokumentiert |
| 3 | `business_os.chat.task` | PLATTFORM | `store.rs:6753` | Kern (Queue) | nur dokumentiert |

App-Erzeugung ist Plattform (C1 §9 Nr. 10). In v4 baut/veröffentlicht creator ein Sync-Artefakt statt
`installed-modules/*` per HTTP zu lesen – Aufgabe des App-Port-Strangs, nicht dieses Handler-Ports. Kein Handler.

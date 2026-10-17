# app-store – v4-Handler (Strang H8, Shell v4)

Stand 15.10.2026. Quelle der Einstufung: `~/.local/state/shell-v4/analysis/E-befehlstypen.md` (§2 Tabelle, §3 Capabilities),
Trennregel Architektur §16.13, Handler-Vertrag §16.16. Rust nur gelesen: `~/.local/state/shell-v4-src/4794af03d/src/core/business_os/`.
Handler-Vertrag: `handlers/<command_type>.js`, `export default async function handle(command, ctx)`; Fehler `throw {code, message}`
(`invalid|forbidden|not_found|conflict|revoked|expired|quota|unavailable`). Tests: `node --test handlers/tests/*.test.mjs`
(In-Memory-ctx `handlers/tests/_ctx.mjs`: eigene Collections = v4-Manifest `collections`, Fremdzugriff = Manifest
`foreign_collections` + hier beantragte Ergänzungen; Capabilities protokolliert und je Test beantwortbar).

**Stand: 0 von 16 Befehlstypen als JS-Handler, 0 über `ctx.cap.legacy.<command_type>`, 16 PLATTFORM (nur dokumentiert). Tests: – grün.**

## Befehlstypen

| # | Befehlstyp | Einstufung | Rust-Ort | Ziel | Stand |
|---|---|---|---|---|---|
| 1 | `ctox.app_store.install` | PLATTFORM | `store.rs:16344` | Kern (App-Lebenszyklus) | nur dokumentiert |
| 2 | `ctox.app_store.uninstall` | PLATTFORM | `store.rs:16375` | Kern (App-Lebenszyklus) | nur dokumentiert |
| 3 | `ctox.module.check_updates` | PLATTFORM | `store.rs:18068` | Kern (App-Lebenszyklus) | nur dokumentiert |
| 4 | `ctox.module.update` | PLATTFORM | `store.rs:17879` | Kern (App-Lebenszyklus) | nur dokumentiert |
| 5 | `ctox.module.rollback_version` | PLATTFORM | `store.rs:18169` | Kern (App-Lebenszyklus) | nur dokumentiert |
| 6 | `ctox.module.release` | PLATTFORM | `store.rs:17697` | Kern (App-Lebenszyklus) | nur dokumentiert |
| 7 | `ctox.module.install_template` | PLATTFORM | `store.rs:17845` | Kern (App-Lebenszyklus) | nur dokumentiert |
| 8 | `ctox.module.list_versions` | PLATTFORM | `store.rs:18149` | Kern (App-Lebenszyklus) | nur dokumentiert |
| 9 | `ctox.module.rollback` | PLATTFORM | `store.rs:18110` | Kern (App-Lebenszyklus) | nur dokumentiert |
| 10 | `ctox.module.save` | PLATTFORM | `store.rs:17773` | Kern (App-Lebenszyklus) | nur dokumentiert |
| 11 | `ctox.module.delete` | PLATTFORM | `store.rs:17810` | Kern (App-Lebenszyklus) | nur dokumentiert |
| 12 | `ctox.module.set_visible` | PLATTFORM | `store.rs:17923` | Kern (App-Lebenszyklus) | nur dokumentiert |
| 13 | `ctox.module.assign_founder` | PLATTFORM | `store.rs:17734` | Kern (App-Lebenszyklus) | nur dokumentiert |
| 14 | `ctox.module.repair_lifecycle_projection` | PLATTFORM | `store.rs:17668` | Kern (App-Lebenszyklus) | nur dokumentiert |
| 15 | `ctox.business_os.app.modify` | PLATTFORM | `store.rs:6753` (Queue, Rechte-Vorprüfung `:6790-6794`) | Kern (Queue) | nur dokumentiert |
| 16 | `business_os.chat.task` | PLATTFORM | `store.rs:6753` | Kern (Queue) | nur dokumentiert |

Installieren, Veröffentlichen, Zurückrollen, Versionen, Sichtbarkeit und Gründerzuweisung sind laut Auftrag
**PLATTFORM** und gehören dem Kern (v4: Instanz-Store §9, Veröffentlichung §16.5, Katalog je App §16.14). Kein Handler.

## SKL-D (15.10.2026)

Toter Test-Haken `appStoreContextChatDetail` (einziger Ort mit `business_os.chat.task` / `ctox.business_os.app.modify`,
kein UI-Sender) entfernt; `app-store.test.mjs` prüft stattdessen den Sichtbereich `buildAppStoreAgentScopeView`. Ein
künftiges „App ändern“ im Store nutzt den Baustein `ctox:shared/context-chat.js` (`ctox.app.modify.request`). Keine
Manifest-Wünsche.

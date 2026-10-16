# coding-agents – v4-Handler (Strang H8, Shell v4)

Stand 15.10.2026. Quelle der Einstufung: `~/.local/state/shell-v4/analysis/E-befehlstypen.md` (§2 Tabelle, §3 Capabilities),
Trennregel Architektur §16.13, Handler-Vertrag §16.16. Rust nur gelesen: `~/.local/state/shell-v4-src/4794af03d/src/core/business_os/`.
Handler-Vertrag: `handlers/<command_type>.js`, `export default async function handle(command, ctx)`; Fehler `throw {code, message}`
(`invalid|forbidden|not_found|conflict|revoked|expired|quota|unavailable`). Tests: `node --test handlers/tests/*.test.mjs`
(In-Memory-ctx `handlers/tests/_ctx.mjs`: eigene Collections = v4-Manifest `collections`, Fremdzugriff = Manifest
`foreign_collections` + hier beantragte Ergänzungen; Capabilities protokolliert und je Test beantwortbar).

**Runde AGENT 4 (16.10.2026) – CAP-3d (§16.44-Form):** `ctox.coding.turn` nimmt nur noch
`{app_id, draft_id, base_draft_rev, prompt, model?, turn_id?}` (Pflicht die ersten vier; `model`/`turn_id` optional,
`turn_id` `[A-Za-z0-9_.:-]{1,128}`, Prompt ≤ 64 KiB) und ruft `ctx.cap.agent.coding.turn` flach mit genau diesen
Feldern. V3-Felder (`module_id`, `faux`, `preset_id`), fremde Felder, Hülle → `invalid` (`payload_invalid:<feld>`), ohne
Capability-Aufruf. Neu `ctox.coding.cancel {turn_id}` → `ctx.cap.agent.coding.cancel`. Antwortet der Kern
`unavailable` (#848 nicht gebunden) oder fehlt die Capability, wirft der Handler einen sauberen Fehler
`{code:'unavailable', retryable:true}` mit Zonentext „Coding Agent im Kern noch nicht verfügbar …“ (auch wenn der
Kern ein rohes `{code, message}` wirft) – kein Absturz, nie der V3-Pfad. Gemeinsame Prüfung in `_coding_turn.js`.
`core-contract.mjs` auf CAP-3d nachgezogen. Tests `node --test handlers/tests/*.test.mjs`: 6/6. `check-caps --calls`:
ABWEICHUNG 0 (coding.turn/cancel/models geprüft). Mutationsbelege (je Datei geändert, Test, per sha256 zurück):
Pflichtfeld-Prüfung entfernt → Test „V3-Form …“ rot; unavailable-Abbildung entfernt → Test „unavailable“ rot;
cancel ruft `agent.coding.turn` → Test „cancel“ rot und `--calls` ABWEICHUNG 1; turn sendet `module_id` statt `app_id`
→ Test „§16.44-Form“ rot und `--calls` ABWEICHUNG 1.
Offen: Die anfragende App (Delegation aus „Bugs & Features“ über `threads`) muss die §16.44-Felder liefern (eigener
Entwurf des Nutzers: `draft_id`, `base_draft_rev`); die alte `module_id`-Delegation endet jetzt mit `invalid` →
`on_failed`. `ctox.coding.cancel` hat noch keinen `commands_in`-Absender; Manifest neu erzeugen (Strang MAN2).

**Runde AGENT (15.10.2026):** Die UI der App ruft keinen der beiden Befehle mehr auf (sie ist jetzt die Übersicht der
Agent-Läufe; der Agent im Quelltext-Modus nutzt den Kernvertrag §16.44 `ctox.coding.turn {app_id, draft_id,
base_draft_rev, prompt, …}` direkt aus der Shell). Die Handler bleiben unverändert als Ziel für `commands_in` aus
`threads` (freigegebene Delegation mit `module_id`-Payload).

**Stand (vor CAP-3d, überholt für turn): 2 von 2 Befehlstypen als JS-Handler, 0 über `ctx.cap.legacy.<command_type>`, 0 PLATTFORM (nur dokumentiert). Tests: 3 grün.**

## Befehlstypen

| # | Befehlstyp | Einstufung | Rust-Ort | Ziel | Stand |
|---|---|---|---|---|---|
| 1 | `ctox.coding.models` | FAEHIGKEIT | `store.rs:15883-15914` | `handlers/ctox.coding.models.js` → `cap.agent.coding.models` | portiert |
| 2 | `ctox.coding.turn` | FAEHIGKEIT | `store.rs:15915-15998` | `handlers/ctox.coding.turn.js` → `cap.agent.coding.turn` | portiert |

Fachlogik im Handler: `source_sanitize_slug` (`store.rs:6213`, `handlers/_coding.js`) für `module_id` + Pflicht,
Ablehnung roher `model`-Übersteuerung, Vorgaben `preset_id:'ctox'`, `faux:false`. Im Kern: Rechte AppsView/AppsModify je
Modul, Preset-Auflösung, Sidecar-Bündel, begrenzter Turn (pi-Sidecar, `src/core/coding_agents/`).

| Capability | Signatur (Vorschlag) | Beispielaufruf | Herkunft |
|---|---|---|---|
| `ctx.cap.agent.coding.models` | `({module_id}) → {models[]…}` | `await ctx.cap.agent.coding.models({module_id:'notes'})` | E §3 |
| `ctx.cap.agent.coding.turn` | `({module_id, prompt, faux, preset_id}) → Turn-Ergebnis (wirft bei Turn-Fehler)` | `await ctx.cap.agent.coding.turn({module_id:'notes', prompt, faux:false, preset_id:'ctox'})` | E §3 (+ preset_id, faux) |

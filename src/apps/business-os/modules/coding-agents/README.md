# Coding Agents

**Übersicht aller Agent-Läufe** (Shell v4, Strang AGENT; Bedienkonzept Fassung 4 §3, Owner-Mängel M-03/M-04/M-05).

Der Coding Agent selbst ist keine eigene App mehr. Er sitzt als einklappbare **Seitenleiste in der Code-Ansicht jeder
App** („Code ansehen“ im App-Menü am Icon-Header; `v4/source/agent-panel.js`), arbeitet nur auf dem eigenen Entwurf
(§16.44: `ctox.coding.turn` / `ctox.coding.cancel`, CAS auf `base_draft_rev`), zeigt seine Änderungen als Diff im
Editor und veröffentlicht erst auf Knopfdruck. Konflikte mit eigenen Änderungen kommen als Patch
(`files/coding-patch:<turn_id>`) und werden in der Seitenleiste zum Übernehmen angeboten.

Diese App listet die Läufe:

- Quelle: Plattform-Collection `coding_turns` (Typ `CodingTurn`, Kern-Stub ab Pin 51b1e33), Titel aus `business_apps`.
  Fremdzugriff im `v4-manifest.json`: `coding_turns: read`, `business_apps: read`.
- Sortierung: laufende zuerst, dann neueste. Je Lauf App-Titel, erste Zeile des Auftrags, Zahl der geänderten Dateien,
  Uhrzeit; der Zustand nur als Punkt (aria-label/title), kein Zustandstext.
- Klick auf einen Lauf → `api.shell.openSource(app_id)`: Code-Ansicht der App im App-Fenster, Agent-Seitenleiste offen.
- Suche (App, ID, Auftrag); Deep-Link `api.shell.navigate('coding-agents', {app:'<id>'})` (Altform `{moduleId}`)
  filtert auf eine App; `snapshot()` hält Suche und Filter.
- Kopf: Standard-Pane-Kopf; das App-Icon oben links ist der Icon-Header der Shell (kein eigenes Kopf-Icon).
- Texte: `locales/de.json`, `locales/en.json`.

Keine Befehle aus der UI: die App sendet weder `ctox.coding.turn` noch `ctox.coding.models`. Die Lauf-Fläche
(`ctox-run-control`) ist mit der Funktion in die Seitenleiste gewandert (`scripts/assert-design-system.mjs` prüft sie
dort).

Tests: `node --test modules/coding-agents/tests/*.test.mjs` (Darstellung/Sortierung + App-API-4.0 gegen den Kern-Stub,
`CTOX_APP_API_STUB=…`); Ende-zu-Ende im Rahmen: Fall AG-7 in `v4/frame/lab/frame.test.mjs`.

## Backend (Handler, §16.44-Form seit CAP-3d)

`handlers/ctox.coding.turn.js` und `handlers/ctox.coding.cancel.js` bedienen Befehle anderer Apps (`commands_in`:
freigegebener `ctox.coding.turn` aus `threads`, z. B. Delegation aus „Bugs & Features“) in der Form des Kernvertrags
§16.44: `turn {app_id, draft_id, base_draft_rev, prompt, model?, turn_id?}`, `cancel {turn_id}` →
`ctx.cap.agent.coding.turn/cancel` (CAP-3d). Die V3-Form (`module_id/faux/preset_id`) gibt `invalid`. Solange der Kern
#848 nicht gebunden hat, antwortet er `unavailable`; der Handler macht daraus einen sauberen Befehlsfehler (Zone der
auslösenden App bzw. `on_failed`). `handlers/ctox.coding.models.js` bleibt bei `{module_id}`. Vertrag und Tests siehe
`V4-HANDLERS.md`.
Der pi-Sidecar ist im `ctox`-Binary eingebettet (`src/core/coding_agents/pi-sidecar`) und läuft je
Turn als begrenzter, abgeschotteter Leaf-Prozess ohne Host-Dateisystem und ohne Umgebung/Geheimnisse des Daemons.
Die früheren Collections `coding_agent_workspace_grants`, `coding_agent_sessions`, `coding_agent_events` stehen weiter
in `collections.schema.json`; die Übersicht liest sie nicht mehr.

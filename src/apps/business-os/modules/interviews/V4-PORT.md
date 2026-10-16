# interviews – Port auf App-API 4.0 (Strang UB2, Welle B)

Stand: ABGESCHLOSSEN (15.10.2026, Berlin 13:35 Start → 13:43 grün, gemessen mit `TZ=Europe/Berlin date`).

## Messwerte

| Prüfung | Befehl | Ergebnis |
|---|---|---|
| Port-Scanner | `node v4/tools/port/scan.mjs --app interviews` | vorher 30 Stellen (CSS 78 ungescoped, 22 mit Shell-Vorfahr) → **0 Stellen**, Exit 0 |
| CSS-Scope | `node v4/tools/port/scope-css.mjs interviews modules/interviews/index.css --check` | gescoped |
| Löschlisten-Wächter | `node v4/tools/guards/guard-v4.mjs --json --app modules/interviews` | 0 Treffer außerhalb `handlers/` (2 in `handlers/_ats.js` gehören dem Handler-Strang) |
| v4-Test gegen Kern-Stub (Pin 66edb98) | `CTOX_APP_API_STUB=~/.local/state/shell-v4/stub-pins/66edb98 node --test src/apps/business-os/modules/interviews/tests/v4-api.test.mjs` | **10/10 grün** |
| Bestehende Suite (auf v4 umgestellt) + Kernlogik | `node --test modules/interviews/tests/interviews.test.mjs modules/interviews/core/*.test.mjs` | 19/19 grün |
| Screenshots | `tests/shots/interviews-1280.png`, `tests/shots/interviews-420.png` | selbst angesehen; 420 gestapelt, kein horizontaler Überlauf (im Test geprüft) |

v4-Test deckt: mount (Daten, Titel, je 1 Abo auf `interview_meetings`/`interview_scorecards`, Grammatik selbst
verdrahtet) · `sim.push` live, Löschung verschwindet · Band/Suche · leere Collections ohne Text/Zone · Termin anlegen
(`api.put`, lokal sofort sichtbar, angenommen, Auswahl gebunden) · Zustandsübergang (`api.patch`) · `sim.rejectNext` →
genau ein Kernel-Eintrag `write.rejected:*`, Serverstand zurück, App meldet nicht doppelt · Pflichtfeld → genau ein
Zoneneintrag, kein Write · snapshot → structuredClone → Remount erhält Video-Link-Entwurf, Auswahl, Statusfilter,
offene Ablage, aufgeklappte Werkbank · unmount: 0 Abos, root leer · keine `pageerror`.
Kein Befehlstest: die App sendet **keine** Befehle (nur Collection-Writes, wie vor dem Port).

## Stellen und Entscheidungen

- **R1/R2**: `mount(host, api, {restored})` → `{unmount, snapshot}`; Abos je Montage in `disposers`.
- **R3**: zwei Live-Abos `api.query(<coll>, {})`. Übergänge `findOne().patch()` → `api.patch`, Anlegen `insert` → `api.put`,
  Import `upsert` → `api.put` je Datensatz. Existenzprüfung über `api.get(...).get()` (sieht eigene Writes synchron).
- **R4**: Readiness entfernt. **R10**: `ensureStyles`/`?v=` entfernt. **R14**: `shared/list-state.js` entfällt
  (`meetingListBody` rendert nur Zeilen); `ctox:shared/pane-grammar.js` dynamisch im `mount` (Node-Unit-Tests importieren
  `index.js` weiter direkt).
- **R11**: Gate-Callout entfernt (HTML + CSS), ebenso „Termin angelegt/aktualisiert“, „Import abgeschlossen“,
  „Offline: Datenbank nicht verfügbar“, Leer-/Sync-Texte (auch `locales/*.json` `empty`). Zone: `interviews.form.candidate_required`
  (warn), `interviews.meeting.missing` (warn), `interviews.meeting.update_failed` / `interviews.meeting.create_failed`
  (error, nur bei lokaler Ablehnung `invalid`; Core-Ablehnungen meldet der Kernel selbst als `write.rejected:<op>`),
  `interviews.import.failed` (error), `interviews.import.partial` (warn). Erfolg still.
- **R8**: `api.shell.locale` live + `t('interviews.<key>')` über eingebettete Texte; Titel `title` (aus Locales) →
  `api.shell.setTitle` und Spaltentitel (vorher `ctx.manifest.title`).
- **Verhaltenskorrektur (für Snapshot nötig, dokumentiert)**: Das Formular wurde bei JEDEM Render neu befüllt; mit zwei
  Live-Abos überschrieb die Scorecard-Emission den wiederhergestellten Entwurf (gemessen im Test). Jetzt nur bei
  geänderter Auswahl oder geändertem Datensatz (`filledSig`). Ein Live-Update desselben Termins befüllt weiter neu
  (wie vorher). Frisch angelegter Termin bleibt ausgewählt, auch vor der nächsten Emission (vorher: `await insert`).
- **R16 CSS**: gescoped. v4-Rahmen: `--shell-v2-icon-size: 0px` am App-root (Icon liegt in v4 in eigener Chrome-Zeile,
  die `calc()`-Einzüge fallen aufs Normalpolster), 70px/62px-Steuerungszonen entfernt, `container: business-app-window /
  inline-size` am App-root. Layoutfehler behoben: `.ctox-pane-title-row { height: 100% }` zog bei offener Filterablage
  die Titelzeile über den ganzen Kopf (Ablage lag unter Liste/Band, im Test gemessen: Klick auf Reset abgefangen).
- **R17**: `schema.js` gelöscht – tief gleich zu `collections.schema.json` (beide Collections, Version 0, keine Migrationen;
  kanonischer JSON-Vergleich).

## Befehlstypen – Einstufung (§16.13)

| Befehlstyp | Rust-Treffer | Einstufung | Ziel |
|---|---:|---|---|
| (UI sendet keine Befehle) | – | – | Termine/Scorecards sind reine Collection-Writes |
| `ats.interview.transcribe` (nur Core, nicht von dieser UI gesendet) | 2 (`store.rs:24292`, `store_ats_commands.rs:1151`) | Fachlogik + Fähigkeit | Handler (Handler-Strang) setzt `transcript_id`; Transkription selbst = Fähigkeit `ctx.cap.stt.*` |

## OFFEN

1. Container je Fenster und Spaltenteiler wie intake OFFEN-1/2 (Rahmen-Strang).
2. Zustandsübergänge sind Einzel-Writes ohne Prüfung der Übergangsregel im Core (wie vorher); ob `state`-Übergänge ein
   Befehl mit Handler werden sollen, entscheidet der Handler-/Manifest-Strang.
3. Befund Fachlogik (unverändert gelassen): `core/scheduling.js:31` `isNoShow` liest `Number(meeting.end)`; ein Termin
   ohne Start wird mit `end: null` angelegt, `Number(null) === 0` ist endlich → sofort „No-Show“ (im Screenshot sichtbar:
   `cand-emil-neu`). Gehört dem Fach-/Handler-Strang.

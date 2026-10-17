# nachweise – Port auf App-API 4.0 (Strang UB2, Welle B)

Stand: ABGESCHLOSSEN (15.10.2026, Berlin 13:44 Start → 14:14 grün, gemessen mit `TZ=Europe/Berlin date`).

## Messwerte

| Prüfung | Befehl | Ergebnis |
|---|---|---|
| Port-Scanner | `node v4/tools/port/scan.mjs --app nachweise` | vorher 31 Stellen (CSS 59 ungescoped) → **0 Stellen**, Exit 0 |
| CSS-Scope | `node v4/tools/port/scope-css.mjs nachweise modules/nachweise/index.css --check` | gescoped |
| Löschlisten-Wächter | `node v4/tools/guards/guard-v4.mjs --json --app modules/nachweise` | 0 Treffer außerhalb `handlers/` (2 in `handlers/_ats.js` gehören dem Handler-Strang) |
| v4-Test gegen Kern-Stub (Pin 66edb98) | `CTOX_APP_API_STUB=~/.local/state/shell-v4/stub-pins/66edb98 node --test src/apps/business-os/modules/nachweise/tests/v4-api.test.mjs` | **10/10 grün** |
| Bestehende Suite (auf v4 umgestellt) + Kernlogik | `node --test modules/nachweise/tests/nachweise.test.mjs modules/nachweise/core/*.test.mjs` | 16/16 grün |
| Screenshots | `tests/shots/nachweise-1280.png`, `tests/shots/nachweise-420.png` | selbst angesehen; 420 gestapelt, kein horizontaler Überlauf (im Test geprüft) |

v4-Test deckt: mount (Daten, Titel, Untertitel, 1 Abo, Bänder 3/2 kritisch, Grammatik selbst verdrahtet) · `sim.push`
live/Löschung · Band „Kritisch“/Suche · leere Collection ohne Text/Zone · Nachweis anlegen (`api.put`, angenommen,
Formular geleert) · `ats.deployment.check`: Dokument sofort in `business_commands` mit unveränderter Nutzlast, Ergebnis
bereit → Zone info, blockiert → derselbe Code als warn mit Gründen · `ats.leistungsnachweis.signoff`: `sim.failCommand`
→ genau ein Zoneneintrag, Eingabe bleibt; `done` → info-Zusammenfassung (Rechnung, Netto), Formular geleert ·
snapshot → structuredClone → Remount erhält Sign-off-Entwurf, Formularentwurf, Auswahl, Statusfilter, offene Ablage und
aufgeklappte Prüf-/Sign-off-Formulare · unmount: 0 Abos, root leer · keine `pageerror`.

## Stellen und Entscheidungen

- **R1/R2**: `mount(host, api, {restored})` → `{unmount, snapshot}`; Abos je Montage in `disposers`.
- **R3**: `api.query('business_credentials', {})`, lokale Sortierung wie vorher (`updated_at_ms || created_at_ms` absteigend).
  Anlegen `insert` → `api.put` (Formular leeren nach `local`), Import `upsert` → `api.put` je Datensatz.
- **R5 Befehle** `ats.deployment.check`, `ats.leistungsnachweis.signoff`: `api.command(type, payload)` mit unveränderter
  Nutzlast, nie warten. `failed` → Zone `nachweise.command.<type>` (error). Ergebnisse haben in der App keine andere
  sichtbare Wirkung (vorher Gate-Callout) und gehen deshalb in die Zone: `nachweise.deployment.check` (info „✓ Einsatzbereit“
  / warn „✗ Nicht einsatzbereit“ + Gründe), `nachweise.signoff.result` (warn „Sign-off blockiert“ + Gründe / info
  „Entleiher-Sign-off gespeichert · Leistungsnachweis · Abrechnung freigegeben · Rechnung · Netto“). Ergebnisform
  `result.result || result` wie vorher.
- **R6**: `ctx.notifications.show` entfernt. **R11**: Gate-Callout (HTML + CSS), „Prüfe Einsatzbereitschaft…“,
  „Sign-off läuft…“, „Offline: …“, „Datenbank nicht verfügbar“, Leer-/Sync-Texte (auch Locales `empty`/`syncing`) entfernt.
  Weitere Zonen-Codes: `nachweise.form.subject_required`, `nachweise.form.record_required` (warn),
  `nachweise.credential.save_failed` (error, nur lokale Ablehnung), `nachweise.import.invalid|empty|partial`.
- **R8**: Locale live + `t('nachweise.<key>')`; `subtitle` (vorher `ctx.manifest.description`) eingebettet, wörtlich aus
  `locales/*.json`. Englisch `notReady` von „Not ready for deployment“ auf „Blocked for deployment“ geändert: der Wächter
  wertet „not ready“ als Zustandsvokabular (Treffer `vocab:en-not-ready`), der Text ist hier aber ein Fachergebnis.
- **R10/R14**: `ensureStyles`/`?v=` entfernt; `shared/list-state.js` entfällt; `ctox:shared/pane-grammar.js` dynamisch im `mount`.
- **R16 CSS**: gescoped; v4-Rahmen wie interviews (`--shell-v2-icon-size: 0px`, `container: business-app-window /
  inline-size` am App-root, 70px-Steuerungszonen entfernt – auch in der 768px-Regel und am Untertitel).
- **R17**: `schema.js` gelöscht – tief gleich zu `collections.schema.json` (kanonischer JSON-Vergleich, keine Migrationen).

## Befehlstypen – Einstufung (§16.13)

| Befehlstyp | Rust-Treffer `grep -rn '"<type>"' src/core --include='*.rs'` | Einstufung | Ziel |
|---|---:|---|---|
| `ats.deployment.check` | 2 (`store.rs:24276`, `store_ats_commands.rs:54`) | Fachlogik | JS-Handler (Handler-Strang): Pflicht-Typen gegen `business_credentials` des Subjekts prüfen → `{ready, blockers[{credential_type, reason}]}` |
| `ats.leistungsnachweis.signoff` | 2 (`store.rs:24288`, `store_ats_commands.rs:709`) | Fachlogik | JS-Handler: Sign-off am Leistungsnachweis, Rechnung über `ats_canonical_invoice` → `{ok, record_id, billing_released, invoice_id, net_total}` bzw. `blockers` |

## OFFEN

1. Container je Fenster und Spaltenteiler wie intake OFFEN-1/2 (Rahmen-Strang).
2. Ergebnis-Darstellung über die Zone: Prüfergebnis und Sign-off-Zusammenfassung waren vorher Inhalt der App (Callout).
   Falls die Zone dafür zu knapp ist (eine Zeile), braucht es ein Fach-Element im Inhalt (z. B. Ergebnisliste je Subjekt
   aus einer Collection) – Entscheidung Owner/Shell.
3. Befund Fachlogik (unverändert): ein ohne „Gültig bis“ angelegter Nachweis bekommt `valid_until_ms: 0` und erscheint als
   „abgelaufen vor 20742 Tage“ (Screenshot 1280, `cand-emil-neu`).

## Wortlaut zurückgedreht (Strang LBL, 15.10.2026 15:45)

- Zurück: EN `notReady` „Blocked for deployment“ → **„Not ready for deployment“** (`index.js`, Ergebnis der Einsatzbereitschaftsprüfung, Zone `nachweise.deployment.check`). DE „Nicht einsatzbereit“ war unverändert.
- Original = `upstream-main`. Markierung `v4-allow: label` (JS: Kommentar in derselben Zeile, HTML: `<!-- v4-allow: label -->` davor, JSON: Markerschlüssel `"//<schlüssel>": "v4-allow: label"` davor) nur an Zeilen, die der Wächter sonst meldet. Prüfung: v4-api-Test gegen Stub-Pin cff5ba5 grün, `scan.mjs --app <id>` unverändert, guard-v4 ohne Treffer außerhalb `handlers/` (10/10).


## Nachtrag Strang VIS (Geometrie-Labor, 15.10.2026)

- G4: Untertitel `.nachweise-sub` bricht um.

Prüfung: `node v4/tools/geometry/geometry-v4.mjs --app nachweise --shared` (Stub-Pin 9fa9948) → alle Läufe OK; `node --test modules/nachweise/tests/v4-api.test.mjs` grün; `scan.mjs --app nachweise` unverändert. Rechte Leerzone der Fenstersteuerung jetzt über `var(--shell-v2-control-inset, …)` (R18), wo die App sie reserviert.

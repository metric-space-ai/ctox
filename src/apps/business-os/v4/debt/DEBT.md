# Shell v4 – Schuldenregister (Strang DEBT)

Stand: Do 15.10.2026 16:04 (Berlin, gemessen), Klon `~/Documents/ctox-shell-v4`, Arbeitsbaum inkl. paralleler Stränge.
Zweck: Keine Altlast überlebt den v4-Umbau stillschweigend. Jeder offene Punkt aus den Port-, Handler-, Rahmen-,
Labor- und Manifest-Berichten steht hier genau einmal (Duplikate zusammengeführt, alle Fundstellen in „Quelle“).
Löschbare Dateien und Blöcke stehen in `LOESCHPLAN.md`, die Zahlenwächter in `ratchet.mjs`.

Quellpfade sind relativ zu `src/apps/business-os/`. Kürzel: `P` = `V4-PORT.md`, `H` = `V4-HANDLERS.md` der genannten App
(`modules/<app>/` bzw. `customer-modules/<app>/`). Zeilennummern gemessen 15.10. 15:3x–15:52; die Berichte werden
parallel weitergeschrieben, bei Abweichung gilt der zitierte Wortlaut.

**Art**: Shell (Rahmen, App-Host, Zone, SW, Werkzeuge der Shell) · Kern (Kernel, Laufzeit, Capability, Vertrag/Stub) ·
App-UI (UI-Code einer App) · Handler (JS-Handler einer App) · Fachentscheidung (Owner muss entscheiden) · Test.
**Besitzer**: Shell v4 · ctox sync v4 · Owner-Fachentscheidung (Dateigrenzen laut
`~/.local/state/ctox-sync-v4/docs/dateigrenzen.md`).
**Termine** (Entwurf §14): **S7** Sa 17.10. 23:00 (App-Ports fertig) · **S8** So 18.10. 12:00 (alle Apps gegen echte
API, Hot-Plug-Abnahme) · **S9** Mo 19.10. 05:00 (Löschliste null, Altwege gelöscht, Legacy-Rest offen ausgewiesen).
Owner-Punkte tragen „vor S9“: ohne Entscheidung bis dahin werden sie in der Endabnahme als offen ausgewiesen, nicht still
geschlossen.

## Zählung

| Art \ Besitzer | Shell v4 | ctox sync v4 | Owner-Fachentscheidung | Summe |
|---|---:|---:|---:|---:|
| Shell | 27 | 0 | 0 | 27 |
| Kern | 0 | 40 | 0 | 40 |
| App-UI | 19 | 0 | 0 | 19 |
| Handler | 12 | 0 | 0 | 12 |
| Fachentscheidung | 0 | 0 | 22 | 22 |
| Test | 6 | 2 | 0 | 8 |
| **Summe** | **64** | **42** | **22** | **128** |

Nach Termin: S7 22 · S8 61 · S9 23 · vor S9 (Owner) 22. Zählung per Skript aus den Tabellen unten
(`awk -F'|' '/^\| D-/{print $4"/"$7}' DEBT.md | sort | uniq -c`), nicht von Hand.

Kennzahlen, die die Einträge stützen (gemessen 15:57, `ratchet-baseline.json`, `measuredAt` 13:57Z):
guard-v4 **1728** Treffer (ohne Tests) in 22 Gruppen: shell/app.js 674, shared 465, OLG 168, scripts 133,
office-engine 88, modules/conversations 76, v4 62, shell/index.html 20, Rest ≤ 7; `ported.json` leer (0 Apps).
scan.mjs **441** Stellen über 42 Apps (40 modules, OLG, code-editor): OLG 332, conversations 102, app-store 5, ctox 1,
outbound 1; CSS ungescoped 856 (OLG 661, conversations 195). Manifest M2: 114 Capabilities, 6 Legacy-Typen laut
Manifest, 161 offene `_todo` (`v4/tools/manifest/REPORT-M2.md:16-28`); im Handler-Code heute 113 verschiedene
`ctx.cap.*`-Pfade ohne legacy (`grep -rhoE "ctx\.cap\.[a-z_]+(\.[a-z_]+)+" modules/*/handlers customer-modules/*/handlers | sort -u`).
Parallele Stränge senken die Zahlen laufend (15:43 → 15:57: guard 1744 → 1728, cv-print-builder und mail auf 0);
Baseline nur über `node ratchet.mjs --update` nachziehen.

## Geschlossen seit den Berichten (nicht gezählt, nur Beleg)

- OFFEN-C `registerHeader`-Form: geschlossen, `registerHeader(el: HTMLElement | null)` (`v4/frame/INTEGRATION.md:136`).
- OFFEN-G / 10-s-Frist ↔ Rückfallziel: mount-Frist 9 s, Doppelpuffer 8,5 s (`v4/frame/INTEGRATION.md:135`).
- `activation.reportFailed`, `windowState`, `kernel.status`-Strom, `link.state online|offline`: im Stub-Pin 67229d2
  vorhanden (`v4/frame/INTEGRATION.md:123-134`; ehemals `v4/lab/app-host/RESULTS.md:64,107,114`).
- Wächter-Fehltreffer `assets/**/provenance.json` (kundenpipeline): guard nimmt die Datei vom Vokabular aus
  (`v4/tools/guards/guard-v4.mjs:544-546`; ehemals `modules/kundenpipeline/V4-PORT.md:62`).
- OLG-Legacy-Rest 8 → 0 im Handler-Code (`customer-modules/outbound-lead-generation/V4-PORT.md:161`); offen bleibt nur
  das veraltete Manifest (D-019).
- OLG §15a Teilschritte 4–8b portiert (`customer-modules/outbound-lead-generation/V4-PORT.md:293-313`); Rest D-099.

## A. Querschnitt (mehrere Apps oder Shell/Kern allgemein)

| ID | App/Bereich | Art | Beschreibung | Quelle | Besitzer | Auslöser/Termin |
|---|---|---|---|---|---|---|
| D-001 | 21 Apps | App-UI | Modulzustand ist modulweit; zwei gleichzeitige Fenster derselben App teilen `state` (echte Mehrfenster-Fähigkeit fehlt). | appsec-pentest P:72 · buchhaltung P:79 · calendar P:53 · coding-agents P:69 · consent P:52 · creator P:89 · customers P:125 · esign P:48 · invoices P:88 · iot P:76 · knowledge P:108 · kundenpipeline P:67 · matching P:93 · news P:77 · notes P:74 · outbound P:130 · reports P:87 · shiftflow P:65 · support P:83 · threads P:155 · tickets P:84 | Shell v4 | S9 (je App `multi_instance:false` im Manifest, bis dahin ausweisen) |
| D-002 | Rahmen | Shell | v4-Rahmen setzt keinen Container `business-app-window` um das Fenster-root; Apps tragen ihn selbst, sonst greifen die `@container`-Regeln nicht. | intake P:65 · interviews P:59 · nachweise P:56 · placements P:54 · submissions P:48 · customers P:121 · matching P:102 · mail P:168 · notes P:72 · shiftflow P:71 · support P:88 · invoices P:96 · tickets P:92 · spreadsheets P:66 | Shell v4 | S7 |
| D-003 | Rahmen | Shell | Spaltenteiler `.ctox-column-resizer[data-resizer]` ist sichtbar, aber ohne Wirkung (v2-Shell verdrahtete ihn, v4 niemand). | intake P:67 · interviews P:59 · nachweise P:56 · placements P:54 · submissions P:48 · support P:88 · invoices P:96 · tickets P:92 | Shell v4 | S7 |
| D-004 | Rahmen | Shell | v2-Kopfzonen (`--shell-v2-icon-size`, 70–80 px Icon-/Steuerungszone, 74-px-Doppelkopf) erzeugen im v4-Rahmen Leerraum; Regel fehlt (Variable 0 oder PORTING-Regel). | documents P:153 · spreadsheets P:102 · reports P:89 · ctox P:145 · calendar P:56 · credentials P:69 · matching P:104 · mail P:170 · placements P:55 · submissions P:48 · support P:90 | Shell v4 | S7 |
| D-005 | App-API | Shell | OFFEN-A: Form von `api.shell.dialog`/`menu` (`prompt` mit Eingabe, `alert`, Menü-Antwort) nicht festgelegt; Stub liefert nur `spec.default`. | `v4/tools/port/app-api.d.ts:9` · iot P:72,75 · appsec-pentest P:76 · knowledge P:106 | Shell v4 | S7 (Form festlegen, Stub-Eintrag bei ctox sync v4 anmelden) |
| D-006 | App-API | Shell | OFFEN-B: Schlüsselraum von `api.shell.t(key)` offen; alle Apps halten eingebettete Rückfalltexte doppelt zu `locales/`. | `v4/tools/port/app-api.d.ts:11` · appsec-pentest P:77 · coding-agents P:72 · knowledge P:107 · research P:88 · matching P:101 | Shell v4 | S7 |
| D-007 | App-API | Fachentscheidung | OFFEN-D: keine Icon- und Drawer-API; jede App hält Glyphen und Drawer selbst (Doppelcode in ≥ 6 Apps). Entscheiden, ob das für v4.0 so bleibt. | `v4/tools/port/app-api.d.ts:14` · documents P:74 · research P:70 · customers P:54 · spreadsheets P:68 · knowledge P:106 · appsec-pentest P:76 | Owner-Fachentscheidung | vor S9 |
| D-008 | App-API | Kern | OFFEN-E: `$options` in Abfragen steht nur im Stub, nicht in Architektur §10.2a Nr. 5. | `v4/tools/port/app-api.d.ts:16` | ctox sync v4 | S8 |
| D-009 | Shell/SW | Shell | OFFEN-F: `ctox:shared/…` nur über den Service Worker auflösbar; Shell-Dateien importieren noch relativ `../../shared/…`. | `v4/tools/port/app-api.d.ts:17` · `v4/frame/RESULTS.md:128` · research P:88 | Shell v4 | S7 |
| D-010 | App-API | Kern | `record_id` und `client_context` fehlen im Befehlsvertrag (`api.command` hat 2 Parameter, Handler-Befehl `{id,type,payload,actor,app_id,app_ver}`); Handler lesen sie optional, ohne `client_context` scheitert jeder Adapter-Writeback. | customers P:118 · threads P:143 · outbound H:134,137 · invoices H:122 · news H:59 · support H:93 · kundenpipeline H:79 · threads H:79 · customers H:98 · OLG P:247 | ctox sync v4 | S8 |
| D-011 | App-API | Kern | Befehl mit vorgegebener ID fehlt (`api.command(type,payload,{id})` bzw. `ctx.command(…,{id})`); research-Lineage passt dadurch **nie** (jede Quittung würde verworfen), news-Anweisung trägt falsche ID. | research P:77 · news P:70 · news H:38 | ctox sync v4 | S8 (blockiert research-Abschluss) |
| D-012 | App-API | Kern | Keine Presence-API (`api.presence.set/subscribe`); „X bearbeitet gerade“ entfällt in 4 Apps. | calendar P:51 · customers P:116 · notes P:69 · threads P:148 | ctox sync v4 | S9 (als offen ausweisen) |
| D-013 | Shell-Vertrag | Shell | Keine Chat-Übergabe/Thread-Rückmeldung: Apps feuern weiter das DOM-Ereignis `ctox-business-os-record-focus` bzw. `ctox-business-os-open-chat` aus root; Shell hört es nicht, `api.shell.openChat`/Rückweg fehlt. | ctox P:138 · documents P:135 · outbound P:123 · tickets P:79 · research P:84 · support P:80 · mail P:155 · desktop P:87 | Shell v4 | S8 |
| D-014 | Shell-Vertrag | Shell | `onRoute` feuert nur nach dem Mount; Startparameter eines noch nicht offenen Ziels (Auth-Assist → browser) kommen nicht an. Vertrag: Params als erstes `onRoute` nachliefern oder `mount(…,{params})`. | browser P:158 · outbound P:132 | Shell v4 | S7 |
| D-015 | Navigation | App-UI | Ziel-Apps lesen die neuen Navigationsparameter noch nicht (ctox: `task_id/command_id/task_status/source_module`, `navigate('shell',{open:'settings'})`, `chat`; documents/spreadsheets: `open_file.ref`). | reports P:85 · ctox P:143 · desktop P:87 · explorer P:84 | Shell v4 | S7 |
| D-016 | Shell-Vertrag | Shell | `api.shell.openFile(ref)` → Zuordnung Dateityp→App in der Shell fehlt; ob `openFile` mehr als `ref` trägt, ist offen. | file-viewer P:61 · explorer P:84 | Shell v4 | S8 |
| D-017 | SW/Ladeweg | Shell | `vendor/`- und app-übergreifende relative Importe (`../../vendor/…`, `../../../office-engine/…`) brechen unter `/~a/<app>@<ver>/`; SW-Auslieferung von `vendor/` oder `ctox:vendor/` fehlt. | documents P:130 · notes P:70 · spreadsheets P:92 · mail P:145 | Shell v4 | S7 |
| D-018 | Manifeste | Shell | 161 offene `_todo` in den v4-Manifesten (Standardpunkte rights/client_limit/app_ver je Manifest + app-spezifische). | `v4/tools/manifest/REPORT-M2.md:28,230-297` | Shell v4 | S8 |
| D-019 | Manifest OLG/outbound | Shell | Manifeste veraltet: OLG führt noch 2 (`_todo`: 2) bzw. 8 `cap.legacy.*`, obwohl der OLG-Handler-Code 0 legacy nutzt; neue Caps (`scrape.*`, `secret.*`, `runtime_state.*`, `command.get`, `web.capture`) fehlen. | OLG P:348 · `v4/tools/manifest/REPORT-M2.md:79` · `v4/manifests/outbound-lead-generation.manifest.json:85` | Shell v4 | S7 (Generator neu laufen lassen) |
| D-020 | Routing | Kern | 9 Befehlstypen haben Handler in zwei Apps (outbound + OLG); der Core muss je Typ eindeutig routen bzw. App-Zuordnung je Absender festlegen. | `v4/tools/manifest/REPORT-M2.md:218-228` · outbound H:161 · OLG P:343 | ctox sync v4 | S8 |
| D-021 | Handler-Laufzeit | Kern | `ctx.read` muss vollständige Ergebnisse bzw. `skip` liefern (keine 200er-Kappe); Versand-Gate und Mahnlauf übersehen sonst Datensätze. | invoices H:70 · outbound H:179 · threads H:99 · news H:66 | ctox sync v4 | S8 |
| D-022 | Handler-Laufzeit | Kern | Relative Importe innerhalb `handlers/` (`./_lib…`, Handler importiert Handler) müssen im vm-Kontext erlaubt sein. | customers H:114 · invoices H:127 · kundenpipeline H:80 · support H:114 | ctox sync v4 | S8 |
| D-023 | Handler-Laufzeit | Kern | vm-Kontext ohne `URL`, `TextEncoder`, `crypto`, ID-Quelle; Wunsch `ctx.uuid()`. Handler tragen eigene SHA-256/UTF-8/URL-Ersatzcodes. | customers H:117 · research H:64 · outbound H:147 · threads H:77 · ctox H:178 | ctox sync v4 | S8 |
| D-024 | Handler-Laufzeit | Kern | Transaktionssemantik offen: read-your-writes, Serialisierung gegen parallele Befehle, verschachtelte `ctx.transaction`. | invoices H:125 · threads H:91 · support H:97 · ctox H:180 | ctox sync v4 | S8 |
| D-025 | Capabilities | Kern | 114 verschiedene Capabilities müssen im Kern existieren (agent 20, iot 17, scrape 15, mail 11, knowledge 8, secret 5+1, pentest 5, browser 5, schedule 4, office 4, doc 3, Rest 16). Ohne sie laufen 309 Handler nur gegen Attrappen. | `v4/tools/manifest/REPORT-M2.md:17,81-198` | ctox sync v4 | S8 |
| D-026 | Capabilities | Kern | Neue Cap-Namen seit M2 nicht im Manifest-Bericht: `cap.web.capture`, `cap.command.get`, `cap.agent.task.get`, `cap.outbound.field_status_witnesses.issue`, `cap.mail.validate_contacts`, `cap.sql.external.write`. | OLG P:226-229 · OLG P:139 | ctox sync v4 | S8 |
| D-027 | Queue-Rückweg | Kern | Drei verschiedene Rückkehrformen nach Agenten-Turn: `reply_to{hook:'onReply'}` (news, cv-print), `on_complete:<typ>` (research), Handler-Neuaufruf mit `command.agent_reply` (OLG). Ein Vertrag nötig, sonst läuft kein Abschluss. | news H:37 · research H:38,55 · OLG P:227 · cv-print-builder H:16 | ctox sync v4 | S8 |
| D-028 | Trigger | Kern | Zeit-/Ereignis-Trigger müssen in der Laufzeit laufen: outbound `scheduler.tick` PT5M (ohne ihn nur per Knopf), support `intake.project` auf `communication_messages`, OLG `update_digest.tick`, threads-Projektionen (W-4), OLG `recover_once`. | outbound P:112 · support H:77 · OLG P:259 · OLG P:317 · `v4/tools/manifest/REPORT-M2.md:200-208,297` | ctox sync v4 | S8 |
| D-029 | Handler-Laufzeit | Kern | Lange Handler (Recherche, Browser) brauchen mehr als 30 s CPU-Grenze bzw. Zeit in der Capability. | outbound H:173 · research H:57 | ctox sync v4 | S8 |
| D-030 | Handler-Laufzeit | Kern | 1-MiB-Ergebnisgrenze: `outbound.audit.export` liefert alle Datensätze von 11 Collections. | outbound H:145 | ctox sync v4 | S9 |
| D-031 | Sync-Pfad | Kern | Peer-Schreibwächter `peer_lead_write_allowed` (outbound_field_review.rs:88) hat in v4 keinen Ort; blockierend für die OLG-Lead-Collection. | OLG P:337 | ctox sync v4 | S8 |
| D-032 | Projektion | Kern | DataFrame→Collection-Projektion fehlt (Kampagnen-DataFrames, `knowledge_table_rows`): `ctx.cursor`/`change.seq`, `dataframe_ref`, Filter/Sortierung auf Parquet. | outbound P:115 · knowledge P:96-103 · research P:73 | ctox sync v4 | S8 |
| D-033 | Dateien | Kern | `cap.office` liest/schreibt noch `*_blob_chunks`; muss über `files/<blob_id>` laufen; Altbestand Chunks → Blobs migrieren. | documents P:118 · spreadsheets P:87 · explorer P:82 · file-viewer P:67 · importer P:68 | ctox sync v4 | S8 |
| D-034 | Dateien | Kern | `files`-API: kein Bereichsabruf, kein Ordnerfeld, Löschrecht/Aufbewahrung offen, sha256-Prüfung im echten Kern unbestätigt. | file-viewer P:65,74 · explorer P:77 · documents P:133 | ctox sync v4 | S8 |
| D-035 | Vorlieben | App-UI | Nutzerweite App-Vorlieben (storageScope) entfallen; heute nur im Fenster-Snapshot (ausgeblendete Firmen, Toolbar, Übersetzungen, Rücksetz-Sicherung). Je App eine Vorlieben-Collection anlegen. | documents P:149 · outbound P:125 · desktop P:85 · matching P:98 | Shell v4 | S9 |
| D-036 | Seeds | Handler | Seed-Daten beim Öffnen als Seiteneffekt (Demo-Buchungen, Kontenrahmen, Kalender, Systemordner, Runbooks) → Befehl/Handler des Eigentümers. | buchhaltung P:85 · calendar P:59 · explorer P:74 · documents P:58 | Shell v4 | S9 |
| D-037 | Transaktionen | Handler | Mehrdokument-Vorgänge im UI ohne Transaktion (Storno, Festschreiben, Bankabgleich, Kalender löschen, Buchung stornieren, Interview-Statusübergänge) → Handler. | buchhaltung P:71 · calendar P:49 · interviews P:60 | Shell v4 | S9 |
| D-038 | Bestandssuiten | Test | Alt-Suiten auf v3-`ctx` nicht umgestellt und nicht gelaufen (appsec 373+958 Z., coding-agents, documents 1340 Z. + 2, spreadsheets 2, mail-browser-qa 1168 Z.). | appsec-pentest P:80 · coding-agents P:76 · documents P:142 · spreadsheets P:97 · mail P:159 | Shell v4 | S8 |
| D-039 | Werkzeug | Shell | `scope-css.mjs`: `:root[data-theme…]`-Präfix trifft nie, Spezifität +(0,1,0) bricht gemeinsame Regeln; Korrektur im Werkzeug, Nachprüfung aller Apps. | documents P:140 · spreadsheets P:101 | Shell v4 | S7 |
| D-040 | Werkzeug | Test | `v4/tools/port/test-api.mjs createTestApi`: `shell` eingefroren, `setTitle` wirft `TypeError`; invoices umgeht es mit Kopie. | invoices P:91 | Shell v4 | S7 |
| D-041 | Wächter | Shell | guard-v4-Fehltreffer: HTML-Attribut `loading="lazy"`, LLM-Prompt-Texte, Fachvokabular (Bankstatus, Testläufer) → Ausnahmeregeln oder Fachentscheidung. | news P:68 · matching P:112 · buchhaltung P:82 · nachweise P:40 | Shell v4 | S7 |
| D-042 | Zustandszone | Shell | Zonenrichtlinie offen: Verfall von `info`, lokale Quittungen („ID kopiert“), Ergebnisse zu groß für eine Zeile, Aktion am Eintrag (`stale_base` → rebase), Eintrag je App statt je Fenster, Sprung über `ref`. | shiftflow P:72 · submissions P:49 · nachweise P:57 · `v4/source/RESULTS.md:172` · `v4/frame/INTEGRATION.md:161` · matching P:106 | Shell v4 | S8 |
| D-043 | Rechte-Spiegel | Kern | Akteur trägt kein `display_name`/`is_admin`, keine Grants je Scope, kein `canWriteCollection`; Spiegel in Apps nur über Rolle. | ctox P:147 · documents P:138 · spreadsheets P:96 · customers P:64 · ctox H:185 · threads H:74 | ctox sync v4 | S8 |
| D-044 | Rechte | Kern | Manifest `command_rights` + Modulzuweisung im Core vor dem Handler (`policy.rs:483-490`); Handler prüfen keine Rechte, Schreibrecht auf jede geschriebene Collection. | support H:78,99 · outbound H:164 · kundenpipeline H:42 · esign H:39 | ctox sync v4 | S8 |
| D-045 | DSGVO | Kern | Lösch-Grabstein des Sync-Stores darf keinen PII-Payload behalten (ATS-Löschung über `ctx.remove`). | esign H:44 | ctox sync v4 | S8 |
| D-046 | Befehlsfolge | Kern | Reihenfolgegarantie offen: Outbox Dokumente-vor-Befehl, Befehle einer App in Einreichungsreihenfolge (Composer-Ketten, outbound-Ketten). | importer P:71 · mail P:134 · outbound P:121 | ctox sync v4 | S8 |
| D-047 | Workjet-Brücke | Kern | Workjet-Hostbrücke `app.js` 13362–15924 + globalThis-Exporte 676–679 gehört Phase 11; blockiert das Löschen von `app.js`. | `~/.local/state/ctox-sync-v4/docs/dateigrenzen.md:7-10` · Entwurf §10 | ctox sync v4 | S9 |
| D-048 | Paritätsbeleg | Test | Handler-Ports nur gegen gelesene Rust-Logik getestet, nie gegen eine Rust-Ausführung (serde-Schlüsselordnung `preserve_order`, `1.0` vs `1`, Hashes von Revision/Version). | matching H:65 · outbound H:149 · esign H:21 · research H:58 · customers H:103 | ctox sync v4 | S8 (ein Paritätslauf über die Build-Spur) |
| D-049 | ATS-Kopien | Handler | `handlers/_ats.js` 7× byte-gleich kopiert, `office-bridge-v4.js` 2× kopiert (keine Cross-App-Importe); Änderungen müssen alle Kopien treffen, Wächter fehlt. | esign H:26-32 · spreadsheets P:54 | Shell v4 | S9 (sha-Wächter in CI) |

## B. Shell-Laufzeit, Rahmen, Quelltext-Modus, Ladeweg

| ID | App/Bereich | Art | Beschreibung | Quelle | Besitzer | Auslöser/Termin |
|---|---|---|---|---|---|---|
| D-050 | Rahmen | Shell | `shared/window-manager.js` hat kein `dispose()`; je Shell-Tausch bleiben `resize`-Listener und ResizeObserver zurück (Leck je Tausch). | `v4/frame/RESULTS.md:121` | Shell v4 | S7 |
| D-051 | Rahmen | Shell | Pins: schmal nur bei angehefteten Apps sichtbar, nicht umsortierbar; Launcher-Reihenfolge hat Speicher, aber keine Bedienung. | `v4/frame/RESULTS.md:123,125` · `v4/frame/INTEGRATION.md:157` | Shell v4 | S8 |
| D-052 | Rahmen | Test | Wiederherstellen aus `windowState.get('shell')` nach Neuladen desselben Tabs eingebaut, aber ungetestet. | `v4/frame/INTEGRATION.md:158` | Shell v4 | S8 |
| D-053 | Rahmen | Shell | `frame_palette` wird nur als Attribut gesetzt, `frame.css` wertet es nicht aus. | `v4/frame/INTEGRATION.md:160` | Shell v4 | S8 |
| D-054 | App-Host | Shell | Shell-Tausch zeigt 12–67 ms leere Fläche; beim App-Swap gelten altes und neues CSS kurz gleichzeitig (Versions-Scoping fehlt). | `v4/lab/app-host/RESULTS.md:68,71` | Shell v4 | S8 |
| D-055 | App-Host | Shell | Remount in Hintergrund-Tabs (gedrosselte Timer/rAF) läuft in den Rückfall; Remount bis `visibilitychange` aufschieben. | `v4/lab/app-host/RESULTS.md:84` | Shell v4 | S8 |
| D-056 | App-Host | Shell | Generische Sicherung kennt nur `id`/`name` (kein `contenteditable`, virtuelle Listen); hängendes `mount` läuft weiter. | `v4/lab/app-host/RESULTS.md:75` | Shell v4 | S9 (ausweisen) |
| D-057 | window_state | Kern | `window_state` enthält ungesicherte Nutzereingaben (PII); Aufbewahrung/Löschung beim Abmelden festlegen. | `v4/lab/app-host/RESULTS.md:88` | ctox sync v4 | S8 |
| D-058 | Aktivierung | Kern | Aktivierungsereignis ohne Veröffentlichungs-Zeitstempel (`t0`/`published_at`); „Veröffentlichen → mounted“ im Feld nicht messbar. | `v4/lab/app-host/RESULTS.md:110` | ctox sync v4 | S9 |
| D-059 | Ladeweg | Test | Safari, iOS-WKWebView, Android-WebView, Electron/Workjet Ops ungemessen; harter Neuladen/SW-Neustart nur Chromium. | `v4/lab/ladeweg/RESULTS.md:137-139,164-165` | Shell v4 | S8 |
| D-060 | Geometrie | Test | Geometrie-Labor (3 Breiten) ist nicht gegen den v4-Rahmen gelaufen; v4-Ersatz `v4/tools/geometry/geometry-v4.mjs` muss es ablösen. | `v4/frame/RESULTS.md:130` · desktop P:90 · shiftflow P:69 · tickets P:87 | Shell v4 | S8 |
| D-061 | Quelltext-Modus | Kern | B1: unverändertes `manifest.json` im Entwurf schlägt das veröffentlichte Manifest (setzte Handler auf gelöschte Datei zurück); B3: Feld per `setManifest` nicht löschbar. | `v4/source/RESULTS.md:163,176` | ctox sync v4 | S8 |
| D-062 | Quelltext-Modus | Shell | Keine Live-Übernahme des Entwurfs aus einem zweiten Tab desselben Nutzers; `parseCollectionsFile` prüft Typen nicht (Ablehnung erst im Kern). | `v4/source/RESULTS.md:183,158` | Shell v4 | S9 |
| D-063 | Quelltext-Modus | Shell | `v4/source/source-mode.css` hängt an `--v4-*`-Aliasen der gelöschten `shell/window-chrome.css`; Kommentare zeigen noch dorthin. | `v4/frame/INTEGRATION.md:111-115` | Shell v4 | S7 |
| D-064 | code-editor | Shell | `desktop-apps/code-editor` als Shell-Bibliothek: `v4/source/editor.js` nutzt nur `resolveMonacoBaseUrl` (zwei Monaco-Lader); Theme folgt `prefers-color-scheme` statt `api.shell.theme`; `data-app` im fremden Fenster. | code-editor P:64,70,72 | Shell v4 | S7 |
| D-065 | v4-Code | Shell | guard-v4 meldet im eigenen v4-Baum 63 Treffer (u. a. `vocab:de-laedt` 42, `ref-legacy-catalog-file` 11) – Löschliste gilt auch für die neue Shell. | `ratchet-baseline.json` Gruppe `v4` | Shell v4 | S9 |
| D-066 | Shell-Chrome | App-UI | App-Code greift außerhalb root: Crew-Stylesheet in `document.head` (tickets), Vendor-CSS per `<link>` (calendar), Debug-Griff `window.__ctoxInvoicesModule`, `globalThis.CTOX_LAST_AGGREGATION` (shiftflow), `globalThis.ctoxBusinessOsDesktop.startFileDrag` (explorer), Shell-CSS mit `.shell-window`-Vorfahr (browser). | tickets P:82 · calendar P:54 · invoices P:89 · shiftflow P:67 · explorer P:88 · browser P:141 | Shell v4 | S7 |
| D-067 | shared | App-UI | `shared/universal-importer.js` ist v3 (`ctx.db`, `ctx.sync`, `commandBus`, `?v=`-CSS, Ladetexte); outbound/mail/OLG/matching nutzen ihn nur mit `dispatch:false`. | outbound P:127 · mail P:148 · `LOESCHPLAN.md` L-31 | Shell v4 | S8 |
| D-068 | office-engine | App-UI | `office-engine/src/business-os-bridge.mjs` (`freezeEmailContent` u. a.) ist v3 (ctx.db, Chunk-Lease, Ergebnis-Leseweg); mail „Rich Text · Word“ scheitert; guard 88 Treffer in `office-engine`. | mail P:139 · `ratchet-baseline.json` Gruppe `office-engine` | Shell v4 | S8 |

## C. Je App (App-UI, Handler, Befunde, Rust-Eigenheiten „treu portiert“)

| ID | App/Bereich | Art | Beschreibung | Quelle | Besitzer | Auslöser/Termin |
|---|---|---|---|---|---|---|
| D-069 | conversations | App-UI | Voller UI-Port steht aus (Plan 1–12 in V4-PORT.md): scan 102 Stellen, CSS ungescoped 195, guard 76; lokale Materialisierung von Outbound-Entscheidungen erst nach Handler-Übernahme entfernen. | conversations P:100,103 · `ratchet-baseline.json` `modules/conversations` | Shell v4 | S7 |
| D-070 | cv-print-builder | Handler | UI portiert (scan/guard 0), aber `ctox.cv_print.apply_parse` liest nur `desktop_files` statt `files`-Ref; Parsing neuer Profile endet `failed`. | cv-print-builder P:113 | Shell v4 | S8 |
| D-071 | app-store | App-UI | Port begonnen, kein V4-PORT.md: scan 5 (R9-actor 4, R17 `schema.js`), guard 1; alle 16 Befehlstypen PLATTFORM (Kern-Lebenszyklus, nur dokumentiert). | app-store H:10-34 · `ratchet-baseline.json` `modules/app-store` | Shell v4 | S7 |
| D-072 | outbound-lead-generation | App-UI | OLG-UI nicht auf App-API 4.0 portiert: scan 332 Stellen + 661 CSS ungescoped, guard 168 (u. a. 122 Zustandstexte, `requireRevision`, `requestNative`, `commandBus`, 5 `setInterval`); `schema.js` vorhanden. | `ratchet-baseline.json` `customer-modules/outbound-lead-generation` · OLG P:134-138 | Shell v4 | S7 |
| D-073 | outbound-lead-generation | Fachentscheidung | Kundenfassung der installierten `module.json` (thesen) weicht von main ab (`sellify_companies`, Logo); vor Aktivierung abgleichen, nichts überschreiben. | OLG P:169 · `v4/tools/manifest/REPORT-M2.md:286-287` | Owner-Fachentscheidung | vor S9 (vor jeder thesen-Aktivierung) |
| D-074 | outbound-lead-generation | Fachentscheidung | Quellen-Zugangsverwaltung (`ctox.secret.*`, `SecretsManage`) ist für Rolle `user` gesperrt – so lassen? | OLG P:197 | Owner-Fachentscheidung | vor S9 |
| D-075 | outbound-lead-generation | Fachentscheidung | Belegregel: Code (Rust-Stand) verlangt 1 Quelle je Feld, Owner-Regel 07.10. „Ziel 2, 1 nur im Notfall“ ist nicht umgesetzt; Prompt-Text nennt „zwei Hosts“. | OLG P:273 · OLG P:318 | Owner-Fachentscheidung | vor S9 |
| D-076 | outbound-lead-generation | Handler | Rust-Befund treu portiert: `hoist_top_level_field_entries` verschiebt `result.custom_fields`, `apply_custom_field_results` sieht im echten Writeback nie ein Feld. | OLG P:288 | Shell v4 | S9 (Folgekarte) |
| D-077 | outbound-lead-generation | Fachentscheidung | Eigentümer offen: `outbound_lead_generation_adapters` (browser und OLG, gleiche Version, anderes Schema), Sellify-Collections (`owner_app:null`), `outbound_research_policies` (kein Schema). | OLG P:332,335,336 · outbound H:156 | Owner-Fachentscheidung | vor S9 |
| D-078 | outbound-lead-generation | Kern | RPC `ctox.outbound.field_review_view.v1` ohne Nutzer im App-Code; MCP-Aktion `outbound.leads.import_and_research` nicht im Manifest (MCP-Strang). | OLG P:341 · OLG P:156 | ctox sync v4 | S9 |
| D-079 | outbound | Handler | 4 Typen laufen noch über `ctx.cap.legacy` (`research_source.test`, `research_source.reprobe`, `sellify.lookup`, `update_digest.send_now`), obwohl OLG dieselben Typen portiert hat; Probe-Grant-Prüfung muss in die Capability. | `modules/outbound/handlers/outbound.research_source.test.js:6` · `…reprobe.js:6` · `…sellify.lookup.js:6` · `…update_digest.send_now.js:6` · outbound H:169 | Shell v4 | S9 (Legacy-Rest offen ausweisen) |
| D-080 | outbound | App-UI | `schema.js` nicht gelöscht (conversations importiert es); ungenutzte Knowledge-Projektionsfunktionen stehen; Filterleiste läuft bei 420 px aus dem Fenster. | outbound P:106,140,138 | Shell v4 | S7 |
| D-081 | outbound | Handler | Rust-Eigenheiten treu portiert: Klassifizierer meldet `approval_required` statt `provider_queue_failed` (P8), `reply.classify` storniert keine Entwürfe (P13), Pfad statt Schlüsselliste bei `skillbook_id` (P3), Rust schreibt außerhalb des Schemas (P9: Sperrliste, `sender_assignments`, `outbound_research_policies`). | outbound H:139,153,156,166 | Shell v4 | S9 (Schema angleichen; Fachkorrektur → D-108) |
| D-082 | ctox | App-UI | `schema.js` nicht gelöscht (11 fremde Importeure, siehe LOESCHPLAN L-14); Manifest führt 9 `workjet_*` als eigene Collections, die die UI nicht nutzt. | ctox P:123,132 | Shell v4 | S8 |
| D-083 | ctox | Fachentscheidung | Unklar, ob §16.13 „Projektionen“ auch die Core-Spiegel (Queue, Harness-Ereignisse, Crew) meint → zweiter Schritt „Core liefert Rohereignisse, App projiziert“. | ctox H:28 | Owner-Fachentscheidung | vor S9 |
| D-084 | ctox | Handler | Abweichungen: Kapazitätsprüfung + Anlage nicht atomar (Wettlauf bei 999 Mitgliedern), `safe_prose`-Regex ohne Unicode-Faltung, Zeitstempelformat `Z`/ms statt `+00:00`/ns. | ctox H:180,182,184 | Shell v4 | S9 (ausweisen) |
| D-085 | ctox | Fachentscheidung | Telemetrie-Hinweise „nicht live“, „noch kein Plan“, „keine Live-Messwerte“ bleiben im Verlauf – Owner-Sichtung (Zustandstext-Regel). | ctox P:149 | Owner-Fachentscheidung | vor S9 |
| D-086 | browser | Kern | Live-Bild und Eingabe brauchen eine nicht persistierte Echtzeit-Spur (`api.live.open`); `browser_frames` darf kein Bildstrom werden. Skriptansicht ohne Weg. | browser P:121,130 | ctox sync v4 | S8 |
| D-087 | browser | Shell | Keine Fokus-API (`api.shell.window.focused`); App führt Fokus selbst, Pacht-/Eingaberegeln hängen daran. | browser P:133 | Shell v4 | S8 |
| D-088 | browser | Fachentscheidung | 9 Befehlstypen ohne Rust und ohne Handler (Knöpfe entfernen oder Handler bauen); `state.steuerungZurueckholen` wird nie gesetzt (Übernahme durch Interaktion greift nie). | browser P:136,162 | Owner-Fachentscheidung | vor S9 |
| D-089 | buchhaltung | Fachentscheidung | GoBD-Festschreibung war clientseitig nie wirksam; Buchungs-/Stornonummern clientseitig gezählt (sichtbar doppelt `J-2026-0002`), `accounting_number_series` ungenutzt. | buchhaltung P:75,77 | Owner-Fachentscheidung | vor S9 (dann Handler) |
| D-090 | calendar | App-UI | EventCalendar-Kopfknöpfe ohne Beschriftung; Kontextspalte bei ≤ 1024 px immer sichtbar. | calendar P:56 | Shell v4 | S8 |
| D-091 | consent | Fachentscheidung | Art.-17-Löschung ohne Rückfrage (`api.shell.dialog` wäre der Ort). | consent P:53 | Owner-Fachentscheidung | vor S9 |
| D-092 | esign | Fachentscheidung | Formular wird beim Absenden sofort geleert; bei `failed` ist die Eingabe weg. Entwurf bis `done` halten? | esign P:49 | Owner-Fachentscheidung | vor S9 |
| D-093 | creator | Kern | `business_apps` braucht ein Herkunftsmerkmal (`source`/`entry`), sonst zeigt das Eigenbau-Band nichts; Quelltext-Schnittstelle für Apps offen. | creator P:84,85 | ctox sync v4 | S8 |
| D-094 | credentials | Kern | Geheimnis-Metadaten als geschwärzte Collection über `ctx.cap.secrets` statt Lesebefehl `ctox.secret.list`; Schwärzung des Werts im Befehlsdokument gehört in die Befehlsannahme; ob Apps `business_commands` patchen dürfen. | credentials P:58,64 · credentials H:25 | ctox sync v4 | S8 |
| D-095 | mail | Kern | Mailserver-Verwaltung ohne Collection (Vorschlag `mail_server_state`); Konten-Entzug aus der Projektion (`scope_revoke`) nicht gebaut; Primärschlüssel ≠ `id` bei `communication_*`. | mail P:118,130,150 | ctox sync v4 | S8 |
| D-096 | mail | App-UI | Befund vorbestehend: `t('approved')`/`t('queued')` liefern im Status-Label Sätze statt Kurzlabel; Manifest-`styles` braucht `editor/editor.css`. | mail P:172,164 | Shell v4 | S8 |
| D-097 | desktop | App-UI | Tote CSS-Regeln `.widget-sync`, `.widget-status`, `.status-pulse`, `.desktop-icon-empty`; Eigentum `desktop_files`/`desktop_file_chunks`/`channel_pairing_state` offen (explorer schreibt `desktop_files`). | desktop P:88,82 · explorer P:70 | Shell v4 | S8 |
| D-098 | documents / spreadsheets | App-UI | Toter Code mechanisch portiert (Kontextmenü/-Chat, `renderRight`, `openNewDocumentDrawer`, `renderWorkflowPanel`, `dispatchSpreadsheetRunbook` …); `ensureRuntimeReady` ohne Gegenstück. | documents P:94 · spreadsheets P:72,94 | Shell v4 | S9 |
| D-099 | outbound-lead-generation | Kern | `recover_once` (:106) und `complete_after_native_email_validation` (:1552) bleiben nativ → Trigger; Zeugen-Ausgabe nach Commit statt in der Transaktion. | OLG P:316-317,329 | ctox sync v4 | S8 |
| D-100 | explorer / importer | Kern | Fremd-Schreibrechte `desktop_files`/`desktop_file_chunks` (write) bzw. Desktop-Fähigkeit; importer-Snapshot bleibt im Desktop-Datenmodell bis Vertragsänderung `app.create` ↔ `files`. | explorer P:71 · importer P:68,72 | ctox sync v4 | S8 |
| D-101 | file-viewer | Kern | Ergebnisform von `ctox.file.materialize` (heute `desktop_files` + Chunks) muss `result.ref` oder `files`-Dokument liefern. | file-viewer P:67 | ctox sync v4 | S8 |
| D-102 | intake / submissions / placements | Handler | Apps lesen `status.result` in der alten Dispatch-Form (`ok/blockers/gate/decision`, `outcome.result ?? outcome`); JS-Handler müssen genau diese Form liefern. | intake P:71 · submissions P:28 · placements P:30 | Shell v4 | S8 |
| D-103 | interviews / nachweise | Fachentscheidung | Fachbefunde: Termin ohne Start wird sofort „No-Show“ (`isNoShow`, `Number(null)`); Nachweis ohne „Gültig bis“ gilt als „abgelaufen vor 20742 Tagen“. | interviews P:62 · nachweise P:60 | Owner-Fachentscheidung | vor S9 |
| D-104 | nachweise | Fachentscheidung | Prüf- und Sign-off-Ergebnis steht nur noch einzeilig in der Zone; Fach-Element im Inhalt nötig? | nachweise P:57 | Owner-Fachentscheidung | vor S9 |
| D-105 | invoices | Handler | Mahnlauf ignoriert `filter.invoice_id` (mahnt alle überfälligen); `payment_allocations.payment_id` = `null` verletzt Schema; 10 echte Stubs („Phase 7+“) + halber Stub `dunning.letter.send` (versendet nichts). | invoices P:77,81 · invoices H:14-19 | Shell v4 | S8 |
| D-106 | kundenpipeline | Kern | Fachlogik ohne Befehlstyp nicht portiert: `project_inbound_messages` (Mail→Vorgang), `enqueue_triage_task`, `cards_json`; braucht Ereignis-Abo `mail.inbound`. | kundenpipeline H:22-29 | ctox sync v4 | S9 (ausweisen) |
| D-107 | kundenpipeline | App-UI | `module.json` trägt `layout.shell_contract: "v2"`; unbenutzte CSS-Regel `.kundenpipeline-command-status`. | kundenpipeline P:65,68 | Shell v4 | S8 |
| D-108 | outbound / threads / support | Fachentscheidung | Rust-Fachfehler treu übernommen, Korrektur durch Owner: `ai.request` prüft `risk_class` nach dem Schreiben (U-8); `support.reply.send` versendet nie (kein Verbraucher, U2); `number_field` nur Ganzzahlen (`confidence 0.82` → 0, U6); SLA-Neuanwendung ohne Ereignis (U7); `resolved` ohne Pflichtprüfung (U8); `apply_suggestion` führt nichts aus (U9); Zuweisungsstrategie/Geschäftszeiten ungelesen (U10). | threads H:93 · support H:90,100,103,106,108,110 | Owner-Fachentscheidung | vor S9 |
| D-109 | support | Fachentscheidung | JSON-Import `open_from_thread` ohne `conversation_id` scheitert immer (`invalid`); ID-Regel festlegen (z. B. `support_conv_<uuidv5>`). | support P:76 · support H:85 | Owner-Fachentscheidung | vor S9 |
| D-110 | threads | App-UI | `ctox_queue_tasks`-Migration 3 fehlt in `collections.schema.json`; `reports.manifest.json` verweist auf gelöschtes `threads/schema.js`; `paging.js` ohne Nutzer. | threads P:151,152,157 | Shell v4 | S7 |
| D-111 | threads | Fachentscheidung | Benachrichtigungen außerhalb des Fensters (Web-Notification, Decision-Hub) entfallen; Zähler „AI arbeitet/Blockiert“ nur für die Auswahl. | threads P:149,153 | Owner-Fachentscheidung | vor S9 |
| D-112 | reports | Kern | Rückweg schreibt in fremde Collection `business_module_reports` (Eigentum ctox); sauber wäre Projektion beim Abschluss von `ctox.coding.turn`. | reports P:81 | ctox sync v4 | S8 |
| D-113 | research | Fachentscheidung | Chat-Übergabe entfällt: Systematic Research läuft immer direkt statt über den Chat (Fachverhalten-Abweichung). | research P:84 | Owner-Fachentscheidung | vor S9 |
| D-114 | research | Handler | `research.systematic.report.create` läuft über `ctx.cap.legacy` (Eigentümer documents-Port); `research.systematic.run.complete` braucht den on_complete-Rückweg (D-027). | research H:26,31 | Shell v4 | S9 (Legacy-Rest ausweisen) |
| D-115 | matching | App-UI | Anforderungs-Auswahl (`matchingPaneState`) geht beim Remount verloren; Laufzeit-Übersetzungen ohne Speicherort. | matching P:95,98 | Shell v4 | S8 |
| D-116 | matching | Handler | Schema-Hülle `{id,kind,title,status,data}`: Handler bewerten UI-angelegte Anforderungen anders als Rust (bewusste Korrektur); `write_import_artifact` entfällt. | matching H:45,56 | Shell v4 | S9 (ausweisen) |
| D-117 | news | Fachentscheidung | Quellen-Pillen „Anmeldung nötig/offen/Fehler/wird repariert“ bleiben im Inhalt – fällt das unter „keine Status-Pills“? Kategorie-Chips nicht im Snapshot. | news P:75,78 | Owner-Fachentscheidung | vor S9 |
| D-118 | notes | Kern | `conflictStrategy: 'field-merge'` im v4-Manifest nicht ausdrückbar (Feld-Merge im Core). | notes P:67 | ctox sync v4 | S8 |
| D-119 | notes | Fachentscheidung | Rückgängig-Knopf ohne Text als Fensterelement – Zustandsanzeige? | notes P:75 | Owner-Fachentscheidung | vor S9 |
| D-120 | customers | Fachentscheidung | Befehlsprotokoll/Debug-Ansicht entfällt ersatzlos; zurück im Inhalt nur per Produktentscheidung. | customers P:127 | Owner-Fachentscheidung | vor S9 |
| D-121 | submissions / placements | Fachentscheidung | Werkbank-Kopf 74 px (zwei v2-Bänder) bei einer Titelzeile; v2-Vertrag „gleiche Kopfhöhen“ in v4 behalten? `offers` im Schema, nie gelesen. | placements P:55,57 · submissions P:48 | Owner-Fachentscheidung | vor S9 |
| D-122 | tickets | Kern | Fall-Zustandsmaschine, Ticket-DB und Projektion bleiben im Kern (`cap.ctox.ticket.run`); UI nur umgestellt. | tickets P:77 · tickets H:40 | ctox sync v4 | S8 |
| D-123 | shiftflow | App-UI | Tab-Leiste und „Dienstplan veröffentlichen“ laufen bei 420 px über den Rand (bestehende v2-Geometrie). | shiftflow P:69 | Shell v4 | S8 |
| D-124 | appsec-pentest / coding-agents | Test | Screenshots des Stub-Harness ohne Shell-Design-Tokens/Pane-Grid belegen nicht die finale Optik. | appsec-pentest P:78 · coding-agents P:74 | Shell v4 | S8 |
| D-125 | knowledge | Kern | Eine View je Nutzer und Tabelle (`kv:<actor>:<table>`): zwei Fenster auf dieselbe Tabelle teilen das Fenster. | knowledge P:102 | ctox sync v4 | S9 (ausweisen) |
| D-126 | iot | Fachentscheidung | Fachlogik aller 17 iot-Typen bleibt in der Engine (`cap.iot.*` statt JS) – bewusst als Capability statt `cap.legacy` gezählt; bestätigen. | iot H:42-44 | Owner-Fachentscheidung | vor S9 |
| D-127 | Kern-Rauchtest | Test | `src/core/rxdb/tools/browser_rust_smoke.js` lädt gelöschte `schema.js` (tickets, invoices, iot, ctox, outbound) – bricht. | tickets P:90 · invoices P:94 · iot P:74 · `LOESCHPLAN.md` L-14/L-15 | ctox sync v4 | S8 (Datei liegt in `src/core/rxdb/`) |
| D-128 | Löschliste | Shell | Altdateien und Altblöcke laut `LOESCHPLAN.md` (app.js, index.html-Altteile, mobile-host, Kataloglisten, schema.js, shared-Wegfall, v2-Prüfskripte, tote Tests) – Ziel guard-v4 = 0. | `LOESCHPLAN.md` | Shell v4 | S9 |

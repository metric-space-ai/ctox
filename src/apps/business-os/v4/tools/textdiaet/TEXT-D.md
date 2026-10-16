# TEXT-D – Text- und Gestaltungsdiät (Strang D)

Rahmen: `~/.local/state/shell-v4/briefs/text-diet-common.md` (Regeln 1–8). Apps: app-store, creator, importer, credentials,
ctox (inkl. Bugs & Features / „CTOX Intake“), coding-agents, browser, appsec-pentest, iot.
Referenzen: `~/.local/state/shell-v4/analysis/G-standard-audit-D.md`; appsec-pentest selbst festgelegt (siehe dort).
Vorher-Bilder: `v4/tools/textdiaet/shots/TEXT-D/before/<app>/`, nachher: `…/TEXT-D/after/<app>/`
(rundgang2.mjs, Stub-Pin 68b83a3, 1280 + 420, `--max-steps 3`). Wächter: `v4/tools/guards/textdiaet-rule.mjs`
(von TEXT-A angelegt, unverändert genutzt).

Wächter-Stand vorher (eigene Apps): 75 Treffer – app-store 9, creator 21, importer 4, credentials 11, ctox 2,
coding-agents 4, browser 7, appsec-pentest 13, iot 4.

## Bugs & Features (modules/reports) – nicht von TEXT-D geändert

`modules/reports` gehört laut `TEXT-B.md` zum Strang TEXT-B und wurde während dieses Laufs dort aktiv bearbeitet
(index.js/html/css/locales 20:20). Um keine fremde Arbeit zu überschreiben, habe ich die Dateien nicht angefasst.
Stand des Owner-Befunds im Klon (gelesen, nicht von mir):
- erledigt durch TEXT-B: Felder „Command“/„Task“ (Roh-IDs) aus dem Detail entfernt; Priorität als „Mittel/Hoch/…“;
  Modulname statt app_id (Detail + Listenzeile); Fußzeile „<modul> · <Status>“ trägt nur noch die Übergabe;
  Kicker „CTOX Intake“ entfernt; Umlaute in de.json („Priorität“, „geändert“). Leere Text-Abschnitte wurden schon
  vorher ausgeblendet (`textCard` gibt bei leerem Wert nichts aus); die Owner-Texte „COMMAND nicht angelegt“,
  „Keine Erwartung hinterlegt.“, „Noch keine CTOX-Annahme …“ existieren im Klon nirgends mehr (älterer Ausbaustand).
- Regel 8 (Detail-Kopf „→|“/„▢“ unter dem Knopfcluster): im Bild **jetzt frei** – 20:3x-Aufnahme
  `v4/tools/textdiaet/shots/TEXT-D/check/reports/1280-01-open.png` und `420-01-open.png`: beide Knöpfe stehen links
  neben dem Cluster (1280: Knöpfe enden bei x≈1095, Cluster beginnt bei x≈1115). Ursache der Behebung: FR6a-Kit
  (`pane-heads.js` setzt `data-v4-under-controls`, frame.css rückt den Kopf ein) zusammen mit der vorhandenen
  App-Regel `index.css:358` (`padding-right: var(--shell-v2-control-inset)`). Keine Änderung durch TEXT-D nötig.
- Beobachtung: Taskleiste zeigt „Fehler & Wünsche“, Fenster-/Pane-Titel „Bugs & Features“ (uneinheitlich, Launcher-Titel
  stammt nicht aus dieser App-Datei).

## credentials

Referenz: GitHub „Repository secrets“ / macOS Passwörter (Audit D §4).
Entfernt (Zeilen = Stand vorher):
- `index.html:79` gelbe Infobox `.ctox-callout.is-warning.cred-writeonly` „Write-only: Werte werden verschlüsselt im
  CTOX-Secret-Store abgelegt und nie an den Browser zurückgegeben.“ (Regel 1, 4).
- `index.html:88` Hinweiszeile `.cred-value-hint` „Das Wertfeld bleibt immer leer — gespeicherte Werte werden nie
  zurückgegeben.“ (Regel 1/3, doppelt zur Box) → **Tooltip** am Wertfeld (`title`, `data-i18n-tooltip="valueHint"`):
  „Werte werden verschlüsselt gespeichert und nie wieder angezeigt.“
- `index.html:92` leere Box `.ctox-callout[data-cred-gate]` samt `setGate()` (nur noch mit '' aufgerufen, toter Pfad).
- `index.html:27` Kicker „CTOX“ über „Zugangsdaten“ (Regel 3/4); `index.html:72` Kicker „Neu“/„Zugangsdatum“ über dem
  Titel „Neue Zugangsdaten“/<Schlüssel> (Regel 3).
- `index.js:469` Fußzeile „N Einträge · Alle“ (Zähler doppelt zum Reiter „Alle(N)“, Regel 3/5); ohne Recht „— Einträge“.
- `no_permission` (Zone) gekürzt: „Keine Berechtigung für Zugangsdaten (Rolle Chef oder Admin).“
- Locale-Schlüssel kicker/editKicker/newKicker/subtitle (de/en, index.js + locales) entfernt; CSS `.cred-value-hint`,
  `.ctox-callout:empty` entfernt.
Anzahl: 2 Boxen + 1 leere Box, 1 Hinweiszeile, 2 Kicker, 1 Fußzähler = 7.
Test angepasst (nicht geschwächt): `tests/v4-api.test.mjs` prüft zusätzlich Tooltip am Wertfeld und 0 `.ctox-callout`.
Tests: `CTOX_APP_API_STUB=~/.local/state/shell-v4/stub-pins/68b83a3 node --import ./v4/tools/node/ctox-shared.mjs --test
modules/credentials/tests/v4-api.test.mjs` → 8/8 grün. Wächter: 11 → 0.

## app-store

Referenz: Apple App Store (macOS) (Audit D §1). Entfernt/ersetzt (Zeilen = Stand vorher):
- `index.html:14` Kicker „Katalog“ über „App Store“, `index.html:79` Kicker „Entdecken“ über dem Kategorietitel,
  `index.html:148` Kicker „Catalog“ im Detail (Regel 3/4; Schlüssel kicker/discoverKicker/drawerKicker aus de/en entfernt).
- `index.html:142` Hinweisbox `.store-shelf-hint` „Scrollen zum Entdecken · Hülle auswählen“ (Regel 1/2) samt CSS.
- `index.html:174` Bildunterschrift „Reale Produktaufnahme · aktueller Build“ (Regel 2).
- `index.js:858-867` Fußzeilen „N Apps“ (Mitte, doppelt zu „Katalog(N)“) und „N Apps insgesamt“ (links) (Regel 3/5).
- `index.js:1468` Platzhalter „No documentation available yet.“ (englisch, leerer Abschnitt) → leer.
- Begriffe: Abschnitt „Modul-Vertrag“/„Module Contract“ → „Beschreibung“; Seitenleiste „Templates“ → „Vorlagen“,
  „Alles“ → „Alle Apps“ (de.json).
- Gekürzt (> 80 Z., Regel 2): Karte „Neue App per KI-Prompt erstellen“-Beschreibung → „Freie App aus einem Prompt
  erstellen.“; Sperrgründe (Tooltip/Zone) Fork/Installieren/Veröffentlichen auf je einen kurzen Satz.
- Bewusst behalten, mit `// v4-allow: text` markiert: 4 Bestätigungsdialoge (Update mit lokalen Änderungen, externe
  GitHub-/Zip-Installation, Zurücksetzen) – das sind Dialogtexte vor riskanten Aktionen, kein Inhaltstext.
Anzahl: 3 Kicker, 1 Hinweisbox, 1 Bildunterschrift, 2 Fußzähler, 1 Platzhalter = 8 entfernt; 4 Begriffe, 4 Sätze gekürzt.
Tests: `… --test modules/app-store/tests/v4-api.test.mjs` → 15/15 grün. Wächter: 9 → 0.
Nicht Teil der Diät (Audit-Top-5, Umbau): Raster statt 3D-Regal, Updates als Seitenleistenpunkt, Kategorie-Glyphen.

## creator

Referenz: Audit D §2 (Auftrags-Composer wie Copilot/Cursor-Chat, Beispiele als Leerzustand). Entfernt/ersetzt:
- `index.html:12` Kicker „Deine Apps“ über „Weiterarbeiten“ (Regel 3); `index.js:543` derselbe Text als erste Meta-Angabe
  in JEDER App-Karte („Deine Apps · Kategorie · v1“) – leere Meta-Zeile wird nicht mehr gerendert.
- `index.html:66` sichtbares Feldlabel „Deine App-Idee“ (dreifach mit Titel „Was möchtest du bauen?“ und Platzhalter,
  Regel 3) → nur noch `aria-label`; Abschnitt per `aria-labelledby` am Titel.
- `index.html:67` Platzhalter „Zum Beispiel: Erstelle eine einfache CRM-App mit Kundenliste, Suche, Status und
  Detailansicht.“ (94 Z.) → „z. B. CRM mit Kundenliste, Suche und Status“ (de/en).
- `index.js:494` Fußzeile „N Einträge · Apps“ (doppelt zu „Apps(N)“, Regel 3/5).
Anzahl: 2 Kicker-Texte, 1 Label, 1 Fußzähler entfernt, 1 Platzhalter gekürzt = 5.
Tests: `… --test modules/creator/tests/v4-api.test.mjs` 10/10, `modules/creator/creator.test.mjs` 23/23 grün. Wächter: 3 → 0.

## importer

Referenz: Import-Assistenten (macOS Migrationsassistent / Windows-Setup) (Audit D §3). Entfernt/ersetzt:
- `index.html:5` Kicker „CTOX“ über „App Importer“; `index.html:14` Untertitel „Quelle rein. Langlebiger CTOX-App-Job
  raus.“ (Regel 2/4).
- `index.html:19-21` Schritt-Untertitel „App auswählen“, „Harness-Job“, „Validierte App“ (Regel 2/4).
- `index.html:30/71/94` Eyebrows „Importquelle“, „Langlebiger App-Job“, „Importergebnis“ (Regel 3).
- `index.html:31` Überschrift „Eine bestehende App nach Shell V2 portieren.“ → „App importieren“ (Systembegriff, Regel 4).
- `index.html:32` Einleitung „Zielkategorie wählen, dann ein öffentliches GitHub-Repository … langlebigen
  Portierungsjob.“ (192 Z., Regel 2).
- `index.html:59/63` Hinweiszeilen „Nur Lesezugriff; die Dateien gehen als Snapshot an CTOX.“ / „Nur lesende Auswahl für
  Projekte mit mehreren Dateien.“ → **Tooltip** am jeweiligen Knopf (`data-imp-tip`, Regel 1).
- `index.html:81` Feld „Command“ mit Befehls-ID im Fortschritt; `index.js:507/515` Befehls-ID als `<code>` im Ergebnis;
  `index.js:514` „Asset … · Catalog …“-Zeile; englischer Ersatztext „source revision recorded“ (Regel 4).
Anzahl: 1 Kicker, 1 Untertitel, 3 Schrittnotizen, 3 Eyebrows, 1 Einleitung, 2 Hinweise→Tooltip, 4 Roh-ID/Technikzeilen
= 15; 1 Überschrift umbenannt.
Tests: `… --test modules/importer/tests/v4-api.test.mjs` → 8/8 grün. Wächter: 4 → 0.

## ctox (Fenster „Crew“, Aufgaben)

Referenz: Linear / Jira (Audit D §5). Entfernt/ersetzt (Zeilen = Stand vorher):
- `index.js:1587` und `:1729` Fußzeile „N Einträge · Alle [· N angeheftet]“ (Zähler doppelt zu „Alle(N)“, Regel 3/5)
  → Fuß nennt nur noch „N angeheftet“, wenn es Angeheftete gibt.
- `index.js:6522` Leerzeile „keine Live-Messwerte“ (ganzer Abschnitt entfällt ohne Messwerte, Regel 2).
- `index.js:2614` „noch kein Plan“ unter dem leeren Fortschrittsbalken (Regel 2).
- `index.js:2636/2640/2643/2664/2677/2680` Leerzustands-Prosa im Verlauf: „Noch kein Verlauf verfügbar“, „nicht live“,
  „Der Verlauf wird mit dem nächsten Arbeitsschritt verfügbar.“, „Zu diesem Ereignis liegt noch kein Detail vor.“,
  „Noch keine aktuelle Arbeit erfasst.“ (Regel 2/4; der deaktivierte Regler zeigt den Zustand schon).
- `index.js:5806` Klasse `ctox-callout` am „Im Dienst“-Streifen im Mitglied-Drawer (Infobox-Optik, Regel 1) → schlichte
  Zeile. `index.js:6736` `tone-warning` ist eine Status-Badge-Farbe → `// v4-allow: text` (Fehlalarm des Wächters).
Anzahl: 2 Fußzähler, 1 Messwert-Leerzeile, 1 Plan-Leerzeile, 5 Verlaufs-Leertexte, 1 Box-Optik = 10.
Tests angepasst (Regel umgedreht, nicht geschwächt): `test.js` erwartet leeren Fuß statt „2 entries · All“/„0 entries ·
All“ und prüft, dass der leere Verlauf KEINE Prosa (`timelineUnavailable[Detail]`) mehr zeigt.
Tests: `node modules/ctox/test.js` 51/51 ok; `… --test modules/ctox/tests/v4-api.test.mjs` 12/12 grün. Wächter: 2 → 0.
Offen (Regel 8, im Bild prüfen): Detailkopf rechts „− 77 % + · Aufgabe bearbeiten · …“ endet direkt links neben dem
Knopfcluster (nicht darunter).

## coding-agents (Fenster „pi Coding Agent“)

Referenz: GitHub Copilot Chat / Cursor (Audit D §7). Entfernt/ersetzt (Zeilen = Stand vorher):
- `index.html:22/65/121` Kicker „pi Coding Agent“ (Projektliste), App-ID als Kicker über dem App-Titel
  (`index.js:685`, „CODING-AGENTS“ über „coding-agents“), „Artefakt“ über „Task-Fortschritt“ (Regel 3/4).
- `index.html:130` Leerzustand „Noch kein Artefakt. Der Agent veröffentlicht hier eine Live-Seite zu seinem Task
  (session.metadata.artifact_html).“ (Regel 2/4, Feldname) → leer.
- `index.js:87/1030` „Noch keine Turns für diese App.“ (Jargon, Regel 2/4) → leer; Suche ohne Treffer: „Keine Treffer.“
- `index.js:1105` „Noch keine Delegationen.“ → leer.
- `index.js:1063` Fußzeile „N Delegationen · <app-id>“ (Zähler doppelt zu „Delegationen(N)“, App-ID) → nur „Agent arbeitet“,
  solange ein Lauf läuft; `index.js:582` Fußzähler „N Apps“ (nur noch „x von y Apps“ beim Suchen).
- `index.html:108` Hinweiszeile `.coding-agents-hint` + `index.js:706` „Wähle links eine App aus, um einen Turn zu
  delegieren.“ (Regel 1/2).
- `index.js:913` rote Box `.ctox-callout.is-danger` mit dem Fehlertext im Ergebnis – derselbe Text geht bereits per
  `api.status.report` in die Zone (`index.js:883/894`); im Inhalt bleibt das Badge „Fehlgeschlagen“ (Regel 1).
- `index.js:1365` `renderSessionBanner()` (Banner-Klasse, nirgends aufgerufen) – toter Code entfernt.
Anzahl: 3 Kicker, 3 Leertexte, 2 Fußzähler, 1 Hinweiszeile, 1 Fehlerbox, 1 toter Banner = 11.
Tests: `… --test modules/coding-agents/tests/v4-api.test.mjs` 9/9 grün. `coding-agents.test.mjs` 15/16: der Banner-Test
prüft jetzt, dass der tote Banner weg ist; **vorbestehend rot** (nicht von mir): „presentation layer …“ erwartet
`const moduleId = String(event?.params?.moduleId …)` im onRoute, der Code nutzt schon in HEAD `routeAppId(event)`.
Wächter: 4 → 0.

## browser

Referenz: Chrome/Safari/Edge (Audit D §6). Entfernt/ersetzt (Zeilen = Stand vorher):
- `index.html:13` Kicker „CTOX Computer“ über „Sitzungen“, `index.html:73` Kicker „Browser“ über dem Seitentitel (Regel 3/4).
- `index.js:2777` Zustands-Pille „Verbunden“/„Bereit“ im Kopf (`data-browser-status-chip`) → immer verborgen
  (Zustand gehört in die Zone; Regel 4/Owner-Regel „keine Status-Pills“).
- `index.js:2236` Fußzeile „N Sitzung(en) · <Band>“, `index.js:2154` „N Adapter · Scraping“ (Regel 3/5).
- `index.js:2191` Hinweisbox `.browser-adapter-hint` „Verwaltet in der Outbound-App („Quellen & Zugänge“).“ (Regel 1).
- `index.html:180` Hinweiszeile `.browser-precise-hint` (Rückmeldungen „Klick gesendet“ …); der Fehlerfall
  „Eingabe fehlgeschlagen“ geht jetzt per `api.status.report` in die Zone.
- Anmeldeleiste (`data-browser-auth-assist`): bleibt als Chrome-artige Berechtigungsleiste unter der Adresszeile (nur mit
  Aktion; `<!-- v4-allow: text -->`), aber Erklärsätze raus: „Zugangsdaten werden ausschließlich über eine CTOX
  Secret-Referenz eingesetzt.“ → Tooltip am Knopf; „Secret verwenden“ → „Gespeicherte Zugangsdaten verwenden“;
  „CTOX verwendet den verschlüsselten serverseitigen Passkey erst nach Ihrer Bestätigung.“ entfernt; Kicker
  „HTTP Basic Authentifizierung“/„Web Stack Anmeldung“ → „Anmeldung“; Anleitungssatz nur noch als Tooltip, gekürzt.
- Zonentext „Steuerung abgelaufen …“ gekürzt; Agenten-Anweisung (`index.js:1198`, Befehls-Payload) als `v4-allow`.
Anzahl: 2 Kicker, 1 Status-Pille, 2 Fußzähler, 1 Hinweisbox, 1 Hinweiszeile, 2 Erklärsätze, 2 Systembegriffe = 11.
Test angepasst: `tests/v4-api.test.mjs` (Scraping-Band) prüft statt „Adapter · Scraping“, dass der Fuß leer ist und nie
den veralteten Sitzungs-Zähler zeigt. Tests: `… --test modules/browser/tests/v4-api.test.mjs` → 11/11 grün. Wächter: 7 → 0.

## iot

Referenz: Home Assistant / Apple Home (Audit D §9). Entfernt/ersetzt (Zeilen = Stand vorher):
- `index.html:12/69`, `index.js:656` Kicker „CTOX IoT“ (Produktname über „Assets & Signale“/„Dashboard“, Regel 3/4);
  `index.js:668/676` Kicker-Fallback „CTOX IoT“ → Kicker nur noch als echter Bereichs-/Asset-Pfad.
- `index.js:1012` Editor-Kicker „CTOX-Auftrag · von CTOX programmiert“ (Regel 3).
- `index.js:992` Infobox „Von CTOX generierte Wächter-Logik (Rhai, im Backend pro Messwert ausgewertet). Status: …“ und
  `index.js:999` Infobox „Von CTOX generierter Widget-Code — render(host, api), gesandboxt. …“ (Regel 1/4) → kurze
  **Tooltips** an den Code-Feldern („Wächter-Logik von CTOX (Rhai), je Messwert ausgewertet“, „Widget-Code von CTOX,
  abgeschottet ausgeführt“).
- Feldlabels „Wenn — die Bedingung (Freitext)“ → „Wenn“, „Dann — der Auftrag an CTOX (wird bei Auslösung als Chat
  gespawnt)“ → „Dann“ (+ Tooltip „Wird bei Auslösung als Chat an CTOX gestartet“); Knopf „Speichern → CTOX programmiert
  den Wächter neu“ → „Speichern“ (Regel 2/3).
- `index.js:476` Fußzeile „N Assets · Alle Bereiche · Alle“ (Regel 3/5).
Anzahl: 4 Kicker, 2 Infoboxen, 3 Erklär-Beschriftungen, 1 Fußzähler = 10.
Tests: `… --test modules/iot/tests/v4-api.test.mjs` 11/11, `modules/iot/iot.test.mjs` 11/11 grün. Wächter: 4 → 0.

Weitere Alt-Tests der geänderten Apps (alle grün): app-store `app-store-mount` 3/3, `app-store-shelf-lifecycle` 3/3,
`app-store.test.mjs` 29/29 (Sperrgrund-Tooltip behält „App-Installationsrecht“); browser `browser-start-contract` 1/1,
`browser.test.mjs` 1/1; `credentials.test.mjs` 17/17; `importer.test.mjs` 7/7.

## appsec-pentest (Sicherheitstests)

**Referenz (selbst festgelegt, Audit D §8 fehlt):** Burp Suite DAST (Enterprise) – „Sites/Scans“-Liste links mit
Status je Scan, „New scan“ als knappe Form (Ziel-URL, Scan-Konfiguration, ein Knopf „Scan“), Ergebnisse als
Issue-Tabelle nach Schweregrad; zweitrangig OWASP ZAP „Automated Scan“ (URL + „Attack“). Gewohnt sind dort: keine
Anleitungstexte im leeren Ergebnis, Statuswerte als kurze lokalisierte Labels, Erklärungen nur als Tooltip/Hilfe-Icon,
Freigabe-/Berechtigungsprobleme als Meldung statt als Box.
Entfernt/ersetzt (Zeilen = Stand vorher):
- `index.html:16/82` Kicker „Sicherheit“ über „Penetration Testing“ bzw. „Test-Arbeitsbereich“ (Regel 3).
- `index.html:129` Hinweiszeile „Aktive Prüfungen testen das Ziel mit echten Anfragen — sie laufen nur mit erteilter
  Freigabe.“ → **Tooltip** an der Checkbox „Aktive Prüfungen“ (gekürzt: „… senden echte Anfragen und brauchen eine
  Freigabe.“).
- `index.html:131` Hinweiszeile „Aktive Prüfungen benötigen eine erteilte Freigabe-ID aus dem Freigaben-Arbeitsbereich.“
  → Tooltip am Feld „Freigabe-ID“.
- `index.js:702` Leerzustand „Dein erstes Audit in drei Schritten: 1. … 2. … 3. …“ (Regel 2) → leer (das Formular mit
  „Audit starten“ steht direkt darüber).
- `index.js:713` Box `.appsec-simple-hint` „Diese Prüfung benötigt eine Active-Check-Freigabe — …“ (Regel 1) →
  Zonenwarnung `appsec.verify.approval` (die Zeile trägt weiter die Pille „Freigabe erforderlich“).
- `index.js:1602` `ctox-callout` um den Filter „Ausgewählter Nachweis · Alle Nachweise“ → schlichte Filterzeile.
- `index.js:1287` Fußzeile „N Einträge · Aktiv“ (Regel 3/5).
- Rohwerte in der Testliste (Regel 4): „confirmation required“, „queued“, „resolved“, „ready“, „open“ → „Freigabe
  erforderlich“, „Eingereiht“, „Behoben“, „Bereit“, „Offen“.
- Zonentexte gekürzt (< 80 Z.): „Erst Test für dieses Ziel anlegen und aktive Prüfungen freigeben.“, „Prüfung braucht
  eine Freigabe-ID für aktive Tests.“; Audit-Nachweis-Strings im Befehls-Payload als `v4-allow`.
Anzahl: 2 Kicker, 2 Hinweiszeilen→Tooltip, 1 Anleitung, 1 Hinweisbox→Zone, 1 Box-Optik, 1 Fußzähler, 5 Rohwerte = 13.
Tests angepasst: `appsec-pentest.test.mjs` (Zonencode statt Box, keine Anleitung, Locale-Schlüssel entfernt),
`appsec-pentest.browser.test.mjs` (leere Ergebnisfläche beim Öffnen, Freigabehinweis in der Zone + 0 Boxen, neuer
Wortlaut). Tests: `node modules/appsec-pentest/appsec-pentest.browser.test.mjs` → „browser lifecycle OK“;
`… --test modules/appsec-pentest/appsec-pentest.test.mjs` 1/1; `… --test modules/appsec-pentest/tests/v4-api.test.mjs`
10/10 grün. Wächter: 13 → 0.

## Abschluss

Wächter: eigene Apps 75 → 0 Treffer (`node v4/tools/guards/textdiaet-rule.mjs --app modules/app-store,…,modules/iot`);
Baseline für die neun Apps per `--write-baseline --app …` gesenkt (nur diese Einträge entfernt, geprüft gegen eine Kopie
der Baseline vorher; andere Stränge unverändert); `--check` Exit 0. CSS-Scope: `scope-css.mjs --check` für app-store,
credentials, appsec-pentest „gescoped“.
Nachher-Rundgang (rundgang2, 1280 + 420): alle neun Apps OK, 0 Seitenfehler außer dem vorbestehenden
`/~a/shell@lab/icon.svg`-404 (app-store, coding-agents – auch im Vorher-Lauf).

Offen / nicht Teil der Diät:
- **Rahmen bei 420 px (FR5/FR6a):** der Knopfcluster ist um den Umschalter „App | Code“ breiter geworden (~150 px); in
  schmalen Fenstern bleibt dem Kopf der linken Spalte kaum Platz: Titel „Zugangsdaten“ → „Z…“, Suchfeld „S…“
  (`after/credentials/420-01-open.png`), iot-Titel verschwindet und Filterzeile scrollt seitlich (332 > 310,
  `after/iot/420-01-open.png`; im Vorher-Lauf ohne „App | Code“ noch ohne Befund). Betrifft alle Apps → Rahmen-Strang.
- app-store: Kategoriezähler „(0)“ in der Seitenleiste (App Store hat keine) – vom Alt-Test `app-store.test.mjs:634`
  festgeschrieben, nicht angefasst; Regal → Raster, Updates-Seitenleiste (Audit-Top-5) offen.
- ctox: Fortschrittsbalken ohne Plan bleibt als leerer Balken (Bauteil, kein Text); Detailkopf-Knöpfe enden direkt
  links neben dem Cluster (nicht darunter).
- coding-agents: `coding-agents.test.mjs` „presentation layer …“ vorbestehend rot (onRoute-Regex vs. `routeAppId`).
- creator/coding-agents: Kontrast des Hauptknopfs (dunkel auf Blau), Composer-Kürzel – Audit-Punkte, keine Textdiät.

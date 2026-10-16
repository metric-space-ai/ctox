# TEXT-B – Text- und Gestaltungsdiät (Strang B)

Rahmen: `~/.local/state/shell-v4/briefs/text-diet-common.md`. Apps: customers, outbound, outbound-lead-generation
(customer-modules/, module.json/collections.schema.json unberührt), kundenpipeline, tickets, support, reports, news,
research, knowledge. Vorher-Bilder: `v4/tools/textdiaet/shots/TEXT-B/before/<app>/`, nachher: `…/after/<app>/`
(rundgang2.mjs, Stub-Pin 68b83a3, 1280 + 420). Wächter: `v4/tools/guards/textdiaet-rule.mjs` (von TEXT-A, unverändert genutzt).

Wächter-Stand vorher (eigene Apps): 223 Treffer – help-text 204, infobox 19.

## customers

Entfernt / ersetzt (Zeilen = Stand vor der Änderung):
- `modules/customers/index.js:1596` Kicker „Arbeitsbereich“ über dem Kontonamen (Doppelbeschriftung, Regel 3).
- `index.js:1778` Kicker „CRM“ über „Kundenkonten“ (Systembegriff, Regel 4).
- `index.js:2320` Kicker-Fallback „Inspector“ (nur noch echter Typname, sonst kein Kicker); `index.js:2436` Kicker „Inspector“.
- `index.js:1620` + `:1624-1629` Fußzeile des Arbeitsbereichs „<Konto> · <Phase> · <Zustand>“ bzw. „N Einträge · Übergabe“
  (wiederholte den Titel; Zähler ohne Nutzerwert, Regel 3/5); CSS `.customers-workbench-footer` entfernt.
- `index.js:4220` `labelFor()` gab unbekannte Rohwerte durch („all · ready“, „needs_review“, „imported“, „resolved“
  in Listenzeilen/Übersicht) → unbekannte Werte erscheinen nicht mehr (Regel 4).
- Begriffe (index.js Wertetabellen + labels.de + locales/de.json): Prospect→Interessent, Renewal→Verlängerung,
  Expansion→Ausbau, Churned→Abgewandert, Healthy→Gesund, At Risk→Gefährdet, Qualification/Proposal/Negotiation/
  Committed/Closed Won/Closed Lost→Qualifizierung/Angebot/Verhandlung/Zugesagt/Gewonnen/Verloren, New Business→
  Neugeschäft, Opportunity/Opportunities→Verkaufschance/Verkaufschancen (alle de-Strings), Won/Lost→Gewonnen/Verloren,
  Review→Prüfen, Match→Abgleich, Confidence→Sicherheit, Stage→Phase, Import Batch→Importlauf.
- Behalten: Reiterzähler „Alle Kunden (6)“ (HubSpot-Listenansichten zeigen Zahlen), Listen-Fuß „6 Einträge“ (Referenz: Datensatzzahl).

Tests: `node --import ./v4/tools/node/ctox-shared.mjs --test modules/customers/tests/v4-api.test.mjs` 12/12 grün;
`node modules/customers/customers.test.mjs` OK.
- Regel 8: keine App-Änderung nötig – siehe Abschnitt „Regel 8“ unten (Rahmen rückt `.ctox-pane-header` selbst ein).

## kundenpipeline

- `modules/kundenpipeline/index.html:6` Kicker „Decision Hub“ über „Kundenpipeline“ (englisch + doppelt).
- `index.html:24` Fußzeile `data-list-status` „2 von 6 Entscheidungen“ (Zähler doppelt zu Reiterzahlen Offen/Alle) samt
  `index.js:154` Schreiber.
- `index.html:51` Kicker „Nächster Schritt“ über „Freigabe“; `index.js:206` Abschnittsüberschrift „Entscheidung treffen“/
  „Option wählen“ (dreifache Beschriftung); `index.js:207` Satz „Vorgang für <Kunde>“ (doppelt zum Feld „Kunde“).
- `index.js:200-201` Erklärsätze unter Annehmen/Ablehnen („Die Entscheidung übernehmen und die serverseitige Folgeaktion
  ausführen.“ / „… ohne eine Folgeaktion auszuführen.“) → gekürzt als Tooltip (`title`) am Knopf; Options-Beschreibungen
  der Agent-Eskalation ebenso als Tooltip.
- `index.js:150` Kartenfuß zeigte Roh-ID „vorgang-001“, `index.js:186` Feld „Vorgang“ fiel auf Roh-ID bzw.
  „Agent-Eskalation“ zurück → nur noch Vorgangstitel, sonst „—“.
- Regel 8: keine App-Änderung (siehe unten); ein erster eigener Einzug der Titelzeilen verdoppelte sich mit dem
  FR6a-Kit-Einzug („Freigabe“ stand senkrecht, Bild after/kundenpipeline vor dem Rückbau) und wurde wieder entfernt.
- Nicht geändert: Fenstertitel „Decision Hub“ (`api.shell.setTitle`, vom v4-api-Test und Manifest-Namen getragen →
  Manifest-/Namensentscheidung, nicht Textdiät).

Tests: v4-api + kundenpipeline.test.mjs 15/15 grün.

## tickets

- `modules/tickets/index.html:10` Kicker „CTOX“/„Crew“ über „Tickets“; `index.html:70` + `index.js:922/946` Detail-Kicker
  „Crew“ bzw. Roh-Ticketschlüssel „ITEM-001“; `index.html:88` + `index.js:991` Kicker „Aktionen“ über Roh-Titel „item-001“
  → Titel der rechten Spalte ist jetzt „Aktionen“, leere Kicker per `:empty` ausgeblendet.
- `index.js:1572` `displayStatus()` gab Rohwerte nur groß geschrieben aus („Open“, „Writeback Pending“, „Case“) →
  deutsche Wertetabelle (Neu/Offen/Wartend/Gelöst/Geschlossen/…); unbekannte Rohwerte erscheinen nicht.
- `index.js:1096` Vorgangskarten-Kopf „case · <risk>“ (Rohwerte) → übersetzter Status; `index.js:1101` Zeile
  „<approval_mode> · A<n>“ (Systembegriffe, zeigte „· A“) entfernt.
- `index.js:1103-1106` Zähler je Vorgang „Freigaben 6 / Übertragene Ergebnisse 6 / Rückfragen 6“ → Freigaben und
  Rückfragen nur, wenn > 0; „Übertragene Ergebnisse“-Zähler entfällt (auch in „Nachweise“, `index.js:939-947`; die Karte
  erscheint nur noch mit einem Prüfstatus).
- `index.js:1145-1148` Crew-Arbeitskarte „SELF-WORK · <state>“ / „6 notes“ (Rohwerte, englisch) → übersetzter Status,
  „6 Notizen“ nur wenn > 0.
- Regel 8: `index.css:54` `--tickets-v2-control-inset` folgt jetzt `var(--shell-v2-control-inset, …)` (PORTING R18).

Tests: tickets v4-api 11/11 grün; tickets.test.mjs grün.

## support

- `modules/support/index.html:17` Kicker „Support Desk“ (englisch, doppelt zu „Warteschlangen“); `index.html:88` Kicker
  „Konversation“ über dem Betreff.
- `index.js:1541` `displayStatus()` gab Rohwerte aus („open“, „completed“, „failed“, „cancelled“, „all“) → deutsche
  Wertetabelle (Offen/Wartend/Gelöst/Erledigt/…), unbekannte Werte unsichtbar; neu `displayPriority()` für Liste und
  Prioritäts-Auswahl (vorher „low/normal/high/urgent“ roh im `<select>`).
- `index.js:521-525` Listenzeile „open · all · inbox-001“ → nur übersetzter Status · Priorität (Roh-Inbox-ID entfernt).
- `index.js:1517` Nachrichtenkopf „standard · standard“ (direction · channel roh) → Eingehend/Ausgehend · E-Mail/Chat/…,
  sonst „Nachricht“.
- `index.js:99-100/872-873` „1h snoozen“ / „Morgen“ → „1 Std. zurückstellen“ / „Bis morgen“.
- Regel 8: support rechnete schon mit `--shell-v2-icon-size`/`--shell-v2-control-inset` – unverändert.
- Offen (nicht Textdiät): „Zuordnung: owner“ zeigt die Akteurs-ID; Namen brauchen das Akteursverzeichnis.

Tests: support v4-api + support.test.mjs 26/26 grün.

## reports

- `modules/reports/index.html:12` Kicker „CTOX Intake“ über „Bugs & Features“.
- `index.js:663` Hinweisbox `reports-delegate-hint` „Die Delegation geht als Freigabe an Threads; erst nach der Freigabe
  startet der Coding-Turn.“ → Tooltip am Knopf „Zur Freigabe einreichen“ („Der Coding Agent startet erst nach der
  Freigabe.“); CSS-Regel entfernt.
- `index.js:860-861` Felder „Command: ctox-command-001“ / „Task: queue-task-001“ (Roh-IDs) entfernt – der Sprung
  „CTOX-Aufgabe zeigen“ trägt die Verknüpfung.
- `index.js:786/859` Priorität roh („high/medium/low“) → Hoch/Mittel/Niedrig; Modul-ID in Liste/Detail → App-Titel aus
  `state.apps` (Fallback ID); Ziel-App-Auswahl ohne „(app-id)“.
- `index.js:805` Detail-Fuß „module-001 · Offen“ (doppelt zum Kicker „Bugs · Offen“) → nur noch Übergabestatus,
  „Freigabe: pending“ → „Freigabe: ausstehend“.
- `locales/de.json`: Prioritaet→Priorität, „Was CTOX geaendert hat“→„geändert“, „CTOX Task zeigen“→„CTOX-Aufgabe zeigen“,
  Rollback→Zurücksetzen, Command→Befehl, Task→Aufgabe.
- Test angepasst (`tests/v4-api.test.mjs:170`): statt Roh-ID „task_77“ im Detailtext prüft er jetzt, dass keine Roh-ID
  erscheint und der Sprung „CTOX-Aufgabe zeigen“ live freigeschaltet wird (Ziel-IDs prüft der Navigationstest).
- Regel 8: reports rechnete schon mit `--shell-v2-control-inset` – unverändert.

Tests: reports v4-api 9/9, reports.test.mjs, test.mjs (9) grün.

## news

- `modules/news/index.html:47` + `index.js:572` Fußzeile „6 Themen“ und `index.html:96` + `index.js:656` „1 Beiträge ·
  Gestern“ (Zähler doppelt zu den Reiterzahlen, dazu Grammatikfehler) entfernt.
- `index.html:137` Platzhalter-Prosa im Feld „Worum geht es?“ („Welche neuen Modelle und Tools haben die KI-Labs
  veröffentlicht, und worüber wird im Netz am meisten diskutiert?“, 112 Z.) → „z. B. neue KI-Modelle und Tools“.
- Regel 8: keine App-Änderung (siehe unten).
- Wächter-Treffer `index.js:841` „notice“ ist ein Zonen-Code-Standardwert (`notify(…, code = 'notice')`), kein Markup –
  Falschmeldung, bewusst unverändert.

Tests: news v4-api + news.test.mjs 19/19 grün.

## research

- `modules/research/index.js:2131` Kopf-Hinweis `research-map-hint` „Ziehen: drehen · Scrollen: zoomen“ → Tooltip
  (`title`) an der Graph-Fläche (aria-label beschreibt es weiterhin).
- `index.js:2224` Status-Pille „Live-Projektion“ / „Knowledge Graph“ / Rohwert „invalid_graph_contract“ → nur noch
  „LIVE · CTOX aktualisiert“ während eines laufenden Laufs (der Vertragsfehler hat sein eigenes Element mit Daten-Attributen).
- `index.js:2251` deaktivierter Knopf mit Zustandstext „Report braucht verifizierte Quellen“ als Beschriftung → Knopf heißt
  immer „Dokument erstellen“, der Grund steht nur noch in Tooltip/aria-label.
- `index.js:2294` Erklärsatz `graphMethod` („Größe zeigt fachliche Vernetzung, …“) entfernt.
- `index.js:3564-3566` Hinweisabsatz `research-measurement-note` („Direkt publizierte dimensionslose UIUC-Koeffizienten …“ /
  „Aus CT/CP … abgeleitet …“) → gekürzt als Tooltip an den Reitern „Direkt“/„Abgeleitete Kräfte & Momente“.
- `index.js:3818` Kicker „Kontext“ über „Research“; `index.js:3840` Kicker „Knowledge Base“ → „Wissensbasis“;
  `index.js:3847` Untertitel-Prosa `defaultTaskDesc` („Research, Knowledge und Berichte teilen eine … Lineage.“) entfernt.
- `index.js:3853-3858` Kennzahlkacheln Kandidaten/Quellen/Messpunkte/Fachberichte (doppelt zu den Reiterzahlen unten)
  entfernt.
- `index.js:3899-3901` Lauffelder „Befehl command-001 / Queue task-queue-001 / Thread -“ (Roh-IDs) entfernt – „In CTOX
  ansehen“ trägt die Verknüpfung; Unterzeile fiel auf Rohwert `commandType`/„Systematic Research“ zurück → nur Titel.
- `index.js:6054-6063` Kopfband `ai-warning-banner` über jedem Fachbericht („Evidence-basierter Fachbericht · N verifizierte
  Quellen · M Messpunkte“: Beschriftung + Zähler über dem eigentlichen Berichtstitel) entfernt.
- Regel 8: research rechnete schon mit `--shell-v2-icon-size`/`--shell-v2-control-inset` – unverändert.
- Restliche Wächter-Treffer research: Agenten-Prompts (`index.js:908/2680/4550/5569/5739-5761/5878`, Locale
  `defaultCriteriaText`, `defaultPromptGeneric`) – Inhalt von Textfeldern/Befehlen, keine Bedienprosa; Seed-Quellenkatalog
  `index.js:200-360` (Fachdaten im Code, Audit-Befund) und toter Code `computedDecisionNotes()` – nicht Textdiät.

Tests: research v4-api 9/9, research.test.mjs + research-graph-data.test.mjs grün (77 gesamt), test.mjs grün.

## knowledge

- `modules/knowledge/index.js:1496` Kartenzeile wiederholte den Titel in Großbuchstaben (`group.domainLabel` = Titel,
  z. B. „BUDGETPLANUNG VERTRIEB SÜD · User“) → Domäne nur noch, wenn sie vom Titel abweicht (Regel 3).
- `index.js:25-27` + `locales/de.json` Herkunft „User“/„User + System“ → „Eigene“/„Eigene + System“.
- `index.js:1428-1432` Zählerzeile „Skillbooks (0) · Runbooks (0) · Tabellen (3)“ → nur Arten mit Inhalt (Regel 5).
- `index.js:1667-1669` Detail-Fuß „Knowledge · Budgetplanung Vertrieb Süd“ (wiederholte Art und Titel) entfernt.
- Wächter-Treffer `index.js:3247` (Propeller-Erklärung) ist bereits Spalten-Tooltip (`data-tooltip`/`aria-label` des
  „i“-Knopfs) – Falschmeldung, unverändert.
- Regel 8: knowledge rechnete schon mit `--shell-v2-icon-size`/`--shell-v2-control-inset` – unverändert.

Tests: knowledge v4-api + knowledge.test.mjs 13/13, test.mjs 30 grün.

## outbound

- `modules/outbound/index.js:3102` + `:3166` Recherche-Box unter dem Kopf (`outbound-research-activity`: „CTOX Research –
  CTOX arbeitet gerade nicht: 2 wartet“ + Zählerkacheln + aufklappbare Liste; blau/gelb/rot je Lage) entfernt; die Pille
  „● 2 wartet“ in der Werkzeugleiste trägt den Stand, die bis zu fünf wartenden/laufenden Einträge stehen jetzt in ihrem
  Tooltip (`researchActivityTooltip`).
- `index.js:2712` Kicker „Outbound“ über „Campaigns & Sequenzen“ entfernt; Titel „Kampagnen & Sequenzen“.
- `index.js:2841` Kampagnenzeile zeigte Rohstatus („active“, „lead_qualified“, „reply_received“, „string“) → Wertetabelle
  Aktiv/Pausiert/Entwurf/Planung/Beendet/…, unbekannte Rohwerte unsichtbar; Status-Filter-Optionen ebenso übersetzt.
- `locales/de.json` (29 Werte): Campaign/Campaigns → Kampagne/Kampagnen (Reiter „Campaign“, „Neue Campaign“, Suche,
  Aktionen, Dialoge); Agenten-Prompt `importPromptLabel` bewusst unverändert.
- `active-outreach.js:160` Erklärsatz „CTOX bereitet Nachrichten, Antworten und Termine vor. Versand erst nach expliziter
  Freigabe.“ entfernt; `:163-164` Debug-Knöpfe „Provider-Status abgleichen“ und „Scheduler-Tick“ aus der Nutzerfläche
  entfernt (Audit B: gehören in den Dienst; Handler bleiben), „Audit-Export“ bleibt.
- `active-outreach.js:446` Klasse `outbound-outreach-sequence-warning` → normaler Detailabschnitt (Inhalt + Knopf
  „Aktuelle Sequenz anwenden“ unverändert; CSS-Regel entfernt).
- `index.js:4753-4755` / `:4773-4775` Erklärabsätze „Füge deine Domains hinzu …“ / „Erstelle vollwertige Mail-Accounts …“ →
  Tooltip an der Abschnittsüberschrift; „Custom Domains & DNS“ → „Eigene Domains & DNS“.
- `index.js:4803` Fußhinweis „Gilt für diese Campaign. Änderungen an Spalten und Prompts werden direkt übernommen.“ entfernt.
- Regel 8: outbound rechnete schon mit `--shell-v2-icon-size`/`--shell-v2-control-inset` – unverändert.
- Restliche Wächter-Treffer outbound: Beispiel-Briefings/Prompt-Vorlagen (`index.js:241-467`, `:597-679`, `:6900-7084`,
  Locale-Prompts) und Skill-/Runbook-Texte für `ctox knowledge` (`:1153-1216`) – Inhalte von Feldern bzw. Agentenaufträgen,
  keine Bedienprosa. Offen (Regel 6, Audit B Nr. 2): Knopfwand je Firmenzeile („In Pipeline/Ausblenden/Nachrecherche/
  Validiert/Log“) → Auswahl + Massenaktion bzw. ⋯-Menü ist Umbau, nicht Textdiät.

Tests: outbound v4-api 11/11, outbound.test.mjs, focus-navigation.test.mjs, refresh.test.mjs grün.

## outbound-lead-generation (customer-modules/, module.json/collections.schema.json unberührt)

- `customer-modules/outbound-lead-generation/index.js:1831` Erklärspanne `leadgen-policy-hint` hinter „Pflichtfelder“
  („— angehakt = Pflicht für die Freigabe, ohne Haken optional … Eigene Felder unten anlegen.“) → Tooltip am Label.
- `index.js:1985` Spanne `leadgen-policy-hint` „— leer = Prompt der Neuen Recherche wird verwendet“ (stand zusätzlich
  als Platzhalter im Feld, Regel 3) entfernt; Platzhalter bleibt.
- `index.js:1987` Satz „Welche Felder für die Freigabe Pflicht sind, steht im Reiter „Pflichtfelder“.“ entfernt.
- `index.js:2100` Einleitungsabsatz `leadgen-digest-intro` des Update-Verteilers („CTOX schickt zu den gewählten Zeiten
  …“) → Tooltip am Schalter „Update-Verteiler aktiv“.
- `index.js:12341` Zustandszeile `leadgen-review-notice` „Belege werden noch geladen“ (Ladezustand im Inhalt) entfernt.
- CSS: tote Regeln `.leadgen-policy-hint`, `.leadgen-digest-intro` entfernt.
- Regel 8: `index.css:1417` feste `padding-right: 70px` der Detailspalte → `var(--shell-v2-control-inset, 70px)`;
  neu: Leadliste (761–1120 px, Detail rutscht in Zeile 2) und Kampagnenspalte (≤ 760 px, gestapelt) halten die rechte
  Ecke frei; links war `--shell-v2-icon-size` schon da.
- Bewusst unverändert (Kunden-App thesen, Owner-Fachregeln wörtlich): Recherche-Kennzahlkacheln, Fehlerzeile
  `letzterRechercheFehler` in der Review (lead-spezifischer Befund), In-Fenster-Dialogtexte (`state.port.meldung/
  bestaetigung`, Wächter-Treffer `:738/1887/1906/3824/…/10071` sind Dialog- bzw. Fehlermeldungen, keine Bedienprosa),
  Agenten-Prompts (`:508-530`, `:1217`, `:7916-7923`, `:9211-9216`, `:9980`, `:10382-10415`), Import-/Quellen-Hinweise
  `leadgen-import-notice` (`:3099/6027/6510`, Ergebnis-/Fehlermeldungen in Dialogen). Kandidaten für einen eigenen
  Durchgang mit Owner-Abnahme.

Tests: OLG v4-api 13/13 grün.


## Regel 8 – reservierte Zonen (Owner 15.10. 20:36)

FR6a ist während dieses Strangs gelandet: `v4/frame/frame.css:181-194/330-338` + `v4/frame/pane-heads.js:71-90`
setzen `--shell-v2-icon-size: 36px`, `--shell-v2-control-inset: var(--shell-controls-inset)` (am Cluster gemessen,
App|Code + Max + Schließen ≈ 160 px) und rücken jeden `.ctox-pane-header`, der eine Zone schneidet, per
`padding-inline-start/end … !important` selbst ein. Folgerung für meine Apps:
- Kopf-Einzüge auf Ebene `.ctox-pane-header` (customers, support, reports, research, knowledge, outbound, tickets,
  OLG, news) werden vom Kit überschrieben – kein Doppel-Einzug.
- Eigene Einzüge an INNEREN Elementen (Titelzeile) addieren sich dagegen → nicht verwenden (kundenpipeline/customers
  wieder entfernt, news zurückgedreht).
- Geblieben sind nur Variablen statt fester Pixel: tickets `--tickets-v2-control-inset` → `var(--shell-v2-control-inset, …)`,
  OLG `padding-right: 70px` → `var(--shell-v2-control-inset, 70px)` plus die je Breite richtige Spalte (Leadliste
  761–1120 px, Kampagnen ≤ 760 px) als v2-Rückfall.
- Sichtprüfung nach FR6a (after/*/1280-01-open.png, 420-01-open.png): bei allen zehn Apps liegt kein Knopf, kein
  Suchfeld und kein Reiter unter Icon oder Knopfcluster.
- Befund für FR6a (nicht mein Arbeitsort): das Kit rückt den GANZEN Kopf ein, auch die zweite Zeile (Suchfeld) unterhalb
  der 40-px-Zone – bei 420 px wird das Suchfeld dadurch beidseitig gestaucht (kundenpipeline „Entscheidungen suc…“,
  support „Suche nach Kunde, Ticket od…“). Apps mit eigener v2-Linksreserve an inneren Elementen (support `index.css:101`)
  rücken zusätzlich ein.

## Ergebnis

- Wächter (eigene Apps, `textdiaet-rule.mjs --app …`): vorher 223 Treffer (help-text 204, infobox 19), nachher 197
  (help-text 192, infobox 5); `--check` grün. Die Rest-Treffer sind Agenten-Prompts, Beispiel-Briefings, Seed-Daten,
  Dialog-/Fehlermeldungen in OLG und zwei Falschmeldungen (news `notify(…, 'notice')`, knowledge Spalten-Tooltip) –
  je App oben benannt. Baseline nicht verändert (Ratchet senkt der Wächter-Strang).
- Rundgang nachher (rundgang2, 1280 + 420): alle 20 Läufe OK, Schreib-/Befehlszahlen identisch zu vorher
  (outbound 11, OLG 39, knowledge 2, sonst 0), 0 Seitenfehler; einzige Geometriemeldung (customers 420 seitlicher
  Scroll in `.ctox-pane-scroll` 651 > 420) bestand schon vorher.
- Tests: alle v4-api-Tests der zehn Apps grün, dazu die app-eigenen Node-Tests (siehe je App). Einziger angepasster
  Test: reports `tests/v4-api.test.mjs:170` (Roh-ID-Prüfung → Prüfung „keine Roh-ID, Sprung live freigeschaltet“).
- Fachverhalten unverändert; einzige sichtbare Funktionsreduktion: outbound-Debugknöpfe „Provider-Status abgleichen“/
  „Scheduler-Tick“ nicht mehr in der Nutzerfläche (Handler `active-outreach.js:1143/1149` bestehen weiter).
- Schlusslauf aller zehn v4-api-Tests: 106/109 grün, 2 übersprungen, 1 rot: tickets `FR5-1 Rahmen-Dialog wirkt …`
  (`tests/v4-api.test.mjs:318-329`). Den Test hat der parallele FR5-Strang während dieses Laufs in die Datei
  eingefügt (Datei war vorher 11/11 grün, von TEXT-B nicht angefasst); er prüft das Eingabeblatt des Rahmens, keine
  Texte dieses Strangs → an FR5.

# Runde 2 (Coordinator 15.10., nach Abnahme 4202fe28d)

Bilder: `v4/tools/textdiaet/shots/TEXT-B/after-r2/<app>/{1280,420}-*.png` (rundgang2, Stub-Pin 68b83a3), 20/20 OK,
Schreib-/Befehlszahlen unverändert (outbound 11, OLG 39, knowledge 2, sonst 0).

## 1. Einzahl/Mehrzahl (je App eine kleine Hilfsfunktion; `shared/` hat keine – `shared/i18n.js` ohne Plural)

- customers `index.js` `countLabel(n, oneKey, manyKey)` + Schlüssel `contactOne`/`opportunityOne`/`linkedRecordOne`
  (labels + locales de/en): Listenzeile „1 Kontakt · 1 Verkaufschance“ (`:1440-1441`); `linkedAppSummary` (`:3842`)
  hatte in beiden Zweigen dasselbe Wort → „1 Verknüpfung“.
- outbound `index.js:2852` Kampagnenzeile „1 Lead“ (`leadShortOne`).
- outbound-lead-generation `index.js` `anzahl(n, eins, viele)`: Kopf „6 Leads“/„1 Lead“ (`:2674`), Dialogtitel
  „Nachrecherche: 1 ausgewählter Lead“ (`:3723/3730`), Sellify-Vermerke (`:4842`), Sellify-Import „1 Kampagne gefunden“,
  „1 Kontakt aus 1 Firma“ (`:8161/8313/8329/8336`), Lückenschluss-Dialoge (`:9017-9046`, auch „1 Feld“). Bereits korrekte
  Stellen (`:3818/3822/3927/4058/7200/7204/7328`) unverändert.
- support `index.js:1610` `messageCountLabel` „1 Nachricht“/„1 message“.
- research `index.js` `countWord(n, eins, viele)`: Aufgabenliste „1 verifizierte Quelle“/„1 Quelle“ (`:2062/2070`),
  Graph-Kopf „1 Element · 1 Verknüpfung · 1 Themenfeld“ (`:2285`), Graphknoten „1 Quelle“, „1 Messpunkt“, „1 Kandidat“
  (`:2980/3008/3084`), Tabellenzeilen „1 Zeile“ – dabei englischer Rohwert „rows“ → „Zeilen“ (`:3735`, locales).
- tickets `index.js:1148` (Runde 1) „1 Notiz“; kundenpipeline, news, reports, knowledge: keine Zahl+Wort-Stellen mehr
  (Fußzeilen weg, knowledge „Skillbooks (1)“ ist Zahl in Klammern).

## 2. Listen-Fußzeilen, die nur den Reiterzähler wiederholen – entfernt

customers „6 Einträge · Alle Kunden“ (`index.js:1532`), outbound „7 Einträge · Alle · Alle Status“ (`:2825`), tickets
„6 Einträge · Alle“ (`:889`), support „5 Einträge · Offen“ (`:586`), reports „6 Meldungen“ (`:737/745`), knowledge
„4 Einträge · Alle“ (`:1374`); kundenpipeline und news schon in Runde 1. Das `data-pg-footer`-Element bleibt (Pane-
Grammatik-Vertrag, von `customers.test.mjs:483`, reports `test.mjs` und knowledge `test.mjs:632` gefordert); je App
blendet `.ctox-pane-footer:has(> [data-pg-footer]:empty)` den leeren Fuß aus. reports' Detail-Fuß (Übergabestatus)
erscheint nur noch, wenn er Inhalt hat. outbound-lead-generation: kein Zählerfuß (Fuß „Letzter Import: …“ bleibt).

## 3. „mller.de“ – liegt an den Labor-Fixtures, nicht an der App

`v4/tools/geometry/fixtures.mjs:175` (ebenso `:161` E-Mail, `:174` URL) bildet Domains mit
`pick(COMPANIES).split(' ')[0].toLowerCase().replace(/[^a-z]/g, '')` – „Müller“ → „mller“. customers zeigt
`account.domain` unverändert (`normalizeDomain`, `index.js:4280`, entfernt nur Schema/www/Pfad). Fixture-Datei nicht
mein Arbeitsort → an den Labor-Strang (Umlaute vor dem Filter transliterieren: ü→ue usw.).

## 4. Konto-Übersicht – keine Feldverwechslung, Fixtures

Geprüft `renderOverviewTab` (`index.js:2546-2551`), `contactRow` (`:2639`), `opportunityRow` (`:4117`), Untertitel
(`:1591`): Jahresumsatz = `annual_recurring_revenue_cents`, rechts in der Kontaktzeile = `job_title` (sonst E-Mail),
Untertitel = `domain · industry`, Verkaufschance = `name` + Phase. Die Werte stammen aus dem Fixture-Generator:
`job_title` passt auf `/title/` → `pick(TITLES)` („Angebot Wartungsvertrag Halle 4“, `fixtures.mjs:168`), `industry`
fällt auf einen Titel/Text zurück („Lieferantenbewertung“), `customer_opportunities.name` passt wegen
`/customer/` im Sammlungsnamen auf `pick(COMPANIES)` („Müller Maschinenbau GmbH“, `:165-166`), ARR ist eine
Zufallszahl in Cent (1300 → „13 €“). Umbenannt: „ARR“ → „Jahresumsatz“ (labels.de, locales/de.json; Tabelle, Formular,
Übersicht).

## 5. Titelumbruch bei 420

- customers: Listentitel „Kundenkonten“ → „Kunden“ (html, labels de/en, locales; Konto-Begriff steckt im App-Namen).
- outbound: Listentitel „Kampagnen & Sequenzen“ → „Kampagnen“ (Sequenzen sind der zweite Reiter im Detail).
- Alle zehn Apps: `@container business-app-window (max-width: 768px)` → `.ctox-pane-title` einzeilig, Ellipse,
  `hyphens: manual`, `.ctox-pane-titles { min-width: 0 }` (nur der Kit-Haltepunkt 768 – reports `test.mjs:218-222`
  erlaubt laut Vertrag §6 nur 1024/768; ein erster Versuch mit 520 px fiel dort rot und wurde umgestellt).
  Bild after-r2/outbound/420: „Kampagnen“, Detailtitel „Projektplan Relaunch K…“ statt Dreizeiler.

## Prüfungen Runde 2

- v4-api aller zehn Apps 107/109 grün, 2 übersprungen (die FR5-1-Tests laufen inzwischen grün); App-eigene Tests:
  customers.test, outbound.test, focus-navigation, refresh, tickets.test, tickets-module-smoke, reports test.mjs +
  reports.test, research test.mjs + research.test + research-graph-data, knowledge test.mjs + knowledge.test,
  support.test, news.test, kundenpipeline.test grün.
- `textdiaet-rule.mjs --check` grün (196 Treffer, vorher 197); `guard-v4.mjs` Exit 0; Wächter-Selbsttests 77/77.
- Beobachtung (nicht TEXT-B): ab ~20:39 rendert customers im Labor teilweise dunkel (Kopf dunkle Schrift auf dunkel,
  Knopfcluster schwarz, Eckmarken „+“) – fällt zeitlich mit Änderungen an `v4/frame/` (frame.css 20:34/20:39,
  index.js 20:38) zusammen; customers-CSS dieser Runde ändert keine Farben. An FR6a/Theme.

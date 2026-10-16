# Textdiät Strang TEXT-A (Shell v4)

Stand: 15.10.2026, laufend fortgeschrieben. Rahmen: `~/.local/state/shell-v4/briefs/text-diet-common.md` (Regeln 1–8).
Apps: mail, calendar, documents, spreadsheets, notes, explorer, file-viewer, desktop, conversations, threads.
Screenshots: vorher `v4/tools/textdiaet/shots/TEXT-A-before/<app>/`, nachher `v4/tools/textdiaet/shots/TEXT-A-after/<app>/`
(rundgang2.mjs `--max-steps 1`, Stub-Pin 68b83a3, 1280 und 420).

## Wächter-Regel (für alle Stränge)

`v4/tools/guards/textdiaet-rule.mjs` (+ `textdiaet-rule.test.mjs`, 7 Tests grün, + `textdiaet-baseline.json`).

- `infobox`: Klassen `*-notice`, `*-alert`, `*-warning`, `*-hint`, `*-info-box`/`*-infobox`, `*-callout`, `*-banner`
  (nackt: `notice`, `infobox`, `info-box`) im App-Markup: HTML-`class`, `class="…"` in JS-Templates, JS-Strings, die reine
  Klassenlisten sind. Nicht: `is-*`/`has-*`-Modifikatoren (`ctox-badge is-warning`), `data-*`/`aria-*`-Namen,
  `classList.contains/closest/querySelector`-Abfragen, CSS-Selektoren.
- `help-text`: sichtbarer Text > 80 Zeichen (≥ 8 Wörter, Prosa) in HTML-Textknoten, JS-Strings/Templates (Text zwischen
  Tags) und `locales/*.json`. Nicht: `api.status.report/clear`, `console.*`, `new Error(…)`/`throw`, Tooltip-Ziele
  (`title`, `aria-*`), Locale-Schlüssel mit status/error/tooltip/prompt/instruction bzw. Endung Error/Failed/Required/…,
  die Katalogbeschreibung `description`. Freigabe: `// v4-allow: text` (gleiche/vorige Zeile).
- Ratchet: `node v4/tools/guards/textdiaet-rule.mjs --check` → Exit 1, wenn eine Datei/Regel über
  `textdiaet-baseline.json` liegt (neue Dateien 0). Nach einer Diät: `--write-baseline --app <id>[,…]` senkt nur diese Apps.
  `--app mail` filtert, `--summary` nur Zahlen, `--json` maschinenlesbar.
- Erst-Baseline (alle Apps, vor TEXT-A): 429 Treffer (help-text 353, infobox 76) in 80 Dateien.

## Ergebnis je App

Zeilennummern = Stand vor der Änderung (Vorher-Datei). Tests: `node --import ./v4/tools/node/ctox-shared.mjs --test modules/<app>/tests/v4-api.test.mjs`.
Regel 8 (Ecken frei): geprüft in den Nachher-Screenshots im FR6a-Rahmen (schwebendes Icon links, Cluster App|Code|□|× rechts,
1280 und 420). Pane-Köpfe mit `.ctox-pane-header` rückt der Rahmen selbst ein (pane-heads.js); App-eigene Einrückungen
rechnen jetzt mit `--shell-v2-control-inset` (= `--shell-controls-inset`) bzw. `--shell-icon-inset`, keine festen Pixel.

### mail — v4-api 8/8 grün, mail.test 26/26 grün

| Datei:Zeile | entfernt / geändert | Regel | wohin |
|---|---|---|---|
| index.html:64 | Kicker „Postfach“ über „Posteingang“ (doppelt zum Navigationstitel) | 3 | `hidden` |
| index.html:194 | `.mail-section-hint` „Änderungen werden sofort angewendet“ | 1 | Tooltip an „Server & Tracking“ |
| index.html:207 | `.mail-section-hint` „DKIM-Schlüssel wird bei Bedarf erzeugt“ | 1 | Tooltip an „Versanddomain & DNS“ |
| index.html:228 / index.js:2613 | Fußzeilensatz „Das Passwort wird nach der Einrichtung aus dem Command entfernt.“ (Systembegriff „Command“) | 1, 4 | Tooltip am Passwortfeld, Text „… aus dem Auftrag entfernt.“ (de/en, locales) |
| index.html:241 | `.mail-route-note` „Original bleibt unverändert; Batch, Ziel und Quelldatensätze werden protokolliert.“ | 2 | Tooltip an „Routing starten“ |
| index.js:788 | Erklärzeilen unter Ordnern („6 Ungelesen“, „Versendete Nachrichten“, „Posteingang und Gesendet“, „Wartet auf Freigabe“, „Prüfung erforderlich“, „App-Übergaben“) | 2, 3 | entfällt; Ordner einzeilig (Name + Zähler) wie Outlook/Apple Mail; E-Mail-Gruppen behalten ihre Zeile |
| index.js:800 | Navigationsfuß „6 Postfach · 6 E-Mail-Gruppen“ | 5 | entfällt |
| index.js:839 | Listenfuß „… · Alle sichtbaren Postfächer“ | 3 | nur noch „1–6 / 6 Nachrichten“ (+ Postfach, wenn gefiltert) |
| index.js:1966 | Import-Hilfesatz „CSV, Excel oder Text mit E-Mail-Adressen übernehmen. Die Empfänger werden …“ | 2 | entfällt (`helperText` nicht mehr übergeben) |
| index.css:883, 897 | feste `--mail-head-inset-right: 70px` | 8 | `var(--shell-v2-control-inset, 70px)` |
| mail-group-model.mjs:17–57 | – (Wächter-Treffer) | – | Auftragsvorgaben = Daten im Textfeld, mit `v4-allow: text` markiert |

Offen: Kopf-Stift „Neue Mail“ doppelt zum großen Knopf „Neue Mail“ (Regel 6) – nicht geändert, Tests/Mobilpfad nutzen beide.
Listenzeilen zeigen `communication--001` als Absender: Fixture-Daten des Labors, kein App-Text.

### calendar — v4-api 10/10 grün

| Datei:Zeile | entfernt / geändert | Regel | wohin |
|---|---|---|---|
| calendar-view-adapter.js:91 | Bibliotheks-Werkzeugleiste (4 leere Knöpfe Monat/Woche/Tag/Liste, doppelt zu den Reitern) | 3, 6 | `end: ''`, index.css blendet `.ec-toolbar` aus |
| index.html:109–111 | Zähler an Ansichtsreitern „Monat(3) Woche(0) Tag(0)“ | 5 | entfällt (Google/Apple Kalender: keine) |
| index.js:971 | Fuß „6 Einträge · Kalender“ | 5 | entfällt |
| index.js:1020–1030 | Zeilenmeta „Sichtbar/Ausgeblendet · … · 1 Termine“ (doppelt zur Checkbox, falscher Plural) | 3 | „… · 1 Termin“ (Singular de/en neu) |
| index.js:1161–1165 | `ctox-callout` „Ausgewählte Buchungsseite“ + Titel (doppelt zum Pane-Titel) | 1, 3 | nur Zeile „30 Min · Nicht veröffentlicht“ |
| index.js:2069 | „Slug ist erforderlich und darf nur Buchstaben, Zahlen und Bindestriche enthalten.“ | 2 | „Slug fehlt (nur Buchstaben, Zahlen, Bindestriche).“ |
| index.js:1490 | Anwesenheits-Badge `calendar-presence-hint` | – | bleibt (Badge „X bearbeitet gerade“, keine Box), `v4-allow: text` |

Regel 8: Kopfaktionen (+ ‹ Kalender ›) rechnen bereits mit `--shell-v2-control-inset`, im Screenshot links vom Cluster.

### documents — v4-api 12/12 grün

| Datei:Zeile | entfernt / geändert | Regel | wohin |
|---|---|---|---|
| index.js:1265 | Fuß „6 Dateien · Alle“ | 5 | nur bei aktivem Filter „n gefiltert“ |
| index.js:1754 | `<small>` „CTOX nutzt den Skill als Wissens-Hub und liest die verknüpften Originalquellen für Nachweise.“ | 2 | Tooltip an der Knowledge-Auswahl |
| index.js:120, locales/de.json:143 | Runbook-Beschreibung 81 Zeichen | 2 | gekürzt („Quellen- und Datenquellenrecherche mit …“) |
| locales/de.json:43, en.json:43 | Zonentext „Zu diesem Dokument wurde keine gespeicherte Version gefunden. Bitte erneut importieren …“ | 2 | „Keine gespeicherte Dokumentversion gefunden.“ (bleibt in der Zone) |
| index.js:3164 | Agenten-Prompt | – | `v4-allow: text` |

### spreadsheets — v4-api 9/9 grün

| Datei:Zeile | entfernt / geändert | Regel | wohin |
|---|---|---|---|
| index.js:1074 | Fuß „6 Dateien · Alle“ | 5 | nur bei aktivem Filter „n gefiltert“ |
| locales/de.json:27, en.json:27 | Zonentext „Zu dieser Tabelle wurde keine gespeicherte Version gefunden. Bitte erneut …“ | 2 | „Keine gespeicherte Tabellenversion gefunden.“ |
| index.js:46–62 | Runbook-Aufträge an den Agenten | – | `v4-allow: text` |

Labor: Editor lädt im Stub nicht (`ctox:lib/ctox-office@1` CORS, bekannter RUND2-Befund) – Zonenfehler im Screenshot ist kein App-Text.

### notes — v4-api 9/9 grün (Test angepasst)

| Datei:Zeile | entfernt / geändert | Regel | wohin |
|---|---|---|---|
| index.html:59 | Kicker „NOTESNOOK“ (Fremdproduktname) über „Notizen“ | 3, 4 | `hidden` |
| index.html:12–13 | Sperrbild „Notesnook Secured“ / „App is encrypted. Enter PIN to unlock.“ | 2, 4 | „Notizen gesperrt“, Satz entfällt |
| index.html:37 | `.nn-pin-hint` „Standard PIN: 1234“ | 1 | Tooltip an der PIN-Anzeige |
| index.html:209 | „Diese Notiz ist mit einem Zero-Knowledge-Passwort gesichert.“ | 2 | Tooltip an „Notiz ist verschlüsselt“ |
| index.html:214 | `.nn-lock-hint` „Standard-Passwort: 1234“ | 1 | Tooltip am Passwortfeld (Platzhalter „Passwort“) |
| index.js:1191 | `ctox-callout nn-restore-banner` „⚠️ Diese Notiz ist im Papierkorb.“ + Knopf | 1 | nur Knopf „Wiederherstellen“ (`.nn-restore-actions`), Satz als Tooltip |
| index.js:1202 | `ctox-callout nn-draft-banner` „Entwurf“ + Speichern/Verwerfen | 1 | nur die zwei Knöpfe (`.nn-draft-actions`), „Entwurf“ als Tooltip |
| index.js:910 | Fuß „6 Einträge · Notizen“ | 5 | entfällt |
| index.css (Ende) | – | 8 | Kopf links `--shell-icon-inset`, Editor-Kopf rechts `--shell-controls-inset`/`--shell-v2-control-inset` |
| index.js:245, 271 | Beispielnotizen | – | `v4-allow: text` (Inhalt) |

Test `tests/v4-api.test.mjs` (snapshot/restored): prüfte `.nn-draft-banner`; jetzt: Speichern-Knopf in `.nn-draft-actions`,
keine Callout-Box, Tooltip „Entwurf“ am Knopf (die Information kommt an, nur nicht als Box).
Offen (Regel 6): Listenkopf trägt 6 Knöpfe (+, Seitenleiste, Tags, Import, Export, Sperre) – Import/Export/Sperre gehören ins „…“-Menü; nicht umgebaut.

### explorer — v4-api 10/10 grün

| Datei:Zeile | entfernt / geändert | Regel | wohin |
|---|---|---|---|
| index.js:146–157 | englische Rohbeschriftungen „On this Desktop“, „Files“, „Business OS“, „Documents“, „Spreadsheets“, „Knowledge“, „Matching Objects“ | 4 | de/en über T: „Dieser Desktop“, „Dateien“, „Apps“, „Dokumente“, „Tabellen“, „Wissen“, „Matching-Objekte“ |
| index.js:359–360 | Titel/Pfad „Files“ | 4 | „Dateien“ |
| index.js:557 | Inspektorzeile „ID fs_ctox“ (technische ID) | 4 | entfällt |

Regel 8: ⋯ + ⇧ und Suche rechnen mit `--shell-v2-control-inset`, im Screenshot links vom Cluster.

### file-viewer — v4-api 9/9 grün

| Datei:Zeile | entfernt / geändert | Regel | wohin |
|---|---|---|---|
| index.js:18, 26 | Zonentext „Keine Vorschau verfügbar – Datei herunterladen oder in einer passenden App öffnen.“ | 2 | „Keine Vorschau verfügbar – bitte herunterladen.“ |

### desktop — v4-api 7/7 grün

| Datei:Zeile | entfernt / geändert | Regel | wohin |
|---|---|---|---|
| index.js:49, locales/de.json:41 | Bestätigung „Aktuelle Desktop-Icons durch die vor dem letzten Zurücksetzen gespeicherte Anordnung ersetzen?“ | 2 | „Anordnung von vor dem letzten Zurücksetzen wiederherstellen?“ |
| index.js:51, locales/de.json:43 | „Alle Desktop-Verknüpfungen und eigenen Anordnungen entfernen und Standard-Icons wiederherstellen?“ | 2 | „Desktop-Icons auf Standard zurücksetzen? Eigene Verknüpfungen gehen verloren.“ (en analog) |
| appCommands.js:32 | Agenten-Anweisung | – | `v4-allow: text` |

### conversations — v4-api 12/12 grün (Test angepasst)

| Datei:Zeile | entfernt / geändert | Regel | wohin |
|---|---|---|---|
| index.html:18, locales | Kicker „Audit aus CTOX-Sicht“ | 2, 4 | `hidden` (Schlüssel jetzt „Posteingang“) |
| index.html:19, index.js:53/299/401, locales | Titel „Conversations“ (englisch, Taskleiste sagt „Konversationen“) | 4 | „Konversationen“ (en bleibt „Conversations“) |
| index.html:94, 125 | Kicker „Timeline“, „Kontext“ über „Konversation“/„Kontakt“ | 3 | entfällt |
| index.js:59/72–77, locales | „Alle Channels“, „Kontakt/Text/Account“, „Aktive Channels“, „CTOX-Account(s)“, „E-Mail-Folder“, „Trust“, „Alle Accounts“ | 4 | „Alle Kanäle“, „Suchen…“, „Aktive Kanäle“, „Konto/Konten“, „E-Mail-Ordner“, „Vertrauen“, „Alle Konten“ |
| index.js:31 + locales | Rohwert „physical_letter“ an Reitern/Karten | 4 | „Brief“/„Letter“ |
| index.js:2080 | Konto-Rohwert „standard/email/internal“ als eigene Zeile | 4 | Tooltip am Kontonamen |
| index.js:2452 | Status-Rohwert „done“ | 4 | „erledigt“ |
| index.js:898 | Fuß „1 Konversationen · Alle Channels“ | 5 | entfällt |

Test: Fenstertitel `'Conversations'` → `'Konversationen'` (de-Locale; gleiche Prüfung über api.shell).
Offen: Timeline-Chips „TASK 2026-01045“ / „WORK ticket-s…-003“ zeigen Rohkennungen (Regel 4) – Herkunft im Chip-Renderer, nicht umgebaut.

### threads — v4-api 14/14 grün, threads.test grün (Test angepasst)

| Datei:Zeile | entfernt / geändert | Regel | wohin |
|---|---|---|---|
| index.html:4 | `ctox-standalone-note` „Dieses Modul-Fragment ist nur innerhalb der Workjet Business OS nutzbar …“ | 2 | entfällt (Kommentar darüber erklärt es) |
| index.js:1161 | Benachrichtigungen als `ctox-callout`-Kästen in den Details | 1 | normale Listenzeile `threads-notification-item` |
| index.html:66 | Reiter „AI“ | 4 | „KI“ (Tooltip „Von der KI bearbeitet“) |
| index.html:116 | Platzhalter „Antwort oder Notiz in diesen Thread schreiben“ | 3 | „Antwort oder Notiz…“ |

Test `tests/threads.test.mjs`: Öffnungs-Tag-Muster ohne `ctox-callout` (prüft weiter die drei Kontext-Attribute).
Befund (nicht behoben): linker Kopf bei 1280 – Filterknopf am Pane-Rand angeschnitten (Kit `.ctox-pane-tabs`/Kopfbreite, RUND2).

## Abschluss TEXT-A

- Wächter für die zehn Apps: `node v4/tools/guards/textdiaet-rule.mjs --app mail,calendar,documents,spreadsheets,notes,explorer,file-viewer,desktop,conversations,threads`
  → 0 Treffer (vorher 57 mit der ersten Regelfassung, 42 nach Präzisierung); Baseline dieser Apps auf 0 gesenkt.
  `--check` über alle Apps: Exit 0.
- Alle zehn v4-api-Tests grün (mail 8, calendar 10, documents 12, spreadsheets 9, notes 9, explorer 10, file-viewer 9, desktop 7,
  conversations 12, threads 14), dazu mail.test 26 und threads.test grün; `textdiaet-rule.test.mjs` 8/8, `guard-v4.test.mjs` grün.
- Angepasste Tests (nicht geschwächt): notes v4-api (Entwurfsaktionen + Tooltip statt Box), conversations v4-api (de-Titel),
  threads.test (Kontext-Attribute an der Benachrichtigungszeile ohne Callout-Klasse).
- Nicht angefasst: v4/frame, v4/shell, shared/*, andere Apps.

## Runde 2 (Auftrag nach Abnahme f55796b25)

Nachher-Bilder: `v4/tools/textdiaet/shots/TEXT-A-round2/<app>/` (1280 und 420, rundgang2 `--max-steps 1`, Stub 68b83a3), alle selbst angesehen.
Hinweis: Während des Laufs hat ein paralleler Rahmen-Strang Knopf-Cluster und Thema umgebaut (dunkles Thema, neue □/×-Knöpfe in
den zuletzt erzeugten conversations-/notes-Bildern) – das ist Rahmenstand, kein App-Befund.

| # | App | Datei | Änderung | Test |
|---|---|---|---|---|
| 1 | mail | index.js `mailPersonLabel` (neu), `renderRecordRow` | Listen-Absender/-Empfänger: Name oder Adresse (auch `email:addr@…`), sonst „Unbekannt“ (de/en, locales `unknownPerson`); Rückfall auf `account_key`/Thread-Schlüssel entfällt | v4-api neu: „Liste zeigt Name/Adresse … nie eine Roh-ID“ (Thread ohne Absender → „Unbekannt“, „Kunde AG“ bleibt; ohne Fix rot) |
| 1 | conversations | index.js `buildTaskBadge`/`buildTaskChip`, index.css | Chips „TASK 2026-01045“ / „WORK ticket-s…“ → „Aufgabe <Titel>“ / „Arbeitsauftrag <Titel>“; Titel aus Projektion (`task_title`, `work_title`/`ticket_title`), sonst Betreff/Anfang der Nachricht; Kennungen nur noch unter „Details“; Navigation unverändert über die Kennung. Gleiches Muster: Antwortzeile im Outbound-Kontext „Antwort: m-a3…“ → „Antwort: <Betreff/Absender>“; `shortenId` entfällt | v4-api „Navigation … Task-Chip“: Chip trägt „Aufgabe“, kein `m-a…`/TASK/WORK, Antwortzeile ohne Kennung (ohne Fix rot) |
| 1 | conversations | index.css | Timeline-Spalte `minmax(0,1fr)`, Karte/Chip schrumpfen: bei 420 kein seitlicher Scroll mehr (die längeren Chips hatten 494 > 404 px ausgelöst) | Screenshot-Test prüft `.conv-timeline` ohne Überlauf |
| 1 | conversations | index.css:1040/1066/1071 | feste 70 px Rechtsreserve → `var(--shell-v2-control-inset, 70px)`; vorher reservierten Kopf (70) und Rahmen (Zeile unter der Knopf-Zone, 166) doppelt, Titel lief bei 420 unter Import/Export. Schmal: Titel kürzt mit Tooltip statt zu überlappen | Rundgang 420: Titel, Import/Export links vom Cluster |
| 2 | mail | index.html:12, tests | Stift „Neue Mail“ im Navigationskopf entfernt; nur der große Knopf bleibt (Outlook/Apple Mail) | v4-api klickt `[data-mail-compose-primary]`; mail-browser-qa prüft „genau ein Neue-Mail-Knopf, kein Stift“ |
| 3 | notes | index.html:62–71, index.js, index.css | Listenkopf: + / Notizbuch / Tag sichtbar, Import/Export/Sperren im „…“-Menü (App-eigenes Dropdown wie das Notiz-„⋮“, schließt bei Auswahl/Escape/Klick außerhalb, Listener beim Unmount abgemeldet) | v4-api neu: „Listenkopf: Hauptaktionen sichtbar, Import/Export/Sperren im …-Menü“ |
| 3 | notes | index.css (Ende), index.html | 420: Editorkopf zeigte Notizbuch/Tags beschriftet, Stern lief über „Vertrieb“, „Tags“ lag unter dem Cluster (Regel 8). Schmal nur Symbol (Name für Screenreader + Tooltip) | Rundgang 420: alle Kopfknöpfe links vom Cluster |
| 4 | threads | index.css:117ff | Ursache: Kopf-Mindestbreite (Einrückung + Titel + Aktionen) > 320-px-Spalte, Kopf 327 px breit, Spaltenrand schnitt Filterknopf (und „Erledigt (1)“) ab. Spalte/Kopf/Titel `min-width: 0`, Titel kürzt; Einrückung über `--shell-icon-inset` | v4-api neu: „linker Kopf: Filterknopf und Suche liegen ganz in der Spalte“ (Kopf ≤ Spalte; ohne Fix 340 > 332, rot) |

Stand nach Runde 2: Wächter für die zehn Apps 0 Treffer, `--check` Exit 0. v4-api: mail 9, calendar 10, documents 12,
spreadsheets 9, notes 11, explorer 10, file-viewer 9, desktop 7, conversations 12, threads 15 – alle grün; dazu mail.test,
threads.test, textdiaet-rule.test, guard-v4.test (104) grün.
Offen: notes-Listentitel kürzt bei 420 auf „N…“ (vier Kopfknöpfe neben der Knopf-Zone); Mail-Fixture des Labors hat keine
Absender, daher zeigt der Rundgang durchgehend „Unbekannt“ (gewollt statt Roh-ID).

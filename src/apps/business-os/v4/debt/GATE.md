# Strang GATE – Shell-V2-Vertrag und Ratchet wieder grün (15.10.2026)

Klon `~/Documents/ctox-shell-v4`, Kopf `002b8ae5c`, Baseline der Ratchet = Stand `4f1503d8f`.
Kein git außer Lesen; kein Commit. v4/frame/* nicht angefasst (Strang FR6d arbeitet dort).

## Ergebnis

| Prüfung | vorher | nachher |
|---|---|---|
| `node scripts/assert-shell-v2-contract.mjs` | 37/38 (esign `media-query`) | **38/38 OK** |
| `node v4/debt/ratchet.mjs` | rot, guard.total 1918, 29 Zähler gestiegen | guard.total **1844**, **1 Zähler gestiegen**: `guard.app.v4.english-raw-term 0 → 3`, alle 3 in `v4/frame/topbar.js` (FR6d) |
| `node --test v4/tools/guards/guard-v4.test.mjs` | 71 | **73/73 grün** (2 neue Tests) |
| `node --test v4/debt/ratchet.test.mjs` | – | 11/11 grün |
| App-Tests (Stub-Pin a8765b9) | – | consent 24, customers 16, esign 20, intake 25, interviews 22, invoices 93, nachweise 22, placements 21, submissions 21, mail 41 (+1 übersprungen), explorer 10, outbound 12, support 27, tickets 12, buchhaltung 21: **alle 0 fail** |
| `node v4/os-primitives/check-caps.mjs` | – | OK |

`ratchet --update` habe ich **nicht** ausgeführt: es ist nur erlaubt, wenn kein Zähler mehr gestiegen ist, und
`v4.english-raw-term` steht bis zum Fix in `v4/frame/topbar.js` auf 3. Sobald FR6d die drei Treffer behebt:
`node v4/debt/ratchet.mjs` (muss grün sein) und dann `node v4/debt/ratchet.mjs --update`. Gesunken sind dann
18 Zähler, darunter guard.total 1882 → 1844, english-raw-term 408 → 378, de-laeuft 28 → 21.

## 1. Shell-V2-Vertrag: esign

`modules/esign/index.css`: das Paar `@container (max-width: 560px)` (unbenannt, traf den nächsten beliebigen
Container) plus `@media (max-width: 560px)` ersetzt durch **einen** Block `@container business-app-window
(max-width: 768px)`. 560 px ging nicht: ein benannter Haltepunkt mit `grid-template` außerhalb von 1024/768 ist
selbst ein Vertragsfund (`breakpoint`). 768 ist die einspaltige Shell-Stufe, auf der die Panes ohnehin stapeln.

Beim Rundgang fiel auf, dass die Aktionsknöpfe der Empfängerzeile in der gestapelten Form in eine eigene Zeile in
die 20-px-Spalte rutschten (Spalte `auto` = 0 px). Im selben Block jetzt `.esign-recipient-actions { grid-column: 3;
grid-row: 1; }`: Pfeile und Entfernen stehen neben dem Namen, E-Mail und Rolle darunter.

Rundgang (`rundgang2.mjs --app esign --widths 1280,420`, Stub-Pin a8765b9, Ausgabe im Scratchpad):
OK 1280 (28 Schritte) und OK 420 (27 Schritte), 0 Fehler, 0 Geometrie. Weil der Rundgang keine Person hinzufügt,
habe ich mit einer Scratch-Kopie des Rundgangs zwei Unterzeichner über die echte Personenauswahl angelegt und
nachgemessen und mir die Bilder angesehen:
- 1280: Zeile 811 px, 5 Spalten `20 | 237 | 277 | 198 | 55`, alles in einer Zeile.
- 420: Zeile 364 px, `20 | 277 | 55`; Name + Aktionen in Zeile 1, E-Mail und Rolle über volle Breite darunter.

Hinweis: ein späterer Lauf derselben Rundgang-Datei brach mit „App-Fenster erscheint nicht“ ab (Launcher-Kachel da,
0 Fenster). Zu dem Zeitpunkt hatte FR6d `v4/frame/launcher.js`, `index.js` und neue Dateien `desktop.js`,
`crew-bar.js` in Arbeit. Die esign-CSS kann das Öffnen nicht verhindern. Nach FR6d den Rundgang wiederholen.

Nebenbefunde esign (vorhanden, nicht von mir, nicht behoben): Listenstatus zeigt Rohwerte `done`/`failed`
(Fixture-Daten, nicht im Statuswörterbuch), Untertitel „string“ (Fixture), der Detailbereich hat in 1280 einen
dunklen Grund und die Kopfzeilen „Anfragen“/„Signatur anfordern“ liegen dunkel auf dunkel unter der Werkzeugleiste.

## 2. Ratchet: jeder neue Treffer

Vorgehen: Baumstand `4f1503d8f` per `git archive` in den Scratchpad, beide Bäume mit dem heutigen `guard-v4.mjs
--json` gemessen und die Treffer (Datei, Regel, Zeilentext) abgeglichen. Unten alle neuen Treffer der gestiegenen
Gruppen, dazu die verschobenen Treffer in Gruppen ohne Anstieg (appsec-pentest, explorer, mail).

Klassen: **a** = sichtbarer Zustandstext außerhalb der Zone, **b** = Code-Wert, **c** = englischer Rohwert.

### Apps

| Fundstelle | Treffer | Klasse | Getan |
|---|---|---|---|
| buchhaltung/index.js:3106 | `'Zwischenablage nicht verfügbar'` | a, schon in der Zone | Der Text geht als `error.message` über `fibuNotice` → `api.status.report`. Marker `// v4-allow: zone` |
| consent/index.js:224 | `lang === 'en' ? 'Unknown' : 'Unbekannt'` | c, englisches Wörterbuch im Code | `// v4-allow: en` |
| customers/index.js:310 | `commandCompletedStatus: 'Resolved'` (en-Block) | c | `// v4-allow: en` |
| esign/index.js:29 | `viewOpen: 'Open'`, `viewDone: 'Closed'` (en-Block) | c | `// v4-allow: en` |
| esign/index.js:33 | `unknown: 'Unknown'` (en-Block) | c | `// v4-allow: en` |
| intake/index.js:649 | `'Unknown'`-Rückfall für en | c | `// v4-allow: en` |
| interviews/index.js:573 | dito | c | `// v4-allow: en` |
| invoices/index.js:63 | `open: 'Open'` (en-Block) | c | `// v4-allow: en` |
| invoices/index.js:65 | `unknownCustomer: 'Unknown'` (en-Block) | c | `// v4-allow: en` |
| nachweise/index.js:47 | `title`/`listTitle: 'Credentials'` (en-Block) | c | `// v4-allow: en` |
| nachweise/index.js:233 | `'Unknown'`-Rückfall für en | c | `// v4-allow: en` |
| placements/index.js:18 | `viewActive: 'Active'` (en-Block) | c | `// v4-allow: en` |
| placements/index.js:526 | `'Unknown'`-Rückfall für en | c | `// v4-allow: en` |
| submissions/index.js:20 | `viewOpen: 'Open'`, `viewDone: 'Closed'` (en-Block) | c | `// v4-allow: en` |
| submissions/index.js:566 | `'Unknown'`-Rückfall für en | c | `// v4-allow: en` |
| mail/index.js:127 | `unknownPerson: 'Unknown'` (en-Block) | c | `// v4-allow: en` |
| explorer/index.js:142 | `srcKnowledge: 'Knowledge'` (en-Block) | c | `// v4-allow: en` |
| explorer/index.js:171 | `kind: 'Knowledge'` | c, **sichtbar** | Nicht geändert: verschobener Alttreffer, gleiche Zeile wie vorher. Offen: `kind` (Document/Spreadsheet/Knowledge/Object/Company) steht roh in Liste und Vorschau, auch auf Deutsch. Gehört übersetzt (T.kind*), eigener Auftrag |
| appsec-pentest/index.js:2802–2803 | `Queued`, `Resolved`, `Ready`, `Open` | c | Nicht geändert: die Gruppe ist nicht gestiegen (27 → 27), die Werte laufen über `tr()`. Bleibt im Altbestand |
| outbound/index.js:2671 | `running: 'Läuft'` (Kampagnenstatus de) | Fachbegriff, kein Systemzustand | `// v4-allow: label` (Präzedenz Strang LBL: Fachstatus im Originalwortlaut, siehe outbound/tests/v4-api.test.mjs `FACH_LABELS`) |
| outbound/index.js:2672 | en-Kampagnenstatus `Active`, `Paused`, `Draft`, `Running`, `Archived`, `Rejected` | c | `// v4-allow: en` |
| outbound/locales/de.json:294 | „Dieses Engagement läuft mit Sequenz-Version …“ | Fachbegriff | Marker-Schlüssel `"//sequenceMismatchDescription": "v4-allow: label"`, wie der JS-Rückfall in active-outreach.js:443 (schon `label`) |
| outbound/locales/de.json:340 | `senderUnavailable: "Nicht verfügbar"` (Absender gesperrt/blockiert) | Fachbegriff | Marker-Schlüssel `"//senderUnavailable": "v4-allow: label"`, wie in en.json:338 und JS active-outreach.js:782 |
| support/index.js:1612 | en-Ticketstatus `New`, `Open`, `Waiting`, `Pending`, `On hold`, `Resolved`, `Solved`, `Closed`, `Completed`, `Done`, `Failed`, `Cancelled`, `Snoozed`; dazu Vokabel `Waiting` | c + Fachbegriff | `// v4-allow: en; v4-allow: label` |
| tickets/index.js:1567 | `Freigabe ausstehend` ×3 (Freigabestatus) | Fachbegriff | `// v4-allow: label` (gleicher Begriff ist in outbound schon `label`) |
| tickets/index.js:1568 | `writeback_pending: 'Übertragung ausstehend'` | a | Text entfernt: Anzeige jetzt `'Geprüft'`. `writeback_pending` ist der Fallschritt nach bestandener Prüfung; der Handler bildet ihn selbst auf `Verified` ab (handlers/_tickets.js:32). „Übertragung ausstehend“ war ein Zustandstext im Inhalt |

### v4 (außerhalb von v4/frame)

| Fundstelle | Treffer | Klasse | Getan |
|---|---|---|---|
| v4/source/agent-panel.js:203 | „Der Coding Agent ist … noch nicht verfügbar“ | a, schon in der Zone | `report` ist `sink.report` der Zustandszone (Zeile 71); der Wächter erkennt nur `status.report`/`zone.report`. Marker `// v4-allow: zone` |
| v4/os-primitives/core-contract.mjs:241–243 | „… (bis Cutover unavailable)“ | b | Beschreibungen im Vertragskatalog, `unavailable` ist der Fehlercode des Kerns, nie Anzeigetext. Marker `// v4-allow: code` (neue Kategorie, siehe unten) |
| v4/shell/kernel-adapter.js:50 | „App-API-Datenteil nicht verfügbar“ | – | verschobener Alttreffer (vorher Zeile 62), unverändert im Baseline-Bestand |
| v4/lab/app-host/results.json:129–273 | 6× `waiting for locator …` (Playwright-Aufruflog), 4× `läuft` (Messzusammenfassungen) | b, Messausgabe | Datei nicht angefasst (Messergebnis, wird nicht umgeschrieben). Laborausgaben zählen jetzt wie Testdateien getrennt (siehe unten) |

### Änderungen am Wächter (`v4/tools/guards/guard-v4.mjs`, mit Tests)

1. **Kategorie `code`**: `// v4-allow: code` in derselben oder der vorigen Zeile hebt Zustandsvokabular auf, gebaut
   wie `label`/`zone`. Test: ohne Marker Treffer, Marker gleiche/vorige Zeile kein Treffer, `codes` und zwei Zeilen
   Abstand weiter Treffer.
2. **Labor-Messausgaben** `^v4/(…/)*lab/(…/)*results*.json$` gelten als Testdateien: Sie werden weiter gezählt,
   aber getrennt (`testHitsSeparate`, mit `--include-tests` wieder drin), nicht ausgenommen. Grund: es sind von
   Messskripten geschriebene Protokolle (Testnamen, Playwright-Logs, zitierte Zonentexte), nie ausgeliefert, nie
   gerendert, und sie lassen sich nicht ehrlich „korrigieren“. Das Muster ist eng: nur JSON, nur unter `v4/`, nur
   unterhalb eines `lab/`-Ordners, nur `results*`. Test mit Positiv- und Negativfällen. Folge: v4.de-laeuft 7 → 0,
   v4.de-vorbereitet 1 → 0. **Das ist die einzige Ausnahme für ganze Dateien. Bitte gegenlesen und ggf. verwerfen**;
   dann steht v4.en-pending-waiting bis zu einem neuen grünen Labor-Messlauf auf 6.

## Für FR6d (v4/frame, nicht angefasst)

| Fundstelle | Treffer | Vorschlag |
|---|---|---|
| v4/frame/topbar.js:62 | `workspace: 'Workspace'` im **de**-Block | Klasse c, sichtbar: deutsch übersetzen (z. B. „Arbeitsbereich“, passend zu `localWorkspace`) oder Glossar |
| v4/frame/topbar.js:68 | `workspace: 'Workspace'` im en-Block | c, englisches Wörterbuch: `// v4-allow: en` |
| v4/frame/topbar.js:73 | `ROLE = { admin: 'Admin', owner: 'Owner', founder: 'Founder', … }` | c, sichtbar im deutschen Menü: „Administrator“, „Inhaber“, „Gründer“ (Glossar), englische Fassung getrennt |
| v4/frame/lab/shots/results-mock.json, results-stub.json | `nicht verfügbar` ×4, `wartet auf` ×2, `läuft` ×2, `unconfirmed` ×1 | durch die Labor-Regel oben schon getrennt gezählt; nichts zu tun |

Nur diese drei topbar-Treffer halten die Ratchet noch rot.

## Nebenwirkung der Testläufe

Die App-Tests schreiben ihre Screenshots in den Baum. Geändert sind dadurch (nicht von Hand, kein git):
`modules/esign/tests/shots/esign-420.png`, `modules/interviews/tests/shots/interviews-{1280,420}.png`,
`modules/support/tests/shots/support-{1280,420,420-queue}.png`, `modules/tickets/tests/shots/tickets-{1280,420,420-detail}.png`.

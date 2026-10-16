# FIX7 – drei App-Fehler aus der welsch-Stichprobe und dem SEED-Lauf (Shell v4)

Klon `~/Documents/ctox-shell-v4`, Basis `src/apps/business-os/`, Stand 15.10.2026. Kein git, keine Handler angefasst.

## 1. E-Signatur „Neue Anfrage“: Auswahl statt ID-Felder

- `modules/esign/index.html`: die Felder „Dokument-ID“ und „Unterzeichner-IDs (kommagetrennt)“ sind weg.
  - **Dokument**: Suchfeld mit Trefferliste (höchstens 8, leere Suche zeigt die neuesten). Die Auswahl erscheint als Chip mit Titel und „Ändern“.
  - **Unterzeichner**: Personenauswahl mit Suche über Name und E-Mail. Jede Zeile hat die Nummer der Signierreihenfolge, Name, E-Mail und Rolle („Unterschreibt“, „Genehmigt“, „Erhält Kopie“) sowie die Knöpfe Nach oben, Nach unten und Entfernen.
  - Enter wählt den ersten Treffer und schickt das Formular nicht ab.
- Quellen, nur lesend:
  - Dokumente kommen aus `documents` (title/filename) und `desktop_files` (name, ohne Ordner).
  - Personen kommen aus `customer_contacts` (Vor- und Nachname, E-Mail) und `applications.candidate` (name, email, id).
  - Ist eine Quelle nicht lesbar, bleibt die Liste leer und die App bricht nicht ab.
- Die Liste und die Detailansicht zeigen den Dokumenttitel und die Namen der Unterzeichner jetzt auch über diese Indizes, wenn der Datensatz selbst keinen Titel oder Namen trägt.
- **Vertrag unverändert**: `ats.signature.request` bekommt `document_id` (die gewählte Dokument-id) und `signers[].id`/`state:'pending'` wie bisher. Neu und additiv sind je Signer `name`, `email`, `role` und `order`. Der Handler reicht `signers` unverändert durch (`handlers/ats.signature.request.js:16`).
- **Deklaration eingetragen** (Generator NICHT gelaufen): `modules/esign/v4-manifest.json` enthält jetzt
  `foreign_collections: { applications:[read], customer_contacts:[read], desktop_files:[read], documents:[read] }`, mit `_basis`.
  Bis `gen-manifest.mjs` läuft, kennt das Labor (fixtures.mjs liest `v4/manifests/esign.manifest.json`) diese Quellen nicht. Deshalb sind die Auswahllisten im Rundgang leer und der Rundgang schickt keinen Befehl ab.
- Handler-Wunsch, nur vermerkt: `ats.signature.request` speichert `document_title` nicht. Die App löst den Titel live über `documents` auf. Ein gespeicherter Titel würde Anfragen auch dann lesbar halten, wenn das Dokument gelöscht ist.
- Offen gelassen: Unterzeichner frei eintippen, wenn die Person noch nicht in Kontakten oder Bewerbungen steht (DocuSign erlaubt das). Dafür braucht es eine Festlegung, welche Signer-id dann gilt.

## 2. Fehlende Übersetzungsschlüssel (customers, esign, support)

- Prüfskript: `v4/tools/textdiaet/check-i18n-keys.mjs`.
  - Es sammelt `state.t/t/tr('k')`, `labels.de|en.k`, `text.k` (esign-Muster), `data-copy`, `data-copy-placeholder` und `data-i18n*`.
  - Dazu zählen String-Literale, die einem bekannten Schlüssel entsprechen. Das sind Schlüsseltabellen wie `{ opportunity: 'selectOpportunity' }`.
  - Geprüft wird gegen das eingebettete Wörterbuch de/en und gegen `locales/de.json`/`en.json`.
  - `--fix` übernimmt fehlende Locale-Werte aus dem eingebetteten Wörterbuch derselben Sprache und erfindet nichts.
- **Ursache „searchOpportunities“**: Eine Textersetzung „Opportunity → Verkaufschance“ hatte im deutschen Wörterbuch von `customers/index.js` die SCHLÜSSEL umbenannt, nicht nur die Werte. Betroffen waren `searchVerkaufschancen`, `newVerkaufschance`, `editVerkaufschance`, `noVerkaufschancen`, `requiredVerkaufschance` und `selectVerkaufschance`. Der deutsche Rückfall gab deshalb den rohen Schlüssel aus. Die Schlüsselnamen sind jetzt zurückgesetzt, die Werte bleiben deutsch.
- `commandCompletedStatus` fehlte in allen vier Quellen. Ergänzt mit „Erledigt“ bzw. „Resolved“ (Dubletten-Filter „resolved“).
- In den Locale-Dateien ergänzt (Werte aus dem eingebetteten Wörterbuch):
  - customers: 3 Schlüssel
  - esign: 38 Schlüssel
  - support: 30 Schlüssel
- Ergebnis: `node v4/tools/textdiaet/check-i18n-keys.mjs` meldet für alle drei Apps 0 Lücken.
- Nicht statisch prüfbar sind dynamische Schlüssel (`state.t(key …)`, `text[STATUS_KEY[s]]`).

## 3. Support: Konversationstitel wie Zendesk

- `modules/support/index.js` `conversationLabel` bildet den Titel in dieser Reihenfolge:
  1. Betreff der ersten Nachricht (früheste Nachricht über alle Thread-Schlüssel, Leerraum normalisiert),
  2. sonst der Anfang ihres Texts (`body_text`/`preview`/`body_html` ohne Tags), höchstens 60 Zeichen, an der Wortgrenze gekürzt, mit „…“,
  3. sonst „Anfrage von <Name>“ (Name aus Kontakt, Absender oder Kunde),
  4. sonst „Anfrage“.
- Suchtext und IDs erscheinen nie im Titel. Die erste Nachricht je Thread kommt aus einem Index, der je Nachrichten-Array gecacht wird (kein O(Zeilen × Nachrichten)).
- Gleicher Ursprung, mitbehoben: check-raw-ids meldete für support vorher 3 Treffer (`team-001`, `account-001`, `case-001`) aus dem Kontextpanel.
  - Team, Kunde und Ticket zeigen jetzt Namen bzw. den Tickettitel.
  - Technische Referenzen blendet `readableRef` aus und zeigt dafür „nicht gesetzt“.
  - Das Feld „ID“ im verknüpften Ticket entfällt.
  - Die Zuordnung (`assignee_id`) bleibt unverändert, weil es kein Benutzerverzeichnis gibt und der bestehende Test sie prüft.

## Abnahme (selbst gefahren, `CTOX_APP_API_STUB=~/.local/state/shell-v4/stub-pins/51b1e33`)

| Prüfung | Ergebnis |
|---|---|
| `node --import ./v4/tools/node/ctox-shared.mjs --test modules/esign/tests/*.test.mjs` | 20/20 grün |
| dto. `modules/customers/tests/*.test.mjs` | 16/16 grün (12 bisher + 4 neu `i18n-keys.test.mjs`) |
| dto. `modules/support/tests/*.test.mjs` | 27/27 grün (+1 neuer Titeltest) |
| `node v4/tools/guards/textdiaet-rule.mjs --app modules/esign,modules/customers,modules/support` | 0 Treffer |
| `node v4/tools/rundgang/check-raw-ids.mjs --app esign,customers,support` | esign 0, customers 0, support 0 (vorher 3) |
| `rundgang2.mjs --app esign,customers,support --widths 1280,420` | 0 Fehler. customers meldet 4 Geometrie-Hinweise (seitlicher Scroll in `ctox-table-wrap`/`ctox-pane-scroll`), die das Layout der Tabellen betreffen und von FIX7 nicht berührt werden |

Neue Tests und Mutationsbelege:

- **esign** (`tests/v4-api.test.mjs` „Befehl: Signatur anfordern …“, umgebaut):
  - Ablauf: Dokument suchen und wählen, zwei Personen hinzufügen (Klick und Enter), Reihenfolge tauschen, Rolle ändern.
  - Erwartet: keine ID im Formulartext, Payload `document_id:'doc-delta'`, Signer-IDs in neuer Reihenfolge mit name/email/role/order.
  - Mutation: Tausch in „Nach oben“ deaktiviert → der Test wird rot (1 fail).
- **customers** (`tests/i18n-keys.test.mjs`): 0 Lücken je App, und `searchOpportunities` hat einen deutschen Text.
  - Mutation: deutschen Schlüssel wieder auf `searchVerkaufschancen` gesetzt → 2 fail.
- **support** (`tests/v4-api.test.mjs` „FIX7 Titel wie Zendesk …“): prüft Betreff vor Text, Text ≤ 60 Zeichen mit „…“, „Anfrage von Gerda Gamma“ und „Anfrage“, und dass im Titel und im Kontext kein Suchtext und keine ID steht.
  - Mutation: `search_text` wieder vorn → 4 fail.
- Die bestehenden Tests sind an das neue Verhalten angepasst:
  - support: Listenerwartung jetzt nach Betreff bzw. Textanfang statt Suchtext.
  - esign: Snapshot trägt jetzt die Empfängerzeilen statt des Komma-Texts.

Screenshots: `v4/tools/textdiaet/shots/FIX7/<app>/<breite>-NN-*.png`. Selbst angesehen:

- esign 1280 und 420: Picker im Formular, keine ID-Felder.
- support 1280: Titel sind die Betreffzeilen der ersten Nachricht, z. B. „Preisanpassung ab 01.01.2027“.
- Stub-Shot `modules/esign/tests/shots/esign-1280.png`: Empfängerzeilen mit Reihenfolge, Rolle und Pfeilen.

## Für den Orchestrator

1. `gen-manifest.mjs` laufen lassen (esign foreign_collections), danach den esign-Rundgang wiederholen. Erst dann sind die Auswahllisten im Labor gefüllt und der Rundgang schickt eine Anfrage ab.
2. Die Listenzeilen „string“ im esign-Labor kommen aus den Fixtures (`document_title: 'string'`), nicht aus der App.

Geänderte Dateien:
- `modules/esign/{index.js,index.html,index.css,v4-manifest.json,locales/de.json,locales/en.json,tests/v4-api.test.mjs}`
- `modules/customers/{index.js,locales/de.json,locales/en.json,tests/i18n-keys.test.mjs (neu)}`
- `modules/support/{index.js,locales/de.json,locales/en.json,tests/v4-api.test.mjs}`
- `v4/tools/textdiaet/{check-i18n-keys.mjs (neu),FIX7.md (neu)}`

Die Tests haben `tests/shots/*.png` neu geschrieben.

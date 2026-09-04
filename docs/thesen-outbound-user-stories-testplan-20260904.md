# THESEN Outbound — User Stories, interaktiv zu testen (Stand 04.09.2026, 07:30 UTC)

**Zweck:** Liste jedes Ablaufs, den ein Nutzer in der App wirklich ausführt, mit
konkreter Abnahmebedingung. Kein Ablauf gilt als in Ordnung, weil Code gelesen
oder ein Unit-Test grün ist — nur, weil er im angemeldeten Browser geklickt und
das Ergebnis gemessen wurde.

**Stand der Instanz beim Schreiben:** Release `branch-main-20260904T064822Z`,
Outbound-App 1.0.88, Browser-Modul 0.2.9, Dienst aktiv, kein Wartungsmodus.

**Bis 13:00 UTC gilt: nur Hotpatching, keine Upgrades.** Kunden-App über
`runtime/business-os/local-modules/…` + `refresh-catalog`, Shell-Module über
`runtime/business-os/modules/<name>/` + Versionsbump in `module.json`.
Rust-Änderungen sind NICHT hotpatchbar — die brauchen ein Upgrade und damit eine
Absprache.

Legende: **✓ getestet** (Datum + Messwert) · **~ teilweise** (Fix ausgeliefert,
Nachtest offen) · **○ offen** · **⛔ blockiert** · **👤 braucht den Owner**

---

## A. Kampagne und Leads

| ID | Story | Abnahme | Stand |
|---|---|---|---|
| A1 | Ich lege eine Kampagne an und importiere Leads als Freitext | Kampagne erscheint in der Liste, Lead liegt mit `campaign` auf dem Server | ✓ 03.09. — `LOESCHTEST-CLAUDE`, 1 Lead, Import `imported` |
| A2 | Ich importiere Leads aus einer Excel-Datei | Vorschau zeigt gültige/doppelte Zeilen, nur gültige landen im Bestand | ○ |
| A3 | Ich importiere Leads aus einem PDF/Dokument | dito | ○ |
| A4 | Ich importiere Leads von einer URL | dito | ○ |
| A5 | Ich importiere eine bestehende Sellify-Kampagne | Kampagnensuche liefert Treffer, Mitglieder kommen als Leads an | ○ |
| A6 | Ich benenne eine Kampagne um | Neuer Name überall, Leads behalten ihre Zuordnung | ○ |
| A7 | Ich lösche eine Kampagne | Nach Bestätigung sind Kampagne und Leads weg (`_deleted`) | ✓ 03.09. — Dialog 3/3, `_deleted: True` |
| A8 | Ich lege einen Lead von Hand an und bearbeite ihn | Editor öffnet, Speichern schreibt die Felder zurück | ○ |
| A9 | Ich suche, filtere und sortiere die Leadliste | Trefferliste stimmt mit der Eingabe überein | ○ |
| A10 | Ich wähle mehrere Leads und starte eine Sammelaktion | Aktion trifft genau die Auswahl | ○ |

## B. Recherche starten und verfolgen

| ID | Story | Abnahme | Stand |
|---|---|---|---|
| B1 | Ich starte eine neue Recherche für einen Lead | Chatfenster mit **einzeiligem** Prompt, Task in der Queue, Harness arbeitet | ✓ 03./04.09. — AKEMI, Schritt 6/6, 60 Tool Calls, „CTOX core · verbunden · Live" |
| B2 | Ich starte eine Nachrecherche für eine Firma, die in Sellify steht | Lauf startet; steht sie NICHT in Sellify, sagt die App das ausdrücklich | ○ — **Defekt:** bricht heute wortlos ab |
| B3 | Ich starte „Alle recherchieren" für eine ganze Kampagne | Je Lead ein Lauf, Fortschritt sichtbar, kein Task geht verloren | ○ |
| B4 | Ich klicke im Chat auf die Task-ID und lande beim richtigen Task | Die CTOX-App zeigt GENAU diesen Task, nie einen Ersatz | ~ Fix `5c3287695` live, Nachtest offen |
| B5 | Ich sehe auf der Karte, wo mein Task im Harness-Loop steckt | Genau ein Wesen für meinen Task, sein Knoten ist markiert | ~ Fix `5c3287695` live, Nachtest offen |
| B6 | Ich breche eine laufende Recherche ab | Lauf endet, Lead fällt in einen sauberen Zustand zurück | ○ |
| B7 | Ich stelle im selben Chat eine Folgeaufgabe | Eingabe geht in denselben Task, Antwort erscheint dort | ○ |
| B8 | Unblocking: Recherche fordert Anmeldung → ich melde mich an → „Erledigt – Recherche fortsetzen" → Lauf geht weiter | Der Lauf setzt fort und liefert Felder, die vorher fehlten | ○ 👤 — Leiste und Knopf sind da (04.09. geprüft), der Anmeldeschritt gehört dem Owner |

## C. Prüfen und Freigeben

| ID | Story | Abnahme | Stand |
|---|---|---|---|
| C1 | Ich sehe alle 32 Felder mit Belegquellen durch | Jedes Feld zeigt Wert, Belegzahl und Quellen-Links | ~ gelesen, Links nicht geöffnet |
| C2 | Ich ändere einen falschen Feldwert | Wert wird gespeichert und als menschlich gepflegt gekennzeichnet | ○ |
| C3 | Ich gebe ein Feld frei | Feld gilt als freigegeben, Zähler „offene Prüfungen" sinkt | ○ |
| C4 | Ich wechsle zwischen den Personen eines Leads | Alle Personenfelder wechseln mit | ✓ 03.09. — 5/5 bei KUKA, 5/5 bei BOOMEX |
| C5 | Ich löse einen Feldkonflikt auf | Konflikt verschwindet, Entscheidung ist festgehalten | ○ |
| C6 | Ich validiere einen Lead | „Validieren" wird klickbar, sobald alle Blocker weg sind | ⛔ kein Lead ist freigabefähig (31 von 34 `pending`) |

## D. Sellify-Übergabe

| ID | Story | Abnahme | Stand |
|---|---|---|---|
| D1 | Ich wiederhole die Sperrvermerk-Prüfung | Banner zählt herunter und verschwindet, wenn alles geprüft ist | ✓ 03.09. — 6 → 1 → 0 |
| D2 | Ich wähle Empfänger für Sellify aus | Nur freie Kontakte lassen sich wählen, gesperrte bleiben gesperrt | ○ |
| D3 | Ich übergebe an Sellify („nur aktualisieren") | Datensatz landet im CRM, Status wechselt auf „Übergeben" | ○ 👤 — Aktion nach außen, gehört dem Owner |
| D4 | Ich übergebe an Sellify inkl. Kampagne | dito, plus Kampagnenzuordnung | ○ 👤 |

## E. Quellen und Adapter

| ID | Story | Abnahme | Stand |
|---|---|---|---|
| E1 | Ich öffne „Quellen & Zugänge" und lese den Status | Jede Zeile in Nutzersprache, technische Meldung nur eingeklappt | ✓ 03.09. |
| E2 | Ich sehe den echten Registry-Status einer Quelle | „In der Scrape-Registry aktiv registriert" statt App-Vermutung | ~ 1.0.87 + Befehl live, Nachtest offen |
| E3 | Ich erzeuge einen Adapter für eine Quelle | Adapter entsteht, Skriptrevision steigt | ○ |
| E4 | Ich prüfe den Datenzugriff einer Quelle | Prüfung läuft, Ergebnis erscheint mit Datum | ○ |
| E5 | Ich sehe das Adapter-Skript an | Skript aus der Scrape-Registry, nicht der leere App-Datensatz | ○ |
| E6 | Ich lösche einen Adapter bzw. eine eigene Quelle | Eintrag verschwindet, Recherche nutzt sie nicht mehr | ○ |
| E7 | Ich aktiviere/deaktiviere eine Quelle | Deaktivierte Quelle wird bei der Recherche übersprungen | ○ |
| E8 | Ich melde mich im CTOX-Browser bei einer Quelle an | Zugang gilt danach als hinterlegt und geprüft | ○ 👤 |
| E9 | Ich bearbeite den Rechercheablauf (Schritte 0..x) und speichere | Neue Anweisung landet im Policy-Datensatz und im nächsten Lauf | ○ |

## F. Browser-App

| ID | Story | Abnahme | Stand |
|---|---|---|---|
| F1 | Ich öffne eine Sitzung und sehe die Seite | Bild wird gezeichnet, Platzhalter verschwindet | ✓ 03.09. — 119.984 farbige Pixel |
| F2 | Ich klicke und tippe in der Seite | Klick und Tastatur erreichen die Seite, Bild aktualisiert sich | ✓ 03.09. — Cookie-Banner weg, Text im Feld |
| F3 | Ich übernehme und gebe die Steuerung frei | Knopf immer erreichbar, Pacht wird gehalten | ~ Übernahme geprüft, Freigabe offen |
| F4 | Ich arbeite mit mehreren Tabs | Tableiste zeigt und wechselt Tabs | ○ |
| F5 | Ich lese die Seite aus / übergebe sie an CTOX | Inhalt landet als Beleg bzw. Datensatz | ○ |
| F6 | Ich sehe die Skript-Ansicht der Sitzung | Skript sichtbar und nachvollziehbar | ○ |
| F7 | Ich lade eine Datei hoch bzw. herunter | Datei kommt an, Ort ist benannt | ○ |

## G. Querschnitt

| ID | Story | Abnahme | Stand |
|---|---|---|---|
| G1 | Ich lade die Seite neu | Kein dauerhafter Wiederherstellungsbildschirm | ~ nur im Verbindungsfenster, `ctoxOperational: ok` |
| G2 | Ich öffne die App | Oberfläche sofort, Daten füllen sich sichtbar nach | ✓ 04.09. — 2 s statt 167 s; Daten weiter ~168 s |
| G3 | Ich arbeite mit zwei Apps nebeneinander | Hintergrundfenster bleibt deckend und lesbar | ✓ 04.09. — `opacity: 1` |
| G4 | Ich öffne per Rechtsklick eine Aufgabe an die Crew | Chat öffnet MIT Bezug zum Datensatz | ○ — **Defekt:** öffnet einen leeren Chat |
| G5 | Ich öffne aus einem Fehler-Task den zugehörigen Chat und mache dort weiter | Chat öffnet, Eingabe geht in denselben Vorgang | ○ — Funktion fehlt heute |
| G6 | Ich arbeite in einem schmalen Fenster | Kein waagerechtes Scrollen, nichts abgeschnitten | ○ |
| G7 | Ich bediene die App mit der Tastatur | Fokus sichtbar, alle Aktionen erreichbar | ○ |
| G8 | Ich stelle die Oberfläche auf Englisch | Keine deutschen Reste in Knöpfen und Meldungen | ○ |

---

## Offene Defekte aus dem interaktiven Testen (noch nicht behoben)

1. **B2** „Nachrecherche" bricht wortlos ab, wenn die Firma nicht in Sellify steht.
2. **G4** Rechtsklick „An die Crew übergeben" öffnet einen leeren Chat ohne Datensatzbezug.
3. **G5** Aus einem Fehler-Task führt kein Weg in den zugehörigen Chat.
4. **Queue-Altlast:** 199 gescheiterte Aufgaben ab 13.07. stehen weiter in der Liste.
   Alle terminal, `cleanup-scope` fasst nur offene an — Entscheidung des Owners nötig,
   ob stilllegen oder Anzeige filtern.
5. **Datenwartezeit:** Oberfläche ist sofort da, die Daten brauchen weiter ~168 s.
   Ursache liegt in der Datenebene der Shell (201 Collections), nicht in der App.
6. **Altbelege:** 81 Felder tragen `verified`, haben aber weniger als zwei unabhängige
   Quell-Hosts. Die Freigabe blockiert sie korrekt; die Kennzeichnung stimmt erst nach
   einer Nachrecherche unter der neuen Regel.

## Was den Owner braucht

- **B8** einmal bei dnbhoovers anmelden und „Erledigt – Recherche fortsetzen" drücken.
  Das schließt die Kette und löst B3 und C6 mit auf.
- **D3/D4** die erste echte Sellify-Übergabe freigeben.
- **E8** die Anmeldung bei den übrigen Quellen.
- Entscheidung zur Queue-Altlast (Punkt 4 oben).

## Reihenfolge, die ich vorschlage

1. B4/B5 nachtesten (Fix ist live, kostet zwei Klicks).
2. B2, G4, G5 beheben — alle drei sind Hotpatch-fähig.
3. B8 gemeinsam durchspielen, danach B3 als echter Kampagnenlauf.
4. C1–C5 und D2 durchgehen, sobald ein Lead freigabefähig ist.
5. Den Rest der Tabelle abarbeiten, angefangen bei E3–E7 und F4–F7.

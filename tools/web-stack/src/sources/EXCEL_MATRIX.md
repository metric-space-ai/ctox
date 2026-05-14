# Thesen-Nachrecherche — Quellenmatrix

Diese Datei übersetzt die Quellenmatrix aus
`/Users/michaelwelsch/Downloads/Thesen_Nachrecherche/Abstimmung 12.05.2026 - Nach- und Neurecherche - final.xlsx`
in eine maschinenlesbare Form. Jede Quelle im CTOX-Webstack-Source-Registry
(`tools/web-stack/src/sources/<id>.rs`) leitet ihre `authoritative_for()`-,
`countries()`- und (für `person-research`) Prioritätsentscheidungen aus
genau diesem Dokument ab.

## Mode/Block-Mapping

| Excel-Block                                | `ResearchMode`                  | Quellen befüllt? |
| ------------------------------------------ | ------------------------------- | ---------------- |
| Sheet `Nach-Recherche` / A. Vorhanden      | `HaveData`                      | nein (keine Recherche) |
| Sheet `Nach-Recherche` / B. Bestand generell | `UpdateInventoryGeneral`      | nein (leere Spalten → leerer Plan pro Feld) |
| Sheet `Nach-Recherche` / B 1. Änderung Person | `UpdatePerson`               | nur Person-Felder |
| Sheet `Nach-Recherche` / B 2. Änderung Firmierung | `UpdateFirm`              | Firma + Person |
| Sheet `Neu-Recherche` / A. Vorhanden       | n/a — Excel-Input               | nein |
| Sheet `Neu-Recherche` / B. Neuer Bestand   | `NewRecord`                     | Firma + Person |

## Normalisierte Quellen-IDs

Tippfehler aus der Excel (`ww.northdata.de`, `wwww.northdata.de`,
`dnbhoovers.com bundesanzeiger.de` mit fehlendem Komma) sind hier
korrigiert. Discovery-Muster (`Impressum`, `Unternehmensseite`, `Google`)
sind **kein** Source-Modul — die deckt der existierende Web-Stack
(`ctox web search`, `ctox web read`) über die generische Provider-Cascade
und das Bemerkungs-Feld der Excel ("Online recherchieren") ab.

| Excel-Token              | Source-ID            | Tier | Modul-Datei            |
| ------------------------ | -------------------- | ---- | ---------------------- |
| `www.bundesanzeiger.de`  | `bundesanzeiger.de`  | P    | `bundesanzeiger.rs`    |
| `www.zefix.ch`           | `zefix.ch`           | P    | `zefix.rs`             |
| `Handelsregister` (impliziert) | `handelsregister.de` | P | `handelsregister.rs`  |
| `www.northdata.de`       | `northdata.de`       | S    | `northdata.rs`         |
| `www.firmenabc.at`       | `firmenabc.at`       | S    | `firmenabc.rs`         |
| `www.companyhouse.de`    | `companyhouse.de`    | S    | `companyhouse.rs`      |
| `app.dnbhoovers.com`     | `dnbhoovers.com`     | C    | `dnbhoovers.rs`        |
| `app.leadfeeder.com`     | `leadfeeder.com`     | C    | `leadfeeder.rs`        |
| `www.linkedin.com`       | `linkedin.com`       | C    | `linkedin.rs`          |
| `www.xing.com`           | `xing.com`           | C    | `xing.rs`              |
| `Impressum`              | (generic web-read)   | —    | nicht modul-gestützt   |
| `Unternehmensseite`      | (generic web-read)   | —    | nicht modul-gestützt   |
| `Google`                 | (generic search)     | —    | nicht modul-gestützt   |

## Matrix `(Mode, Country, FieldKey) → Source-Priority`

Reihenfolge spiegelt die Excel-Reihenfolge wider (links → rechts → fallback
auf Discovery-Muster). Quellen sind nach Tier sortiert; bei Gleichstand
gilt die Excel-Ordnung. Discovery-Muster sind kursiv: sie laufen über
`ctox web search` ohne Source-Modul-Pin.

### Mode `UpdatePerson` (B 1 — nur Person-Felder)

#### Deutschland
| FieldKey            | Source-Priorität                                                                   |
| ------------------- | ---------------------------------------------------------------------------------- |
| `person_geschlecht` | `linkedin.com`, `xing.com`, *Unternehmensseite*                                    |
| `person_titel`      | *Impressum*, `companyhouse.de`, *Unternehmensseite*                                |
| `person_vorname`    | `northdata.de`, *Impressum*, *Unternehmensseite*                                   |
| `person_nachname`   | `northdata.de`, *Impressum*, *Unternehmensseite*                                   |
| `person_funktion`   | `linkedin.com`, `xing.com`, `dnbhoovers.com`                                       |
| `person_position`   | *Impressum*, `northdata.de`, `dnbhoovers.com`, *Unternehmensseite*                 |
| `person_email`      | `leadfeeder.com`                                                                   |
| `person_linkedin`   | `linkedin.com`                                                                     |
| `person_xing`       | `xing.com`                                                                         |
| `person_telefon`    | *(keine Quelle in Excel — best effort via Impressum/Unternehmensseite, sonst leer)* |

#### Österreich
| FieldKey            | Source-Priorität                                                                                                     |
| ------------------- | -------------------------------------------------------------------------------------------------------------------- |
| `person_geschlecht` | `firmenabc.at`, `linkedin.com`, `xing.com`, *Unternehmensseite*                                                      |
| `person_titel`      | `firmenabc.at`, *Impressum*, *Unternehmensseite*                                                                     |
| `person_vorname`    | `northdata.de`, *Impressum*, `firmenabc.at`, `northdata.de`                                                          |
| `person_nachname`   | `northdata.de`, *Impressum*, `firmenabc.at`                                                                          |
| `person_funktion`   | *Unternehmensseite*, `linkedin.com`, `xing.com`                                                                      |
| `person_position`   | *Impressum*, `northdata.de`                                                                                          |
| `person_email`      | `leadfeeder.com`                                                                                                     |
| `person_linkedin`   | `linkedin.com`                                                                                                       |
| `person_xing`       | `xing.com`                                                                                                           |
| `person_telefon`    | *(keine Quelle in Excel)*                                                                                            |

#### Schweiz
| FieldKey            | Source-Priorität                                                                |
| ------------------- | ------------------------------------------------------------------------------- |
| `person_geschlecht` | `linkedin.com`, `xing.com`, *Unternehmensseite*                                 |
| `person_titel`      | *Unternehmensseite*, `linkedin.com`, `xing.com`                                 |
| `person_vorname`    | `northdata.de`, *Unternehmensseite*                                             |
| `person_nachname`   | `northdata.de`, *Unternehmensseite*                                             |
| `person_funktion`   | *Unternehmensseite*, `linkedin.com`, `xing.com`                                 |
| `person_position`   | *Unternehmensseite*, `northdata.de`                                             |
| `person_email`      | `leadfeeder.com`                                                                |
| `person_linkedin`   | `linkedin.com`                                                                  |
| `person_xing`       | `xing.com`                                                                      |
| `person_telefon`    | *(keine Quelle in Excel)*                                                       |

### Mode `UpdateFirm` (B 2 — Firma + Person)

#### Deutschland
| FieldKey            | Source-Priorität                                                                                 |
| ------------------- | ------------------------------------------------------------------------------------------------ |
| `firma_name`        | `northdata.de`, *Impressum*                                                                      |
| `firma_anschrift`   | `northdata.de`, *Impressum*                                                                      |
| `firma_plz`         | `northdata.de`, *Impressum*                                                                      |
| `firma_ort`         | `northdata.de`, *Impressum*                                                                      |
| `firma_email`       | *Impressum*, `leadfeeder.com`, `dnbhoovers.com`                                                  |
| `firma_domain`      | *Google*, `dnbhoovers.com`, `leadfeeder.com`                                                     |
| `person_geschlecht` | *Unternehmensseite*, `linkedin.com`, `xing.com`                                                  |
| `person_titel`      | *Impressum*, `companyhouse.de`, *Unternehmensseite*                                              |
| `person_vorname`    | *Impressum*, `northdata.de`, *Unternehmensseite*                                                 |
| `person_nachname`   | *Impressum*, `northdata.de`, *Unternehmensseite*                                                 |
| `person_funktion`   | *Unternehmensseite*, `linkedin.com`, `xing.com`, `dnbhoovers.com`                                |
| `person_position`   | *Impressum*, `northdata.de`, `dnbhoovers.com`, *Unternehmensseite*                               |
| `person_email`      | `leadfeeder.com`                                                                                 |
| `person_linkedin`   | `linkedin.com`                                                                                   |
| `person_xing`       | `xing.com`                                                                                       |
| `wz_code`, `umsatz`, `mitarbeiter`, `sellify_nummer` | *(in der Excel-B2-Person-Sektion ohne Quelle — werden nur in B/Neu rezugriffen, s. u.)* |

#### Österreich
| FieldKey            | Source-Priorität                                                                                                          |
| ------------------- | ------------------------------------------------------------------------------------------------------------------------- |
| `firma_name`        | `northdata.de`, *Impressum*, `firmenabc.at`                                                                               |
| `firma_anschrift`   | `northdata.de`, *Impressum*, `firmenabc.at`                                                                               |
| `firma_plz`         | `northdata.de`, *Impressum*, `firmenabc.at`                                                                               |
| `firma_ort`         | `northdata.de`, *Impressum*, `firmenabc.at`                                                                               |
| `firma_email`       | *Impressum*, `leadfeeder.com`, `firmenabc.at`                                                                             |
| `firma_domain`      | `firmenabc.at`, *Google*                                                                                                  |
| `person_geschlecht` | `firmenabc.at`, `linkedin.com`, `xing.com`, *Unternehmensseite*                                                           |
| `person_titel`      | `firmenabc.at`, *Impressum*, *Unternehmensseite*                                                                          |
| `person_vorname`    | *Impressum*, `firmenabc.at`, `northdata.de`                                                                               |
| `person_nachname`   | *Impressum*, `firmenabc.at`, `northdata.de`                                                                               |
| `person_funktion`   | *Unternehmensseite*, `linkedin.com`, `xing.com`                                                                           |
| `person_position`   | *Impressum*, `northdata.de`                                                                                               |
| `person_email`      | `leadfeeder.com`                                                                                                          |
| `person_linkedin`   | `linkedin.com`                                                                                                            |
| `person_xing`       | `xing.com`                                                                                                                |

#### Schweiz
| FieldKey            | Source-Priorität                                                                                                              |
| ------------------- | ----------------------------------------------------------------------------------------------------------------------------- |
| `firma_name`        | `northdata.de`, *Unternehmensseite*, `zefix.ch`                                                                               |
| `firma_anschrift`   | `northdata.de`, *Unternehmensseite*, `zefix.ch`                                                                               |
| `firma_plz`         | `northdata.de`, *Unternehmensseite*, `zefix.ch`                                                                               |
| `firma_ort`         | `northdata.de`, *Unternehmensseite*, `dnbhoovers.com`                                                                         |
| `firma_email`       | *Unternehmensseite*, `leadfeeder.com`, `dnbhoovers.com`                                                                       |
| `firma_domain`      | *Google*, `dnbhoovers.com`, `leadfeeder.com`                                                                                  |
| `person_geschlecht` | `linkedin.com`, `xing.com`, *Unternehmensseite*                                                                               |
| `person_titel`      | *Unternehmensseite*, `linkedin.com`, `xing.com`                                                                               |
| `person_vorname`    | *Unternehmensseite*, `northdata.de`                                                                                           |
| `person_nachname`   | `northdata.de`, *Unternehmensseite*                                                                                           |
| `person_funktion`   | *Unternehmensseite*, `linkedin.com`, `xing.com`                                                                               |
| `person_position`   | *Unternehmensseite*, `northdata.de`                                                                                           |
| `person_email`      | `leadfeeder.com`                                                                                                              |
| `person_linkedin`   | `linkedin.com`                                                                                                                |
| `person_xing`       | `xing.com`                                                                                                                    |

### Mode `NewRecord` (Neu-Recherche B — Greenfield)

#### Deutschland
| FieldKey            | Source-Priorität                                                                                  |
| ------------------- | ------------------------------------------------------------------------------------------------- |
| `firma_name`        | *Impressum*, `northdata.de`, `bundesanzeiger.de`, `dnbhoovers.com`                                |
| `firma_anschrift`   | *Impressum*, `northdata.de`, `dnbhoovers.com`                                                     |
| `firma_plz`         | *Impressum*, `northdata.de`, `dnbhoovers.com`                                                     |
| `firma_ort`         | *Impressum*, `northdata.de`, `dnbhoovers.com`                                                     |
| `firma_email`       | *Impressum*, `leadfeeder.com`, `dnbhoovers.com`                                                   |
| `firma_domain`      | *Google*, `dnbhoovers.com`, `leadfeeder.com`                                                      |
| `wz_code`           | `dnbhoovers.com`, `leadfeeder.com`                                                                |
| `umsatz`            | `dnbhoovers.com`, `bundesanzeiger.de`, *Unternehmensseite*                                        |
| `mitarbeiter`       | `dnbhoovers.com`, `leadfeeder.com`, *Unternehmensseite*                                           |
| `person_geschlecht` | `linkedin.com`, `xing.com`, *Unternehmensseite*                                                   |
| `person_titel`      | *Impressum*, `companyhouse.de`, *Unternehmensseite*                                               |
| `person_vorname`    | *Impressum*, `northdata.de`, *Unternehmensseite*                                                  |
| `person_nachname`   | *Impressum*, `northdata.de`, *Unternehmensseite*                                                  |
| `person_funktion`   | *Unternehmensseite*, `linkedin.com`, `xing.com`, `dnbhoovers.com`                                 |
| `person_position`   | *Impressum*, `northdata.de`, `dnbhoovers.com`, *Unternehmensseite*                                |
| `person_email`      | `leadfeeder.com`                                                                                  |
| `person_linkedin`   | `linkedin.com`                                                                                    |
| `person_xing`       | `xing.com`                                                                                        |

#### Österreich
| FieldKey            | Source-Priorität                                                                                                              |
| ------------------- | ----------------------------------------------------------------------------------------------------------------------------- |
| `firma_name`        | *Impressum*, `firmenabc.at`, `northdata.de`                                                                                   |
| `firma_anschrift`   | `northdata.de`, *Impressum*, `firmenabc.at`                                                                                   |
| `firma_plz`         | `northdata.de`, *Impressum*, `firmenabc.at`                                                                                   |
| `firma_ort`         | `northdata.de`, *Impressum*, `firmenabc.at`                                                                                   |
| `firma_email`       | *Impressum*, `leadfeeder.com`, `firmenabc.at`                                                                                 |
| `firma_domain`      | `firmenabc.at`, *Google*                                                                                                      |
| `wz_code`           | `dnbhoovers.com`, `leadfeeder.com`                                                                                            |
| `umsatz`            | `dnbhoovers.com`, `leadfeeder.com`                                                                                            |
| `mitarbeiter`       | `dnbhoovers.com`, `leadfeeder.com`                                                                                            |
| `person_geschlecht` | `firmenabc.at`, `linkedin.com`, `xing.com`, *Unternehmensseite*                                                               |
| `person_titel`      | `firmenabc.at`, *Impressum*, *Unternehmensseite*                                                                              |
| `person_vorname`    | *Impressum*, `firmenabc.at`, `northdata.de`                                                                                   |
| `person_nachname`   | *Impressum*, `firmenabc.at`, `northdata.de`                                                                                   |
| `person_funktion`   | *Unternehmensseite*, `linkedin.com`, `xing.com`                                                                               |
| `person_position`   | *Impressum*, `northdata.de`                                                                                                   |
| `person_email`      | `leadfeeder.com`                                                                                                              |
| `person_linkedin`   | `linkedin.com`                                                                                                                |
| `person_xing`       | `xing.com`                                                                                                                    |

#### Schweiz
| FieldKey            | Source-Priorität                                                                                                              |
| ------------------- | ----------------------------------------------------------------------------------------------------------------------------- |
| `firma_name`        | *Unternehmensseite*, `northdata.de`, `zefix.ch`                                                                               |
| `firma_anschrift`   | *Unternehmensseite*, `northdata.de`, `zefix.ch`                                                                               |
| `firma_plz`         | `northdata.de`, *Unternehmensseite*, `zefix.ch`                                                                               |
| `firma_ort`         | `northdata.de`, *Unternehmensseite*, `dnbhoovers.com`, `zefix.ch`                                                             |
| `firma_email`       | *Unternehmensseite*, `leadfeeder.com`, `dnbhoovers.com`                                                                       |
| `firma_domain`      | *Google*, `dnbhoovers.com`, `leadfeeder.com`                                                                                  |
| `wz_code`           | `dnbhoovers.com`, `leadfeeder.com`                                                                                            |
| `umsatz`            | `dnbhoovers.com`, *Unternehmensseite*                                                                                         |
| `mitarbeiter`       | `dnbhoovers.com`, `leadfeeder.com`, *Unternehmensseite*                                                                       |
| `person_geschlecht` | `linkedin.com`, `xing.com`, *Unternehmensseite*                                                                               |
| `person_titel`      | *Unternehmensseite*, `linkedin.com`, `xing.com`                                                                               |
| `person_vorname`    | *Unternehmensseite*, `northdata.de`, `zefix.ch`                                                                               |
| `person_nachname`   | `northdata.de`, `zefix.ch`, *Unternehmensseite*                                                                               |
| `person_funktion`   | *Unternehmensseite*, `linkedin.com`, `xing.com`, `zefix.ch`                                                                   |
| `person_position`   | *Unternehmensseite*, `northdata.de`, `zefix.ch`                                                                               |
| `person_email`      | `leadfeeder.com`                                                                                                              |
| `person_linkedin`   | `linkedin.com`                                                                                                                |
| `person_xing`       | `xing.com`                                                                                                                    |

### Mode `HaveData` & `UpdateInventoryGeneral`

Beide produzieren **leere Recherche-Pläne**: `HaveData` weil die Stammdaten
schon vorliegen, `UpdateInventoryGeneral` weil die Excel keine Quellen
gefüllt hat. `person-research` skippt diese Modi per Konvention; ein Agent,
der explizit eine Auffrischung möchte, wählt stattdessen `UpdateFirm`
oder `UpdatePerson`.

## Tipp- und Vereinheitlichungs-Notizen

- `ww.northdata.de` und `wwww.northdata.de` aus der Excel → `northdata.de`.
- `dnbhoovers.com bundesanzeiger.de` (fehlendes Komma, DE / Umsatz) →
  `dnbhoovers.com` und `bundesanzeiger.de` als zwei Einträge.
- `www.northdata.de Unternehmensseite` (mehrfach in CH) → zwei Einträge.
- `Person - Position* GF` (DE / B 1, Person) ist als `person_position`
  geführt; das *-Suffix („nicht zu 100 %") überträgt sich in
  `Confidence::Medium` als Default für diese Felder.

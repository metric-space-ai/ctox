# SCAN-BASELINE – Port-Aufwand je App (App-API 4.0)

Gemessen 15.10.2026 13:09 (Berlin) mit `node v4/tools/port/scan.mjs --md` im Klon `~/Documents/ctox-shell-v4`
(Zweig shell-v4/prototype, Arbeitsbaum inkl. paralleler Stränge). 40 Verzeichnisse unter `modules/` mit `module.json`.

Zählweise: „Stellen“ = Treffer (Zeile × Regel) in .js/.mjs/.html ohne Tests, ohne Kommentarzeilen, ohne `handlers/`
(Node-Laufzeit, eigener Vertrag); eine Zeile kann mehrere Regeln treffen (2418 Treffer = 2379 Treffer auf 2285 verschiedenen Codezeilen + 39 × R17 schema.js). „Zeilen (Code)“ = Zeilen der gescannten Dateien. „CSS ungescoped“ = Selektoren außerhalb von
`[data-app="<id>"]` (separat, nicht in „Stellen“). Regeln R1–R17: `PORTING.md` §1. Der Scanner ist eine Fundliste,
keine Vollständigkeitsgarantie (Muster-basiert; Stichproben: buchhaltung R3, shiftflow R2, matching R7, browser R9,
mail R11 geprüft, Treffer echt; `*_tests.js`/`e2e*` nach Stichprobe ausgeschlossen).

knowledge ist bereits portiert (0). Vor dem Port (Stand HEAD, gleiche Scanner-Fassung):
| knowledge | 3975 | 98 | 1 | 33 | 1 | 9 | 4 | 9 | 1 | 4 | 2 | 4 | 16 | 4 | 7 | 2 | 0 | 1 | 303 |

| App | Zeilen (Code) | Stellen | R1 | R2 | R3 | R4 | R5 | R6 | R7 | R8 | R9 | R10 | R11 | R12 | R13 | R14 | R15 | R17 | CSS ungescoped |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| app-store | 3152 | 72 | 1 | 8 | 7 | 10 | 2 | 1 | 11 | 2 | 5 | 8 | 9 | 0 | 2 | 5 | 0 | 1 | 173 |
| appsec-pentest | 3474 | 38 | 1 | 4 | 0 | 6 | 17 | 0 | 2 | 3 | 0 | 2 | 0 | 0 | 0 | 2 | 0 | 1 | 153 |
| browser | 4575 | 96 | 1 | 6 | 10 | 26 | 9 | 9 | 0 | 2 | 24 | 1 | 3 | 1 | 2 | 1 | 0 | 1 | 186 |
| buchhaltung | 7022 | 107 | 1 | 5 | 66 | 4 | 1 | 2 | 16 | 3 | 2 | 1 | 3 | 0 | 0 | 2 | 0 | 1 | 218 |
| calendar | 2869 | 64 | 1 | 5 | 35 | 3 | 0 | 2 | 4 | 3 | 0 | 4 | 4 | 0 | 0 | 2 | 0 | 1 | 155 |
| coding-agents | 1695 | 40 | 1 | 6 | 0 | 7 | 6 | 3 | 0 | 2 | 3 | 4 | 6 | 0 | 0 | 1 | 0 | 1 | 159 |
| consent | 1021 | 30 | 1 | 7 | 4 | 4 | 4 | 0 | 0 | 1 | 1 | 4 | 3 | 0 | 0 | 0 | 0 | 1 | 65 |
| conversations | 3816 | 102 | 5 | 6 | 27 | 12 | 3 | 10 | 0 | 2 | 3 | 5 | 16 | 5 | 5 | 2 | 0 | 1 | 195 |
| creator | 1479 | 36 | 1 | 8 | 1 | 7 | 1 | 5 | 2 | 1 | 1 | 5 | 1 | 0 | 1 | 1 | 0 | 1 | 94 |
| credentials | 948 | 32 | 1 | 5 | 2 | 3 | 1 | 2 | 1 | 3 | 3 | 5 | 3 | 0 | 0 | 2 | 0 | 1 | 51 |
| ctox | 10006 | 130 | 1 | 18 | 10 | 20 | 13 | 9 | 4 | 2 | 10 | 7 | 19 | 1 | 7 | 7 | 1 | 1 | 490 |
| customers | 5349 | 38 | 1 | 7 | 4 | 5 | 2 | 3 | 0 | 3 | 1 | 2 | 6 | 0 | 1 | 2 | 0 | 1 | 181 |
| cv-print-builder | 3052 | 66 | 1 | 8 | 18 | 17 | 3 | 2 | 1 | 1 | 0 | 7 | 2 | 0 | 4 | 1 | 0 | 1 | 246 |
| desktop | 2681 | 115 | 4 | 2 | 32 | 7 | 3 | 23 | 5 | 7 | 5 | 7 | 4 | 2 | 9 | 4 | 0 | 1 | 69 |
| documents | 5170 | 141 | 1 | 24 | 30 | 9 | 3 | 20 | 2 | 3 | 5 | 4 | 25 | 7 | 3 | 4 | 0 | 1 | 383 |
| esign | 844 | 30 | 1 | 8 | 4 | 4 | 4 | 1 | 0 | 2 | 0 | 2 | 3 | 0 | 0 | 0 | 0 | 1 | 74 |
| explorer | 1640 | 47 | 1 | 1 | 19 | 9 | 0 | 8 | 0 | 0 | 0 | 5 | 1 | 0 | 1 | 1 | 0 | 1 | 106 |
| file-viewer | 443 | 37 | 1 | 1 | 8 | 16 | 2 | 0 | 0 | 0 | 1 | 5 | 1 | 0 | 0 | 1 | 0 | 1 | 22 |
| importer | 843 | 20 | 1 | 0 | 4 | 2 | 4 | 0 | 0 | 2 | 1 | 1 | 0 | 0 | 2 | 2 | 0 | 1 | 134 |
| intake | 845 | 24 | 1 | 7 | 4 | 4 | 1 | 0 | 0 | 1 | 0 | 2 | 3 | 0 | 0 | 0 | 0 | 1 | 74 |
| interviews | 1021 | 30 | 1 | 8 | 7 | 4 | 0 | 0 | 0 | 1 | 0 | 2 | 5 | 0 | 0 | 1 | 0 | 1 | 78 |
| invoices | 3131 | 28 | 1 | 4 | 4 | 2 | 1 | 3 | 0 | 1 | 0 | 5 | 4 | 0 | 0 | 2 | 0 | 1 | 64 |
| iot | 2039 | 37 | 1 | 5 | 1 | 3 | 2 | 0 | 12 | 3 | 0 | 6 | 1 | 0 | 0 | 2 | 0 | 1 | 128 |
| knowledge | 3781 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| kundenpipeline | 550 | 13 | 1 | 4 | 1 | 2 | 2 | 0 | 0 | 0 | 0 | 1 | 0 | 1 | 0 | 0 | 0 | 1 | 52 |
| mail | 5746 | 75 | 1 | 9 | 9 | 8 | 3 | 2 | 0 | 5 | 18 | 4 | 9 | 1 | 3 | 2 | 0 | 1 | 438 |
| matching | 13739 | 132 | 1 | 7 | 60 | 4 | 9 | 1 | 24 | 1 | 0 | 7 | 6 | 1 | 6 | 4 | 0 | 1 | 467 |
| nachweise | 1066 | 31 | 1 | 9 | 5 | 4 | 2 | 1 | 0 | 1 | 0 | 2 | 4 | 0 | 0 | 1 | 0 | 1 | 59 |
| news | 2286 | 31 | 1 | 2 | 14 | 0 | 2 | 0 | 0 | 3 | 0 | 4 | 0 | 0 | 2 | 2 | 0 | 1 | 187 |
| notes | 3362 | 41 | 1 | 8 | 11 | 4 | 0 | 2 | 3 | 3 | 0 | 4 | 1 | 0 | 0 | 3 | 0 | 1 | 215 |
| outbound | 11689 | 125 | 1 | 18 | 35 | 4 | 19 | 1 | 21 | 4 | 2 | 6 | 4 | 4 | 2 | 3 | 0 | 1 | 424 |
| placements | 1027 | 30 | 1 | 8 | 4 | 4 | 3 | 1 | 0 | 1 | 0 | 2 | 4 | 0 | 0 | 1 | 0 | 1 | 54 |
| reports | 1815 | 60 | 1 | 15 | 3 | 11 | 3 | 4 | 0 | 3 | 6 | 4 | 4 | 3 | 1 | 1 | 0 | 1 | 73 |
| research | 8854 | 97 | 1 | 16 | 12 | 24 | 4 | 3 | 0 | 3 | 4 | 2 | 12 | 9 | 3 | 2 | 1 | 1 | 466 |
| shiftflow | 3587 | 145 | 1 | 49 | 60 | 2 | 0 | 22 | 1 | 3 | 0 | 4 | 1 | 0 | 0 | 1 | 0 | 1 | 224 |
| spreadsheets | 2765 | 104 | 1 | 22 | 18 | 7 | 1 | 32 | 2 | 3 | 5 | 3 | 4 | 2 | 0 | 3 | 0 | 1 | 118 |
| submissions | 861 | 29 | 1 | 8 | 4 | 4 | 2 | 1 | 0 | 1 | 0 | 2 | 4 | 0 | 0 | 1 | 0 | 1 | 60 |
| support | 2454 | 36 | 1 | 4 | 2 | 7 | 2 | 1 | 0 | 3 | 1 | 6 | 4 | 2 | 0 | 2 | 0 | 1 | 79 |
| threads | 3092 | 53 | 1 | 6 | 4 | 10 | 3 | 3 | 6 | 2 | 1 | 4 | 2 | 1 | 7 | 2 | 0 | 1 | 169 |
| tickets | 1778 | 56 | 1 | 16 | 2 | 6 | 2 | 0 | 2 | 3 | 3 | 8 | 7 | 0 | 0 | 5 | 0 | 1 | 99 |
| **Summe (40 Apps)** | 135567 | **2418** | 46 | 354 | 541 | 285 | 139 | 177 | 119 | 87 | 105 | 157 | 188 | 40 | 61 | 78 | 2 | 39 | 6682 |

Summe aller 40 Apps **vor** dem knowledge-Port: 2418 + 98 = **2516 Stellen**, CSS ungescoped 6682 + 303 = 6985.

Neu messen: `node v4/tools/port/scan.mjs --md` (Tabelle), `--app <id>` (Fundliste mit Vorschlägen), `--json`.

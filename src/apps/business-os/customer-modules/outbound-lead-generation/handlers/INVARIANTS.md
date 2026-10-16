# outbound-lead-generation – Geschäfts-Invarianten als Handler-Tests (Strang INV, Shell v4)

Stand: 15.10.2026. Mit dem Rust-Code fallen die Wächter in `person_research_command.rs` und
`person_research_gap_closure.rs` weg (Rust-Stand `4794af03d`, nur gelesen; Liste aus
`docs/testaudit/business_os_b.tsv`, Spalte `protects`). Invarianten 1–4: `modules/outbound/handlers/INVARIANTS.md`.

**WARTET AUF OLG3.** `outbound.lead.research_writeback` ist noch eine Legacy-Weiterleitung (Port läuft in Strang
OLG3). Beide Tests prüfen deshalb die schon portierten `_lib`-Funktionen, über die der Handler die Invariante halten
wird. Sobald der Handler portiert ist, müssen dieselben Fälle über `handle(command, ctx)` gegen `tests/_ctx.mjs`
laufen (Lead-Datensatz, Terminalstatus `research_status`); danach die Mutationstabelle um die Handler-Zeile ergänzen.

- Tests: `tests/invariants.test.mjs`, je Invariante genau ein Test (`INV5`, `INV6`).
- Mutationsbeleg: `node tests/mutation-proof.mjs` (Temp-Kopie von `handlers/`, Arbeitsbaum bleibt unberührt).

| Inv. | Invariante | Rust-Vorbild (entfällt) | JS-Test | Mutation (Datei: Zeile vorher → nachher) | Beleg |
|---|---|---|---|---|---|
| 5 | Ein Beleg bestätigt nie eine andere Person | `historical_evidence_never_certifies_another_person_or_current_value` (person_research_command.rs:3445) | `INV5 evidence never certifies another person`: (a) gleicher Wert und Zitat, Beleg gehört Person A, Feld Person B → nicht verifiziert, `needs_review`, Kontakte unverändert; (b) Beleg ohne Personenbindung zählt 0; Gegenprobe: Beleg von Person B verifiziert | `_lib/research-evidence.js`: `if (evidenceKey !== personKey) return false;` → `if (false) return false;` | rot („evidence of person A must not verify person B“), zurück grün |
| 6 | Ein beleglos zurückgeschriebenes Ergebnis schließt keinen Lead ab | `a_writeback_without_any_evidence_is_rejected` (person_research_gap_closure.rs:5950, Gate :1707), `completed_research_requires_durable_lead_writeback` (person_research_command.rs:2636), Terminalregel :2032 | `INV6 an evidence-free writeback never completes a lead`: (a) nur `no_match` ohne Quellen/Versuche → abgelehnt; (b) Werte ohne Beleg → `needs_review`; (c) alter Beleg für alten Wert stützt neuen Wert nicht; (d) `verified` ohne Quelle passiert das grobe Gate, wird aber vom Sanitizer herabgestuft und als Ablehnung gezählt; Gegenprobe: eine passende zitierte Quelle → `completed` | `_lib/research-writeback-checks.js`: `if (!(anyVerified \|\| …)) {` → `if (false) {` · `_lib/research-outcome-patch.js`: `… && unverifiedFieldKeys.length === 0 ? 'completed'` → `… > 0 ? 'completed'` · `_lib/research-writeback-sanitize.js`: `if (unabhaengig < benoetigt) {` → `if (false) {` | alle drei rot, zurück grün |

Letzter Lauf (15.10.2026, Node 26.6): `node --test tests/invariants.test.mjs` → 2 pass;
`node --test tests/*.test.mjs` → 80 pass, 0 fail (Zahl wächst durch OLG3);
`node tests/mutation-proof.mjs` → 4/4 Mutationen rot, Baseline und Wiederherstellung grün, Exit 0.

## Hinweise

- `tests/_ctx.mjs` dieser App wurde nicht geändert (die Tests brauchen den Kontext erst mit dem portierten Handler).
  Die dortige `ctx.transaction` hat denselben Mangel wie die outbound-Attrappe vor der Ergänzung (paralleles
  verlorenes Update, Rollback über fremde Commits); vor einem Handler-Test mit parallelen Writebacks dieselbe
  Ergänzung übernehmen (Revision + `{code:'conflict'}`, siehe `modules/outbound/handlers/tests/_ctx.mjs`).
- Die Fälle (d) und die Mutationen auf `research-writeback-sanitize.js` / `research-outcome-patch.js` hängen an
  Dateien, die OLG3 gerade bearbeitet. Ändert OLG3 eine Zeile, meldet `mutation-proof.mjs` „MUSTER 0x gefunden“
  und endet mit Exit 1 – dann die Mutation auf die neue Zeile nachziehen, nicht löschen.
- Owner-Regel: `requiredIndependentSources` liefert 1 je Feld (Rust-Stand 23.09.); die Gegenprobe in INV6 nutzt
  genau eine Quelle. Wird die Regel vom 07.10. („Ziel 2 Quellen“) umgesetzt, muss die Gegenprobe zwei Quellen bekommen.
- Kein Befund im Sinne eines Invariantenbruchs: die portierten `_lib`-Funktionen halten 5 und 6. Offen ist allein
  der Handler-Weg (Terminalregel `!rejections.is_empty() || open_fields … → needs_review`), der mit OLG3 kommt.

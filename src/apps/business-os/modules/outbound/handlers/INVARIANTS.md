# outbound – Geschäfts-Invarianten als Handler-Tests (Strang INV, Shell v4)

Stand: 15.10.2026. Mit dem Rust-Code fallen die Wächter in `store_outbound_commands.rs` weg
(Rust-Stand `4794af03d`, nur gelesen; Liste aus `docs/testaudit/business_os_b.tsv`, Spalte `protects`).
Die Invarianten 1–4 leben hier als JS-Handler-Tests weiter, die Invarianten 5–6 in
`customer-modules/outbound-lead-generation/handlers/INVARIANTS.md`.

- Tests: `tests/invariants.test.mjs`, je Invariante genau ein Test (`INV1` … `INV4`) mit 2–4 Fällen.
- Mutationsbeleg: `node tests/mutation-proof.mjs` – bricht je Zeile eine Temp-Kopie von `handlers/`,
  erwartet den Invariantentest rot (kein Syntaxfehler), danach wieder grün; der Arbeitsbaum bleibt unberührt.
- Befunde liefen als `todo`-Tests (`BEFUND INV…`). INV1-B1 und INV3-B3 sind seit Strang FIXINV (15.10.2026) behoben;
  ihre Gegenbeispiele sind jetzt Fälle der echten Tests `INV1`/`INV3`. Als `todo` bleibt nur `BEFUND INV4` (bewusst belassen).

| Inv. | Invariante | Rust-Vorbild (entfällt) | JS-Test | Mutation (Datei: Zeile vorher → nachher) | Beleg |
|---|---|---|---|---|---|
| 1 | Versand nur bei aktueller freigegebener Revision | `outbound_message_send_approved_requires_matching_current_revision`, `outbound_send_approved_blocked_after_rejection_or_change_request` | `INV1 send only for the current approved revision`: (a) nie freigegeben, (b) Inhalt nach Freigabe geändert (neue Revision), (c) freigegeben und danach abgelehnt, (d) Inhalt nach Freigabe am Handler vorbei geändert, `revision_id` unverändert – je Fall Text, Betreff, Empfänger, Anhang (INV1-B1), (e) Bestands-Freigabe ohne `content_hash`: geänderter Inhalt gesperrt, unveränderter geht raus; Gegenprobe: freigegebene Fassung geht genau einmal raus | `_lib/send.js`: `if (!(await hasMatchingApproval(…))) {` → `if (false) {` · `_lib/send.js`: `if (str(message, ['approval_status']) !== 'approved') fail(` → `if (false) fail(` · FIXINV: `_lib/outbound.js` Hash-Vergleich `approvedHash === messageContentHash(message)` → `true`, Bestands-Vergleich `revision_id === messageRevision(message)` → `true`, `approvalCoversContent(…)` → `true`, Anhänge aus dem Hash entfernt; `outbound.message.approve.js`: `content_hash` nicht gesetzt | alle 7 rot, zurück grün |
| 2 | Kein doppelter Versand, auch bei Wiederholung/Crash | `outbound_send_approved_is_idempotent_for_already_queued_message`, `outbound_message_send_is_idempotent_after_queueing` | `INV2 no double send …`: (a) gleicher Versand zweimal, (b) Abbruch nach `mail.send` vor dem Speichern + Wiederholung, (c) zwei gleiche Befehle parallel | `outbound.message.send_approved.js`: `if (QUEUED.has(existingSendStatus) && …` → `if (false && …` · `idempotency_key: \`${messageId}:${revision}\`` → `` `${command.id}` `` | beide rot, zurück grün |
| 3 | Gesperrte/abgemeldete Empfänger nie anschreiben | `outbound_message_send_blocked_when_recipient_is_suppressed`, `outbound_unsubscribe_reply_creates_suppression_and_blocks_send`, `outbound_send_gate_blocks_bounce_and_unhealthy_sender` | `INV3 suppressed or unsubscribed …`: Altform Adresse (Groß-/Kleinschreibung) und Domain, Schemaform `scope:'email'` (Groß-/Kleinschreibung), `scope:'domain'` inkl. Subdomain (INV3-B3), Abmeldung per `outbound.reply.classify` nach der Freigabe; Gegenproben: `active:0` (Adresse, Domain) und Look-alike-Domain sperren nicht; der Antwortpfad schreibt nur die Schemaform | `_lib/send.js`: `if (reason != null) fail('conflict'` → `if (false) fail('conflict'` · `_lib/outbound.js`: Domain-Treffer → `if (false)` · FIXINV: Schemaform-Erkennung `scope==='email'` bzw. `'domain'` → `if (false)`, Subdomain-Treffer entfernt, `active` ignoriert, neuer Eintrag mit `status` statt `active` | alle 7 rot, zurück grün |
| 4 | Tageslimit auch bei parallelen Befehlen | `outbound_daily_limit_enforced_under_parallel_commands` | `INV4 daily limit holds under parallel send commands`: 6 freigegebene Mails, Limit 3, `Promise.all`, Konfliktverlierer werden erneut gesendet; nie > 3 zugestellt, Zähler ≤ 3 und ≥ zugestellt, Rest `quota` | `outbound.message.send_approved.js`: `await ctx.transaction((tx) => reserveAccountSendSlot(tx, …))` → `await reserveAccountSendSlot(ctx, …)` | rot („parallel burst overshot the cap: 6 > 3“), zurück grün |

Letzter Lauf (15.10.2026, Strang FIXINV, Node 26.6): `node --test tests/*.test.mjs` → 113 Tests, 112 pass, 0 fail,
1 todo (`BEFUND INV4`); `node tests/mutation-proof.mjs` → 17/17 Mutationen rot, Baseline und Wiederherstellung grün, Exit 0;
`CTOX_APP_API_STUB=~/.local/state/shell-v4/stub-pins/9fa9948 node --test ../tests/v4-api.test.mjs` → 9/9 pass.

## Ergänzung der Attrappe (`tests/_ctx.mjs`)

Mangel: `ctx.transaction` kopierte den ganzen Bestand und ersetzte ihn beim Commit; zwei parallele
Transaktionen überschrieben sich still (verlorenes Update), und ein Rollback stellte einen alten Stand
über fremde Commits. INV4 war damit nicht prüfbar. Minimal ergänzt, angelehnt an den echten handler-kit
(„concurrent modification rejects transaction snapshot rather than losing another write“, befund-app-laufzeit.md):
eine Store-Revision, die jeder Commit erhöht; eine Transaktion, deren Snapshot veraltet ist, scheitert mit
`{code:'conflict'}`; ein Fehler verwirft nur die Arbeitskopie. Alle 100 vorhandenen Tests bleiben grün.

## Voraussetzung außerhalb der App

INV2 (b)/(c) hält nur, weil `cap.mail.send` auf `idempotency_key` dedupliziert. Die Attrappe bildet das ab
(`mailbox()` im Test); der Capability-Strang muss diese Zusage im echten `cap.mail.send` geben und testen.
Ohne sie wird nach einem Abbruch zwischen `mail.send` und dem Speichern zweimal versendet.

## Befunde

- **INV1-B1 – Gate vertraut gespeicherter `revision_id`. BEHOBEN (Strang FIXINV, 15.10.2026), Beschreibung unten.** `_lib/send.js:64`
  `const revisionId = str(message, ['revision_id']) ?? messageRevision(message);` (Rust identisch
  `store_outbound_commands.rs:6917`). Gegenbeispiel (`BEFUND INV1` im Test): Nachricht freigeben, dann per direktem
  Schreibzugriff auf `outbound_messages` nur `body_text` ändern → `send_approved` versendet „Unapproved body“.
  Die Handler-Pfade (`update_draft`, `request_approval`) setzen die Revision neu; offen bleibt jeder Schreibweg
  außerhalb der Handler (Browser-Rechte auf `outbound_messages` im `_rights`-Modell). Fix-Vorschlag: im Gate die
  Revision aus dem Inhalt berechnen und mit gespeicherter Revision UND Freigabe vergleichen.
- **INV3-B3 – Sperrlisten-Einträge in Schemaform werden ignoriert. BEHOBEN (Strang FIXINV, 15.10.2026), Beschreibung unten.** (Hinweis Schema-Strang,
  `v4/schemas/REPORT.md`, outbound). `collections.schema.json` verlangt für `outbound_suppression_entries`
  `scope/value/active`; das Gate liest nur `status`, `email`/`recipient_email`, `domain`
  (`_lib/outbound.js:327-331`). Rust hatte dieselbe Lücke: `store_outbound_delivery_policy.rs:191-205`.
  Gegenbeispiel (`BEFUND INV3` im Test): `{scope:'email', value:'optout@acme.test', active:1}` und
  `{scope:'domain', value:'blocked.test', active:1}` → beide Empfänger bekommen die Mail. Dazu: der einzige
  UI-Schreiber (`active-outreach.js:1702`) schreibt die Altform `email/status` ohne die Pflichtfelder des Schemas;
  validiert v4 gegen das Schema, wird die Sperre gar nicht gespeichert, folgt die UI dem Schema, sieht das Gate sie nicht.
  Schema und Gate müssen sich auf eine Form einigen (Gate liest beide Formen, `active:0` = inaktiv).
- **INV4-B2 – Slot-Leck nach Commit-Konflikt** (kein Invariantenbruch). **Bewusst belassen** (Strang FIXINV): das Limit wird
  nie überschritten, nur unterschritten; `BEFUND INV4` bleibt als `todo`-Test stehen. Scheitert nach `mail.send` der
  Abschluss-Commit (`outbound.message.send_approved.js:120`, `ctx.transaction`) an einem Konflikt, bleibt die
  Nachricht `approved_not_sent`, der Slot ist verbraucht. Die Wiederholung reserviert einen zweiten Slot für
  dieselbe Mail: im Test zählt der Zähler 3 bei 2 zugestellten Mails (`BEFUND INV4`). Das Limit wird unterschritten,
  nie überschritten; Rust duldete dasselbe als „leaked-but-counted slot“.
- **Nebenbefund INV2 (c):** zwei gleiche parallele Sendebefehle – der Verlierer schreibt per `failWith` aus seiner
  veralteten Kopie `send_blocked` auf die Nachricht, während der Gewinner die Mail schon übergeben hat. Endzustand
  hängt von der Commit-Reihenfolge ab; zugestellt wird trotzdem genau eine Mail.

## Behebung (Strang FIXINV, 15.10.2026)

**INV1-B1 – Freigabe an den Inhalt gebunden.**
- `_lib/outbound.js` `messageContentHash(message)`: `content_<stableDigest>` über alle versandrelevanten Felder – Kanal,
  Absender (kanonischer Kontoschlüssel, damit die Kanonisierung im Versandpfad die Freigabe nicht entwertet), Empfänger
  (`recipient_email` normalisiert, `recipient_address_text`), Betreff, `body_text`, `body_html`, Briefdokument
  (`document_id`, `document_version_id`, `document_pdf_url`) und Anhänge (`attachments` bzw. `payload.attachments`).
- `outbound.message.approve.js` schreibt den Hash als `content_hash` auf die Freigabe, NACH der Kopie der Befehls-Payload
  (ein Befehl kann keinen eigenen Hash mitgeben).
- Gate (`_lib/send.js` → `hasMatchingApproval(tx, messageId, revisionId, message)` → `approvalCoversContent`): eine
  passende Freigabe muss zusätzlich den Inhalt decken, den der Handler gleich versendet. Freigaben mit `content_hash`:
  Hash-Gleichheit. Bestands-Freigaben ohne `content_hash`: `revision_id` der Freigabe gegen die aus dem Inhalt NEU
  berechnete Revision (nie gegen die gespeicherte). Abweichung → derselbe Fehler wie das bestehende Freigabe-Gate:
  `conflict`, „approved outbound message has no matching approval for current revision“, `send_block_reason`
  `approval_required`. Damit macht jede Inhaltsänderung nach der Freigabe die Freigabe ungültig, auch am Handler vorbei.

**INV3-B3 – eine Form für Sperrlisten-Einträge.**
- Geschrieben wird nur noch die Schemaform `{id, scope:'email'|'domain', value, reason, active:1, source, engagement_id?,
  created_at_ms, updated_at_ms}` (`suppressionEntry()` in `_lib/outbound.js`): UI `active-outreach.js`
  (`addSuppressionFromEngagement`, einzige UI-Anlagestelle) und Antwortpfad `applyReplySuppression`
  (`outbound.reply.classify`/`reply.match`). Rust schrieb dort EINEN Alteintrag mit `email` UND `domain`, den das Gate als
  Adress- plus Domain-Sperre las; dieselbe Wirkung in Schemaform sind zwei Einträge (`supp_reply_<…>` scope email,
  `supp_reply_domain_<digest(domain|reason)>` scope domain).
- Gate `recipientSuppressionReason` liest beide Formen: Schemaform (`scope/value`; `active` 0/false = inaktiv, wenn
  vorhanden) und Altform (`email`/`recipient_email`, `domain`, `status` inactive/deleted/expired). E-Mail exakt,
  ASCII-case-insensitive; Domain deckt die Empfänger-Domain und jede Subdomain (`eu.x.test` unter `x.test`, nicht
  `notx.test`), führendes `@`/`.` im Wert wird ignoriert. Adresstreffer gehen vor Domaintreffern (Grund wie bisher).
- Hinweis: der Subdomain-Treffer gilt einheitlich für beide Formen, also auch für Domain-Einträge im Altbestand (vorher
  nur exakte Domain). Strenger, nie lockerer.


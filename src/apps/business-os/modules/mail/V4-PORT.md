# mail – Port auf App-API 4.0 (Strang UD7, Welle D)

Stand 15.10.2026, Berlin-Zeit gemessen. Erster Lauf brach ~14:40 (Sitzungslimit) mitten im Port ab; Wiederaufnahme
15:06, fertig 15:50. Stub-Pin `~/.local/state/shell-v4/stub-pins/cff5ba5` (`CTOX_APP_API_STUB`). Kein git, keine Subagents.
Handler (`handlers/ctox.mailserver.*`, Strang H8) nicht angefasst.

## Ergebnis (gemessen)

| Prüfung | Befehl | Ergebnis |
|---|---|---|
| Port-Scanner | `node v4/tools/port/scan.mjs --app mail` | Baseline **75 Stellen + 438 CSS-Selektoren** (SCAN-BASELINE.md) → bei Wiederaufnahme 2 + 438 → **0 Stellen, 0 ungescoped**, Exit 0 |
| CSS-Scope | `scope-css.mjs mail modules/mail/index.css --check` und `… editor/editor.css --check` | beide gescoped |
| Löschlisten-Wächter | `node v4/tools/guards/guard-v4.mjs --json --app modules/mail` | bei Wiederaufnahme 29 Treffer → **0** (Testdateien separat: 20, davon 17 in `tests/mail-browser-qa.mjs`, siehe OFFEN mail-8) |
| v4-Test gegen Kern-Stub | `CTOX_APP_API_STUB=… node --test modules/mail/tests/v4-api.test.mjs` | **8/8 grün** (+1 Screenshot-Test mit `MAIL_SHOTS=1`: 9/9) |
| Bestehende Suiten (umgestellt) | `node --test modules/mail/tests/mail.test.mjs modules/mail/tests/mail-body-renderer-parent.test.mjs modules/mail/editor/tests/*.test.mjs modules/mail/handlers/tests/*.test.mjs` | **52/52 grün** (esbuild/playwright aus `~/Documents/ctox/src/apps/business-os/node_modules` per Resolve-Hook; der Klon hat kein `node_modules`) |
| Screenshots | `tests/shots/mail-1280.png`, `mail-420.png`, `mail-420-list.png` | angesehen: 1280 drei Spalten (Ordner · Liste · Thread), 420 einspaltig, Inspektor als Blatt im App-Fenster, Liste ohne Überlauf (≤ 1 px gemessen) |

v4-Test deckt ab: mount zeigt Live-Daten (Thread + Nachrichtentext im Inspektor), Titel über `api.shell.setTitle`, je
Collection genau ein Abo (11) · `sim.push` erscheint live, `_deleted` verschwindet · leere Collections: Liste und
Inspektor leer, keine `.ctox-empty`/`role=status`/`aria-busy`, keine Zustandswörter, keine Zone · Schreiben: neue
E-Mail-Gruppe → `api.put` nach `outbound_campaigns`, live in der Gruppenliste · Befehl: Entwurf speichern →
`outbound.engagement.create` + `outbound.message.prepare` ohne Warten, `sim.failCommand` → genau ein Fehler in der Zone
(`mail.command.outbound.message.prepare`, `ref` = Befehls-ID) · `done` → Erfolgsmeldung `mail.notice` erst dann
(Mailserver-Domain), kein `get_config`-Leseweg · snapshot → structuredClone → Remount mit `restored` erhält Auswahl
(Thread), Ansicht und den offenen Composer-Entwurf; alte Abos abgemeldet · unmount: alle 11 Abos 0, root leer, keine
`pageerror`.

## Bereiche (Reihenfolge der Arbeit)

1. **Einstieg** `index.js`: `mount(host, api, {restored})` → `{unmount, snapshot}`; `api.shell.setTitle('Mail')`,
   `api.setVisible([communication_accounts, communication_threads, communication_messages, outbound_messages])`, Markup
   über `fetch(new URL('./index.html', import.meta.url))` ohne `?v=`; `STYLE_BUILD`/`ensureStyles` entfernt (Shell lädt
   CSS aus dem Manifest). Pane-Grammatik verdrahtet die App selbst (`autoWirePaneGrammar(root)` aus
   `ctox:shared/pane-grammar.js`, PORTING R14) – vorher tat das die V2-Shell per DOM-Heuristik.
2. **Schleifen raus** (PORTING R12): `setInterval(revalidateAccounts, 15_000)` (Konten-Revalidierung), `waitForCommand`
   mit `sleep(250)`-Poll bis 45 s und `MAIL_READ_TIMEOUT_MS = 10_000` (Lese-Timeout mit AbortController) entfallen
   ersatzlos; Fokus-Neuladen ebenso. Einziger Timer: 120-ms-Entprellung der Live-Emissionen mehrerer Collections
   (`scheduleRefresh`, läuft nur nach einer Emission, bei unmount gelöscht).
3. **Daten nur aus Collections** (PORTING R3/R4): je Montage 11 `api.query(name, {})`-Abos, Lesen synchron aus dem
   Live-Stand (`liveRows`). Entfallen: `readNativeCollectionDocument`-Gegenlesepfad für Konten
   (`authoritativeVisibleEmailAccounts`), `isTransientCollectionReadError`, Readiness (`view.readiness`,
   `collectionReadiness`), `projectCommandResult` (schrieb Befehlsergebnisse der outbound-Befehle selbst in
   `outbound_engagements/messages/approvals` – das projizieren jetzt die outbound-Handler) und der Befehls-Leseweg
   `commandOutcome(get_config)` für die Mailserver-Verwaltung (→ OFFEN mail-1). Kontosicht folgt der Projektion
   `communication_accounts` (Core entzieht Zugriff durch Änderung/Löschung der Projektion).
4. **Befehle ohne Warten** (PORTING R5): `dispatchCommand`/`sendCommand` → `api.command(type, {...payload, record_id,
   client_context})`, sofortige Rückkehr. `failed` → `api.status.report({level:'error', code:'mail.command.<type>',
   ref: <id>})`, `done` → `clear(code)`; Erfolgsmeldungen, die vorher nach dem Warten auf den Terminalstatus kamen,
   gehen jetzt bei `done` als `info` (`mail.notice`) in die Zone (`doneText`/`onDone`). Mehrteilige Abläufe (Composer:
   engagement.create → message.prepare → request_approval → approve → send_approved; Sammelaktionen; Routing in
   Blöcken) werden in fester Reihenfolge abgesetzt, die Meldung hängt am letzten Befehl (→ OFFEN mail-3).
5. **Zustandstexte raus** (PORTING R11): `index.html` ohne Lesefehler-Kasten (`data-mail-read-error` + „Erneut
   versuchen“), ohne Leer-Kästen (`data-mail-list-empty`, `data-mail-detail-empty`), ohne `role="status"`-Bereiche
   (Composer, Mailserver, Routing), ohne „Serverstatus wird geladen“. JS ohne „Wird aktualisiert…“, Leerhinweise
   („Noch keine E-Mail-Gruppen“, „Keine passenden Ordner“, „Keine Mail ausgewählt“, „Keine Domain konfiguriert“,
   „Kein E-Mail-Postfach verbunden“), „Mailserver läuft/startet oder ist beeinträchtigt/gestoppt“ (Laufzustand jetzt
   nur Punkt `is-running/is-degraded` + Schaltknopf, Beeinträchtigung → `mail.mailserver.health` warn), Import-Label
   „Empfänger werden geprüft…“. Validierungs-/Fehlerhinweise → Zone (`mail.composer`, `mail.route`,
   `mail.group.validation`, `mail.mailserver`, `mail.mailserver.denied`, `mail.action.failed`, `mail.read`,
   `mail.write.<collection>`, `mail.focus.unavailable`). Locales ohne `emptyTitle/emptyBody/syncingTitle/syncingBody/
   refreshing/loadingMailboxes/noSelection*/noGroups/noMailbox/noDomains/noManagedMailboxes`.
   **`// v4-allow: label`** an fachlichen Datensatzstatus-Beschriftungen (kein App-Zustand): Ordnerbeschreibung
   „Wartet auf Freigabe“, Versandschritte je Nachricht („Freigabe ausstehend“, „Versand ausstehend“, „Zustellung
   ausstehend“), Zustell-Zeitleiste („Freigabe ausstehend“, „Zustellung läuft“ / „Wartet auf das terminale
   SMTP-Ergebnis“) – das ist der Status des Datensatzes `outbound_messages`, Fachverhalten unverändert.
6. **Fenster-/Navigationszustand** (PORTING R13): Spaltenbreiten, Konto, Ordner/Gruppe, Auswahl, Seite, beide
   Pane-Grammatiken und der offene Composer-Entwurf (`composeFrom/To/Campaign/Subject/Body`, explizit über refs – die
   Felder tragen nur data-Attribute) über `snapshot()`/`restored` statt `ctx.storageScope`. Deep-Link nur über
   `api.shell.onRoute` (vorher `location.hash`); „In Outbound öffnen“ über `api.shell.navigate('outbound', …)`.
7. **Rechte** (PORTING R9): `api.actor.{id, role, display_name}` nur als Spiegel (Admin-Ansicht der
   Mailserver-Verwaltung, Kontosicht); `assertCanWriteCampaigns` prüft nichts mehr im UI (Core entscheidet, `api.put`
   → `forbidden` → Zone).
8. **Editor** `editor/mail-content-editor.mjs`: `options.ctx` → `options.api`; `ensureStyles`/`STYLE_REVISION` und
   `?v=` am office-engine-Import entfernt; Word-Sprung `openDocumentsArtifact(api, …)` über
   `api.shell.navigate('documents', {record, record_id, documentId, version…})` statt `ctx.openDesktopApp`;
   „HTML-Editor wird geladen…“ entfernt. Test auf v4 umgestellt.
9. **CSS**: `index.css` (352 Selektoren, 34 mit Shell-Vorfahr `.shell-window[data-shell-contract="v2"]`) und
   `editor/editor.css` (86) unter `[data-app="mail"]` (scope-css). Layoutfehler aus dem Screenshot behoben: die
   ausgeblendete Sammelleiste (`hidden` → kein Grid-Platz) verschob Liste und Fuß der Listenspalte in die falschen
   Zeilen (Fuß stand mitten im Fenster) → feste `grid-row` 3/4/5.
10. **schema.js gelöscht** (R17): Collection-Namen (14), `version`, `required`, `properties` gemessen gleich zu
    `collections.schema.json` (Quellen: conversations, ctox, app-store, documents/collections.schema.json). Abweichung:
    die in schema.js mitgeführten Migrationen (`business_commands` 1/2: `inbound_channel` aus `module`;
    `outbound_messages` 1 aus conversations) stehen nicht in `collections.schema.json` – sie gehören den Quell-Apps
    (Plattform bzw. conversations/outbound), nicht mail. schema.js war zudem kaputt (`documents/schema.js` existiert
    nicht mehr).
11. **Tests**: `tests/mail.test.mjs` auf v4 umgestellt (Schema-Gleichheit gegen die JSON-Quellen statt
    Referenzgleichheit der JS-Schemata; Readiness-Fall entfernt; nativer Gegenlesepfad → Projektionstest;
    `commandOutcome` muss fehlen; Icon-Test ohne Shell-Provider; Quellwächter: `mount(host, api…)`, kein
    `setInterval/readNativeCollectionDocument/commandBus/waitForCommand/ctx.`, keine Leer-/Status-Bereiche im HTML;
    esbuild-Plugin für `ctox:shared/`). Neu `tests/v4-api.test.mjs`.

## Befehlstypen – Einstufung (§16.13)

Rust-Bezüge: `grep -rn '"<type>"' src/core --include='*.rs' | wc -l` im Klon (15.10. 15:45).

| Befehlstyp | Rust | Einstufung | Ziel |
|---|---:|---|---|
| `ctox.mailserver.get_config` | 5 | Fähigkeit `cap.mail.server.config.get` | Handler `handlers/ctox.mailserver.get_config.js` (H8, fertig); UI ruft ihn nicht mehr (kein Befehls-Leseweg, OFFEN mail-1) |
| `ctox.mailserver.save_runtime` | 3 | Fähigkeit (+ Prüfregeln) | Handler fertig (H8) |
| `ctox.mailserver.save_domain` | 4 | Fähigkeit (+ Vorgaben) | Handler fertig (H8) |
| `ctox.mailserver.delete_domain` | 3 | Fähigkeit | Handler fertig (H8) |
| `ctox.mailserver.save_user` | 6 | Fähigkeit (+ Vorgaben, Passwort per Secret-Intake) | Handler fertig (H8) |
| `ctox.mailserver.delete_user` | 3 | Fähigkeit | Handler fertig (H8) |
| `outbound.engagement.create` | 29 | Fachlogik (outbound) | `modules/outbound/handlers/` (H3) |
| `outbound.message.prepare` | 25 | Fachlogik (outbound) | `modules/outbound/handlers/` (H3) |
| `outbound.message.request_approval` | 20 | Fachlogik (outbound) | `modules/outbound/handlers/` (H3) |
| `outbound.message.approve` | 22 | Fachlogik (outbound) | `modules/outbound/handlers/` (H3) |
| `outbound.message.send_approved` | 25 | Fachlogik + Fähigkeit Mail-Transport (outbound) | `modules/outbound/handlers/` (H3) |
| `outbound.provider.reconcile` | 2 | Fähigkeit Mail-Transport (outbound) | `modules/outbound/handlers/` (H3) |
| `outbound.campaign.status.set` | 5 | Fachlogik (outbound) | `modules/outbound/handlers/` (H3) |
| `support.conversation.open_from_thread` | 9 | Fachlogik (support) | `modules/support/handlers/` |
| `business_os.chat.task` | 154 | Core-generisch (Queue) | bleibt (Routing an andere Apps) |
| `mail.series_email.import` | 0 | – | wird nicht gesendet (`dispatch:false`, Import bleibt lokal im Composer) |

## OFFEN (mail)

1. **→ erledigt (MAIL2 D, s. Nachtrag).** ~~Mailserver-Verwaltung ohne Collection~~: Domains (DKIM/SPF/DMARC), Postfächer, Laufzeitkonfiguration,
   Laufzustand und Erreichbarkeit kamen nur aus dem Ergebnis von `ctox.mailserver.get_config` (Befehls-Leseweg). Ohne
   Collection bleiben die Listen leer; Mutationen (`save_runtime/save_domain/delete_domain/save_user/delete_user`)
   laufen als Befehle. „Neu laden“ ist ausgeblendet. **Vorschlag**: Core/Handler projiziert in eine Collection
   `mail_server_state` (Plattform-Projektion, nur Rolle admin/`SecretsManage`), Dokumente je Art:
   `{id:'runtime', kind:'runtime', config:{enabled, hostname, bind_host, smtp_port, imap_port,
   outbound_throttle_per_min, max_connections}, status:{state}, health:{smtp_reachable,
   imap_reachable, queue_pending, delivery_success, delivery_failed}, updated_at_ms}`,
   `{id:'domain:<name>', kind:'domain', domain_name, dkim_selector, dkim_public_key, spf_record, dmarc_record,
   updated_at_ms}`, `{id:'user:<address>', kind:'user', username, owner_user_id, communication_account_key,
   updated_at_ms}` (nie Passwörter, nie DKIM-Privatschlüssel). Die UI liest dann `api.query('mail_server_state', {})`
   in `view.mailserver.{runtimeConfig, runtimeStatus, health, domains, users}` – Render-Code steht schon.
2. **Konten-Autorität**: Der frühere native Gegenlesepfad bestätigte jedes Konto vor dem Anzeigen. In v4 ist die
   lokale Projektion `communication_accounts` die Wahrheit; der Core muss Konten, auf die ein Nutzer keinen Zugriff
   (mehr) hat, aus dessen Projektion entfernen (Bereiche/`scope_revoke`, im Stub nicht gebaut, README §„Bewusst nicht
   gebaut“). Die UI filtert zusätzlich wie bisher nach `owner_user_id`/`shared_user_ids` (nur Darstellung).
3. **→ erledigt (MAIL2 A, s. Nachtrag).** ~~Abhängige Befehlsketten~~: Composer (`engagement.create → message.prepare → [request_approval → approve →
   send_approved]` je Empfänger) lief vorher strikt nacheinander mit Warten auf Abschluss; jetzt in Reihenfolge
   abgesetzt, ohne Warten. Ob der Core Befehle einer App in Einreichungsreihenfolge abarbeitet, legt der Vertrag nicht
   fest. **Vorschlag**: Sammelbefehl `outbound.message.compose {campaign_id, sender_account_id, recipients[], subject,
   body_text, body_html, mode:'draft'|'approval'|'send', content_revision_id}` im outbound-Handler-Strang.
4. **→ erledigt bis auf Befund B1 (MAIL2 B, s. Nachtrag).** ~~Word-Inhalt einfrieren~~: `editor/mail-content-editor.mjs` → `createBusinessOsOfficeBridge(…).freezeEmailContent`
   (office-engine) ist v3 (ctx.db, Chunk-Lease, Befehl mit Ergebnis-Leseweg). Ohne v4-ctx schlägt „Rich Text · Word“
   speichern fehl (Fehler landet in der Zone, nichts wird geschrieben). Vorschlag: Befehl
   `documents.freeze_email_content {document_id, version_id}`, Handler schreibt das Ergebnis in die Collection
   `mail_content_artifacts {id, document_id, version_id, html_ref, text_ref, assets[{content_id, ref, sha256,
   mime_type}], updated_at_ms}`; Blobs über `api.files.get(ref)`. Gehört dem office-engine-/documents-Strang.
5. **App-übergreifende relative Importe**: `editor/` lädt `../../../office-engine/src/business-os-bridge.mjs` und
   (dynamisch) `../../../vendor/easy-email-editor/index.mjs`. Bei getrennter Auslieferung `/~a/<app_id>@<ver>/…`
   (§16.15) brechen diese Pfade; braucht `ctox:shared/…`-Ausweis oder Kopie ins App-Artefakt (Shell-/SW-Strang).
   `ctox:shared/universal-importer.js` ist selbst noch v3 (`ctx.host/ctx.sync`); mail übergibt nur `{host: root}`
   und `dispatch:false`.
6. **→ erledigt (MAIL2 C, s. Nachtrag).** ~~Primärschlüssel ≠ `id`~~: `communication_accounts` (`account_key`), `communication_threads` (`thread_key`),
   `communication_messages` (`message_key`) haben laut Schema einen anderen Primärschlüssel; die App-API 4.0
   adressiert Dokumente über `id`. Test-Fixtures tragen beides. Manifest-/Projektionsstrang muss `id` festlegen
   (Vorschlag: `id` = bisheriger Schlüssel). Die UI liest Schlüsselfelder, nur der Nachrichtentext-Mount nutzt
   `message.id`.
7. **Quellfokus-Rückmeldung**: `reportRequestedSourceFocus` sendet weiterhin das DOM-Ereignis
   `ctox-business-os-record-focus` (Rückmeldung an den Chat-Thread, `return_thread_id`). In v4 gibt es dafür keinen
   Shell-Kanal; das Ereignis bleibt im App-root. Vorschlag: Rückweg über `api.shell.navigate(<quelle>, {focus_result})`
   oder eine Plattform-Collection – Shell-Strang.
8. **`tests/mail-browser-qa.mjs`** (1168 Zeilen, Standalone-Playwright) ist ein v3-Harness (`mount(ctx)`,
   `commandBus`, Readiness „Mail wird synchronisiert“, native Kontoprüfung „Postfach derzeit nicht verfügbar“) und läuft
   gegen den v4-Mount nicht mehr. Readiness- und Native-Szenarien entfallen ersatzlos (Zustand jetzt Zone, keine
   native Prüfung); Icon-Abnahme, Editor-Themes/-Viewports und Composer-Abläufe sind nach `tests/v4-api.test.mjs`
   zu überführen (nicht in diesem Lauf geschafft; Guard-Testtreffer 17 stammen von dort).
9. **Manifest** (nicht angefasst): `styles` muss `index.css` **und** `editor/editor.css` enthalten (der Editor lud
   seine CSS bisher selbst); `foreign_collections` für alle 11 gelesenen Collections (communication_*, outbound_*,
   business_users, business_module_catalog); `handlers` (6× `ctox.mailserver.*`) und `capabilities_required`
   (`mail.server.*`) → Strang v4/manifests.
10. **Container-Namen**: Spalten-Umbruch hängt an `@container business-app-window` (1024/768) und `mail-window` –
    die v4-Shell muss den Fensterrahmen `business-app-window` nennen (im Test nachgebildet), wie matching OFFEN 4.
11. **Kopfzeile links**: der linke Pane-Kopf reserviert Platz für die V2-Fensterknöpfe (sichtbar im 1280-Screenshot);
    in v4 liegt die Fensterleiste außerhalb von root – Geometrie-Labor (Shell-Strang), wie matching OFFEN 5.
12. Befund (vorbestehend, nicht geändert): `t('approved')`/`t('queued')` liefern im Status-Label der Nachricht die
    Satzform aus `FALLBACK_LABELS` („Nachricht wurde freigegeben.“) statt des Kurzlabels – Fachverhalten unverändert
    gelassen.

## Nachtrag MAIL2 (15.10.2026, Folgepunkte OFFEN 1, 3, 4, 6)

| Prüfung | Befehl | Ergebnis |
|---|---|---|
| v4-Test gegen Kern-Stub | `CTOX_APP_API_STUB=~/.local/state/shell-v4/stub-pins/cff5ba5 node --test modules/mail/tests/v4-api.test.mjs` | **8/8 grün** (+ Screenshots mit `MAIL_SHOTS=1`: 9/9, neu `tests/shots/mail-1280-settings.png`, angesehen: Einstellungen gefüllt) |
| outbound-Handler | `cd modules/outbound/handlers && node --test tests/*.test.mjs` | Exit 0, 115 Tests (112 grün, 3 todo = INV-Befunde des Invarianten-Strangs), davon 8 neu `outbound.message.compose.test.mjs` |
| Schemata | `node v4/schemas/build-schemas.mjs` + `node v4/schemas/check-schemas.mjs` | 4448 grün, 0 rot, 16 Generationen mit Migration (vorher 13) |
| Port-Scanner | `node v4/tools/port/scan.mjs --app mail` | 0 Stellen, CSS ungescoped 0 |
| Bestandssuiten | `node --test modules/mail/tests/mail.test.mjs modules/mail/tests/mail-body-renderer-parent.test.mjs modules/mail/editor/tests/*.test.mjs` (esbuild per Resolve-Hook) | 51/51 grün |

- **A Verfasskette → EIN Befehl.** `submitComposer` sendet nur noch `outbound.message.compose` (`buildComposePayload`:
  `{mode, campaign_id, sender_account_id, subject, body_text, body_html, content_revision_id, created_at_ms,
  recipients:[{recipient_email, engagement_id, message_id}]}` + wie jeder Befehl `record_id`/`client_context`); Ids
  erzeugt die UI weiter selbst (Auswahl der letzten Nachricht). Erfolgsmeldung bei `done`, Fehler genau einmal in die Zone
  (`mail.command.outbound.message.compose`, `ref` = Befehls-ID). Handler + Test + Beschreibung:
  `modules/outbound/handlers/outbound.message.compose.js`, `tests/outbound.message.compose.test.mjs`, outbound
  `V4-HANDLERS.md` „Nachtrag MAIL2“ (Ergebnis je Modus identisch zur Einzelkette; Datensätze aller Empfänger in einer
  Transaktion; Versand nach dem Commit). `buildComposeCommandBundle` entfällt.
- **B Word-Inhalt** *(durch MAIL3 ersetzt, s. Nachtrag MAIL3; Text unten = historische Referenz).* `editor/mail-content-editor.mjs` importiert die Office-Brücke nicht mehr (auch OFFEN 5 für diesen
  Pfad erledigt). `freezeEmailContentViaCommand(api, {recordId, versionId})` sendet
  `office.document.freeze_email_content {document_id, version_id}` (opts `record_id`), wartet push-basiert auf den
  Status (kein Polling/Timeout), nimmt bei `done` `result.artifact` und lädt HTML/Text inhaltsadressiert über
  `api.files.get(<sha256>)`. Ref im Umschlag: `{storage:'files', ref:<sha256>, blob_id, media_type, sha256}` statt
  `document_blob_chunks`. `failed` → Fehler mit Befehls-ID → Zone `mail.command.office.document.freeze_email_content`.
  Tests: 4 neue Fälle in `editor/tests/mail-content-editor.test.mjs`. **Befund B1 (documents-Strang):**
  `modules/documents/handlers/_lib/office.js:83` (`loadOfficeTarget`) verlangt `command.app_id === 'documents'`; ein
  Befehl aus mail (`app_id:'mail'`) scheitert dort mit `invalid` („office command module mismatch“). Für den
  App-übergreifenden Aufruf muss der Handler `app_id` als Absender verstehen (Prüfung auf `binding.moduleId` nur noch
  gegen den Befehlstyp) – nicht in diesem Strang geändert. **Befund B2:** Das Ergebnis kommt über
  `CommandState.result` (Vertrag `index.d.ts:83-88`); der Handler stempelt es zusätzlich auf `document_versions`.
- **C Schlüssel.** `v4/schemas/conversations.collections.schema.json`: `communication_accounts|threads|messages` haben
  `primaryKey: 'id'` (string, maxLength 256, Pflicht), `*_key` bleibt Pflicht- und Indexfeld (`indexes`), `version` 0→1.
  Beleg: die Projektion schreibt `id` = Schlüssel seit der Basis (`src/core/mission/channels/business_os_projection.rs:541,
  :657, :775 @4794af03d`). Migration: A9 `migrate_fn` `migrations/<collection>.js` (`id ← *_key`, vorhandene id bleibt)
  und RxDB-Strategie v1 `set_from_first_truthy {field:'id', paths:['id', '<key>']}`. **Abweichung vom Auftrag
  („deklarative steps“):** die A9-Schrittsprache des Stubs (`index.d.ts:276-280`: rename/add/remove/cast) kennt keine
  Kopie – `rename` würde `*_key` löschen – und `check-schemas.mjs` lehnt nicht-leere `steps` ab („im Check nicht
  nachgebildet“); deklarativ ist die Kopie daher nur in der RxDB-Strategie. Generator: Tabelle `FIXES.conversations`
  (+ Ops `primaryKey`, `requiredAdd`, `indexes`), `KEY_COPY`/`RXDB_OPS` in `build-schemas.mjs`. Hinweis: der Lauf
  übernimmt auch fremde, bereits vorliegende Eingaben ins Ergebnis (`ctox-platform`: `app_drafts`, `mail_server_state`
  aus dem Plattformmanifest des Kerns). mail-UI: Identität überall über `id` (Konten-Auswahl/Absender, Thread-Zeilen
  `data-mail-record-id`/`data-context-record-id`, Auswahl, Sammelschlüssel, Route-Snapshot, Thread↔Nachricht über
  `message.thread_key === thread.id`); `*_key` nur noch als Fremdschlüssel (`thread.account_key`,
  `message.thread_key`). Fixtures in `tests/mail.test.mjs` tragen `id`.
- **D mail_server_state.** Collection im Abo (12 statt 11 Abos je Montage; ohne Recht → forbidden → Verwaltung leer).
  `mailserverStateFromRows` bildet die Plattformform `{id, kind, name, status, data}` (Plattformmanifest, Kern §16.28)
  auf `view.mailserver.{runtimeConfig, runtimeStatus, health, domains, users}` ab (runtime: `status`, `data.config`,
  `data.health`; domain: `name` + DNS-Werte in `data`; user: `name` + `data.owner_user_id`). Fixture im v4-Test (runtime,
  Domain, Postfach); Test prüft Domain/DKIM/SPF, Postfach, Servername, Port und Queue-Zähler in den Einstellungen.
- **Offen für andere Stränge:** Manifest (mail: `foreign_collections` + `mail_server_state`; outbound: Handler
  `outbound.message.compose`); Befund B1 (documents); Kern: Schreiber der `mail_server_state`-Dokumente in genau der
  oben genannten Form (`data.config/health` für runtime) – die Form ist hier aus dem Plattformschema abgeleitet, nicht
  aus einem Kern-Schreiber belegt.


## Nachtrag MAIL3 – Word-Inhalt ohne documents-Abhängigkeit (15.10.2026)

Ersetzt MAIL2 B samt Befund B1. Der Editor sendet den mail-eigenen Befehl `mail.content.freeze` (Konstante
`MAIL_CONTENT_FREEZE_COMMAND`, Funktion `freezeWordContentViaCommand(api, {source, documentId, versionId})`), Payload
`{source: BlobRef, document_id?, version_id?}`, ohne `record_id` (kein Bezug auf einen Fremd-Datensatz). Der Handler
`handlers/mail.content.freeze.js` friert nur über OS-Primitive ein (`office.edit.prepare`, `office.render` to
`email_html`, `blob.materialize`); Beschreibung, Signaturen, Tests und Mutationsbelege: `V4-HANDLERS.md` „Nachtrag MAIL3“.
`modules/documents` unverändert.

- Editor: `normalizeDocumentArtifact` übernimmt optional `source`/`source_blob` als BlobRef (`normalizeSourceBlobRef`);
  `serialize()` nimmt `artifact.html`/`artifact.text` als `compiledHtmlRef`/`compiledTextRef` (files-Referenzen
  `{storage:'files', ref, sha256, size, media_type, blob}`), lädt Inhalt über `api.files.get(ref)`. Ohne `source` kein
  Befehl, Fehler `invalid` → Zone `mail.action.failed`; `failed` → Zone `mail.command.mail.content.freeze` (`ref` =
  Befehls-ID). Einzige Änderung in `index.js` dafür: dieser Zonencode (`index.js:1368-1369`).
- Prüfungen: `node --test modules/mail/tests/v4-api.test.mjs` (Stub 9fa9948) 8 grün + 1 Skip (Screenshots);
  `node --test modules/mail/editor/tests/*.test.mjs` 22/22 grün (nach der parallelen Verlagerung von
  `vendor/easy-email-editor/` nach `modules/mail/vendor/` durch einen anderen Strang; dazwischen kurz 1 rot wegen der
  gelöschten Altablage, nicht durch MAIL3). `handlers/tests/*.test.mjs` 13 grün; `tests/mail.test.mjs` + `mail-body-renderer-parent.test.mjs`
  32 grün. Der Klon hat kein `node_modules`: playwright/esbuild per Resolve-Hook aus `~/Documents/ctox/src/apps/business-os`.
- `grep -rn freeze_email_content modules/mail`: nur Kommentare, diese Doku-Referenzen und Negativ-Assertions in Tests.

## VENDOR (Strang VENDOR, 15.10.2026)
- `easy-email-editor/` liegt in `modules/mail/vendor/`; `editor/mail-content-editor.mjs` lädt `../vendor/easy-email-editor/index.mjs`; Build-Ziel in `src/scripts/vendor-builds/easy-email-editor/build-upstream.mjs` umgestellt.

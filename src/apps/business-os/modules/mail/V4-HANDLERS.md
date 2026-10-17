# mail – v4-Handler (Strang H8, Shell v4)

Stand 15.10.2026. Quelle der Einstufung: `~/.local/state/shell-v4/analysis/E-befehlstypen.md` (§2 Tabelle, §3 Capabilities),
Trennregel Architektur §16.13, Handler-Vertrag §16.16. Rust nur gelesen: `~/.local/state/shell-v4-src/4794af03d/src/core/business_os/`.
Handler-Vertrag: `handlers/<command_type>.js`, `export default async function handle(command, ctx)`; Fehler `throw {code, message}`
(`invalid|forbidden|not_found|conflict|revoked|expired|quota|unavailable`). Tests: `node --test handlers/tests/*.test.mjs`
(In-Memory-ctx `handlers/tests/_ctx.mjs`: eigene Collections = v4-Manifest `collections`, Fremdzugriff = Manifest
`foreign_collections` + hier beantragte Ergänzungen; Capabilities protokolliert und je Test beantwortbar).

**Stand: 7 von 7 Befehlstypen als JS-Handler (6 Mailserver + `mail.content.freeze`, Strang MAIL3), 0 über
`ctx.cap.legacy.<command_type>`, 0 PLATTFORM (nur dokumentiert). Tests: `node --test handlers/tests/*.test.mjs` 13 grün.**

## Befehlstypen

| # | Befehlstyp | Einstufung | Rust-Ort | Ziel | Stand |
|---|---|---|---|---|---|
| 1 | `ctox.mailserver.get_config` | FAEHIGKEIT | `store.rs:16415-16518` | `handlers/ctox.mailserver.get_config.js` → `cap.mail.server.config.get` | portiert |
| 2 | `ctox.mailserver.save_runtime` | FAEHIGKEIT (+ Prüfregeln) | `store.rs:16519-16634` | `handlers/ctox.mailserver.save_runtime.js` → `cap.mail.server.runtime.save` | portiert |
| 3 | `ctox.mailserver.save_domain` | FAEHIGKEIT (+ Vorgaben) | `store.rs:16635-16730` | `handlers/ctox.mailserver.save_domain.js` → `cap.mail.server.domain.save` | portiert |
| 4 | `ctox.mailserver.delete_domain` | FAEHIGKEIT | `store.rs:16731-16771` | `handlers/ctox.mailserver.delete_domain.js` → `cap.mail.server.domain.delete` | portiert (MAIL3: Kernroute) |
| 5 | `ctox.mailserver.save_user` | FAEHIGKEIT (+ Vorgaben) | `store.rs:16772-16851` | `handlers/ctox.mailserver.save_user.js` → `cap.mail.server.user.save` | portiert |
| 6 | `ctox.mailserver.delete_user` | FAEHIGKEIT | `store.rs:16852-16899` | `handlers/ctox.mailserver.delete_user.js` → `cap.mail.server.user.delete` | portiert (MAIL3: Kernroute) |
| 7 | `mail.content.freeze` | GEMISCHT (neu, MAIL3) | fachlich wie `store_office_commands.rs:324-430` | `handlers/mail.content.freeze.js` → `cap.office.edit.prepare`, `cap.office.render`, `cap.blob.materialize` | neu, 7 Tests |

Fachlogik im Handler: Rückfall auf aktuelle Laufzeitwerte und alle Prüfregeln von `save_runtime`, Vorgaben für
DKIM-Selektor/SPF/DMARC, `owner_user_id`-Rückfall, Postfachadresse/Konto-Schlüssel. Im Kern bleiben: Mailserver-Store
(`stalwart_*`), DKIM-Erzeugung, `stalwart_config`-Prüfung, Anwenden der Laufzeit, TCP-Erreichbarkeitsprobe,
Kommunikationskonto, Rechte `SecretsManage`. **Passwörter** (save_user) kommen weiter aus dem Secret-Intake des Befehls;
der Handler sieht sie nie (Test prüft, dass ein Payload-`password` nicht weitergereicht wird).

**Nicht hier:** Die übrigen Sender der Mail-UI gehören anderen Apps: `outbound.*` (Strang H3, `modules/outbound/handlers`),
`support.conversation.open_from_thread` (support), `business_os.chat.task` (PLATTFORM, Queue).

| Capability | Signatur (Vorschlag) | Beispielaufruf | Herkunft |
|---|---|---|---|
| `ctx.cap.mail.server.config.get` | `() → {domains[], users[], runtime_config, runtime_status, health}` | `await ctx.cap.mail.server.config.get({})` | E §3 `mail.server.config.get` |
| `ctx.cap.mail.server.runtime.save` | `({settings}) → {runtime_status}` | `await ctx.cap.mail.server.runtime.save({settings})` | E §3 `mail.server` (Name `runtime.save` ergänzt) |
| `ctx.cap.mail.server.domain.save` | `({domain_name, dkim_selector, dkim_private_key:null, spf_record, dmarc_record}) → {dkim_public_key}` (CAP-3 #831: Privatschlüssel nur null, Kern erzeugt) | `await ctx.cap.mail.server.domain.save({domain_name:'example.org', dkim_selector:'default', dkim_private_key:null, spf_record, dmarc_record})` | E §3 |
| `ctx.cap.mail.server.user.save` | `({command_id, password:{secret_intake:true}, username, owner_user_id, communication_account}) → void` | `await ctx.cap.mail.server.user.save({command_id: command.id, password:{secret_intake:true}, username, owner_user_id, communication_account})` | E §3; Passwort nur per Secret-Intake |
| ~~`ctx.cap.mail.server.delete`~~ | ersetzt (MAIL3) durch die Kernrouten unten | – | – |
| `ctx.cap.mail.server.domain.delete` | native Payload `{domain_name}` (core-contract.mjs, caps/mail.rs route mail.server/domain.delete) | `await ctx.cap.mail.server.domain.delete({domain_name:'example.org'})` | Kernvertrag CAP-2 |
| `ctx.cap.mail.server.user.delete` | native Payload `{username}`; Konto-Schlüssel `email:<username klein>` leitet der native Handler ab (store.rs:16879) | `await ctx.cap.mail.server.user.delete({username:'ada@example.org'})` | Kernvertrag CAP-2 |


## Nachtrag MAIL3 – `mail.content.freeze` (15.10.2026)

Owner-Regel: Apps stehen für sich; mail hängt nur an Shell, `ctox:shared/` und OS-Primitiven des Kerns. Bis MAIL2 sandte
der Editor den documents-Befehl `office.document.freeze_email_content`; dessen Handler verlangt `app_id === 'documents'`
(`modules/documents/handlers/_lib/office.js:83`), mail scheiterte daran im Echtbetrieb. Jetzt hat mail einen eigenen
Handler; `modules/documents` ist unverändert (nur als fachliche Referenz gelesen).

**Vertrag.** Payload `{source: BlobRef{sha256,size,transfer_id}, document_id?, version_id?, expected_source_sha256?}`.
`document_id`/`version_id` sind nur Herkunftsetiketten im Ergebnis; mail löst sie nicht auf und liest keine
documents-Collection. Ergebnis (Befehlsergebnis, keine Schreibvorgänge):
`{ok, operation:'mail.content.freeze', artifact:{schema_version:1, state:'frozen', compiler_id, document_id?,
document_version_id?, source:BlobRef, source_sha256, html, text, assets[], diagnostics[], frozen_at_ms}}` mit
files-Referenzen `html`/`text`/`assets[i]` = `{storage:'files', ref:<sha256>, sha256, size, media_type, blob:BlobRef}`
(Assets zusätzlich `content_id`). Der Editor übernimmt `html`/`text` unverändert als `compiledHtmlRef`/`compiledTextRef`
und lädt den Inhalt über `api.files.get(ref)`.

**Ablauf (Fachlogik im Handler).** BlobRef-Form prüfen wie `caps/blob.rs BlobRef::validate` (64 hex, size ≤ 64 MiB,
transfer_id) → `invalid`; abweichender `expected_source_sha256` → `conflict` (beides vor jeder Capability);
`office.edit.prepare` → Session `{kind:'document', source, editor}` (sonst `unavailable`); `office.render` mit genau dieser
Session (OfficeSession ist `deny_unknown_fields`); HTML-/Text-BlobRef fehlen → `unavailable`; jede Transportdatei (HTML,
Text, Assets) einmal `blob.materialize`, Hash ≠ Referenz → `conflict`. `office.edit.apply` wird bewusst nicht gerufen:
Einfrieren wendet keine Änderungen an.

| Capability (nur OS-Primitive) | Signatur (Kernvertrag `befund-cap-2.md`, `caps/office.rs`, `caps/blob.rs` @ 83d979cf4) | Aufruf im Handler |
|---|---|---|
| `ctx.cap.office.edit.prepare` | `({kind, blob_id:BlobRef}) → {session:{kind,source,editor}, model, manifest, protocol, protocol_version, sha256}` | `prepare({kind:'document', blob_id: source})` |
| `ctx.cap.office.render` | `({session, to:'email_html'}) → {blob_id:BlobRef(html), sha256, text_blob:BlobRef, assets:[{content_id,mime_type,blob_id}], compiler_id}` | `render({session:{kind:'document', source, editor}, to:'email_html'})` |
| `ctx.cap.blob.materialize` | `({blob_id:BlobRef}) → {blob_id, sha256}` | je HTML/Text/Asset |

**Tests** `handlers/tests/content-freeze.test.mjs` (7): Aufrufform/Reihenfolge und Ergebnisform, ungültige Quelle (inkl.
„nur documents-ID“) ohne Capability-Aufruf, Hash-Erwartung, fehlende Session/Text-Blob, Materialisierungs-Konflikt,
fehlende Capability, Quelltext ohne documents-Import/Fremd-Collection/`ctx.get|read|put|command`.
**Mutationsbelege** (je Mutation am echten Handler, danach sha256-identisch wiederhergestellt `da793b27…`, 7/7 grün):
Materialize-Hashvergleich → `false` ⇒ rot „materialisierter Blob weicht … ab“; Quell-Hash-Prüfung → `false` ⇒ rot
„erwarteter Quell-Hash weicht ab“; `to:'email_html'` → `'docx'` ⇒ rot Haupttest.

**Manifest (nicht in diesem Strang, `v4/manifests/mail.manifest.json` liegt außerhalb `modules/mail/`).** Beantragt:
`handlers["mail.content.freeze"] = "handlers/mail.content.freeze.js"`; `capabilities_required` + `office.edit`,
`office.render`, `blob` (Manifest-Namen laut Kernvertrag); `command_rights["mail.content.freeze"]` = Rollen mit
Schreibrecht auf Mail-Gruppen. Die `foreign_collections` `documents`, `document_versions`, `document_blob_chunks` braucht
der Einfrierpfad nicht mehr (nur noch Etikett/Deep-Link); `documents` liest `index.js:2343` weiter für die Anzeige.

**Aufrufprüfung (Orchestrator-Ergänzung).** `node v4/os-primitives/check-caps.mjs --calls`: in `modules/mail` keine
ABWEICHUNG mehr; alle 10 mail-Aufrufe in Klasse OK (u. a. `office.edit.prepare`, `office.render` mit vorbereiteter
`session`, `blob.materialize`, `mail.server.domain.delete({domain_name})`, `mail.server.user.delete({username})`). Der
Gesamt-Exit bleibt 1 wegen Abweichungen anderer Apps. Für das Manifest damit auch: `capabilities_required`
`mail.server.delete` → `mail.server.domain.delete`, `mail.server.user.delete` (bzw. Manifest-Name `mail.server`).

**Offen.** (1) Woher die Quelle kommt: der Word-Artefakt im Editor trägt jetzt optional `source` (BlobRef); ohne sie
sendet der Editor keinen Befehl und meldet `invalid` (Zone `mail.action.failed`). Wer den BlobRef liefert (Übergabe aus
documents per `api.files`/Shell-Dateiauswahl oder Kern-Abbildung files-ref → transfer_id), ist nicht entschieden; die
App-API `api.files` kennt heute nur `ref` = sha256, keine `transfer_id`. (2) Ob vom Kern publizierte Cap-Blobs über
`api.files.get(sha256)` im Browser ankommen, hängt an der BlobAccess-/Transfer-Anbindung (befund-cap-2 „Harness-/
Transfer-Integration“) – nicht im Stub prüfbar.

## SKL-D: Kontext-Chat über den Baustein `ctox:shared/context-chat.js` (15.10.2026)

„An CTOX übergeben“ (`buildMailRouteCommands`, generischer Zweig): vorher `business_os.chat.task`; jetzt `buildContextChatCommand` → `ctox.chat.task` (Datensätze als `context.record_snapshot`, `source_record_ids`/`route_kind` bleiben oben für `routeCommandForRecord`, das den Schnappschuss jetzt auch unter `context` findet). `support.conversation.open_from_thread` unverändert.

### Manifest-Wünsche (Kern §16.40, Sender-Seite)

```json
{ "commands_out": [{ "app": "ctox", "type": "ctox.chat.task" }] }
```
Gegenstück in `modules/ctox/V4-HANDLERS.md` (`commands_in` mit `from:'*'`). Zusätzlich braucht der Nutzer das
Befehlsrecht der Ziel-App ctox (`ctox.app.modify.request`: `apps.source.edit` bzw. owner/admin/chef, im Handler geprüft).

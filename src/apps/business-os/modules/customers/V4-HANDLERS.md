# customers – v4-Handler (Strang H4, Shell v4)

Quelle der Fachlogik: `~/.local/state/shell-v4-src/4794af03d/src/core/business_os/store_customer_commands.rs`
(2005 Zeilen, Einstieg `handle_customers_active_command` :20, Prädikat `store.rs:21445 is_customers_active_command`),
Helfer `store.rs:18285-18476` (`outbound_*`), `store_projections.rs:1397 upsert_business_record`,
`mission/channels/account_helpers.rs:277 stable_digest`. Einstufung laut `E-befehlstypen.md` Z. 194-213: alle 20 FACHLOGIK.

Handler-Vertrag (Architektur §16.16): `handlers/<command_type>.js`, `export default async function handle(command, ctx)`.
Jeder Handler arbeitet in **einer** `ctx.transaction` (alle Writes atomar) und ruft `ctx.now()` genau einmal
(Rust: `now_ms()` einmal je Befehl). Gemeinsame Helfer in `handlers/_lib.js` (treu portierte `outbound_*`/`customers_*`).

**Stand: 20 / 20 portiert, 0 × `ctx.cap.legacy.*`, 0 native Fähigkeiten nötig.** 71 Tests grün.

## Befehlstypen

Reihenfolge = UI-Häufigkeit (`index.js` hat je Typ genau einen Builder `:3306-3530`, Aufrufer `:937-1006, :3057`;
gereiht nach Arbeitsablauf Anlegen → Pflegen → Abschließen, ohne UI-Sender zuletzt).

| # | Befehlstyp | Einstufung | Rust-Ort | Ziel | Stand | Tests |
|---|---|---|---|---|---|---|
| 1 | `customers.account.create` | FACHLOGIK | `store_customer_commands.rs:32 → :68` | `handlers/customers.account.create.js` | portiert | 4 |
| 2 | `customers.account.update` | FACHLOGIK | `:33 → :118` | `handlers/customers.account.update.js` | portiert | 2 |
| 3 | `customers.contact.create` | FACHLOGIK | `:35 → :232` | `handlers/customers.contact.create.js` | portiert | 3 |
| 4 | `customers.contact.update` | FACHLOGIK | `:36 → :294` | `handlers/customers.contact.update.js` | portiert | 2 |
| 5 | `customers.opportunity.create` | FACHLOGIK | `:38 → :409` | `handlers/customers.opportunity.create.js` | portiert | 4 |
| 6 | `customers.opportunity.update` | FACHLOGIK | `:41 → :482` | `handlers/customers.opportunity.update.js` | portiert | 2 |
| 7 | `customers.opportunity.move_stage` | FACHLOGIK | `:44 → :555` | `handlers/customers.opportunity.move_stage.js` (exportiert zusätzlich `moveStage`) | portiert, A1 | 4 |
| 8 | `customers.task.create` | FACHLOGIK | `:53 → :638` | `handlers/customers.task.create.js` | portiert | 2 |
| 9 | `customers.task.complete` | FACHLOGIK | `:55 → :726` | `handlers/customers.task.complete.js` | portiert | 2 |
| 10 | `customers.task.update` | FACHLOGIK | `:54 → :677` | `handlers/customers.task.update.js` | portiert | 2 |
| 11 | `customers.note.create` | FACHLOGIK | `:56 → :755` | `handlers/customers.note.create.js` | portiert | 2 |
| 12 | `customers.note.update` | FACHLOGIK | `:57 → :789` | `handlers/customers.note.update.js` | portiert | 2 |
| 13 | `customers.opportunity.close_won` | FACHLOGIK | `:47 → :611` | `handlers/customers.opportunity.close_won.js` | portiert, B1 | 3 |
| 14 | `customers.opportunity.close_lost` | FACHLOGIK | `:50 → :611` | `handlers/customers.opportunity.close_lost.js` | portiert, B1 | 2 |
| 15 | `customers.account.archive` | FACHLOGIK | `:34 → :189` | `handlers/customers.account.archive.js` | portiert | 2 |
| 16 | `customers.contact.archive` | FACHLOGIK | `:37 → :365` | `handlers/customers.contact.archive.js` | portiert | 2 |
| 17 | `customers.import.from_outbound` | FACHLOGIK | `:60 → :922` (+ `:1580`, `:1605`, `:1627`, `:1643`) | `handlers/customers.import.from_outbound.js` | portiert, B3, W3 | 5 |
| 18 | `customers.dedupe.resolve` | FACHLOGIK | `:63 → :1135` | `handlers/customers.dedupe.resolve.js` | portiert, B2 | 2 |
| 19 | `customers.view.save` | FACHLOGIK | `:59 → :866` (+ `:1545`) | `handlers/customers.view.save.js` | portiert | 3 |
| 20 | `customers.activity.record` | FACHLOGIK | `:58 → :832` | `handlers/customers.activity.record.js` | portiert (kein UI-Sender) | 2 |

Zusätzlich: `tests/_lib.test.mjs` (11, u. a. Abdeckung „jeder Rust-Typ hat genau eine Handler-Datei“ und
`stableDigest` gegen `node:crypto`), `tests/_ctx.test.mjs` (4, Attrappe selbst), `tests/scenario.rust-parity.test.mjs`
(4, Portierung der Rust-Tests `:1665` und `:1807` plus Idempotenz und Ergebnisgröße). Der dritte Rust-Test
(`:1856` Import + Dublette + resolve) steckt im ersten Test von `customers.import.from_outbound.test.mjs`.

Nicht im Umfang: `customers.import.file` – der UI-Importer läuft mit `dispatch: false` lokal im Browser
(`index.js:3090`, `importCustomerRowsFromPayload`); dafür gibt es keinen Rust-Zweig und keinen Handler.
Rechteprüfung (`store_policy.rs`, `DataWrite` auf Scope `customers`) ist Plattform und läuft vor dem Handler im Core.

Abnahme: `cd src/apps/business-os/modules/customers/handlers && node --test 'tests/*.test.mjs'` → 71 pass, 0 fail.

## ctx-Attrappe (`tests/_ctx.mjs`)

`createCtx({now, seed, foreign})` bildet den Vertrag nach: `read` (Mango-Teilmenge `$eq $ne $gt $gte $lt $lte $in
$nin $exists $not $elemMatch $and $or $nor`, Punktpfade, fehlend ≠ null, Sortierung mit Typordnung, ohne `sort` nach
`id`, `limit/skip`), `get` (tief eingefrorene Kopie oder `null`), `put/patch/remove` → `{status:'accepted', rev}`,
`transaction` (Overlay, Verwerfen bei `throw`), `command` (aufgezeichnet), `model` → `unavailable`, `http` →
`forbidden`, `cap.<name>.<fn>` (aufgezeichnet), `now`, `log`. Writes werden gegen `collections.schema.json` geprüft
(`required`, `type`, `maxLength`); Fremd-Collections nur mit `foreign`-Deklaration, sonst `forbidden`.
`run(ctx, type, payload, {id, actor, record_id})` lädt den Handler und baut den Befehl.

## Fehlercodes

Rust kennt nur Fehlertexte; die Texte sind 1:1 übernommen, die Codes ordnet der Port zu:
`… is required`, `… is not supported`, `probability …`, `contact name or email is required` → `invalid`;
`… not found` → `not_found`; `… already exists`, `closed opportunities cannot move stage` → `conflict`.

## Befunde in der Rust-Fachlogik (treu übernommen, nicht repariert)

- **B1** `close_won`/`close_lost` (`:611-636`): `probability` (100/0) und `lost_reason` werden nur in den Befehls-Payload
  gelegt; `move_stage` übernimmt davon nur `stage` und `position`. Die Opportunity erhält daher weder 100 % noch den
  Verlustgrund, obwohl `lost_reason` Pflicht ist. Tests halten das fest (`close_won`/`close_lost`-Tests). Reparatur
  ist eine Fachentscheidung (Owner), nicht Teil dieses Ports.
- **B2** `dedupe.resolve` (`:1135`): `merge`/`create_new` speichern nur die Entscheidung; kein Konto wird zusammengeführt
  oder angelegt.
- **B3** `import.from_outbound` (`:1022`): kein Existenz-Check für `account_id` – ein vorhandenes Konto gleicher ID wird
  überschrieben. Die Dubletten-Suche (`:1580`) trifft auch archivierte Konten (`is_deleted=true`), weil Rust nur die
  Zeilen-Spalte `deleted` prüft.
- **B4** `contact.archive` berechnet `search_text` nicht neu (anders als `account.archive`) – übernommen.

## Bewusste Abweichungen von Rust

- **A1** Atomarität: Rust schreibt ohne umschließende Transaktion. In `move_stage` wird die Opportunity geschrieben,
  bevor `account_id` geprüft wird; bei fehlendem Konto bleibt in Rust die verschobene Opportunity ohne Aktivität stehen.
  In v4 rollt `ctx.transaction` alles zurück. Alle übrigen Handler prüfen vor dem ersten Write wie Rust.
- **A2** Schemaprüfung: Rust prüft keine Feldtypen. Der v4-Store prüft gegen `collections.schema.json`; Werte, die Rust
  angenommen hätte (z. B. `employee_count: "50"`, `primary_contact_id: null` per Merge, ein Outbound-Kontakt als String
  statt Objekt), enden in v4 als `invalid`.
- **A3** Metafelder: Rust lädt `_rev`/`_deleted` aus `payload_json` mit und gibt sie in Ergebnissen/eingebetteten Kopien
  (`payload.outbound_company`) zurück. Der Port entfernt `_rev _deleted _meta _attachments` beim Laden; `_rev`/`_deleted`
  verwaltet in v4 der Store. Rückgaben sind wie in Rust der Stand **vor** dem Upsert (ohne neues `updated_at_ms`).
- **A4** Fehlerform: Handler werfen `{code, message}` (Brief). §16.16 nennt `new CtoxError({code, message})`; sobald die
  Laufzeit `CtoxError` bereitstellt, ist nur `_lib.js fail()` zu tauschen.

## Unklarheiten

- **U1** `record_id`: v4-Befehle haben kein `record_id`. Rust nutzt es als Rückfall für IDs (`store.rs:18316/18340`) und die
  v3-UI setzt es bei allen Update/Archiv/Close/Import/Dedupe-Befehlen. Der Port liest `command.record_id`, falls
  vorhanden (Übergang). Ohne `record_id` ändern sich bei `import.from_outbound` ohne `account_id`/`import_batch_id`
  die IDs: Rust nimmt die Firmen-ID (`record_id`) als Konto- und Batch-ID, v4 `acct_/import_<digest(command.id)>`.
  → Die v4-UI muss die Ziel-IDs im Payload mitschicken (Shell-v4-UI-Port).
- **U2** Digest-Gleichheit von `contact_<…>` (`:1055`) und `filter_/sort_<…>` (`:1564`): Rust hasht die serde_json-
  Display-Form (`preserve_order` aktiv, `indexmap` in `Cargo.lock`). `JSON.stringify` stimmt für übliche Daten überein;
  Abweichungen möglich bei ganzzahlartigen Objektschlüsseln (JS sortiert sie vor) und Gleitkommadarstellung (`1.0`).
- **U3** Ganzzahlen: Rust unterscheidet `1` und `1.0` (`as_i64`); JS nicht. `putDefaultI64`/`i64` nutzen
  `Number.isInteger`, also gilt `1.0` als Ganzzahl.
- **U4** Erster Treffer bei der Dubletten-Suche: Rust-SQL ohne `ORDER BY` (praktisch Einfügereihenfolge), v4 `id` aufsteigend.
- **U5** Pipeline-Suche (`:1605`): Rust vergleicht `company_id` getrimmt, der Port per Mango exakt.
- **U6** `actor.id`: Rust nimmt `session.user.id`, sonst `business-os-user`; der Port `command.actor.id`, sonst derselbe Rückfall.

## Wünsche an Laufzeit und Manifest („ctox sync v4“)

- **W1** Relative Importe innerhalb `handlers/` (`./_lib.js`, Handler importiert Handler `moveStage`) müssen im
  `vm`-Kontext erlaubt sein; Dateien mit `_`-Präfix sind keine Handler.
  Beispiel: `import { moveStage } from './customers.opportunity.move_stage.js';`
- **W2** Global `URL` (WHATWG) im Handler-Kontext für `domainFromWebsite` (rust-url ist ebenfalls WHATWG). Ohne `URL`
  greift ein manueller Rückfall in `_lib.js`. Kein `crypto` nötig: SHA-256 ist in `_lib.js` rein in JS umgesetzt.
- **W3** `module.json` (nicht in meiner Freigabe) braucht:
  `"handlers": { "customers.account.create": "handlers/customers.account.create.js", … alle 20 … }` und
  `"foreign_collections": { "outbound_companies": { "owner_app": "outbound", "ops": ["read"] },
  "outbound_pipeline_items": { "owner_app": "outbound", "ops": ["read"] } }`; das Outbound-Manifest muss beide unter
  `exports` mit `read` freigeben (Eigentümer laut `modules/outbound/collections.schema.json`).
- **W4** `capabilities_required: []` – customers braucht keine native Fähigkeit.

## Capability-Wünsche

Keine. Die gesamte customers-Logik ist Datenpflege, Validierung und Ableitung; es gibt keinen Mail-, Modell-, Datei- oder
externen Abruf. Daher auch kein `ctx.cap.legacy.*`-Aufruf (offen gezählt: **0**).

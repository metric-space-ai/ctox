# Business OS — App-Deployment & Lifecycle-Umsetzungsplan

Status: Entwurf · Datum: 2026-06-25 · Scope: `src/core/business_os/`,
`src/apps/business-os/`, Schema-Contract.

## Umsetzungsstand (2026-06-26, Branch `feat/bos-app-deployment-lifecycle`)

**Design-Abweichung (bewusst, umgesetzt):** Statt einer neuen Collection
`business_module_installs` (§2.1) + Schema-Migration wird der Ledger über das
bestehende `business_module_versions` (origin `install`/`update`/`app_create` =
Sync-Punkte mit `bundle_sha256`) abgebildet und die Katalog-/Update-Felder
werden in die bestehende `business_module_catalog`-Projektion gefaltet (wie
`version_states`). `update_available` ist rein hash-basiert
(`installed_from_bundle_sha256 != catalog_bundle_sha256`). Damit entfällt der
guard-protektierte Schema-/`dist`-/Cache-Buster-Pfad (AP-S) komplett — kein
Schema-Contract-Change nötig.

**Umgesetzt + getestet (committet):**
- AP-N2/N3/N4/N5 + B2/B3 — Katalog-Anker, Vanilla-Baseline = jüngster
  Sync-Punkt, Lifecycle-`update_available`, Shell-Update-Punkt. (`cargo check`)
- AP-N6/N7 — `ctox.module.update` (vanilla/discard, atomarer Swap,
  Auto-Recovery) + Policy `AppsInstall`. (Policy-Deny-Test)
- AP-B4/B5 — App-Store-Update auf Katalog-Diff + `ctox.module.update`,
  Guarded-Dialog. (`app-store.test.mjs`)
- AP-N8 (GitHub-Quelle) — typisierter `AppStoreInstallRequest`, SSRF-guarded
  Fetch über `importer::fetch_url_guarded`, `app_source`-Provenienz mit
  `verified:false` (externe Apps ohne Datenrechte bis Review), „Aus GitHub
  installieren"-Flow + „nicht verifiziert"-Badge. Sicherheitsmodell:
  Bestätigung + Pinning. (Helfer-Test)

**Offen:** AP-N1/B1 (Base-Set minimal — braucht `instance_visible`-Ledger +
Install-Pfad + Produktentscheidung welcher Base-Set, §10; ändert Sichtbarkeit
bestehender Instanzen, daher mit Migration). AP-N8 Zip-Upload (aus dem
Chunk-Store, nicht HTTP) + `ctox.module.check_updates` für GitHub-Upstream.

---

Dieser Plan setzt das Review vom 2026-06-25 um (Provisioning, App-Store,
Templating, Versionierung, Update-Mechanik). Er ist **patch-genau** gegen den
aktuellen Code formuliert und in atomare Arbeitspakete (AP) geschnitten, die
einzeln gebaut, getestet und committet werden können.

Verbindliche Leitplanken (aus `AGENTS.md` / `src/core/business_os/AGENTS.md` /
`docs/ctox-rxdb.md`):

- Kein HTTP-Daten-Bridge. Alle neuen Modul-/Update-Records laufen über
  RxDB/WebRTC und `business_commands`.
- Schema-Änderungen: Contract-Fixture ändern → beide Seiten regenerieren →
  `dist/` neu bauen → die **drei** `?v=`-Cache-Buster bumpen → Suiten grün.
  Nie eine generierte Seite von Hand editieren.
- Policy-gated Mutationen bleiben server-autoritativ (`policy.rs`).
- Keine neuen Prozess-Env-Toggles; Laufzeitkonfig über den SQLite-Runtime-Store.
- `rxdb_peer.rs`-Lifecycle-Invarianten nicht anfassen.

---

## 1. Ziel & Zielmodell

**Problem heute (Kurzfassung, code-belegt):**

- Fresh-Install ist nicht minimal: `module_ships_on_first_install`
  (`src/core/business_os/store.rs:7698`) shippt jeden Scope `core|starter|
  internal`; `STARTER_MODULE_IDS` (`store.rs:65`) bündelt die ganze
  ATS-Pipeline → ~26/33 Module als Tab. `default_installed` ist toter Code
  (`store.rs:7215` überschreibt es).
- Kein Update-Handler für installierte → neuere gepackte Version (komplettes
  Command-Set `store.rs:16069`; getestet `store.rs:31410`).
- Vanilla-Erkennung existiert (`module_version_states`, `store.rs:8300`), aber
  gegen den `MIN(seq)`-Install-Snapshot statt gegen die Upstream-Quelle → kann
  Nutzer-Edit nicht von Upstream-Upgrade unterscheiden.
- Kein Update-Indikator in der Shell; nur ein toter App-Store-Button
  (`modules/app-store/index.js:1388`, gated auf `download_url`).
- Update-Detection hängt an einem Out-of-Band-Browser→GitHub-Fetch
  (`modules/app-store/index.js:24`) statt am Datapfad.

**Zielmodell — drei getrennte Versions-Achsen + ein Upstream-Anker:**

| Achse | Bedeutung | Träger (neu/bestehend) |
|---|---|---|
| Katalog-Version | Was das gepackte Modul aktuell mitbringt (Upstream) | **neu:** `catalog_version` + `catalog_bundle_sha256` im `business_module_catalog`-Doc |
| Instanz-Version | Womit *diese* Instanz dieses Modul installiert hat + aktueller Zustand | **neu:** Collection `business_module_installs` |
| User-Release | Freigabe einer selbstgebauten App ans Team | **bestehend, unverändert:** `business_module_releases` (`store.rs:28263`) |

Abgeleitete Zustände (rein aus den drei Achsen, kein GitHub-Fetch):

- `vanilla` ⇔ `current_bundle_sha256 == installed_from_bundle_sha256`
- `update_available` ⇔ `installed_from_catalog_version < catalog_version`

**Invarianten:**

1. Eine frische Instanz kommt nur mit dem **Base-Set** als Tab hoch (Default:
   die 7 `CORE_MODULE_IDS`). Alles andere ist im App-Store sichtbar und
   installierbar, aber kein Tab.
2. „Update" zieht ausschließlich die **In-Repo-Katalog-Version**; es lädt keine
   arbiträre URL.
3. Vanilla-Apps updaten per 1-Klick; modifizierte Apps nur über einen
   expliziten, bestätigten Pfad (kein stilles Überschreiben).
4. „Release/Publish" und „Update" sind im Code und im UI strikt getrennt.

---

## 2. Datenmodell-Änderungen

### 2.1 Neue Collection `business_module_installs`

Pro-Instanz-Ledger: welches Modul ist als Tab installiert, woraus, und ist es
modifiziert. Ersetzt die heute nur implizit über `MIN(seq)` abgeleitete
Provenienz.

Felder:

| Feld | Typ | Quelle |
|---|---|---|
| `id` (= `module_id`) | string | PK |
| `module_id` | string | |
| `source_module_id` | string | Katalog-Modul, aus dem installiert wurde (für Templates ≠ `module_id`) |
| `installed_from_catalog_version` | string | Katalog-`version` zum Install-Zeitpunkt |
| `installed_from_bundle_sha256` | string | `compute_module_bundle` der Quelle zum Install-Zeitpunkt |
| `current_bundle_sha256` | string | live, projiziert |
| `instance_visible` | bool | Tab-Sichtbarkeit (Base-Set + explizit installierte) |
| `installed_by_user_id` | string | |
| `installed_at_ms` / `updated_at_ms` | number | |

In den Contract aufnehmen (siehe AP-S). Repliziert über WebRTC wie die anderen
`business_module_*`-Collections.

### 2.2 `business_module_versions` in den Contract aufnehmen

Heute **nicht** im `business_os_schema_contract.json` (verifiziert: nur
`acl/catalog/releases/reports/source_files`). Es repliziert nur als
komprimierter `version_states`-Blob im Katalog-Doc. Als First-Class-Collection
aufnehmen, damit die Versions-Timeline sauber synct (origin: `install`/`edit`/
`release`/`rollback`/**neu:** `update`).

### 2.3 Katalog-Doc-Felder

In `module_catalog_for_rxdb` (`store.rs:4277`) pro Modul-Eintrag ergänzen:

- `catalog_version` (String, = Manifest-`version`, normalisiert)
- `catalog_bundle_sha256` (String, = `compute_module_bundle(app_root,
  source_module_id).sha256`)

Das ist der heute fehlende Upstream-Anker. `compute_module_bundle`
(`store.rs:8018`) existiert bereits und hasht `(path, per-file sha256)`
deterministisch — hier auf die **Quelle** (`modules/<id>`) anwenden, nicht auf
die Live-Kopie.

---

## 3. Native Änderungen (Rust)

### AP-N1 — Provisioning auf minimalen Base-Set reduzieren

Datei: `src/core/business_os/store.rs`

- `module_ships_on_first_install` (`store.rs:7698`): von
  `matches!(scope, "core" | "starter" | "internal")` auf
  `matches!(scope, "core" | "internal")` ändern. `starter` shippt **nicht**
  mehr automatisch.
- `STARTER_MODULE_IDS` (`store.rs:65`) bleibt als Datenstruktur erhalten, wird
  aber zur **Empfehlungs-/Paket-Markierung** umgedeutet: im Katalog ein Flag
  `recommended: true` bzw. ein Paket-Tag (z.B. `bundle: "recruiting"`), kein
  Ship-Trigger. `backfill_starter_module_data_grants` (`store.rs:2890`) erst
  beim tatsächlichen Install greifen lassen, nicht beim Provisioning.
- Default-Base-Set = `CORE_MODULE_IDS` (`store.rs:56`): `ctox`, `knowledge`,
  `app-store`, `desktop`, `reports`, `tickets`, `threads`. (Entscheidung offen,
  ob `reports`/`tickets`/`threads` Base bleiben — siehe §8.)

Akzeptanz: frische Instanz projiziert nur Base-Set in `modules[]` als
`instance_visible`; restliche core/starter/store-Module landen weiterhin im
Katalog/Marketplace, aber nicht als Tab.

### AP-N2 — Katalog-Projektion: Upstream-Anker

Datei: `store.rs`, `module_catalog_for_rxdb` (`store.rs:4277`)

- Pro Modul `catalog_version` + `catalog_bundle_sha256` ergänzen (§2.3).
- Hash-Berechnung memoisieren (einmal pro Projektion), da
  `compute_module_bundle` das Quellverzeichnis liest.

### AP-N3 — Install-Ledger schreiben

Dateien: `store.rs` (Install-/Template-/App-Create-Pfade)

Überall, wo heute ein Modul installiert wird, einen
`business_module_installs`-Record schreiben:

- `install_template_module` (`store.rs:7400`): `source_module_id` =
  Template-`source_module`, `installed_from_catalog_version` =
  Katalog-`version` der Quelle, `installed_from_bundle_sha256` =
  `compute_module_bundle(quelle)`.
- App-Create-Pfad (`ctox.business_os.app.create`): `source_module_id` =
  `module_id`, Baseline aus dem erzeugten Bundle.
- Base-Set-Provisioning (AP-N1): Ledger-Records mit `instance_visible=true`.

`template_id` (heute write-only gestempelt, `store.rs:7466`) wird durch
`source_module_id` im Ledger ersetzt/ergänzt und tatsächlich gelesen.

### AP-N4 — Lifecycle-Projektion: `update_available` + Versionen

Datei: `store.rs`, `projected_module_lifecycle` (`store.rs:3029`)

Neue projizierte Felder am Lifecycle-Objekt:

- `installed_version` (= `installed_from_catalog_version` aus dem Ledger)
- `catalog_version` (aus dem Katalog-Eintrag)
- `update_available` = `semver_lt(installed_from_catalog_version,
  catalog_version)`
- `modification_state` = `vanilla` | `customized` | `unknown` (aus
  `module_version_states` mit korrigiertem Baseline, siehe AP-N5)

Wichtig: heute liefert die Projektion für nicht-runtime-installierte Module
`("packaged", …)` und überspringt das Versions-Gating. `update_available` muss
**unabhängig** davon gesetzt werden (auch packaged/Base-Module können veralten,
wenn der Repo-Katalog neuer ist als der Install-Anker).

### AP-N5 — Vanilla-Erkennung gegen die richtige Referenz

Datei: `store.rs`, `module_version_states` (`store.rs:8300`)

- Baseline-Quelle wechseln: statt `MIN(seq)`-Snapshot aus
  `business_module_versions` den `installed_from_bundle_sha256` aus
  `business_module_installs` verwenden.
- `modified = current_sha != installed_from_sha`.
- `current_sha` weiter via `compute_module_bundle` der Live-Kopie.

### AP-N6 — Neues Command `ctox.module.update`

Datei: `store.rs`, Command-Dispatch (`store.rs:16069` ff.)

Folgt exakt dem bestehenden Muster (vgl. `ctox.source.rollback_snapshot`,
`store.rs:16104`):

```
"ctox.module.update" => {
    let request: ModuleUpdateRequest = serde_json::from_value(command.payload.clone())
        .context("invalid ctox.module.update payload")?;
    let session = rxdb_authenticated_session(root, &command)?;
    let module_id = source_sanitize_slug(&request.module_id);
    anyhow::ensure!(!module_id.is_empty(), "module_id is required");
    let decision = module_policy_decision(root, &session,
        BusinessOsPermission::AppsInstall, &module_id)?;       // siehe AP-N7
    if let Some(outcome) = reject_command_if_policy_denied(root, &command, &decision)? {
        return Ok(outcome);
    }
    let outcome = update_module_to_catalog(root, app_root, &session, request)?;
    return write_rxdb_control_command_outcome(root, &command, "completed",
        None, Some("completed"), outcome);
}
```

`ModuleUpdateRequest { module_id, target_catalog_version,
expected_baseline_sha256, mode }` mit `mode ∈ {vanilla, discard}`.

`update_module_to_catalog`:

1. Ledger + Katalog laden. Prüfen `catalog_version == target_catalog_version`
   (Optimistic-Concurrency; sonst „Katalog hat sich geändert").
2. `current_sha` berechnen.
   - `mode = vanilla`: hart verlangen `current_sha == expected_baseline_sha256
     == installed_from_bundle_sha256`. Sonst Fehler (Modul ist modifiziert).
   - `mode = discard`: erlauben (verwirft lokale Änderungen explizit).
3. Quelle (`modules/<source_module_id>`) in das installierte Verzeichnis
   kopieren — **transaktional**: in ein Temp-Dir kopieren, dann atomar
   umschwenken; kein nackter `remove_dir_all` wie in `install_app_module`
   (`store.rs:8649`).
4. Ledger updaten: `installed_from_catalog_version`/`_bundle_sha256` =
   Ziel. `business_module_versions`-Snapshot mit `origin = "update"`.
5. Katalog-Stamp/Reprojektion triggern (wie bestehende Pfade).

Dieser Command **ersetzt** den Missbrauch von `ctox.app_store.install`
(`install_app_module`, `store.rs:8555`) für interne Katalog-/Template-Module.

### AP-N7 — Policy

Datei: `src/core/business_os/policy.rs` (`evaluate`, `policy.rs:307`)

`ctox.module.update` über `BusinessOsPermission::AppsInstall` gaten (heute
Chef|Admin, `policy.rs:321`). Begründung: Update mutiert installierte Bytes auf
Workspace-Ebene — gleiche Klasse wie Install/Uninstall, nicht wie das
founder-scoped `AppsModify`. Alternative (founder darf eigenes Modul updaten):
`AppsModify` für `mode=vanilla` + `AppsInstall` für `mode=discard`. **Default:
`AppsInstall` für beide** (siehe §8, offene Entscheidung). Keine neue
Permission-Variante nötig.

### AP-N8 — `install_app_module` absichern oder zurückbauen

Datei: `store.rs:8555`

- Für interne Module: durch `ctox.module.update` ersetzt; Aufrufpfad aus dem
  App-Store entfernen.
- Für echte externe ZIP-Installs (falls beibehalten): Allowlist + Checksum
  erzwingen; den stillen `remove_dir_all` (`store.rs:8649`) durch den
  transaktionalen Swap aus AP-N6 ersetzen. **Diesen Pfad nicht erweitern.**

---

## 4. Browser-Änderungen (JS)

### AP-B1 — Sichtbarkeits-Gate an den Ledger koppeln

Datei: `src/apps/business-os/shared/app-lifecycle.js`,
`canSeeModuleForAppVersion` (`app-lifecycle.js:297`)

- Tab-Sichtbarkeit an `instance_visible` (aus dem projizierten Ledger) koppeln,
  nicht mehr an „ist nicht runtime-installed → immer sichtbar"
  (`app-lifecycle.js:299`).
- App-Store-Sichtbarkeit bleibt: nicht-installierte Katalogmodule erscheinen im
  Store, aber nicht als Launcher-Kachel/Tab.
- `registry.json`-Browser-Merge (`app.js:6940`,
  `mergePackagedCatalogModules`) zurückbauen: nur noch für Build-Validierung,
  nicht als Laufzeit-Seed. Damit verschwindet die
  `local`/`starter`/`store`-Divergenz.

### AP-B2 — Lifecycle-Badge um Update-Feld erweitern

Datei: `app-lifecycle.js`, `appLifecycleState`/`appLifecycleBadge`
(`app-lifecycle.js:211`, `:304`)

- `update_available`, `installed_version`, `catalog_version`,
  `modification_state` aus der nativen Projektion durchreichen.
- Wichtig: heute returnt `appLifecycleState` für packaged Module früh
  (`app-lifecycle.js:232`). Das Update-Feld muss auch dort gesetzt werden.

### AP-B3 — Update-Punkt in der Shell

Dateien: `src/apps/business-os/app.js`, `app.css`,
`src/apps/business-os/modules/desktop/index.js`

- `renderStartMenuLifecycleBadge` (`app.js:8351`) bzw. `buildStartMenuItem`
  (`app.js:8374`): einen kleinen `.start-menu-update-dot` rendern, wenn
  `lifecycle.update_available`. Der Hook (Badge-Slot pro Kachel) existiert
  bereits. Heutiges Early-Return bei `!runtimeInstalled`
  (`app.js:8360`) so anpassen, dass der Update-Punkt auch für Base-/packaged
  Module erscheint.
- Desktop-Icon (`modules/desktop/index.js:216`, `buildIcon`): analoger Punkt.
- CSS: neue Klasse `.start-menu-update-dot` (kleiner Akzent-Punkt; eigene
  `data-state`-freie Klasse, getrennt vom Lifecycle-Badge `app.css:446`).
- Optional: einmalige Update-Toast über `shared/notifications.js`.

### AP-B4 — App-Store auf Katalog-Diff statt GitHub-Fetch

Datei: `modules/app-store/index.js`

- `updateStateFor` (`index.js:1353`): Quelle ist der projizierte
  Katalog-Diff (`catalog_version` vs `installed_version` aus dem Ledger), nicht
  mehr `remote.download_url`.
- Den direkten GitHub-Fetch (`index.js:24`, `refreshMarketplace`) entfernen
  oder hinter ein optionales, klar getrenntes „externer Marketplace"-Feature
  ziehen (nicht der Default-Pfad, nicht im Datapfad-Kritikbereich).
- „Aktualisieren"-Button (`index.js:1388`): dispatcht `ctox.module.update`
  statt `installMarketplaceItem(item, {update:true})` (`index.js:215`).
- Bei `modification_state === 'customized'`: Bestätigungsdialog (AP-B5) statt
  direktem Dispatch.

### AP-B5 — Guarded-Update-Dialog für modifizierte Apps

Datei: `modules/app-store/index.js` + `shared/dialogs.js`

- `vanilla` + `update_available` → 1-Klick (`mode: 'vanilla'`).
- `customized` + `update_available` → Dialog mit:
  (i) „Lokale Änderungen verwerfen und updaten" (`mode: 'discard'`, vorher
  automatische Recovery-Version via bestehendem
  `record_module_version`/Rollback-Mechanismus `store.rs:8351`);
  (ii) später: Drei-Wege-Diff-Ansicht (`installed_from` = Anker, `catalog` =
  neu, `current` = lokal). Mindestanforderung jetzt: expliziter,
  bestätigter Discard — kein stilles Überschreiben.

---

## 5. Schema-Contract- & Build-Workflow

Reihenfolge (verbindlich, vgl. `docs/ctox-rxdb.md` §9 / `run-all.mjs`-Header):

1. `business_module_installs` (+ `business_module_versions`) im
   `src/core/business_os/business_os_schema_contract.json` deklarieren.
2. Beide Seiten regenerieren: `business_os_schema_hashes.json` (Rust,
   `include_str!` in `rxdb_peer.rs`) und der Browser-Spiegel
   `CTOX_BUSINESS_OS_SCHEMA_HASHES` in
   `src/apps/business-os/rxdb/src/schema.mjs`. Nie eine Seite einzeln editieren.
3. RxDB-`dist/` mit dem gepinnten esbuild-Befehl neu bauen
   (`docs/ctox-rxdb.md` §“esbuild”).
4. Die **drei identischen** `?v=`-Cache-Buster bumpen.
5. Suiten grün: `node src/apps/business-os/rxdb/tests/run-all.mjs`,
   `cargo test --manifest-path src/core/rxdb/Cargo.toml`,
   `cargo fmt --check --manifest-path src/core/rxdb/Cargo.toml`.

Guards, die das erzwingen: `contract-drift-smoke`, `bundle-reproducible-smoke`,
`schema-hash-registry-smoke`.

---

## 6. Migration & Rollout

- **Bestandsdaten:** Beim ersten Start nach dem Upgrade einen
  Ledger-Backfill fahren: für jedes aktuell sichtbare/installierte Modul einen
  `business_module_installs`-Record aus dem vorhandenen
  `business_module_versions`-`MIN(seq)`-Snapshot ableiten
  (`installed_from_bundle_sha256` = bisheriger Baseline). So bleibt
  bestehender „modified"-Status erhalten und es entsteht kein
  Fehl-Update-Signal.
- **Base-Set-Reduktion ist sichtbarkeitsverändernd.** Auf Test-Instanzen
  (Kunstmen/Ninja) ggf. IndexedDB leeren / neu pullen (vgl.
  `reference_business_os_two_store_and_boot`). Bestehende Instanzen behalten
  ihre sichtbaren Apps über den Ledger-Backfill (`instance_visible=true` für
  alles, was vorher Tab war) — die Reduktion greift nur für **frische**
  Instanzen.
- **Reihenfolge der Auslieferung:** AP-N2/AP-N4/AP-B2/AP-B3 (Anker + Badge,
  rein additiv) zuerst — sie ändern nichts am Provisioning und sind sofort
  nützlich. AP-N1/AP-B1 (Base-Set + Sichtbarkeit) als zweite, bewusst getaktete
  Welle.

---

## 7. Arbeitspakete in Reihenfolge

Jedes AP = ein atomarer Commit + gezielter Test (vgl.
`project_parallel_sessions_same_checkout`: früh/atomar committen).

| # | AP | Dateien | Validierung |
|---|---|---|---|
| 1 | AP-S Schema: `business_module_installs` + `business_module_versions` in Contract | contract.json, hashes.json, schema.mjs, dist, 3× `?v=` | `run-all.mjs`, rxdb crate tests |
| 2 | AP-N2 Katalog-Anker | store.rs:4277, 8018 | `cargo test` modul/catalog |
| 3 | AP-N3 Install-Ledger schreiben | store.rs:7400, app-create | targeted test |
| 4 | AP-N5 Baseline korrigieren | store.rs:8300 | `module_version_states_*`-Test |
| 5 | AP-N4 Lifecycle `update_available` | store.rs:3029 | projektion-test |
| 6 | AP-B2 Badge-Datenfeld | app-lifecycle.js | app-lifecycle.test.mjs |
| 7 | AP-B3 Shell-Update-Punkt | app.js, app.css, desktop | manuell/preview |
| 8 | AP-N6+N7 Command `ctox.module.update` + Policy | store.rs:16069, policy.rs | command-test, policy-test |
| 9 | AP-B4+B5 App-Store auf Katalog-Diff + Dialog | app-store/index.js, dialogs.js | app-store.test.mjs |
| 10 | AP-N1+AP-B1 Base-Set + Sichtbarkeit | store.rs:65/7698, app-lifecycle.js:297, app.js:6940 | provisioning-test + Migration |
| 11 | AP-N8 `install_app_module` zurückbauen/absichern | store.rs:8555 | test |
| 12 | AP-C Aufräumen (§9) | diverse | suiten |

---

## 8. Tests & Validierung

- Rust: `cargo fmt --check`, `cargo check`, gezielte
  `cargo test` (Module/Katalog/Policy/Command). RxDB-Crate:
  `cargo test --manifest-path src/core/rxdb/Cargo.toml`.
- Browser-Datapfad: `node src/apps/business-os/rxdb/tests/run-all.mjs` (nach
  dist-Rebuild + Cache-Buster).
- Modul-Suiten: `app-store.test.mjs`, `app-lifecycle.test.mjs`,
  `permissions.test.mjs`.
- Neue Tests:
  - Fresh-Provisioning: nur Base-Set `instance_visible`.
  - `update_available` true, wenn Katalog-`version` > Ledger-Version.
  - `ctox.module.update mode=vanilla` lehnt modifizierte Module ab; `discard`
    legt Recovery-Version an und updatet.
  - Baseline = `installed_from`, nicht `MIN(seq)`.
  - Policy: User/Founder ohne `AppsInstall` werden abgelehnt.

---

## 9. Aufräum-Liste (direkt aus dem Design)

- Dead Code: `ctox.module.install_template`-UI-Pfad
  (`app-store/index.js:885`) entfernen/konsolidieren.
- Doppeltes `install_template_module`/`copy_dir_recursive` in `store.rs:7400`
  vs `server.rs:1697` entmischen (server-Kopie setzt `install_scope` nicht und
  überspringt die Versions-Aufnahme).
- `registry.json`: `local`-Scope, `default_installed`, `store.distribution`
  entfernen oder zu Build-Validierungs-Hints degradieren (nativ tot).
- Leeres `runtime/business-os/installed-modules/matching/` löschen
  (irreführend; Runtime liegt unter `runtime/business-os/installed-modules/`).

---

## 10. Offene Entscheidungen

1. **Base-Set-Umfang.** Default = 7 `CORE_MODULE_IDS`. Sollen
   `reports`/`tickets`/`threads` Base bleiben, oder ist der harte Kern nur
   `ctox`+`desktop`+`app-store`+`knowledge`? → Produktentscheidung.
2. **Update-Berechtigung für Founder.** `AppsInstall` (nur Chef/Admin) vs
   `AppsModify` (Founder auf eigenem Modul) für `mode=vanilla`.
3. **Externer Marketplace.** GitHub-Fetch ganz streichen oder als optionales,
   klar getrenntes Feature behalten (mit Allowlist/Signatur)? Default dieses
   Plans: streichen; In-Repo-Katalog ist die Quelle.
4. **„Recruiting-Paket".** Soll `STARTER_MODULE_IDS` als 1-Klick-Bundle im Store
   erscheinen (`bundle: "recruiting"`) oder nur als Einzel-Apps mit
   `recommended`-Tag?

---

## 11. Referenzen

- Review-Notiz (Memory): `project_app_deployment_review_2026_06_25`.
- Datapfad/Contract: `docs/ctox-rxdb.md`,
  `src/core/business_os/AGENTS.md`.
- Lifecycle/Policy: `src/apps/business-os/shared/app-lifecycle.js`,
  `src/core/business_os/policy.rs`.
- Boot/Provisioning-Kontext (Memory):
  `reference_business_os_two_store_and_boot`.

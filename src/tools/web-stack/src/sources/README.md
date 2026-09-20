# Web-Stack Source Modules

Source-Module sind die quellenspezifische Erweiterung des
CTOX-Webstacks. Jedes Modul stellt eine externe Recherche-Site oder API
(Bundesanzeiger, Northdata, Zefix, LinkedIn Sales Nav, …) als
standardisiertes Plug-in zur Verfügung. `ctox web search` / `ctox web read`
treiben diese Module über den Trait [`SourceModule`](./mod.rs); das
übergeordnete `ctox web person-research` (Phase 4) benutzt die Registry,
um pro `(mode, country, field)` aus [`EXCEL_MATRIX.md`](./EXCEL_MATRIX.md)
einen Recherche-Plan zu bauen.

## Wann ist ein Modul „authoritative" für ein Feld?

Genau dann, wenn die Quelle in der DACH-Recherchematrix
([`EXCEL_MATRIX.md`](./EXCEL_MATRIX.md)) für mindestens einen
(`mode`, `country`) für dieses Feld gelistet ist. `authoritative_for()`
ist also nicht „beste Quelle", sondern „darf laut Matrix dieses Feld
schreiben". Das verhindert, dass z. B. LinkedIn den Firmen-Namen
schreibt, nur weil ein Profil eine Firmenseite verlinkt.

## Drei Routing-Klassen

Pro Quelle entscheidet die Trait-Implementierung, welcher Pfad gilt:

1. **Crawl-Pfad** — `shape_query` liefert Query + Domain-Pin, `extract_fields`
   parst die Read-Antwort. `fetch_direct` bleibt `None`. Beispiele:
   Bundesanzeiger, Northdata, Firmenabc, Companyhouse, Handelsregister.
2. **API-Pfad** — `fetch_direct` schickt einen direkten REST-/GraphQL-Call
   an die offizielle API der Quelle. `shape_query` bleibt typischerweise
   `None`. Credentials kommen aus dem CTOX-Secret-Store
   (`ctox secret get --scope credentials --name <NAME>`); welchen Namen
   das Modul erwartet, deklariert es über `requires_credential()`.
   Beispiele: Zefix, D&B Hoovers, Leadfeeder, LinkedIn, XING.

   Leadfeeder current API keys use `LEADFEEDER_API_KEY` as `X-Api-Key` against
   `https://api.leadfeeder.com/v1` (`account_id` required). Explicit account
   selection is `LEADFEEDER_ACCOUNT_ID`; if unset, `GET /v1/accounts` may
   resolve a single authorized account and must fail closed when several exist.
   Legacy Token auth is `LEADFEEDER_LEGACY_API_TOKEN` plus
   `LEADFEEDER_AUTH_SCHEME=legacy` only. Browser capture stays a separate
   deliberate mode (`LEADFEEDER_BROWSER_LOGIN`, scrape `access_mode` /
   `auth_mode` = `authenticated_browser`).
   Do not send a current key as `Authorization: Token token=...`, default
   account `me`, or call credit-consuming retrieve/enrichment endpoints.
   Native API fetch uses `fetch_direct_with_resolver` and reads
   `LEADFEEDER_API_KEY` / `LEADFEEDER_LEGACY_API_TOKEN` only from the injected
   credentials resolver (`scope=credentials`). Account id and auth scheme stay
   non-secret runtime config. An absent resolver is unavailable, not missing,
   and never falls back to `runtime_env_kv` or browser capture.
   Native identity requires equal name bodies after punctuation and case
   normalization. Only trailing legal suffixes are separated; explicitly
   different suffixes are rejected, while an omitted suffix is tolerated.
   Unicode, short names and meaningful body words are preserved. High match
   scores do not accept subsidiaries, supersets, or aliases. Blank provider
   IDs and explicit `address.country_code` mismatches are rejected, and
   multiple distinct IDs for the same name fail closed. Provider fields travel
   through a local `leadfeeder_fields:` snippet object so domains, employee
   counts/ranges, and industry are not inferred from display position.

   Scrape-execute currently forwards company/country/source_id (and optional
   adapter_test). That is enough for the supported API path: the script calls
   `ctox web search --source leadfeeder.com`, and `fetch_direct` reads the
   runtime/secret selectors above. `target.json` `access_mode` is **not**
   consulted unless the native command forwards `access_mode` or `auth_mode`
   into script input. Forward `authenticated_browser` only when browser
   capture is deliberately selected; forward `native_api` to pin API mode.
3. **Browser-Pfad** — Crawl wie (1), aber `extract_fields` weiß, dass die
   Roh-Antwort aus `ctox web browser-automation` kommt (JS-Wall).
   Handelsregister nutzt diesen Pfad direkt.

Unabhängig von diesen Routing-Klassen besitzt jeder registrierte öffentliche
Scrape-Adapter einen generischen, begrenzten Zugriffs-Fallback: Nur nach
`blocked` oder `temporary_unreachable` wird die kanonische Quell-Domain einmal
in einer quellgebundenen Business-OS-Browsersitzung aufgerufen und deren ID bei
erfolgreichem Warm-up an genau einen Adapter-Retry übergeben. Der Fallback
übergibt weder Credentials noch Seiteninhalt, folgt keinen fremden Domains und
löst keine CAPTCHAs. Andere Klassifikationen starten ihn nicht.

## Datei-Konventionen

Pro Quelle wird **genau eine** Datei angefasst:

```
tools/web-stack/src/sources/<id>.rs
tools/web-stack/fixtures/sources/<id>/<example>.{html|json|pdf}
```

`<id>` ist der kanonische Hostname ohne `www.`/`app.` (siehe
`EXCEL_MATRIX.md`). Beispiele: `bundesanzeiger.de`, `zefix.ch`,
`dnbhoovers.com`. Aliase erlaubt das Modul über `aliases()`.

Niemals anfassen aus einem Source-Subagent:
- [`mod.rs`](./mod.rs) — Trait und Registry sind Phase-0-Territorium.
- `../web_search.rs`, `../surface.rs`, `../lib.rs` — Phase 3.
- Andere `sources/<x>.rs` — andere Subagents arbeiten parallel.

Neue externe Crate-Dependencies dürfen nur in `tools/web-stack/Cargo.toml`
landen, mit Begründung im Commit. Wo möglich: existierende Deps benutzen
(`ureq` für HTTP, `scraper` für HTML, `ctox_pdf_parse` für PDF,
`serde_json` für JSON-APIs).

## Fixtures

Tests laden eingefrorene Roh-Antworten aus `fixtures/sources/<id>/`.
Pro Quelle mindestens **ein** Beispiel mit:

- einem bekannten Firmennamen, der reproduzierbar Treffer liefert,
- der vollständigen Roh-Antwort der Quelle (HTML/JSON/PDF),
- ggf. einer `expected.json` mit dem erwarteten `extract_fields`-Output.

`extract_fields` ist gegen Fixture-Daten unit-testbar und MUSS
deterministisch sein. Live-Tests gegen die echte Quelle laufen
zusätzlich, aber `#[ignore]`-markiert:

```rust
#[test]
#[ignore = "live network; run with: cargo test -- --ignored sources::<id>::live"]
fn live_smoke() { ... }
```

So bleibt die Standard-Test-Suite hermetisch, während ein Operator/Agent
periodisch die Live-Pfade prüfen kann.

## Credentials

Tier-C-Module lesen ihren Token über das `ctox secret get`-CLI ihres
eigenen Binaries — `runtime_config::get` ist für unverschlüsselte
Laufzeit-Konfiguration gedacht und nicht für Auth-Tokens. Konvention:

```rust
fn requires_credential(&self) -> Option<&'static str> {
    Some("DNB_DIRECT_API_KEY")
}
```

Wenn der Token fehlt: `fetch_direct` gibt
`Some(Err(SourceError::CredentialMissing { secret_name }))` zurück.
`person-research` weiß dann, dass die Quelle bei diesem Tenant inert ist,
und probiert die nächste Quelle in der Priority-Liste.

## Acceptance pro Modul

- `cargo build -p ctox-web-stack` grün
- `cargo test -p ctox-web-stack --lib sources::<id>` grün (Unit-Tests
  gegen Fixtures)
- `cargo test -p ctox-web-stack -- --ignored sources::<id>::live` grün
  (oder dokumentiert `credential_missing` für Tier C ohne Test-Token)
- Diff: max. 3 Dateien (`sources/<id>.rs`, Fixtures, evtl. `Cargo.toml`)

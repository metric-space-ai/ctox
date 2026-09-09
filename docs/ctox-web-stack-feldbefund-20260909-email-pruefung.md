# Feldbefund web-stack 09.09.2026 — die E-Mail-Prüfung bekommt nie die Adresse

Gilt für `ctox-web-stack` aus dem Workjet-Pin
(`rev = cdf64f856bbbaa469ff16c03f9dd8b07d491fef5`), Datei
`src/tools/web-stack/src/sources/scrape_bridge.rs`. Der Fix gehört ins
Workjet-Repo plus Pin-Bump; die Kopie im CTOX-Baum ist vom Workspace
ausgeschlossen und wird nicht gebaut.

## Befund

Auf dem Kundenmandanten THESEN, über 25 Leads gemessen:

    Leads mit mindestens einer Kontakt-E-Mail: 11
    Leads mit VALIDIERTER Kontakt-E-Mail:       0

`person_email_validation` steht ausnahmslos auf `no_match` oder
`action_required`. Da die Freigabeprüfung eine validierte Adresse verlangt,
kann **kein einziger Lead** an Sellify übergeben werden — unabhängig von allem
anderen.

## Ursache

`run_via_runtime_target` baut die Eingabe für `ctox scrape execute` so:

```rust
let input = json!({
    "company": company,
    "country": country.as_iso(),
    "source_id": source_id,
});
```

Für Verzeichnisquellen ist das richtig. Für die Prüfziele `experte-de` und
`mailtester-com` nicht: deren Extraktor beantwortet eine Frage über EINE
Adresse und bricht ohne sie ab mit

    CTOX_SCRAPE_INPUT_JSON.email missing

Der Lauf endet als `portal_drift`, und die Recherche liest das als Beweis, die
Adresse sei nicht prüfbar.

## Dass die Prüfung funktioniert, ist belegt

Auf der VM, 09.09.2026 20:12 UTC, von Hand mit Adresse aufgerufen:

    ctox scrape execute --target-key experte-de --trigger-kind manual \
      --input-json '{"email":"info@weicon.de"}'

    → field: person_email_validation | value: valid
      note:  EXPERTE.de verdict: info@weicon.de | Gültig
      gate:  verification_status verified, http_status 200, eligible true

Die Mechanik trägt. Es fehlt allein die Adresse in der Eingabe.

## Warum der Worker es nicht selbst tun kann

Naheliegend wäre, den Recherche-Worker den Aufruf machen zu lassen. Das wurde
versucht: die Recherchevorgabe nennt seit App 1.0.120 den vollständigen Befehl
samt `--input-json`. Der Worker hat ihn befolgt und zurückgemeldet:

> „E-Mail-Validierung gemäß Anweisung 4a über registriertes Scrape-Ziel
> (experte-de / mailtester-com); diese Scrape-Ziele konnten in dieser Sandbox
> nicht ausgeführt werden."

Die Worker-Sandbox verweigert die ctox-CLI (`CapEff=0`, `NoNewPrivs=1`,
`Permission denied` auf den State-Root). Der Aufruf muss also vom Daemon
kommen, nicht vom Worker.

## Vorgeschlagener Patch

`run_via_runtime_target` erhält einen optionalen `candidate_email` und legt ihn
in die Eingabe:

```rust
pub fn run_via_runtime_target(
    …,
    owner_user_id: Option<&str>,
    candidate_email: Option<&str>,
) -> ScrapeBridgeResult {
    let mut input = json!({
        "company": company,
        "country": country.as_iso(),
        "source_id": source_id,
    });
    if let Some(email) = candidate_email
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        input["email"] = json!(email);
    }
```

Aufrufer ist `src/core/business_os/person_research_command.rs` (eine Stelle).
Dort liegt die bereits gefundene Adresse in `result` unter
`/fields/person_email/value`, ersatzweise in `person_records[].person_email`.

## Gegenprobe für den Fix

Weicon GmbH & Co. KG, nach Löschen und Neurecherche am 09.09.2026:

- `person_email`: **verifiziert**, vier persönliche Adressen
  (`r.weidling@`, `a.weidling@`, `s.beilmann@`, `p.jennings@weicon.de`).
- `person_email_validation`: `no_match` mit obiger Sandbox-Begründung.

Greift der Patch, muss dieses Feld beim nächsten Lauf `valid` werden, mit
`source_url https://www.experte.de/email-pruefen` und verifiziertem Belegtor.

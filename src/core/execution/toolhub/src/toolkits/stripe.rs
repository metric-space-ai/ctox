//! Stripe toolkit (REST v1, `application/x-www-form-urlencoded` bodies).
//! Action set modelled on Activepieces `piece-stripe` (MIT).
//!
//! Auth: the piece uses `PieceAuth.SecretText` and sends the secret key
//! (`sk_live_…`/`sk_test_…` or a restricted `rk_…` key) as
//! `Authorization: Bearer <key>` — the same header the `stripe` Node SDK used by
//! a few actions sets. `api.stripe.com` is global (no per-account base URL), so
//! no base-url input is needed. Stripe has no OAuth scopes; the `scopes` of every
//! tool therefore carry the restricted-key *permission level* the call needs
//! (`read`, `write`).
//!
//! Every id travels in the URL path and is checked with `path_segment` plus the
//! Stripe id prefix before interpolation; query values and form fields are
//! percent-encoded. Amounts that the piece takes in main currency units are
//! converted to the smallest unit the API expects (`Math.round(x * 100)`).
use anyhow::{anyhow, bail, Result};
use serde_json::{json, Value};

use super::common::*;
use super::{path_segment, ToolkitAdapter};
use crate::http::HttpRequest;
use crate::model::*;

pub const SLUG: &str = "stripe";

const READ: &str = "read";
const WRITE: &str = "write";

const INVOICE_STATUS: &[&str] = &["draft", "open", "paid", "uncollectible", "void"];
const PAYOUT_STATUS: &[&str] = &["pending", "in_transit", "paid", "failed", "canceled"];
const SUBSCRIPTION_STATUS: &[&str] = &[
    "all",
    "active",
    "past_due",
    "unpaid",
    "canceled",
    "incomplete",
    "incomplete_expired",
    "trialing",
    "paused",
];

// ---------------------------------------------------------------- schema fragments

fn choice(desc: &str, options: &[&str]) -> Value {
    json!({"type": "string", "description": desc, "enum": options})
}

fn flag(desc: &str) -> Value {
    json!({"type": "boolean", "description": desc})
}

/// Amount in the currency's main unit (the piece multiplies by 100 before sending).
fn money(desc: &str) -> Value {
    json!({"type": "number", "description": desc})
}

fn limit_prop() -> Value {
    json!({"type": "integer",
        "description": "How many records to return in this page, 1 to 100. Default 10 (the Stripe default).",
        "minimum": 1, "maximum": 100})
}

fn moment(desc: &str) -> Value {
    json!({"type": "string",
        "description": format!("{desc} ISO 8601 date-time, e.g. 2026-09-01T00:00:00Z.")})
}

fn obj_prop(desc: &str) -> Value {
    json!({"type": "object", "description": desc})
}

fn cursor(desc: &str) -> Value {
    s(desc)
}

// ---------------------------------------------------------------- id / value helpers

/// Stripe object id (`cus_…`, `in_…`, …) from the path-safe charset.
fn stripe_id(args: &Value, key: &str, prefix: &str) -> Result<String> {
    let id = path_segment(args, key)?;
    if !id.starts_with(prefix) {
        bail!("`{key}` must be a Stripe id starting with `{prefix}`, got \"{id}\"");
    }
    Ok(id.to_string())
}

/// `limit` only when the caller passed one, clamped to Stripe's 1..=100.
fn opt_limit(args: &Value) -> Option<String> {
    args.get("limit")
        .and_then(Value::as_u64)
        .map(|l| l.clamp(1, 100).to_string())
}

fn req_num(args: &Value, key: &str) -> Result<f64> {
    args.get(key)
        .and_then(Value::as_f64)
        .filter(|v| v.is_finite())
        .ok_or_else(|| anyhow!("missing argument `{key}`"))
}

/// Main-unit amount -> smallest currency unit (`Math.round(amount * 100)`).
fn cents(args: &Value, key: &str) -> Result<i64> {
    let v = req_num(args, key)?;
    let c = (v * 100.0).round() as i64;
    if c <= 0 {
        bail!("`{key}` must be a positive amount, got {v}");
    }
    Ok(c)
}

fn flag_value(args: &Value, key: &str) -> bool {
    args.get(key).and_then(Value::as_bool).unwrap_or(false)
}

fn push_opt(form: &mut Vec<(String, String)>, field: &str, v: Option<&str>) {
    if let Some(v) = v {
        form.push((field.to_string(), v.to_string()));
    }
}

/// `metadata` JSON object -> `metadata[k]=v` form fields (nil values dropped,
/// like the piece's `toFormBody`).
fn put_metadata(form: &mut Vec<(String, String)>, args: &Value, key: &str) -> Result<()> {
    match args.get(key) {
        None | Some(Value::Null) => Ok(()),
        Some(Value::Object(m)) => {
            for (k, v) in m {
                let value = match v {
                    Value::String(s) => s.clone(),
                    Value::Null => continue,
                    other => other.to_string(),
                };
                form.push((format!("metadata[{k}]"), value));
            }
            Ok(())
        }
        Some(_) => bail!("`{key}` must be a JSON object"),
    }
}

/// Address fields -> `address[line1]=…` form fields, exactly the nesting the
/// piece's `toFormBody` produces.
fn push_address(form: &mut Vec<(String, String)>, args: &Value) {
    for field in ["line1", "postal_code", "city", "state", "country"] {
        push_opt(form, &format!("address[{field}]"), opt_str(args, field));
    }
}

fn enum_of(args: &Value, key: &str, allowed: &[&str]) -> Result<Option<String>> {
    match opt_str(args, key) {
        Some(v) if allowed.contains(&v) => Ok(Some(v.to_string())),
        Some(_) => bail!("`{key}` must be one of: {}", allowed.join(", ")),
        None => Ok(None),
    }
}

// ---------------------------------------------------------------- time

/// ISO-8601 date or date-time -> epoch seconds, mirroring the piece's
/// `Math.floor(new Date(x).getTime() / 1000)`. Naive date-times are read as UTC.
fn epoch_seconds(key: &str, v: &str) -> Result<i64> {
    let bad = || anyhow!("`{key}` must be an ISO 8601 date or date-time, got \"{v}\"");
    let num = |s: &str| -> Result<i64> {
        s.bytes()
            .all(|b| b.is_ascii_digit())
            .then(|| s.parse().ok())
            .flatten()
            .ok_or_else(bad)
    };
    let b = v.as_bytes();
    if b.len() < 10 || b[4] != b'-' || b[7] != b'-' {
        return Err(bad());
    }
    let (y, mo, d) = (num(&v[0..4])?, num(&v[5..7])?, num(&v[8..10])?);
    if !(1..=12).contains(&mo) || !(1..=31).contains(&d) {
        return Err(bad());
    }
    let mut secs = days_from_civil(y, mo, d) * 86_400;
    let mut rest = &v[10..];
    if !rest.is_empty() {
        let Some(t) = rest
            .strip_prefix('T')
            .or_else(|| rest.strip_prefix('t'))
            .or_else(|| rest.strip_prefix(' '))
        else {
            return Err(bad());
        };
        if t.len() < 5 || t.as_bytes()[2] != b':' {
            return Err(bad());
        }
        let (hh, mi) = (num(&t[0..2])?, num(&t[3..5])?);
        if hh > 23 || mi > 59 {
            return Err(bad());
        }
        secs += hh * 3600 + mi * 60;
        rest = &t[5..];
        if let Some(s) = rest.strip_prefix(':') {
            if s.len() < 2 {
                return Err(bad());
            }
            let ss = num(&s[0..2])?;
            if ss > 60 {
                return Err(bad());
            }
            secs += ss;
            rest = &s[2..];
        }
        if let Some(frac) = rest.strip_prefix('.') {
            let digits = frac.bytes().take_while(|b| b.is_ascii_digit()).count();
            if digits == 0 {
                return Err(bad());
            }
            rest = &frac[digits..];
        }
        if rest.is_empty() || rest == "Z" || rest == "z" {
            // already UTC
        } else if rest.starts_with('+') || rest.starts_with('-') {
            // +01:30 means local time is ahead of UTC, so UTC is 1.5h earlier.
            let sign = if rest.starts_with('-') { 1 } else { -1 };
            let off = &rest[1..];
            let (oh, om) = match off.len() {
                5 if off.as_bytes()[2] == b':' => (num(&off[0..2])?, num(&off[3..5])?),
                4 => (num(&off[0..2])?, num(&off[2..4])?),
                _ => return Err(bad()),
            };
            if oh > 23 || om > 59 {
                return Err(bad());
            }
            secs += sign * (oh * 3600 + om * 60);
        } else {
            return Err(bad());
        }
    }
    Ok(secs)
}

/// `created_after` style props -> epoch-seconds query value.
fn epoch_opt(args: &Value, key: &str) -> Result<Option<String>> {
    match opt_str(args, key) {
        Some(v) => Ok(Some(epoch_seconds(key, v)?.to_string())),
        None => Ok(None),
    }
}

/// Days since 1970-01-01 (Howard Hinnant's `days_from_civil`).
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

// ---------------------------------------------------------------- encoding

/// Form-encoded request body (`application/x-www-form-urlencoded`), the encoding
/// every write action in the piece uses.
fn form_request(method: &str, url: String, pairs: &[(String, String)]) -> HttpRequest {
    let mut req = HttpRequest::new(method, url);
    if !pairs.is_empty() {
        req.body = Some(
            url::form_urlencoded::Serializer::new(String::new())
                .extend_pairs(pairs.iter().map(|(k, v)| (k.as_str(), v.as_str())))
                .finish()
                .into_bytes(),
        );
    }
    req.header("content-type", "application/x-www-form-urlencoded")
}

// ---------------------------------------------------------------- adapter

pub struct Stripe {
    spec: ToolkitSpec,
    api_base: String,
}

impl Default for Stripe {
    fn default() -> Self {
        Self::with_api_base("https://api.stripe.com/v1")
    }
}

#[allow(clippy::too_many_lines)]
impl Stripe {
    pub fn with_api_base(api_base: &str) -> Self {
        let t = |slug, title: (&str, &str), desc: (&str, &str), props: Props<'_>, req: &[&str], scopes: &[&str], m| {
            tool(SLUG, slug, title, desc, props, req, scopes, m)
        };
        let spec = ToolkitSpec {
            slug: SLUG.into(),
            name: "Stripe".into(),
            description: LocalizedText::new(
                "Zahlungen, Kunden, Rechnungen, Preise und Abos in Stripe lesen und verwalten.",
                "Read and manage payments, customers, invoices, prices and subscriptions in Stripe.",
            ),
            auth_schemes: vec![AuthScheme::Bearer],
            tools: vec![
                t(
                    "STRIPE_GET_CUSTOMER",
                    ("Kunden lesen", "Get customer"),
                    (
                        "Liest einen Kunden per ID (cus_…).",
                        "Reads one customer by ID (cus_…).",
                    ),
                    &[("customer_id", s("Kunden-ID, beginnt mit cus_"))],
                    &["customer_id"],
                    &[READ],
                    false,
                ),
                t(
                    "STRIPE_LIST_CUSTOMERS",
                    ("Kunden auflisten", "List customers"),
                    (
                        "Listet Kunden auf, optional nach exakter E-Mail gefiltert.",
                        "Lists customers, optionally filtered by exact email.",
                    ),
                    &[
                        ("email", s("Nur Kunden mit dieser exakten E-Mail")),
                        ("limit", limit_prop()),
                        ("starting_after", cursor("Kunden-ID; liefert die Seite nach diesem Cursor.")),
                    ],
                    &[],
                    &[READ],
                    false,
                ),
                t(
                    "STRIPE_SEARCH_CUSTOMERS",
                    ("Kunden suchen", "Search customers"),
                    (
                        "Durchsucht Kunden mit der Stripe-Suchabfrage (E-Mail, Name …).",
                        "Searches customers with the Stripe search query (email, name, …).",
                    ),
                    &[
                        ("email", s("Exakte E-Mail; wird zu email:'…'")),
                        ("query", s("Rohe Stripe-Suchabfrage, z. B. name:'Jane' – überschreibt E-Mail")),
                    ],
                    &[],
                    &[READ],
                    false,
                ),
                t(
                    "STRIPE_GET_INVOICE",
                    ("Rechnung lesen", "Get invoice"),
                    (
                        "Liest eine Rechnung per ID (in_…).",
                        "Reads one invoice by ID (in_…).",
                    ),
                    &[("invoice_id", s("Rechnungs-ID, beginnt mit in_"))],
                    &["invoice_id"],
                    &[READ],
                    false,
                ),
                t(
                    "STRIPE_LIST_INVOICES",
                    ("Rechnungen auflisten", "List invoices"),
                    (
                        "Listet Rechnungen auf, optional nach Kunde, Status oder Abo.",
                        "Lists invoices, optionally by customer, status or subscription.",
                    ),
                    &[
                        ("customer", s("Nur Rechnungen dieses Kunden (cus_…)")),
                        ("status", choice("Nur Rechnungen mit diesem Status", INVOICE_STATUS)),
                        ("subscription", s("Nur Rechnungen dieses Abos (sub_…)")),
                        ("limit", limit_prop()),
                    ],
                    &[],
                    &[READ],
                    false,
                ),
                t(
                    "STRIPE_SEARCH_INVOICES",
                    ("Rechnungen suchen", "Search invoices"),
                    (
                        "Durchsucht Rechnungen mit einer Stripe-Suchabfrage.",
                        "Searches invoices with a Stripe search query.",
                    ),
                    &[
                        ("query", s("Stripe-Suchabfrage, z. B. total>1000 AND status:'paid'")),
                        ("limit", limit_prop()),
                    ],
                    &["query"],
                    &[READ],
                    false,
                ),
                t(
                    "STRIPE_GET_PAYMENT_INTENT",
                    ("Zahlung lesen", "Get payment"),
                    (
                        "Liest eine Zahlung per PaymentIntent-ID (pi_…).",
                        "Reads one payment by PaymentIntent ID (pi_…).",
                    ),
                    &[("payment_intent_id", s("PaymentIntent-ID, beginnt mit pi_"))],
                    &["payment_intent_id"],
                    &[READ],
                    false,
                ),
                t(
                    "STRIPE_LIST_PAYMENT_INTENTS",
                    ("Zahlungen auflisten", "List payments"),
                    (
                        "Listet Zahlungen auf, optional nach Kunde oder Zeitraum.",
                        "Lists payments, optionally by customer or time range.",
                    ),
                    &[
                        ("customer", s("Nur Zahlungen dieses Kunden (cus_…)")),
                        ("created_after", moment("Nur Zahlungen ab diesem Zeitpunkt,")),
                        ("created_before", moment("Nur Zahlungen bis diesem Zeitpunkt,")),
                        ("limit", limit_prop()),
                        ("starting_after", cursor("PaymentIntent-ID-Cursor für die nächste Seite.")),
                    ],
                    &[],
                    &[READ],
                    false,
                ),
                t(
                    "STRIPE_GET_CHARGE",
                    ("Charge lesen", "Get charge"),
                    (
                        "Liest eine Charge (buchungstechnischer Beleg) per ID.",
                        "Reads one charge (the payment ledger entry) by ID.",
                    ),
                    &[("charge_id", s("Charge-ID, beginnt mit ch_"))],
                    &["charge_id"],
                    &[READ],
                    false,
                ),
                t(
                    "STRIPE_LIST_CHARGES",
                    ("Charges auflisten", "List charges"),
                    (
                        "Listet Charges auf, optional nach Kunde, PaymentIntent oder Zeitraum.",
                        "Lists charges, optionally by customer, PaymentIntent or time range.",
                    ),
                    &[
                        ("customer", s("Nur Charges dieses Kunden (cus_…)")),
                        ("payment_intent", s("Nur Charges dieser Zahlung (pi_…)")),
                        ("created_after", moment("Nur Charges ab diesem Zeitpunkt,")),
                        ("created_before", moment("Nur Charges bis diesem Zeitpunkt,")),
                        ("limit", limit_prop()),
                    ],
                    &[],
                    &[READ],
                    false,
                ),
                t(
                    "STRIPE_GET_REFUND",
                    ("Rückerstattung lesen", "Get refund"),
                    (
                        "Liest eine Rückerstattung per ID (re_…).",
                        "Reads one refund by ID (re_…).",
                    ),
                    &[("refund_id", s("Rückerstattungs-ID, beginnt mit re_"))],
                    &["refund_id"],
                    &[READ],
                    false,
                ),
                t(
                    "STRIPE_LIST_REFUNDS",
                    ("Rückerstattungen auflisten", "List refunds"),
                    (
                        "Listet Rückerstattungen auf, optional pro Zahlung oder Charge.",
                        "Lists refunds, optionally per payment or charge.",
                    ),
                    &[
                        ("payment_intent", s("Nur Rückerstattungen dieser Zahlung (pi_…)")),
                        ("charge", s("Nur Rückerstattungen dieser Charge (ch_…)")),
                        ("limit", limit_prop()),
                    ],
                    &[],
                    &[READ],
                    false,
                ),
                t(
                    "STRIPE_GET_PAYOUT",
                    ("Auszahlung lesen", "Get payout"),
                    (
                        "Liest eine Auszahlung aufs Bankkonto per ID (po_…).",
                        "Reads one bank-account payout by ID (po_…).",
                    ),
                    &[("payout_id", s("Auszahlungs-ID, beginnt mit po_"))],
                    &["payout_id"],
                    &[READ],
                    false,
                ),
                t(
                    "STRIPE_LIST_PAYOUTS",
                    ("Auszahlungen auflisten", "List payouts"),
                    (
                        "Listet Auszahlungen auf, optional nach Status oder Ankunftszeit.",
                        "Lists payouts, optionally by status or arrival date.",
                    ),
                    &[
                        ("status", choice("Nur Auszahlungen mit diesem Status", PAYOUT_STATUS)),
                        ("arrival_date_after", moment("Ankunftsdatum ab,")),
                        ("arrival_date_before", moment("Ankunftsdatum bis,")),
                        ("limit", limit_prop()),
                    ],
                    &[],
                    &[READ],
                    false,
                ),
                t(
                    "STRIPE_GET_BALANCE",
                    ("Guthaben lesen", "Get balance"),
                    (
                        "Liest das aktuelle Kontoguthaben (verfügbar + ausstehend).",
                        "Reads the current account balance (available + pending).",
                    ),
                    &[],
                    &[],
                    &[READ],
                    false,
                ),
                t(
                    "STRIPE_GET_PRODUCT",
                    ("Produkt lesen", "Get product"),
                    (
                        "Liest ein Produkt des Produktkatalogs per ID (prod_…).",
                        "Reads one catalog product by ID (prod_…).",
                    ),
                    &[("product_id", s("Produkt-ID, beginnt mit prod_"))],
                    &["product_id"],
                    &[READ],
                    false,
                ),
                t(
                    "STRIPE_LIST_PRODUCTS",
                    ("Produkte auflisten", "List products"),
                    (
                        "Listet Produkte auf, optional nur aktive oder inaktive.",
                        "Lists products, optionally only active or inactive ones.",
                    ),
                    &[
                        ("active", choice("Nach aktivem Status filtern", &["true", "false"])),
                        ("limit", limit_prop()),
                    ],
                    &[],
                    &[READ],
                    false,
                ),
                t(
                    "STRIPE_GET_PRICE",
                    ("Preis lesen", "Get price"),
                    (
                        "Liest einen Preis per ID (price_…).",
                        "Reads one price by ID (price_…).",
                    ),
                    &[("price_id", s("Preis-ID, beginnt mit price_"))],
                    &["price_id"],
                    &[READ],
                    false,
                ),
                t(
                    "STRIPE_LIST_PRICES",
                    ("Preise auflisten", "List prices"),
                    (
                        "Listet Preise auf, optional nach Produkt, Typ oder Währung.",
                        "Lists prices, optionally by product, type or currency.",
                    ),
                    &[
                        ("product", s("Nur Preise dieses Produkts (prod_…)")),
                        ("active", choice("Nach aktivem Status filtern", &["true", "false"])),
                        ("type", choice("Nur Einmal- oder Abo-Preise", &["one_time", "recurring"])),
                        ("currency", s("Dreibuchstaben-Währungscode, z. B. usd")),
                        ("limit", limit_prop()),
                    ],
                    &[],
                    &[READ],
                    false,
                ),
                t(
                    "STRIPE_GET_SUBSCRIPTION",
                    ("Abo lesen", "Get subscription"),
                    (
                        "Liest ein Abo per ID (sub_…).",
                        "Reads one subscription by ID (sub_…).",
                    ),
                    &[("subscription_id", s("Abo-ID, beginnt mit sub_"))],
                    &["subscription_id"],
                    &[READ],
                    false,
                ),
                t(
                    "STRIPE_LIST_SUBSCRIPTIONS",
                    ("Abos auflisten", "List subscriptions"),
                    (
                        "Listet Abos auf, optional nach Kunde, Status oder Preis.",
                        "Lists subscriptions, optionally by customer, status or price.",
                    ),
                    &[
                        ("customer", s("Nur Abos dieses Kunden (cus_…)")),
                        ("status", choice("Nur Abos mit diesem Status", SUBSCRIPTION_STATUS)),
                        ("price", s("Nur Abos mit diesem Preis (price_…)")),
                        ("limit", limit_prop()),
                    ],
                    &[],
                    &[READ],
                    false,
                ),
                t(
                    "STRIPE_CREATE_CUSTOMER",
                    ("Kunden anlegen", "Create customer"),
                    (
                        "Legt einen Kunden mit Kontaktdaten und Adresse an; braucht --confirm.",
                        "Creates a customer with contact details and address; requires --confirm.",
                    ),
                    &[
                        ("email", s("E-Mail, z. B. jane@example.com")),
                        ("name", s("Name, z. B. Jane Doe")),
                        ("description", s("Interne Notiz, nicht für den Kunden sichtbar")),
                        ("phone", s("Telefonnummer")),
                        ("line1", s("Adresszeile 1")),
                        ("postal_code", s("Postleitzahl")),
                        ("city", s("Stadt")),
                        ("state", s("Bundesland/Region")),
                        ("country", s("Zweibuchstaben-Ländercode, z. B. US oder DE")),
                    ],
                    &["email", "name"],
                    &[WRITE],
                    true,
                ),
                t(
                    "STRIPE_UPDATE_CUSTOMER",
                    ("Kunden ändern", "Update customer"),
                    (
                        "Ändert nur die übergebenen Felder eines Kunden; braucht --confirm.",
                        "Changes only the customer fields you pass; requires --confirm.",
                    ),
                    &[
                        ("customer_id", s("Kunden-ID, beginnt mit cus_")),
                        ("email", s("Neue E-Mail")),
                        ("name", s("Neuer Name")),
                        ("description", s("Neue interne Notiz")),
                        ("phone", s("Neue Telefonnummer")),
                        ("line1", s("Adresszeile 1")),
                        ("postal_code", s("Postleitzahl")),
                        ("city", s("Stadt")),
                        ("state", s("Bundesland/Region")),
                        ("country", s("Zweibuchstaben-Ländercode, z. B. US oder DE")),
                    ],
                    &["customer_id"],
                    &[WRITE],
                    true,
                ),
                t(
                    "STRIPE_CREATE_PRODUCT",
                    ("Produkt anlegen", "Create product"),
                    (
                        "Legt ein Produkt im Produktkatalog an; braucht --confirm.",
                        "Creates a catalog product; requires --confirm.",
                    ),
                    &[
                        ("name", s("Produktname, im Checkout sichtbar")),
                        ("description", s("Beschreibung, unter dem Produktnamen sichtbar")),
                        ("active", flag("Auf false setzen, um das Produkt zu verstecken")),
                        ("images", str_list("Bis zu 8 öffentliche Bild-URLs")),
                        ("url", s("Öffentliche Seite zu diesem Produkt")),
                        ("metadata", obj_prop("Zusätzliche Schlüssel/Wert-Daten, JSON-Objekt")),
                    ],
                    &["name"],
                    &[WRITE],
                    true,
                ),
                t(
                    "STRIPE_CREATE_PRICE",
                    ("Preis anlegen", "Create price"),
                    (
                        "Legt einen Einmal- oder Abo-Preis für ein Produkt an; braucht --confirm.",
                        "Creates a one-time or recurring price for a product; requires --confirm.",
                    ),
                    &[
                        ("product_id", s("Produkt-ID, beginnt mit prod_")),
                        ("unit_amount", money("Betrag in der Hauptwährung, z. B. 10.50 für 10,50 $")),
                        ("currency", s("Dreibuchstaben-Währungscode, z. B. usd")),
                        ("recurring_interval", choice("Abrechnungsintervall; one_time = Einmalzahlung", &["one_time", "day", "week", "month", "year"])),
                        ("recurring_interval_count", json!({"type": "integer",
                            "description": "Alle N Intervalle abrechnen, z. B. 3 mit month = quartalsweise.",
                            "minimum": 1, "maximum": 365})),
                    ],
                    &["product_id", "unit_amount", "currency", "recurring_interval"],
                    &[WRITE],
                    true,
                ),
                t(
                    "STRIPE_CREATE_INVOICE",
                    ("Rechnung anlegen", "Create invoice"),
                    (
                        "Legt eine Entwurfsrechnung für einen Kunden an; braucht --confirm.",
                        "Creates a draft invoice for a customer; requires --confirm.",
                    ),
                    &[
                        ("customer_id", s("Kunden-ID, beginnt mit cus_")),
                        ("currency", s("Dreibuchstaben-Währungscode, z. B. usd oder eur")),
                        ("description", s("Memo, das der Kunde auf der Rechnung sieht")),
                    ],
                    &["customer_id", "currency"],
                    &[WRITE],
                    true,
                ),
                t(
                    "STRIPE_CREATE_PAYMENT_INTENT",
                    ("Zahlung anlegen", "Create payment"),
                    (
                        "Legt eine Zahlung an; Betrag in Hauptwährung, z. B. 10.50. Braucht --confirm.",
                        "Creates a payment; amount in main currency units, e.g. 10.50. Requires --confirm.",
                    ),
                    &[
                        ("amount", money("Betrag in der Hauptwährung, z. B. 10.50 für 10,50 $")),
                        ("currency", s("Dreibuchstaben-Währungscode, z. B. usd")),
                        ("customer_id", s("Kunden-ID (cus_…), verknüpft die Zahlung")),
                        ("payment_method_id", s("Zahlungsmethode (pm_…), nötig bei „sofort abbuchen“")),
                        ("confirm", flag("Zahlungsmethode sofort abbuchen")),
                        ("return_url", s("Rückkehr-URL nach einer Bankprüfung, nötig bei „sofort abbuchen“")),
                        ("description", s("Beschreibung, im Stripe-Dashboard sichtbar")),
                        ("receipt_email", s("Beleg an diese E-Mail statt an die Kunden-E-Mail")),
                    ],
                    &["amount", "currency"],
                    &[WRITE],
                    true,
                ),
                t(
                    "STRIPE_CREATE_REFUND",
                    ("Rückerstattung anlegen", "Create refund"),
                    (
                        "Erstattet eine Zahlung ganz oder teilweise; braucht --confirm.",
                        "Refunds a payment in full or in part; requires --confirm.",
                    ),
                    &[
                        ("payment_intent_id", s("PaymentIntent-ID der Zahlung (pi_…)")),
                        ("amount", money("Teilbetrag in der Hauptwährung; leer = voller Betrag")),
                        ("reason", choice("Grund", &["duplicate", "fraudulent", "requested_by_customer"])),
                        ("metadata", obj_prop("Zusätzliche Schlüssel/Wert-Daten, JSON-Objekt")),
                    ],
                    &["payment_intent_id"],
                    &[WRITE],
                    true,
                ),
                t(
                    "STRIPE_FINALIZE_INVOICE",
                    ("Rechnung abschließen", "Finalize invoice"),
                    (
                        "Schließt eine Entwurfsrechnung ab und macht sie zahlbar; braucht --confirm.",
                        "Finalizes a draft invoice so it becomes payable; requires --confirm.",
                    ),
                    &[
                        ("invoice_id", s("Entwurfs-Rechnungs-ID, beginnt mit in_")),
                        ("auto_advance", flag("Stripe zieht die Rechnung nach dem Abschluss automatisch ein")),
                    ],
                    &["invoice_id"],
                    &[WRITE],
                    true,
                ),
                t(
                    "STRIPE_CANCEL_SUBSCRIPTION",
                    ("Abo kündigen", "Cancel subscription"),
                    (
                        "Kündigt ein Abo sofort oder zum Periodenende; braucht --confirm.",
                        "Cancels a subscription now or at period end; requires --confirm.",
                    ),
                    &[
                        ("subscription_id", s("Abo-ID, beginnt mit sub_")),
                        ("cancel_at_period_end", flag("An: läuft bis zum Periodenende weiter. Aus: endet sofort.")),
                    ],
                    &["subscription_id"],
                    &[WRITE],
                    true,
                ),
            ],
        };
        Self {
            spec,
            api_base: api_base.trim_end_matches('/').to_string(),
        }
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.api_base, path)
    }

    /// GET with optional percent-encoded query pairs.
    fn get(&self, path: &str, pairs: &[(&str, Option<String>)]) -> Result<HttpRequest> {
        Ok(HttpRequest::new("GET", self.url(&format!("{path}{}", query(pairs)))))
    }

    /// HTTP request for `tool`, mirroring the Activepieces `run()` bodies.
    #[allow(clippy::too_many_lines)]
    fn request(&self, tool: &str, args: &Value) -> Result<HttpRequest> {
        let mut form: Vec<(String, String)> = Vec::new();
        match tool {
            // AP: lib/actions/retrieve-customer.ts (get-customer.ts is the same call)
            "STRIPE_GET_CUSTOMER" => {
                let id = stripe_id(args, "customer_id", "cus_")?;
                self.get(&format!("/customers/{id}"), &[])
            }
            // AP: lib/actions/list-customers.ts
            "STRIPE_LIST_CUSTOMERS" => self.get(
                "/customers",
                &[
                    ("email", opt_str(args, "email").map(String::from)),
                    ("limit", opt_limit(args)),
                    ("starting_after", opt_str(args, "starting_after").map(String::from)),
                ],
            ),
            // AP: lib/actions/search-customers.ts (search-customer.ts adds the same
            // email:'…' escaping for its Email-only variant)
            "STRIPE_SEARCH_CUSTOMERS" => {
                let q = match opt_str(args, "query") {
                    Some(q) => q.to_string(),
                    None => match opt_str(args, "email") {
                        Some(e) => {
                            format!("email:'{}'", e.replace('\\', "\\\\").replace('\'', "\\'"))
                        }
                        None => bail!("provide either `query` or `email` to search"),
                    },
                };
                self.get("/customers/search", &[("query", Some(q))])
            }
            // AP: lib/actions/retrieve-invoice.ts (get-invoice.ts / find-invoice.ts are the same call)
            "STRIPE_GET_INVOICE" => {
                let id = stripe_id(args, "invoice_id", "in_")?;
                self.get(&format!("/invoices/{id}"), &[])
            }
            // AP: lib/actions/list-invoices.ts
            "STRIPE_LIST_INVOICES" => self.get(
                "/invoices",
                &[
                    ("customer", opt_str(args, "customer").map(String::from)),
                    ("status", enum_of(args, "status", INVOICE_STATUS)?),
                    ("subscription", opt_str(args, "subscription").map(String::from)),
                    ("limit", opt_limit(args)),
                ],
            ),
            // AP: lib/actions/search-invoices.ts
            "STRIPE_SEARCH_INVOICES" => self.get(
                "/invoices/search",
                &[
                    ("query", Some(req_str(args, "query")?.to_string())),
                    ("limit", opt_limit(args)),
                ],
            ),
            // AP: lib/actions/retrieve-payment-intent.ts (get-payment-intent.ts is the same call)
            "STRIPE_GET_PAYMENT_INTENT" => {
                let id = stripe_id(args, "payment_intent_id", "pi_")?;
                self.get(&format!("/payment_intents/{id}"), &[])
            }
            // AP: lib/actions/list-payment-intents.ts
            "STRIPE_LIST_PAYMENT_INTENTS" => self.get(
                "/payment_intents",
                &[
                    ("customer", opt_str(args, "customer").map(String::from)),
                    ("limit", opt_limit(args)),
                    ("starting_after", opt_str(args, "starting_after").map(String::from)),
                    ("created[gte]", epoch_opt(args, "created_after")?),
                    ("created[lte]", epoch_opt(args, "created_before")?),
                ],
            ),
            // AP: lib/actions/retrieve-charge.ts
            "STRIPE_GET_CHARGE" => {
                let id = stripe_id(args, "charge_id", "ch_")?;
                self.get(&format!("/charges/{id}"), &[])
            }
            // AP: lib/actions/list-charges.ts
            "STRIPE_LIST_CHARGES" => self.get(
                "/charges",
                &[
                    ("customer", opt_str(args, "customer").map(String::from)),
                    ("payment_intent", opt_str(args, "payment_intent").map(String::from)),
                    ("limit", opt_limit(args)),
                    ("created[gte]", epoch_opt(args, "created_after")?),
                    ("created[lte]", epoch_opt(args, "created_before")?),
                ],
            ),
            // AP: lib/actions/get-refund.ts
            "STRIPE_GET_REFUND" => {
                let id = stripe_id(args, "refund_id", "re_")?;
                self.get(&format!("/refunds/{id}"), &[])
            }
            // AP: lib/actions/list-refunds.ts
            "STRIPE_LIST_REFUNDS" => self.get(
                "/refunds",
                &[
                    ("payment_intent", opt_str(args, "payment_intent").map(String::from)),
                    ("charge", opt_str(args, "charge").map(String::from)),
                    ("limit", opt_limit(args)),
                ],
            ),
            // AP: lib/actions/retrieve-payout.ts (get-payout.ts is the same call)
            "STRIPE_GET_PAYOUT" => {
                let id = stripe_id(args, "payout_id", "po_")?;
                self.get(&format!("/payouts/{id}"), &[])
            }
            // AP: lib/actions/list-payouts.ts
            "STRIPE_LIST_PAYOUTS" => self.get(
                "/payouts",
                &[
                    ("status", enum_of(args, "status", PAYOUT_STATUS)?),
                    ("limit", opt_limit(args)),
                    ("arrival_date[gte]", epoch_opt(args, "arrival_date_after")?),
                    ("arrival_date[lte]", epoch_opt(args, "arrival_date_before")?),
                ],
            ),
            // AP: lib/actions/retrieve-balance.ts
            "STRIPE_GET_BALANCE" => self.get("/balance", &[]),
            // AP: lib/actions/get-product.ts
            "STRIPE_GET_PRODUCT" => {
                let id = stripe_id(args, "product_id", "prod_")?;
                self.get(&format!("/products/{id}"), &[])
            }
            // AP: lib/actions/list-products.ts
            "STRIPE_LIST_PRODUCTS" => self.get(
                "/products",
                &[
                    ("active", enum_of(args, "active", &["true", "false"])?),
                    ("limit", opt_limit(args)),
                ],
            ),
            // AP: lib/actions/get-price.ts
            "STRIPE_GET_PRICE" => {
                let id = stripe_id(args, "price_id", "price_")?;
                self.get(&format!("/prices/{id}"), &[])
            }
            // AP: lib/actions/list-prices.ts
            "STRIPE_LIST_PRICES" => self.get(
                "/prices",
                &[
                    ("product", opt_str(args, "product").map(String::from)),
                    ("active", enum_of(args, "active", &["true", "false"])?),
                    ("type", enum_of(args, "type", &["one_time", "recurring"])?),
                    ("currency", opt_str(args, "currency").map(String::from)),
                    ("limit", opt_limit(args)),
                ],
            ),
            // AP: lib/actions/get-subscription.ts
            "STRIPE_GET_SUBSCRIPTION" => {
                let id = stripe_id(args, "subscription_id", "sub_")?;
                self.get(&format!("/subscriptions/{id}"), &[])
            }
            // AP: lib/actions/list-subscriptions.ts
            "STRIPE_LIST_SUBSCRIPTIONS" => self.get(
                "/subscriptions",
                &[
                    ("customer", opt_str(args, "customer").map(String::from)),
                    ("status", enum_of(args, "status", SUBSCRIPTION_STATUS)?),
                    ("price", opt_str(args, "price").map(String::from)),
                    ("limit", opt_limit(args)),
                ],
            ),
            // AP: lib/actions/create-customer.ts
            "STRIPE_CREATE_CUSTOMER" => {
                form.push(("email".into(), req_str(args, "email")?.to_string()));
                form.push(("name".into(), req_str(args, "name")?.to_string()));
                push_opt(&mut form, "description", opt_str(args, "description"));
                push_opt(&mut form, "phone", opt_str(args, "phone"));
                push_address(&mut form, args);
                Ok(form_request("POST", self.url("/customers"), &form))
            }
            // AP: lib/actions/update-customer.ts
            "STRIPE_UPDATE_CUSTOMER" => {
                let id = stripe_id(args, "customer_id", "cus_")?;
                push_opt(&mut form, "email", opt_str(args, "email"));
                push_opt(&mut form, "name", opt_str(args, "name"));
                push_opt(&mut form, "description", opt_str(args, "description"));
                push_opt(&mut form, "phone", opt_str(args, "phone"));
                push_address(&mut form, args);
                if form.is_empty() {
                    bail!("nothing to update: pass at least one field to change");
                }
                Ok(form_request("POST", self.url(&format!("/customers/{id}")), &form))
            }
            // AP: lib/actions/create-product.ts
            "STRIPE_CREATE_PRODUCT" => {
                form.push(("name".into(), req_str(args, "name")?.to_string()));
                push_opt(&mut form, "description", opt_str(args, "description"));
                if let Some(active) = args.get("active").and_then(Value::as_bool) {
                    form.push(("active".into(), active.to_string()));
                }
                push_opt(&mut form, "url", opt_str(args, "url"));
                for (index, image) in str_vec(args, "images").into_iter().enumerate() {
                    form.push((format!("images[{index}]"), image));
                }
                put_metadata(&mut form, args, "metadata")?;
                Ok(form_request("POST", self.url("/products"), &form))
            }
            // AP: lib/actions/create-price.ts
            "STRIPE_CREATE_PRICE" => {
                form.push(("product".into(), stripe_id(args, "product_id", "prod_")?));
                form.push(("unit_amount".into(), cents(args, "unit_amount")?.to_string()));
                form.push(("currency".into(), req_str(args, "currency")?.to_string()));
                match enum_of(args, "recurring_interval", &["one_time", "day", "week", "month", "year"])?.as_deref() {
                    None | Some("one_time") => {}
                    Some(interval) => {
                        form.push(("recurring[interval]".into(), interval.to_string()));
                        if let Some(n) = args
                            .get("recurring_interval_count")
                            .and_then(Value::as_u64)
                        {
                            form.push(("recurring[interval_count]".into(), n.to_string()));
                        }
                    }
                }
                Ok(form_request("POST", self.url("/prices"), &form))
            }
            // AP: lib/actions/create-invoice.ts
            "STRIPE_CREATE_INVOICE" => {
                form.push(("customer".into(), stripe_id(args, "customer_id", "cus_")?));
                form.push(("currency".into(), req_str(args, "currency")?.to_string()));
                push_opt(&mut form, "description", opt_str(args, "description"));
                Ok(form_request("POST", self.url("/invoices"), &form))
            }
            // AP: lib/actions/create-payment-intent.ts
            "STRIPE_CREATE_PAYMENT_INTENT" => {
                let confirm = flag_value(args, "confirm");
                let payment_method = opt_str(args, "payment_method_id");
                let return_url = opt_str(args, "return_url");
                if confirm && payment_method.is_none() {
                    bail!("add `payment_method_id`, or turn `confirm` off");
                }
                if confirm && return_url.is_none() {
                    bail!("add `return_url`, or turn `confirm` off");
                }
                form.push(("amount".into(), cents(args, "amount")?.to_string()));
                form.push(("currency".into(), req_str(args, "currency")?.to_string()));
                push_opt(&mut form, "customer", opt_str(args, "customer_id"));
                push_opt(&mut form, "payment_method", payment_method);
                if confirm {
                    form.push(("confirm".into(), "true".into()));
                }
                push_opt(&mut form, "return_url", return_url);
                push_opt(&mut form, "description", opt_str(args, "description"));
                push_opt(&mut form, "receipt_email", opt_str(args, "receipt_email"));
                Ok(form_request("POST", self.url("/payment_intents"), &form))
            }
            // AP: lib/actions/create-refund.ts
            "STRIPE_CREATE_REFUND" => {
                form.push((
                    "payment_intent".into(),
                    stripe_id(args, "payment_intent_id", "pi_")?,
                ));
                if args.get("amount").and_then(Value::as_f64).is_some() {
                    form.push(("amount".into(), cents(args, "amount")?.to_string()));
                }
                push_opt(&mut form, "reason", enum_of(args, "reason", &["duplicate", "fraudulent", "requested_by_customer"])?.as_deref());
                put_metadata(&mut form, args, "metadata")?;
                Ok(form_request("POST", self.url("/refunds"), &form))
            }
            // AP: lib/actions/finalize-invoice.ts
            "STRIPE_FINALIZE_INVOICE" => {
                let id = stripe_id(args, "invoice_id", "in_")?;
                if let Some(auto_advance) = args.get("auto_advance").and_then(Value::as_bool) {
                    form.push(("auto_advance".into(), auto_advance.to_string()));
                }
                Ok(form_request(
                    "POST",
                    self.url(&format!("/invoices/{id}/finalize")),
                    &form,
                ))
            }
            // AP: lib/actions/cancel-subscription.ts — the piece calls the `stripe`
            // Node SDK (`client.subscriptions.update(sub, {cancel_at_period_end: true})`
            // / `client.subscriptions.cancel(sub)`); the SDK methods are the documented
            // REST calls POST /v1/subscriptions/{id} and DELETE /v1/subscriptions/{id}.
            "STRIPE_CANCEL_SUBSCRIPTION" => {
                let id = stripe_id(args, "subscription_id", "sub_")?;
                if flag_value(args, "cancel_at_period_end") {
                    form.push(("cancel_at_period_end".into(), "true".into()));
                    Ok(form_request(
                        "POST",
                        self.url(&format!("/subscriptions/{id}")),
                        &form,
                    ))
                } else {
                    Ok(form_request(
                        "DELETE",
                        self.url(&format!("/subscriptions/{id}")),
                        &form,
                    ))
                }
            }
            other => bail!("unknown Stripe tool `{other}`"),
        }
    }
}

impl ToolkitAdapter for Stripe {
    fn spec(&self) -> &ToolkitSpec {
        &self.spec
    }

    fn build_request(&self, tool: &str, args: &Value) -> Result<HttpRequest> {
        self.request(tool, args)
    }
    // `parse_response` stays the default: Stripe answers 2xx calls with the JSON
    // object itself (the piece returns `response.body` verbatim); non-2xx bodies
    // (`{"error": …}`) are turned into provider errors by the hub.
}

#[cfg(test)]
mod tests {
    use super::*;

    fn form_of(req: &HttpRequest) -> Vec<(String, String)> {
        url::form_urlencoded::parse(req.body.as_deref().expect("form body"))
            .map(|(k, v)| (k.into_owned(), v.into_owned()))
            .collect()
    }

    #[test]
    fn paths_and_prefixes() {
        let s = Stripe::with_api_base("https://stripe.test/v1");
        let r = s
            .build_request("STRIPE_GET_CUSTOMER", &json!({"customer_id": "cus_Ab12-_."}))
            .unwrap();
        assert_eq!(r.method, "GET");
        assert_eq!(r.url, "https://stripe.test/v1/customers/cus_Ab12-_.");
        assert!(r.body.is_none());

        for (tool, key, id) in [
            ("STRIPE_GET_CUSTOMER", "customer_id", "pi_bad"),
            ("STRIPE_GET_INVOICE", "invoice_id", "cus_bad"),
            ("STRIPE_GET_PRICE", "price_id", "prod_bad"),
        ] {
            assert!(
                s.build_request(tool, &json!({key: id})).is_err(),
                "{tool} must reject {id}"
            );
        }
        assert!(s
            .build_request("STRIPE_GET_CUSTOMER", &json!({"customer_id": "../x"}))
            .is_err());

        // Default base URL is the real Stripe API.
        assert_eq!(
            Stripe::default()
                .build_request("STRIPE_GET_BALANCE", &json!({}))
                .unwrap()
                .url,
            "https://api.stripe.com/v1/balance"
        );
    }

    #[test]
    fn list_queries_are_encoded_and_optional() {
        let s = Stripe::default();
        let r = s
            .build_request(
                "STRIPE_LIST_PAYMENT_INTENTS",
                &json!({"customer": "cus_X1", "limit": 5, "created_after": "2026-01-31T00:00:00Z"}),
            )
            .unwrap();
        assert_eq!(
            r.url,
            "https://api.stripe.com/v1/payment_intents?customer=cus_X1&limit=5\
             &created%5Bgte%5D=1769817600"
        );

        // Without a limit the parameter is not sent (Stripe's own default applies).
        let r = s.build_request("STRIPE_LIST_CUSTOMERS", &json!({})).unwrap();
        assert_eq!(r.url, "https://api.stripe.com/v1/customers");
        let r = s
            .build_request("STRIPE_LIST_CUSTOMERS", &json!({"limit": 500}))
            .unwrap();
        assert_eq!(r.url, "https://api.stripe.com/v1/customers?limit=100");

        let err = s
            .build_request("STRIPE_LIST_INVOICES", &json!({"status": "nope"}))
            .unwrap_err();
        assert!(err.to_string().contains("`status` must be one of"), "{err}");
    }

    #[test]
    fn search_query_is_built_and_escaped() {
        let s = Stripe::default();
        let r = s
            .build_request("STRIPE_SEARCH_CUSTOMERS", &json!({"email": "ja'ne@x.com"}))
            .unwrap();
        assert!(r.url.starts_with("https://api.stripe.com/v1/customers/search?query="));
        assert_eq!(
            r.url.rsplit('=').next().unwrap(),
            "email%3A%27ja%5C%27ne%40x.com%27"
        );
        let r = s
            .build_request("STRIPE_SEARCH_CUSTOMERS", &json!({"query": "name:'Jane'"}))
            .unwrap();
        assert!(r.url.contains("query=name%3A%27Jane%27"), "{}", r.url);
        assert!(s
            .build_request("STRIPE_SEARCH_CUSTOMERS", &json!({}))
            .is_err());
    }

    #[test]
    fn write_bodies_mirror_to_form_body() {
        let s = Stripe::default();

        // create-customer: flat fields + address[...] nesting, nil values dropped.
        let r = s
            .build_request(
                "STRIPE_CREATE_CUSTOMER",
                &json!({"email": "jane@example.com", "name": "Jane Doe", "city": "Berlin"}),
            )
            .unwrap();
        assert_eq!(r.method, "POST");
        assert_eq!(r.url, "https://api.stripe.com/v1/customers");
        assert_eq!(
            form_of(&r),
            vec![
                ("email".into(), "jane@example.com".into()),
                ("name".into(), "Jane Doe".into()),
                ("address[city]".into(), "Berlin".into()),
            ]
        );

        // update-customer refuses empty updates.
        assert!(s
            .build_request("STRIPE_UPDATE_CUSTOMER", &json!({"customer_id": "cus_1"}))
            .is_err());

        // create-price: main-unit amount -> cents, recurring only for subscriptions.
        let r = s
            .build_request(
                "STRIPE_CREATE_PRICE",
                &json!({"product_id": "prod_1", "unit_amount": 12.99, "currency": "usd",
                        "recurring_interval": "month", "recurring_interval_count": 3}),
            )
            .unwrap();
        assert_eq!(
            form_of(&r),
            vec![
                ("product".into(), "prod_1".into()),
                ("unit_amount".into(), "1299".into()),
                ("currency".into(), "usd".into()),
                ("recurring[interval]".into(), "month".into()),
                ("recurring[interval_count]".into(), "3".into()),
            ]
        );
        let r = s
            .build_request(
                "STRIPE_CREATE_PRICE",
                &json!({"product_id": "prod_1", "unit_amount": 10.50, "currency": "usd",
                        "recurring_interval": "one_time"}),
            )
            .unwrap();
        assert!(!form_of(&r).iter().any(|(k, _)| k.starts_with("recurring")));

        // create-payment-intent: confirm requires payment method and return URL.
        assert!(s
            .build_request(
                "STRIPE_CREATE_PAYMENT_INTENT",
                &json!({"amount": 10.5, "currency": "usd", "confirm": true})
            )
            .is_err());
        let r = s
            .build_request(
                "STRIPE_CREATE_PAYMENT_INTENT",
                &json!({"amount": 10.5, "currency": "usd", "customer_id": "cus_1",
                        "payment_method_id": "pm_1", "confirm": true,
                        "return_url": "https://example.com/return", "metadata": {"a": 1}}),
            )
            .unwrap();
        let form = form_of(&r);
        for (k, v) in [
            ("amount", "1050"),
            ("currency", "usd"),
            ("customer", "cus_1"),
            ("payment_method", "pm_1"),
            ("confirm", "true"),
            ("return_url", "https://example.com/return"),
        ] {
            assert!(form.contains(&(k.to_string(), v.to_string())), "{form:?}");
        }

        // create-refund: metadata flattens to metadata[k]=v.
        let r = s
            .build_request(
                "STRIPE_CREATE_REFUND",
                &json!({"payment_intent_id": "pi_1", "amount": 2.05,
                        "reason": "duplicate", "metadata": {"order": "6735"}}),
            )
            .unwrap();
        assert_eq!(
            form_of(&r),
            vec![
                ("payment_intent".into(), "pi_1".into()),
                ("amount".into(), "205".into()),
                ("reason".into(), "duplicate".into()),
                ("metadata[order]".into(), "6735".into()),
            ]
        );

        // cancel-subscription: SDK update -> POST, SDK cancel -> DELETE.
        let r = s
            .build_request(
                "STRIPE_CANCEL_SUBSCRIPTION",
                &json!({"subscription_id": "sub_1", "cancel_at_period_end": true}),
            )
            .unwrap();
        assert_eq!(r.method, "POST");
        assert_eq!(r.url, "https://api.stripe.com/v1/subscriptions/sub_1");
        assert_eq!(form_of(&r), vec![("cancel_at_period_end".into(), "true".into())]);
        let r = s
            .build_request("STRIPE_CANCEL_SUBSCRIPTION", &json!({"subscription_id": "sub_1"}))
            .unwrap();
        assert_eq!(r.method, "DELETE");
        assert!(r.body.is_none());
    }

    #[test]
    fn epoch_conversion() {
        assert_eq!(epoch_seconds("t", "2026-01-31").unwrap(), 1_769_817_600);
        assert_eq!(epoch_seconds("t", "2026-01-31T00:00:00Z").unwrap(), 1_769_817_600);
        assert_eq!(
            epoch_seconds("t", "2026-05-01T12:30:00+02:00").unwrap(),
            1_777_631_400
        );
        assert_eq!(epoch_seconds("t", "2026-05-01T00:00:00.500Z").unwrap(), 1_777_593_600);
        for bad in ["", "31.01.2026", "2026-13-01", "2026-01-31T25:00:00Z", "2026-01-31 nope"] {
            assert!(epoch_seconds("t", bad).is_err(), "{bad} must not parse");
        }
        let s = Stripe::default();
        assert!(s
            .build_request("STRIPE_LIST_CHARGES", &json!({"created_after": "yesterday"}))
            .is_err());
    }
}

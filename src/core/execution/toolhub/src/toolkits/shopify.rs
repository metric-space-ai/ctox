//! Shopify toolkit (Admin REST API, API version `2026-10`). Action set modelled on the
//! non-AI actions of Activepieces `piece-shopify` (MIT).
//!
//! DE: Die 25 Aktionen stammen alle aus `src/lib/common/index.ts` der Activepieces-Piece;
//! jede Anfrage unten ist `httpClient.sendRequest` 1:1 nachgebaut (Pfad, Methode, Body-Felder).
//! EN: All 25 actions come from the piece's `src/lib/common/index.ts`; every request below is a
//! 1:1 reproduction of its `httpClient.sendRequest` call (path, method, body fields).
//!
//! Auth: die Piece kennt zwei Varianten, beide senden denselben Header
//! `X-Shopify-Access-Token: <token>` (kein Prefix) – der Legacy-Custom-App-Admin-Token
//! (`CustomAuth`) und das Token des Dev-Dashboard-Apps (`OAuth2` client_credentials gegen
//! `https://{shopName}.myshopify.com/admin/oauth/access_token`). Die Dev-Dashboard-Variante
//! lässt sich hier nicht abbilden: der Token-Endpunkt ist pro Shop und es gibt keinen
//! Authorization-Code-Flow, den der Hub fahren kann. Deshalb wird `AuthScheme::ApiKey` mit
//! dem Header der Piece verwendet; das Client-Credentials-Token wird damit genauso gesendet.
//!
//! Basis-URL: `https://{shop}.myshopify.com/admin/api/2026-10` (Piece: `getBaseUrl` +
//! `SHOPIFY_API_VERSION`). Die Basis-URL hängt vom Store ab, deshalb hat jedes Tool ein
//! erforderliches `shop_name` (derselbe Pflicht-Wert wie `shopName` in der Piece-Auth) und
//! `with_api_base` pinnt die Basis einmal pro Instanz, wenn der Host den Store kennt.
//! Es ist bewusst **keine** Beispiel-Domain hartkodiert.
//!
//! Bewusst nicht portiert: `upload_product_image` (Datei-Upload via `Property.File`),
//! `custom_api_call` und alle `actions/ai/*`-Aktionen (KI-Assistent) sowie Trigger/Webhooks.
//!
//! Zwei Piece-Props sind bewusst nicht im Schema, weil die Piece sie im `run()` nie an die
//! API schickt und ein Body-Feld ohne TS-Quelle nicht erlaubt wäre: das `status`-Dropdown von
//! `create_product`/`update_product` (steht im Props-Objekt, wird aber nicht ins `product`
//! gelegt) und `authorization` von `create_transaction` (wird im `run()` nicht an
//! `createTransaction` übergeben).
use anyhow::{bail, Result};
use serde_json::{json, Map, Value};

use super::common::*;
use super::{path_segment, ToolkitAdapter};
use crate::http::{HttpRequest, HttpResponse};
use crate::model::*;

pub const SLUG: &str = "shopify";

/// `SHOPIFY_API_VERSION` in `src/lib/common/auth.ts`.
pub const API_VERSION: &str = "2026-10";

const READ_CUSTOMERS: &str = "read_customers";
const WRITE_CUSTOMERS: &str = "write_customers";
const READ_PRODUCTS: &str = "read_products";
const WRITE_PRODUCTS: &str = "write_products";
const READ_ORDERS: &str = "read_orders";
const WRITE_ORDERS: &str = "write_orders";
const WRITE_DRAFT_ORDERS: &str = "write_draft_orders";
const READ_FULFILLMENTS: &str = "read_fulfillments";
const WRITE_FULFILLMENTS: &str = "write_fulfillments";
const READ_LOCATIONS: &str = "read_locations";
const WRITE_INVENTORY: &str = "write_inventory";
const READ_THEMES: &str = "read_themes";
const WRITE_CONTENT: &str = "write_content";

/// `ShopifyFulfillmentEventStatuses` in `src/lib/common/types.ts`.
const FULFILLMENT_EVENT_STATUSES: &[&str] = &[
    "attempted_delivery",
    "carrier_picked_up",
    "confirmed",
    "delayed",
    "delivered",
    "failure",
    "in_transit",
    "label_printed",
    "label_purchased",
    "out_for_delivery",
    "picked_up",
    "ready_for_pickup",
];
/// `ShopifyTransactionKinds` in `src/lib/common/types.ts`.
const TRANSACTION_KINDS: &[&str] = &["authorization", "sale", "capture", "void", "refund"];

pub struct Shopify {
    spec: ToolkitSpec,
    /// `Some` when the host pinned the store (see `with_api_base`); then `shop_name`
    /// from the tool args is not used to build the URL.
    api_base: Option<String>,
}

impl Default for Shopify {
    fn default() -> Self {
        Self { spec: Self::build_spec(), api_base: None }
    }
}

/// Optional Shopify id as a number. The piece uses `Property.Number` for most ids and
/// `Property.ShortText` for the customer/order ones, so both JSON shapes are accepted here;
/// the value itself must be numeric, like every Shopify REST id.
fn opt_id(args: &Value, key: &str) -> Result<Option<u64>> {
    match args.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Number(n)) => n
            .as_u64()
            .map(Some)
            .ok_or_else(|| anyhow::anyhow!("`{key}` must be a positive id")),
        Some(Value::String(v)) if v.trim().is_empty() => Ok(None),
        Some(Value::String(_)) => id_segment(args, key)?
            .parse::<u64>()
            .map(Some)
            .map_err(|_| anyhow::anyhow!("`{key}` is out of range")),
        Some(_) => bail!("`{key}` must be a numeric Shopify id"),
    }
}

/// Shopify object id as a validated path segment (digits only).
fn id_segment(args: &Value, key: &str) -> Result<String> {
    let raw = match args.get(key) {
        Some(Value::Number(n)) if n.is_u64() => n.to_string(),
        Some(Value::String(_)) => path_segment(args, key)?.to_string(),
        _ => bail!("missing argument `{key}`"),
    };
    if raw.is_empty() || !raw.bytes().all(|b| b.is_ascii_digit()) {
        bail!("`{key}` must be a numeric Shopify id");
    }
    Ok(raw)
}

/// `SHOP_NAME_PATTERN` (`^[a-zA-Z0-9][a-zA-Z0-9-]*$`) from `src/lib/common/auth.ts`.
fn valid_shop_name(shop: &str) -> bool {
    let mut bytes = shop.bytes();
    match bytes.next() {
        Some(b) if b.is_ascii_alphanumeric() => {}
        _ => return false,
    }
    bytes.all(|b| b.is_ascii_alphanumeric() || b == b'-')
}

/// Numeric id prop (`Property.Number` in the piece), without a meaningless default.
fn num_id(desc: &str) -> Value {
    json!({"type": "integer", "description": desc, "minimum": 1})
}

/// `Property.Number` that may be negative (`available_adjustment`).
fn num_any(desc: &str) -> Value {
    json!({"type": "integer", "description": desc})
}

/// `email_marketing_consent` of `create_customer` / `update_customer`.
fn marketing_consent(args: &Value, fallback: &str) -> Value {
    let state = if args.get("accepts_marketing").and_then(Value::as_bool).unwrap_or(false) {
        "subscribed"
    } else {
        fallback
    };
    json!({"state": state, "opt_in_level": "unknown"})
}

/// Only the keys the caller actually set (the piece drops `undefined` fields when the
/// request body is serialised).
fn put(m: &mut Map<String, Value>, args: &Value, key: &str, field: &str) {
    if let Some(v) = opt_str(args, key) {
        m.insert(field.into(), json!(v));
    }
}

fn put_id(m: &mut Map<String, Value>, args: &Value, key: &str, field: &str) -> Result<()> {
    if let Some(id) = opt_id(args, key)? {
        m.insert(field.into(), json!(id));
    }
    Ok(())
}

fn flag(desc: &str, default: bool) -> Value {
    json!({"type": "boolean", "description": desc, "default": default})
}

fn choice(desc: &str, options: &[&str]) -> Value {
    json!({"type": "string", "description": desc, "enum": options})
}

impl Shopify {
    /// Pin the Admin API root of one store, e.g. `https://my-shop.myshopify.com/admin/api/2026-10`.
    /// Without it every call derives the root from its `shop_name` argument.
    pub fn with_api_base(api_base: &str) -> Self {
        Self {
            spec: Self::build_spec(),
            api_base: Some(api_base.trim_end_matches('/').to_string()),
        }
    }

    /// Pin the store by name (validates it like the piece's `isValidShopName`).
    pub fn with_shop_name(shop_name: &str) -> Result<Self> {
        let base = shop_base(shop_name)?;
        Ok(Self::with_api_base(&base))
    }

    fn build_spec() -> ToolkitSpec {
        let t = |slug, title, desc, props: Props<'_>, req: &[&str], scopes: &[&str], m| {
            tool(SLUG, slug, title, desc, props, req, scopes, m)
        };
        ToolkitSpec {
            slug: SLUG.into(),
            name: "Shopify".into(),
            description: LocalizedText::new(
                "Shopify-Admin-API: Produkte, Kunden, Bestellungen, Fulfillments, Transaktionen und Lager lesen und bearbeiten.",
                "Shopify Admin API: read and write products, customers, orders, fulfillments, transactions and locations.",
            ),
            auth_schemes: vec![AuthScheme::ApiKey {
                header_name: "X-Shopify-Access-Token".into(),
                prefix: String::new(),
            }],
            tools: vec![
                // --- read / search ---
                t(
                    "SHOPIFY_GET_CUSTOMER",
                    ("Kunde lesen", "Get customer"),
                    (
                        "Liest einen Kunden per Kunden-ID.",
                        "Reads one customer by customer id.",
                    ),
                    &with_shop(&[("customer_id", num_id("Kunden-ID / customer id"))]),
                    &["shop_name", "customer_id"],
                    &[READ_CUSTOMERS],
                    false,
                ),
                t(
                    "SHOPIFY_LIST_CUSTOMERS",
                    ("Kunden auflisten", "List customers"),
                    (
                        "Listet alle Kunden des Shops; pro Seite 250, über `page_info` blättern.",
                        "Lists all customers of the shop; 250 per page, page with `page_info`.",
                    ),
                    &with_shop(&[("page_info", s("Paging-Cursor (page_info) aus der vorherigen Antwort / paging cursor from the previous response"))]),
                    &["shop_name"],
                    &[READ_CUSTOMERS],
                    false,
                ),
                t(
                    "SHOPIFY_LIST_CUSTOMER_ORDERS",
                    ("Bestellungen eines Kunden", "List customer orders"),
                    (
                        "Listet die Bestellungen eines Kunden.",
                        "Lists the orders of one customer.",
                    ),
                    &with_shop(&[("customer_id", num_id("Kunden-ID / customer id"))]),
                    &["shop_name", "customer_id"],
                    &[READ_ORDERS],
                    false,
                ),
                t(
                    "SHOPIFY_GET_PRODUCT",
                    ("Produkt lesen", "Get product"),
                    ("Liest ein Produkt per Produkt-ID.", "Reads one product by product id."),
                    &with_shop(&[("product_id", num_id("Produkt-ID / product id"))]),
                    &["shop_name", "product_id"],
                    &[READ_PRODUCTS],
                    false,
                ),
                t(
                    "SHOPIFY_LIST_PRODUCTS",
                    ("Produkte auflisten", "List products"),
                    (
                        "Listet Produkte, optional nach Titel sowie nach Erstell-/Änderungsdatum gefiltert.",
                        "Lists products, optionally filtered by title and by created/updated date.",
                    ),
                    &with_shop(&[
                        ("title", s("Nur Produkte mit diesem Titel / only products with this title")),
                        ("created_at_min", s("Nur Produkte, die nach diesem ISO-Zeitpunkt erstellt wurden / created after this ISO date-time")),
                        ("updated_at_min", s("Nur Produkte, die nach diesem ISO-Zeitpunkt geändert wurden / updated after this ISO date-time")),
                        ("page_info", s("Paging-Cursor (page_info) aus der vorherigen Antwort / paging cursor from the previous response")),
                    ]),
                    &["shop_name"],
                    &[READ_PRODUCTS],
                    false,
                ),
                t(
                    "SHOPIFY_GET_PRODUCT_VARIANT",
                    ("Produktvariante lesen", "Get product variant"),
                    (
                        "Liest eine Produktvariante per Varianten-ID.",
                        "Reads one product variant by variant id.",
                    ),
                    &with_shop(&[("variant_id", num_id("Varianten-ID / variant id"))]),
                    &["shop_name", "variant_id"],
                    &[READ_PRODUCTS],
                    false,
                ),
                t(
                    "SHOPIFY_GET_TRANSACTION",
                    ("Transaktion lesen", "Get transaction"),
                    (
                        "Liest eine Zahlungstransaktion einer Bestellung.",
                        "Reads one payment transaction of an order.",
                    ),
                    &with_shop(&[
                        ("order_id", num_id("Bestell-ID / order id")),
                        ("transaction_id", num_id("Transaktions-ID / transaction id")),
                    ]),
                    &["shop_name", "order_id", "transaction_id"],
                    &[READ_ORDERS],
                    false,
                ),
                t(
                    "SHOPIFY_LIST_TRANSACTIONS",
                    ("Transaktionen einer Bestellung", "List order transactions"),
                    (
                        "Listet alle Transaktionen (Autorisierung, Capture, Refund, Void) einer Bestellung.",
                        "Lists all transactions (authorization, capture, refund, void) of an order.",
                    ),
                    &with_shop(&[("order_id", num_id("Bestell-ID / order id"))]),
                    &["shop_name", "order_id"],
                    &[READ_ORDERS],
                    false,
                ),
                t(
                    "SHOPIFY_GET_FULFILLMENT",
                    ("Fulfillment lesen", "Get fulfillment"),
                    (
                        "Liest ein Fulfillment (Versand) einer Bestellung.",
                        "Reads one fulfillment (shipment) of an order.",
                    ),
                    &with_shop(&[
                        ("order_id", num_id("Bestell-ID / order id")),
                        ("fulfillment_id", num_id("Fulfillment-ID / fulfillment id")),
                    ]),
                    &["shop_name", "order_id", "fulfillment_id"],
                    &[READ_FULFILLMENTS],
                    false,
                ),
                t(
                    "SHOPIFY_LIST_FULFILLMENTS",
                    ("Fulfillments einer Bestellung", "List order fulfillments"),
                    (
                        "Listet die Fulfillments einer Bestellung, um z. B. Fulfillment-IDs zu finden.",
                        "Lists the fulfillments of an order, e.g. to find fulfillment ids.",
                    ),
                    &with_shop(&[("order_id", num_id("Bestell-ID / order id"))]),
                    &["shop_name", "order_id"],
                    &[READ_FULFILLMENTS],
                    false,
                ),
                t(
                    "SHOPIFY_LIST_LOCATIONS",
                    ("Lager auflisten", "List locations"),
                    (
                        "Listet alle Lager/Standorte des Shops; liefert die IDs für Bestandsaktionen.",
                        "Lists all locations of the shop; yields the ids inventory actions need.",
                    ),
                    &with_shop(&[]),
                    &["shop_name"],
                    &[READ_LOCATIONS],
                    false,
                ),
                t(
                    "SHOPIFY_GET_ASSET",
                    ("Theme-Asset lesen", "Get theme asset"),
                    (
                        "Liest eine Theme-Datei (Template, Section, Stylesheet) nach Asset-Key.",
                        "Reads a theme file (template, section, stylesheet) by asset key.",
                    ),
                    &with_shop(&[
                        ("theme_id", num_id("Theme-ID / theme id")),
                        ("key", s("Asset-Key, z. B. assets/theme.css / asset key, e.g. assets/theme.css")),
                    ]),
                    &["shop_name", "theme_id", "key"],
                    &[READ_THEMES],
                    false,
                ),
                // --- writes ---
                t(
                    "SHOPIFY_CREATE_CUSTOMER",
                    ("Kunde anlegen", "Create customer"),
                    (
                        "Legt einen Kunden an (schreibt, braucht --confirm).",
                        "Creates a customer (writes, requires --confirm).",
                    ),
                    &with_shop(&[
                        ("email", s("E-Mail des Kunden / customer email")),
                        ("first_name", s("Vorname / first name")),
                        ("last_name", s("Nachname / last name")),
                        ("phone_number", s("Telefonnummer / phone number")),
                        ("tags", s("Komma-getrennte Tags / comma-separated tags")),
                        ("verified_email", flag("E-Mail als verifiziert markieren / mark email as verified", false)),
                        ("send_email_invite", flag("Einladungs-E-Mail senden / send an invite email", false)),
                        ("accepts_marketing", flag("Marketing-Einwilligung / marketing consent", false)),
                    ]),
                    &["shop_name"],
                    &[WRITE_CUSTOMERS],
                    true,
                ),
                t(
                    "SHOPIFY_UPDATE_CUSTOMER",
                    ("Kunde ändern", "Update customer"),
                    (
                        "Ändert die übergebenen Felder eines Kunden (schreibt, braucht --confirm).",
                        "Changes the given fields of a customer (writes, requires --confirm).",
                    ),
                    &with_shop(&[
                        ("customer_id", num_id("Kunden-ID / customer id")),
                        ("email", s("Neue E-Mail / new email")),
                        ("first_name", s("Vorname / first name")),
                        ("last_name", s("Nachname / last name")),
                        ("phone_number", s("Telefonnummer / phone number")),
                        ("tags", s("Komma-getrennte Tags / comma-separated tags")),
                        ("verified_email", flag("E-Mail als verifiziert markieren / mark email as verified", false)),
                        ("send_email_invite", flag("Einladungs-E-Mail senden / send an invite email", false)),
                        ("accepts_marketing", flag("Marketing-Einwilligung / marketing consent", false)),
                    ]),
                    &["shop_name", "customer_id"],
                    &[WRITE_CUSTOMERS],
                    true,
                ),
                t(
                    "SHOPIFY_CREATE_PRODUCT",
                    ("Produkt anlegen", "Create product"),
                    (
                        "Legt ein Produkt an (schreibt, braucht --confirm). Ohne Bild.",
                        "Creates a product (writes, requires --confirm). No image.",
                    ),
                    &with_shop(&[
                        ("title", s("Produkttitel / product title")),
                        ("description_html", s("Beschreibung, HTML erlaubt / description, HTML allowed")),
                        ("product_type", s("Produkttyp zur Filterung / product type for filtering")),
                        ("vendor", s("Hersteller / vendor")),
                        ("tags", s("Komma-getrennte Tags / comma-separated tags")),
                    ]),
                    &["shop_name", "title"],
                    &[WRITE_PRODUCTS],
                    true,
                ),
                t(
                    "SHOPIFY_UPDATE_PRODUCT",
                    ("Produkt ändern", "Update product"),
                    (
                        "Ändert die übergebenen Felder eines Produkts (schreibt, braucht --confirm).",
                        "Changes the given fields of a product (writes, requires --confirm).",
                    ),
                    &with_shop(&[
                        ("product_id", num_id("Produkt-ID / product id")),
                        ("title", s("Neuer Titel / new title")),
                        ("description_html", s("Beschreibung, HTML erlaubt / description, HTML allowed")),
                        ("product_type", s("Produkttyp zur Filterung / product type for filtering")),
                        ("vendor", s("Hersteller / vendor")),
                        ("tags", s("Komma-getrennte Tags / comma-separated tags")),
                    ]),
                    &["shop_name", "product_id"],
                    &[WRITE_PRODUCTS],
                    true,
                ),
                t(
                    "SHOPIFY_CREATE_ORDER",
                    ("Bestellung anlegen", "Create order"),
                    (
                        "Legt eine Bestellung mit einer Position an (schreibt, braucht --confirm).",
                        "Creates an order with one line item (writes, requires --confirm).",
                    ),
                    &with_shop(&[
                        ("product_id", num_id("Produkt-ID der Position / product id of the line item")),
                        ("variant_id", num_id("Varianten-ID der Position / variant id of the line item")),
                        ("title", s("Titel der Position / title of the line item")),
                        ("quantity", int("Menge / quantity", 1, 100_000, 1)),
                        ("price", s("Preis der Position / price of the line item")),
                        ("customer_id", num_id("Kunden-ID / customer id")),
                        ("email", s("E-Mail für Beleg / email for the receipt")),
                        ("tags", s("Komma-getrennte Tags / comma-separated tags")),
                        ("send_receipt", flag("Beleg senden (nur mit `email`) / send receipt (needs `email`)", false)),
                        ("send_fulfillment_receipt", flag("Fulfillment-Beleg senden (nur mit `email`) / send fulfillment receipt (needs `email`)", false)),
                    ]),
                    &["shop_name"],
                    &[WRITE_ORDERS],
                    true,
                ),
                t(
                    "SHOPIFY_UPDATE_ORDER",
                    ("Bestellung ändern", "Update order"),
                    (
                        "Ändert E-Mail, Telefon, Tags oder Notiz einer Bestellung (schreibt, braucht --confirm).",
                        "Changes email, phone, tags or note of an order (writes, requires --confirm).",
                    ),
                    &with_shop(&[
                        ("order_id", num_id("Bestell-ID / order id")),
                        ("email", s("Neue E-Mail / new email")),
                        ("phone_number", s("Neue Telefonnummer / new phone number")),
                        ("tags", s("Komma-getrennte Tags / comma-separated tags")),
                        ("note", s("Notiz zur Bestellung / note on the order")),
                    ]),
                    &["shop_name", "order_id"],
                    &[WRITE_ORDERS],
                    true,
                ),
                t(
                    "SHOPIFY_CLOSE_ORDER",
                    ("Bestellung schließen", "Close order"),
                    (
                        "Schließt eine Bestellung ab, ohne sie zu stornieren (schreibt, braucht --confirm).",
                        "Closes an order without cancelling it (writes, requires --confirm).",
                    ),
                    &with_shop(&[("order_id", num_id("Bestell-ID / order id"))]),
                    &["shop_name", "order_id"],
                    &[WRITE_ORDERS],
                    true,
                ),
                t(
                    "SHOPIFY_CANCEL_ORDER",
                    ("Bestellung stornieren", "Cancel order"),
                    (
                        "Storniert eine Bestellung; kann Rückerstattungen auslösen (schreibt, braucht --confirm).",
                        "Cancels an order; may trigger refunds (writes, requires --confirm).",
                    ),
                    &with_shop(&[("order_id", num_id("Bestell-ID / order id"))]),
                    &["shop_name", "order_id"],
                    &[WRITE_ORDERS],
                    true,
                ),
                t(
                    "SHOPIFY_CREATE_DRAFT_ORDER",
                    ("Entwurfsbestellung anlegen", "Create draft order"),
                    (
                        "Legt eine Entwurfsbestellung (offen, editierbar) an (schreibt, braucht --confirm).",
                        "Creates a draft order (open, editable) (writes, requires --confirm).",
                    ),
                    &with_shop(&[
                        ("product_id", num_id("Produkt-ID der Position / product id of the line item")),
                        ("variant_id", num_id("Varianten-ID der Position / variant id of the line item")),
                        ("title", s("Titel der Position / title of the line item")),
                        ("quantity", int("Menge / quantity", 1, 100_000, 1)),
                        ("price", s("Preis der Position / price of the line item")),
                        ("customer_id", num_id("Kunden-ID / customer id")),
                    ]),
                    &["shop_name"],
                    &[WRITE_DRAFT_ORDERS],
                    true,
                ),
                t(
                    "SHOPIFY_CREATE_TRANSACTION",
                    ("Transaktion anlegen", "Create transaction"),
                    (
                        "Bucht eine Zahlungstransaktion (Capture, Refund, Void …) auf eine Bestellung (schreibt, braucht --confirm).",
                        "Records a payment transaction (capture, refund, void, …) on an order (writes, requires --confirm).",
                    ),
                    &with_shop(&[
                        ("order_id", num_id("Bestell-ID / order id")),
                        ("kind", choice("Art der Transaktion / transaction kind", TRANSACTION_KINDS)),
                        ("amount", s("Betrag / amount")),
                        ("currency", s("Währungscode, z. B. EUR / currency code, e.g. EUR")),
                        ("parent_id", num_id("ID der übergeordneten Transaktion / id of the parent transaction")),
                        ("source", s("Quelle, `external` für eine Barzahlung / source, `external` for a cash transaction")),
                        ("test", flag("Testtransaktion / test transaction", false)),
                    ]),
                    &["shop_name", "order_id", "kind"],
                    &[WRITE_ORDERS],
                    true,
                ),
                t(
                    "SHOPIFY_CREATE_FULFILLMENT_EVENT",
                    ("Fulfillment-Event anlegen", "Create fulfillment event"),
                    (
                        "Trägt ein Versand-Event (unterwegs, zugestellt …) auf ein Fulfillment ein (schreibt, braucht --confirm).",
                        "Posts a shipment event (in transit, delivered, …) on a fulfillment (writes, requires --confirm).",
                    ),
                    &with_shop(&[
                        ("order_id", num_id("Bestell-ID / order id")),
                        ("fulfillment_id", num_id("Fulfillment-ID / fulfillment id")),
                        ("status", choice("Status des Events / event status", FULFILLMENT_EVENT_STATUSES)),
                        ("message", s("Nachricht zum Event / message on the event")),
                    ]),
                    &["shop_name", "order_id", "fulfillment_id", "status"],
                    &[WRITE_FULFILLMENTS],
                    true,
                ),
                t(
                    "SHOPIFY_ADJUST_INVENTORY_LEVEL",
                    ("Bestand anpassen", "Adjust inventory level"),
                    (
                        "Verschiebt die verfügbare Menge eines Inventar-Artikels an einem Lager (schreibt, braucht --confirm).",
                        "Shifts the available quantity of an inventory item at a location (writes, requires --confirm).",
                    ),
                    &with_shop(&[
                        ("inventory_item_id", num_id("Inventar-Artikel-ID / inventory item id")),
                        ("location_id", num_id("Lager-ID aus SHOPIFY_LIST_LOCATIONS / location id from SHOPIFY_LIST_LOCATIONS")),
                        ("available_adjustment", num_any("Positive Werte erhöhen, negative senken / positive increases, negative decreases")),
                    ]),
                    &["shop_name", "inventory_item_id", "location_id", "available_adjustment"],
                    &[WRITE_INVENTORY],
                    true,
                ),
                t(
                    "SHOPIFY_CREATE_COLLECT",
                    ("Produkt zu Sammlung hinzufügen", "Add product to collection"),
                    (
                        "Verknüpft ein Produkt mit einer manuellen Sammlung (schreibt, braucht --confirm).",
                        "Links a product to a manual collection (writes, requires --confirm).",
                    ),
                    &with_shop(&[
                        ("product_id", num_id("Produkt-ID / product id")),
                        ("collection_id", num_id("Sammlungs-ID / collection id")),
                    ]),
                    &["shop_name", "product_id", "collection_id"],
                    &[WRITE_CONTENT],
                    true,
                ),
            ],
        }
    }

    /// Admin API root: pinned by the host, else derived from the `shop_name` argument.
    fn base(&self, args: &Value) -> Result<String> {
        if let Some(b) = &self.api_base {
            return Ok(b.clone());
        }
        shop_base(req_str(args, "shop_name")?)
    }
}

/// `getBaseUrl(shopName)` in `src/lib/common/auth.ts`.
fn shop_base(shop_name: &str) -> Result<String> {
    if !valid_shop_name(shop_name) {
        bail!("Invalid shop name. Enter only the part before .myshopify.com in your store URL (letters, numbers and hyphens), for example \"my-shop\" for https://my-shop.myshopify.com.");
    }
    Ok(format!("https://{shop_name}.myshopify.com/admin/api/{API_VERSION}"))
}

/// Prepend the `shop_name` prop every action shares (the store is part of the URL).
fn with_shop<'a>(props: &[(&'a str, Value)]) -> Vec<(&'a str, Value)> {
    let mut v = vec![(
        "shop_name",
        s("Shop-Name, der Teil vor .myshopify.com (Pflichtwert der Piece-Auth); wird ignoriert, wenn die Instanz die Basis per with_api_base fest verdrahtet hat. / Shop name, the part before .myshopify.com (the piece's required auth prop); ignored when the instance pins the base via with_api_base."),
    )];
    v.extend_from_slice(props);
    v
}

/// `line_items: [{product_id, variant_id, title, quantity, price}]` of `create_order`
/// and `create_draft_order`; unset fields are dropped like `undefined` in the piece.
fn line_item(args: &Value) -> Result<Value> {
    let mut m = Map::new();
    put_id(&mut m, args, "product_id", "product_id")?;
    put_id(&mut m, args, "variant_id", "variant_id")?;
    put(&mut m, args, "title", "title");
    m.insert(
        "quantity".into(),
        json!(args.get("quantity").and_then(Value::as_u64).unwrap_or(1)),
    );
    put(&mut m, args, "price", "price");
    Ok(Value::Object(m))
}

/// Customer fields shared by `create_customer` and `update_customer`.
fn customer_fields(args: &Value, consent_fallback: &str) -> Map<String, Value> {
    let mut m = Map::new();
    put(&mut m, args, "email", "email");
    put(&mut m, args, "first_name", "first_name");
    put(&mut m, args, "last_name", "last_name");
    put(&mut m, args, "phone_number", "phone");
    put(&mut m, args, "tags", "tags");
    if let Some(v) = args.get("verified_email").and_then(Value::as_bool) {
        m.insert("verified_email".into(), json!(v));
    }
    if let Some(v) = args.get("send_email_invite").and_then(Value::as_bool) {
        m.insert("send_email_invite".into(), json!(v));
    }
    m.insert("email_marketing_consent".into(), marketing_consent(args, consent_fallback));
    m
}

impl ToolkitAdapter for Shopify {
    fn connection_fields(&self) -> Vec<ConnectionField> {
        vec![ConnectionField::new(
            "shop_name",
            ("Shop-Name", "Shop name"),
            (
                "Der Teil vor .myshopify.com, z. B. „mein-shop“ für https://mein-shop.myshopify.com.",
                "The part before .myshopify.com, e.g. \"my-shop\" for https://my-shop.myshopify.com.",
            ),
            "my-shop",
            true,
        )]
    }
    fn connection_api_base(&self, params: &std::collections::BTreeMap<String, String>) -> Result<Option<String>> {
        if let Some(v) = params.get("shop_name") {
            shop_base(&host_label(v, "myshopify.com", "shop_name")?)?;
        }
        Ok(None)
    }
    fn connection_args(&self, params: &std::collections::BTreeMap<String, String>) -> Vec<(String, String)> {
        params
            .get("shop_name")
            .and_then(|v| host_label(v, "myshopify.com", "shop_name").ok())
            .map(|v| vec![("shop_name".to_string(), v)])
            .unwrap_or_default()
    }
    fn spec(&self) -> &ToolkitSpec {
        &self.spec
    }

    /// AP: lib/actions/get-customer.ts + lib/common/index.ts (getCustomer)
    /// AP: lib/actions/get-customers.ts + lib/common/index.ts (getCustomers)
    /// AP: lib/actions/get-customer-orders.ts + lib/common/index.ts (getCustomerOrders)
    /// AP: lib/actions/get-product.ts + lib/common/index.ts (getProduct)
    /// AP: lib/actions/get-products.ts + lib/common/index.ts (getProducts)
    /// AP: lib/actions/get-product-variant.ts + lib/common/index.ts (getProductVariant)
    /// AP: lib/actions/get-transaction.ts + lib/common/index.ts (getTransaction)
    /// AP: lib/actions/get-transactions.ts + lib/common/index.ts (getTransactions)
    /// AP: lib/actions/get-fulfillment.ts + lib/common/index.ts (getFulfillment)
    /// AP: lib/actions/get-fulfillments.ts + lib/common/index.ts (getFulfillments)
    /// AP: lib/actions/get-locations.ts + lib/common/index.ts (getLocations)
    /// AP: lib/actions/get-asset.ts + lib/common/index.ts (getAsset)
    /// AP: lib/actions/create-customer.ts + lib/common/index.ts (createCustomer)
    /// AP: lib/actions/update-customer.ts + lib/common/index.ts (updateCustomer)
    /// AP: lib/actions/create-product.ts + lib/common/index.ts (createProduct)
    /// AP: lib/actions/update-product.ts + lib/common/index.ts (updateProduct)
    /// AP: lib/actions/create-order.ts + lib/common/index.ts (createOrder)
    /// AP: lib/actions/update-order.ts + lib/common/index.ts (updateOrder)
    /// AP: lib/actions/close-order.ts + lib/common/index.ts (closeOrder)
    /// AP: lib/actions/cancel-order.ts + lib/common/index.ts (cancelOrder)
    /// AP: lib/actions/create-draft-order.ts + lib/common/index.ts (createDraftOrder)
    /// AP: lib/actions/create-transaction.ts + lib/common/index.ts (createTransaction)
    /// AP: lib/actions/create-fulfillment-event.ts + lib/common/index.ts (createFulfillmentEvent)
    /// AP: lib/actions/adjust-inventory-level.ts + lib/common/index.ts (adjustInventoryLevel)
    /// AP: lib/actions/create-collect.ts + lib/common/index.ts (createCollect)
    #[allow(clippy::too_many_lines)]
    fn build_request(&self, tool: &str, args: &Value) -> Result<HttpRequest> {
        // The base is resolved up front so an invalid `shop_name` fails before anything else.
        let base = self.base(args)?;
        Ok(match tool {
            "SHOPIFY_GET_CUSTOMER" => HttpRequest::new(
                "GET",
                format!("{base}/customers/{}.json", id_segment(args, "customer_id")?),
            ),
            "SHOPIFY_LIST_CUSTOMERS" => HttpRequest::new(
                "GET",
                format!(
                    "{base}/customers.json{}",
                    query(&[("page_info", opt_str(args, "page_info").map(String::from))])
                ),
            ),
            "SHOPIFY_LIST_CUSTOMER_ORDERS" => HttpRequest::new(
                "GET",
                format!("{base}/customers/{}/orders.json", id_segment(args, "customer_id")?),
            ),
            "SHOPIFY_GET_PRODUCT" => HttpRequest::new(
                "GET",
                format!("{base}/products/{}.json", id_segment(args, "product_id")?),
            ),
            "SHOPIFY_LIST_PRODUCTS" => HttpRequest::new(
                "GET",
                format!(
                    "{base}/products.json{}",
                    query(&[
                        ("title", opt_str(args, "title").map(String::from)),
                        ("created_at_min", opt_str(args, "created_at_min").map(String::from)),
                        ("updated_at_min", opt_str(args, "updated_at_min").map(String::from)),
                        ("page_info", opt_str(args, "page_info").map(String::from)),
                    ])
                ),
            ),
            "SHOPIFY_GET_PRODUCT_VARIANT" => HttpRequest::new(
                "GET",
                format!("{base}/variants/{}.json", id_segment(args, "variant_id")?),
            ),
            "SHOPIFY_GET_TRANSACTION" => HttpRequest::new(
                "GET",
                format!(
                    "{base}/orders/{}/transactions/{}.json",
                    id_segment(args, "order_id")?,
                    id_segment(args, "transaction_id")?
                ),
            ),
            "SHOPIFY_LIST_TRANSACTIONS" => HttpRequest::new(
                "GET",
                format!("{base}/orders/{}/transactions.json", id_segment(args, "order_id")?),
            ),
            "SHOPIFY_GET_FULFILLMENT" => HttpRequest::new(
                "GET",
                format!(
                    "{base}/orders/{}/fulfillments/{}.json",
                    id_segment(args, "order_id")?,
                    id_segment(args, "fulfillment_id")?
                ),
            ),
            "SHOPIFY_LIST_FULFILLMENTS" => HttpRequest::new(
                "GET",
                format!("{base}/orders/{}/fulfillments.json", id_segment(args, "order_id")?),
            ),
            "SHOPIFY_LIST_LOCATIONS" => HttpRequest::new("GET", format!("{base}/locations.json")),
            "SHOPIFY_GET_ASSET" => HttpRequest::new(
                "GET",
                format!(
                    "{base}/themes/{}/assets.json{}",
                    id_segment(args, "theme_id")?,
                    query(&[("key", Some(req_str(args, "key")?.to_string()))])
                ),
            ),
            "SHOPIFY_CREATE_CUSTOMER" => {
                // `not_subscribed` is the fallback the piece uses when `acceptsMarketing` is unset.
                HttpRequest::new("POST", format!("{base}/customers.json"))
                    .json(&json!({"customer": Value::Object(customer_fields(args, "not_subscribed"))}))
            }
            "SHOPIFY_UPDATE_CUSTOMER" => {
                // The piece's update action uses `unsubscribed` as its fallback state.
                HttpRequest::new(
                    "PUT",
                    format!("{base}/customers/{}.json", id_segment(args, "customer_id")?),
                )
                .json(&json!({"customer": Value::Object(customer_fields(args, "unsubscribed"))}))
            }
            "SHOPIFY_CREATE_PRODUCT" => {
                let mut p = Map::new();
                p.insert("title".into(), json!(req_str(args, "title")?));
                for (key, field) in [
                    ("description_html", "body_html"),
                    ("product_type", "product_type"),
                    ("vendor", "vendor"),
                    ("tags", "tags"),
                ] {
                    put(&mut p, args, key, field);
                }
                // The piece always sends the (possibly empty) images array.
                p.insert("images".into(), json!([]));
                HttpRequest::new("POST", format!("{base}/products.json")).json(&json!({"product": Value::Object(p)}))
            }
            "SHOPIFY_UPDATE_PRODUCT" => {
                let mut p = Map::new();
                for (key, field) in [
                    ("title", "title"),
                    ("description_html", "body_html"),
                    ("product_type", "product_type"),
                    ("vendor", "vendor"),
                    ("tags", "tags"),
                ] {
                    put(&mut p, args, key, field);
                }
                p.insert("images".into(), json!([]));
                HttpRequest::new(
                    "PUT",
                    format!("{base}/products/{}.json", id_segment(args, "product_id")?),
                )
                .json(&json!({"product": Value::Object(p)}))
            }
            "SHOPIFY_CREATE_ORDER" => {
                let mut o = Map::new();
                o.insert("line_items".into(), json!([line_item(args)?]));
                if let Some(c) = opt_id(args, "customer_id")? {
                    o.insert("customer".into(), json!({"id": c}));
                }
                if let Some(email) = opt_str(args, "email") {
                    o.insert("email".into(), json!(email));
                    o.insert(
                        "send_receipt".into(),
                        json!(args.get("send_receipt").and_then(Value::as_bool).unwrap_or(false)),
                    );
                    o.insert(
                        "send_fulfillment_receipt".into(),
                        json!(args.get("send_fulfillment_receipt").and_then(Value::as_bool).unwrap_or(false)),
                    );
                }
                put(&mut o, args, "tags", "tags");
                HttpRequest::new("POST", format!("{base}/orders.json")).json(&json!({"order": Value::Object(o)}))
            }
            "SHOPIFY_UPDATE_ORDER" => {
                let mut o = Map::new();
                put(&mut o, args, "email", "email");
                put(&mut o, args, "phone_number", "phone");
                put(&mut o, args, "tags", "tags");
                put(&mut o, args, "note", "note");
                HttpRequest::new(
                    "PUT",
                    format!("{base}/orders/{}.json", id_segment(args, "order_id")?),
                )
                .json(&json!({"order": Value::Object(o)}))
            }
            "SHOPIFY_CLOSE_ORDER" => HttpRequest::new(
                "POST",
                format!("{base}/orders/{}/close.json", id_segment(args, "order_id")?),
            )
            .json(&json!({})),
            "SHOPIFY_CANCEL_ORDER" => HttpRequest::new(
                "POST",
                format!("{base}/orders/{}/cancel.json", id_segment(args, "order_id")?),
            )
            .json(&json!({})),
            "SHOPIFY_CREATE_DRAFT_ORDER" => {
                let mut d = Map::new();
                d.insert("line_items".into(), json!([line_item(args)?]));
                if let Some(c) = opt_id(args, "customer_id")? {
                    d.insert("customer".into(), json!({"id": c}));
                }
                HttpRequest::new("POST", format!("{base}/draft_orders.json"))
                    .json(&json!({"draft_order": Value::Object(d)}))
            }
            "SHOPIFY_CREATE_TRANSACTION" => {
                let kind = req_str(args, "kind")?;
                if !TRANSACTION_KINDS.contains(&kind) {
                    bail!("`kind` must be one of {}", TRANSACTION_KINDS.join(", "));
                }
                let mut t = Map::new();
                t.insert("kind".into(), json!(kind));
                put(&mut t, args, "amount", "amount");
                put(&mut t, args, "currency", "currency");
                put(&mut t, args, "source", "source");
                put_id(&mut t, args, "parent_id", "parent_id")?;
                if let Some(v) = args.get("test").and_then(Value::as_bool) {
                    t.insert("test".into(), json!(v));
                }
                HttpRequest::new(
                    "POST",
                    format!("{base}/orders/{}/transactions.json", id_segment(args, "order_id")?),
                )
                .json(&json!({"transaction": Value::Object(t)}))
            }
            "SHOPIFY_CREATE_FULFILLMENT_EVENT" => {
                let status = req_str(args, "status")?;
                if !FULFILLMENT_EVENT_STATUSES.contains(&status) {
                    bail!(
                        "`status` must be one of {}",
                        FULFILLMENT_EVENT_STATUSES.join(", ")
                    );
                }
                let mut e = Map::new();
                e.insert("status".into(), json!(status));
                put(&mut e, args, "message", "message");
                HttpRequest::new(
                    "POST",
                    format!(
                        "{base}/orders/{}/fulfillments/{}/events.json",
                        id_segment(args, "order_id")?,
                        id_segment(args, "fulfillment_id")?
                    ),
                )
                .json(&json!({"event": Value::Object(e)}))
            }
            "SHOPIFY_ADJUST_INVENTORY_LEVEL" => {
                let adjustment = args
                    .get("available_adjustment")
                    .and_then(Value::as_i64)
                    .ok_or_else(|| anyhow::anyhow!("missing argument `available_adjustment`"))?;
                HttpRequest::new("POST", format!("{base}/inventory_levels/adjust.json")).json(
                    &json!({
                        "inventory_item_id": id_segment(args, "inventory_item_id")?,
                        "location_id": id_segment(args, "location_id")?,
                        "available_adjustment": adjustment,
                    }),
                )
            }
            "SHOPIFY_CREATE_COLLECT" => HttpRequest::new("POST", format!("{base}/collects.json")).json(
                &json!({"collect": {
                    "product_id": id_segment(args, "product_id")?,
                    "collection_id": id_segment(args, "collection_id")?,
                }}),
            ),
            other => bail!("unknown Shopify tool `{other}`"),
        })
    }

    /// The piece returns the unwrapped REST field (`body.customer`, `body.products`, …);
    /// list results carry a `count` next to the untouched array.
    fn parse_response(&self, tool: &str, response: &HttpResponse) -> Result<Value> {
        let v = response.json()?;
        let one = |field: &str| -> Result<Value> {
            match v.get(field) {
                Some(x) if !x.is_null() => Ok(x.clone()),
                _ => bail!("Shopify returned no `{field}`"),
            }
        };
        let list = |field: &str| -> Value {
            let items = v[field].as_array().cloned().unwrap_or_default();
            json!({field: items.clone(), "count": items.len()})
        };
        Ok(match tool {
            "SHOPIFY_LIST_CUSTOMERS" => list("customers"),
            "SHOPIFY_LIST_CUSTOMER_ORDERS" => list("orders"),
            "SHOPIFY_LIST_PRODUCTS" => list("products"),
            "SHOPIFY_LIST_TRANSACTIONS" => list("transactions"),
            "SHOPIFY_LIST_FULFILLMENTS" => list("fulfillments"),
            "SHOPIFY_LIST_LOCATIONS" => list("locations"),
            "SHOPIFY_GET_CUSTOMER"
            | "SHOPIFY_CREATE_CUSTOMER"
            | "SHOPIFY_UPDATE_CUSTOMER" => one("customer")?,
            "SHOPIFY_GET_PRODUCT" | "SHOPIFY_CREATE_PRODUCT" | "SHOPIFY_UPDATE_PRODUCT" => {
                one("product")?
            }
            "SHOPIFY_GET_PRODUCT_VARIANT" => one("variant")?,
            "SHOPIFY_GET_ASSET" => one("asset")?,
            "SHOPIFY_GET_TRANSACTION" | "SHOPIFY_CREATE_TRANSACTION" => one("transaction")?,
            "SHOPIFY_GET_FULFILLMENT" => one("fulfillment")?,
            "SHOPIFY_CREATE_FULFILLMENT_EVENT" => one("fulfillment_event")?,
            "SHOPIFY_CREATE_ORDER"
            | "SHOPIFY_UPDATE_ORDER"
            | "SHOPIFY_CLOSE_ORDER"
            | "SHOPIFY_CANCEL_ORDER" => one("order")?,
            "SHOPIFY_CREATE_DRAFT_ORDER" => one("draft_order")?,
            "SHOPIFY_CREATE_COLLECT" => one("collect")?,
            // The adjust response has no wrapper object (`{"inventory_level": …}`); the piece
            // reads a non-existent `order` key, so the raw body is passed through.
            "SHOPIFY_ADJUST_INVENTORY_LEVEL" => v,
            other => bail!("unknown Shopify tool `{other}`"),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SHOP: &str = "my-shop";

    fn args(v: Value) -> Value {
        let mut m = v.as_object().cloned().unwrap_or_default();
        m.insert("shop_name".into(), json!(SHOP));
        Value::Object(m)
    }

    #[test]
    fn shop_name_builds_the_admin_base() {
        let s = Shopify::default();
        let r = s
            .build_request("SHOPIFY_GET_PRODUCT", &args(json!({"product_id": 788032119674292922u64})))
            .unwrap();
        assert_eq!(r.method, "GET");
        assert_eq!(
            r.url,
            "https://my-shop.myshopify.com/admin/api/2026-10/products/788032119674292922.json"
        );
        assert!(s
            .build_request("SHOPIFY_GET_PRODUCT", &args(json!({"product_id": "../x"})))
            .is_err());
        assert!(s
            .build_request("SHOPIFY_GET_PRODUCT", &json!({"product_id": 1}))
            .is_err());
        assert!(s
            .build_request(
                "SHOPIFY_GET_PRODUCT",
                &json!({"product_id": 1, "shop_name": "-bad"})
            )
            .is_err());
        assert!(s.build_request("SHOPIFY_LIST_PRODUCTS", &args(json!({"title": "Shirt & Co"}))).is_ok());
    }

    #[test]
    fn pinned_base_wins_over_the_argument() {
        let s = Shopify::with_api_base("https://pinned.myshopify.test/admin/api/2026-10/");
        let r = s
            .build_request("SHOPIFY_LIST_LOCATIONS", &args(json!({})))
            .unwrap();
        assert_eq!(r.url, "https://pinned.myshopify.test/admin/api/2026-10/locations.json");
        let s = Shopify::with_shop_name("other-shop").unwrap();
        assert_eq!(
            s.build_request("SHOPIFY_LIST_LOCATIONS", &args(json!({}))).unwrap().url,
            "https://other-shop.myshopify.com/admin/api/2026-10/locations.json"
        );
        assert!(Shopify::with_shop_name("not_a_shop").is_err());
    }

    #[test]
    fn query_values_are_encoded_and_bodies_follow_the_piece() {
        let s = Shopify::with_shop_name(SHOP).unwrap();
        let p = s
            .build_request(
                "SHOPIFY_LIST_PRODUCTS",
                &args(json!({"title": "Shirt & Co", "updated_at_min": "2026-01-01T00:00:00Z"})),
            )
            .unwrap();
        assert!(p.url.contains("title=Shirt+%26+Co"), "{}", p.url);
        assert!(p.url.contains("updated_at_min=2026-01-01T00%3A00%3A00Z"), "{}", p.url);

        let a = s
            .build_request(
                "SHOPIFY_GET_ASSET",
                &args(json!({"theme_id": 828155753, "key": "assets/theme.liquid"})),
            )
            .unwrap();
        assert!(a.url.ends_with("/themes/828155753/assets.json?key=assets%2Ftheme.liquid"), "{}", a.url);

        let c = s
            .build_request("SHOPIFY_CREATE_CUSTOMER", &args(json!({"email": "a@b.de"})))
            .unwrap();
        assert_eq!(c.method, "POST");
        let body: Value = serde_json::from_slice(c.body.as_deref().unwrap()).unwrap();
        assert_eq!(body["customer"]["email"], "a@b.de");
        assert_eq!(body["customer"]["email_marketing_consent"]["state"], "not_subscribed");

        let u = s
            .build_request(
                "SHOPIFY_UPDATE_CUSTOMER",
                &args(json!({"customer_id": 191167, "tags": "vip", "accepts_marketing": true})),
            )
            .unwrap();
        assert_eq!(u.method, "PUT");
        let body: Value = serde_json::from_slice(u.body.as_deref().unwrap()).unwrap();
        assert_eq!(body["customer"]["email_marketing_consent"]["state"], "subscribed");
        assert!(body["customer"].get("email").is_none());

        let o = s
            .build_request(
                "SHOPIFY_CREATE_ORDER",
                &args(json!({"product_id": 788032119674292922u64, "variant_id": 642667041524721922u64, "price": "19.99"})),
            )
            .unwrap();
        let body: Value = serde_json::from_slice(o.body.as_deref().unwrap()).unwrap();
        assert_eq!(body["order"]["line_items"][0]["quantity"], 1);
        assert_eq!(body["order"]["line_items"][0]["price"], "19.99");
        assert!(body["order"].get("email").is_none());

        let i = s
            .build_request(
                "SHOPIFY_ADJUST_INVENTORY_LEVEL",
                &args(json!({"inventory_item_id": 808950810, "location_id": 24826418, "available_adjustment": -3})),
            )
            .unwrap();
        let body: Value = serde_json::from_slice(i.body.as_deref().unwrap()).unwrap();
        assert_eq!(body["available_adjustment"], -3);
        assert_eq!(body["inventory_item_id"], "808950810");

        assert!(s
            .build_request("SHOPIFY_ADJUST_INVENTORY_LEVEL", &args(json!({"inventory_item_id": 1, "location_id": 2})))
            .is_err());
        assert!(s
            .build_request(
                "SHOPIFY_CREATE_FULFILLMENT_EVENT",
                &args(json!({"order_id": 1, "fulfillment_id": 2, "status": "nope"}))
            )
            .is_err());
        assert!(s
            .build_request("SHOPIFY_CREATE_TRANSACTION", &args(json!({"order_id": 1, "kind": "nope"})))
            .is_err());
        assert!(s.build_request("SHOPIFY_NOPE", &args(json!({}))).is_err());
    }
}
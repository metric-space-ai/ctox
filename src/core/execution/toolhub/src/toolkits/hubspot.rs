//! HubSpot toolkit (REST v3/v4 + the legacy contacts/v1 list endpoints).
//! Action set modelled on Activepieces `piece-hubspot` (MIT). The Activepieces
//! actions that use `@hubspot/api-client` are expressed here as the plain REST
//! calls the SDK wraps (`GET/POST/PATCH /crm/v3/objects/...`).
use anyhow::{bail, Result};
use serde_json::{json, Map, Value};

use super::common::*;
use super::{path_segment, ToolkitAdapter};
use crate::http::{HttpRequest, HttpResponse};
use crate::model::*;

pub const SLUG: &str = "hubspot";

const CONTACTS_R: &str = "crm.objects.contacts.read";
const CONTACTS_W: &str = "crm.objects.contacts.write";
const COMPANIES_R: &str = "crm.objects.companies.read";
const COMPANIES_W: &str = "crm.objects.companies.write";
const DEALS_R: &str = "crm.objects.deals.read";
const DEALS_W: &str = "crm.objects.deals.write";
const TICKETS_R: &str = "crm.objects.tickets.read";
const TICKETS_W: &str = "crm.objects.tickets.write";
const LISTS_R: &str = "crm.lists.read";
const LISTS_W: &str = "crm.lists.write";
const OWNERS_R: &str = "settings.users.read";
const AUTOMATION: &str = "automation";
/// Association labels span both ends of the pair, so they need both scopes.
const ASSOC_R: &[&str] = &[CONTACTS_R, COMPANIES_R, DEALS_R, TICKETS_R];
/// Property schemas, one scope per object type that can be introspected.
const SCHEMAS_R: &[&str] = &[
    "crm.schemas.contacts.read",
    "crm.schemas.companies.read",
    "crm.schemas.deals.read",
    "crm.schemas.custom.read",
];

/// https://developers.hubspot.com/docs/api-reference/search/guide#limitations
const MAX_SEARCH_PAGE_SIZE: u64 = 200;

/// Default properties returned by the Activepieces Find/Get actions.
const CONTACT_PROPS: &[&str] = &[
    "firstname",
    "lastname",
    "email",
    "company",
    "website",
    "mobilephone",
    "phone",
    "fax",
    "address",
    "city",
    "state",
    "zip",
    "salutation",
    "country",
    "jobtitle",
    "hs_createdate",
    "hs_email_domain",
    "hs_object_id",
    "lastmodifieddate",
    "hs_persona",
    "hs_language",
    "lifecyclestage",
    "createdate",
    "numemployees",
    "annualrevenue",
    "industry",
];
const COMPANY_PROPS: &[&str] = &[
    "name",
    "domain",
    "industry",
    "about_us",
    "phone",
    "address",
    "address2",
    "city",
    "state",
    "zip",
    "country",
    "website",
    "type",
    "description",
    "founded_year",
    "hs_createdate",
    "hs_lastmodifieddate",
    "hs_object_id",
    "is_public",
    "timezone",
    "total_money_raised",
    "total_revenue",
    "owneremail",
    "ownername",
    "numberofemployees",
    "annualrevenue",
    "lifecyclestage",
    "createdate",
    "web_technologies",
];
const DEAL_PROPS: &[&str] = &[
    "dealtype",
    "dealname",
    "amount",
    "description",
    "closedate",
    "createdate",
    "num_associated_contacts",
    "hs_forecast_amount",
    "hs_forecast_probability",
    "hs_manual_forecast_category",
    "hs_next_step",
    "hs_object_id",
    "hs_lastmodifieddate",
    "hubspot_owner_id",
    "hubspot_team_id",
];
const TICKET_PROPS: &[&str] = &[
    "subject",
    "content",
    "source_type",
    "createdate",
    "hs_pipeline",
    "hs_pipeline_stage",
    "hs_resolution",
    "hs_ticket_category",
    "hs_ticket_id",
    "hs_ticket_priority",
    "hs_lastmodifieddate",
    "hubspot_owner_id",
    "hubspot_team_id",
];

/// `NOTE_TO` / `TASK_TO` from `common/constants.ts`.
const NOTE_ASSOCIATION_IDS: &[(&str, i64)] = &[("contact", 202), ("company", 190), ("deal", 214), ("ticket", 228)];
const TASK_ASSOCIATION_IDS: &[(&str, i64)] = &[("contact", 204), ("company", 192), ("deal", 216), ("ticket", 230)];

pub struct HubSpot {
    spec: ToolkitSpec,
    api_base: String,
}

impl Default for HubSpot {
    fn default() -> Self {
        Self::with_api_base("https://api.hubapi.com")
    }
}

fn find_props(object: &str) -> Vec<(&'static str, Value)> {
    vec![
        (
            "property_name",
            s(&format!(
                "Internal property to compare, e.g. {object}. List names via HUBSPOT_LIST_OBJECT_PROPERTIES."
            )),
        ),
        ("value", s("Exact match value")),
        (
            "property_name_2",
            s("Optional second property; the record must match both conditions."),
        ),
        ("value_2", s("Ignored unless `property_name_2` is also set")),
        ("properties", str_list("Additional properties to return")),
    ]
}

fn get_props(id_key: &'static str, id_desc: &'static str) -> Vec<(&'static str, Value)> {
    vec![
        (id_key, s(id_desc)),
        ("properties", str_list("Additional properties to return")),
    ]
}

impl HubSpot {
    pub fn with_api_base(api_base: &str) -> Self {
        let t = |slug, title, desc, props: Props<'_>, req: &[&str], scopes: &[&str], m| {
            tool(SLUG, slug, title, desc, props, req, scopes, m)
        };
        let spec = ToolkitSpec {
            slug: SLUG.into(),
            name: "HubSpot".into(),
            description: LocalizedText::new(
                "HubSpot-CRM: Kontakte, Firmen, Deals und Tickets suchen, lesen und anlegen.",
                "HubSpot CRM: search, read and write contacts, companies, deals and tickets.",
            ),
            auth_schemes: vec![
                AuthScheme::OAuth2(OAuth2Scheme {
                    authorize_url: "https://app.hubspot.com/oauth/authorize".into(),
                    token_url: "https://api.hubapi.com/oauth/v1/token".into(),
                    // Scopes of the Activepieces piece; ticket read/write is added
                    // because the Ticket actions need it.
                    default_scopes: [
                        "crm.lists.read",
                        "crm.lists.write",
                        "crm.objects.companies.read",
                        "crm.objects.companies.write",
                        "crm.objects.contacts.read",
                        "crm.objects.contacts.write",
                        "crm.objects.custom.read",
                        "crm.objects.custom.write",
                        "crm.objects.deals.read",
                        "crm.objects.deals.write",
                        "crm.objects.line_items.read",
                        "crm.objects.leads.read",
                        "crm.objects.leads.write",
                        "crm.objects.tickets.read",
                        "crm.objects.tickets.write",
                        "crm.schemas.companies.read",
                        "crm.schemas.contacts.read",
                        "crm.schemas.custom.read",
                        "crm.schemas.deals.read",
                        "crm.schemas.line_items.read",
                        "automation",
                        "e-commerce",
                        "tickets",
                        "content",
                        "settings.currencies.read",
                        "settings.users.read",
                        "settings.users.teams.read",
                        "files",
                        "forms",
                        "scheduler.meetings.meeting-link.read",
                    ]
                    .iter()
                    .map(|s| (*s).to_string())
                    .collect(),
                    pkce: false,
                    scope_separator: " ".into(),
                    refreshable: true,
                    extra_authorize_params: Vec::new(),
                }),
                AuthScheme::Bearer,
            ],
            tools: vec![
                t(
                    "HUBSPOT_SEARCH_OBJECTS",
                    ("CRM-Objekte durchsuchen", "Search CRM objects"),
                    (
                        "Durchsucht beliebige CRM-Objekte mit voller Filter-, Sortier- und Paging-Unterstützung.",
                        "Searches any CRM object type with full filter, sort and paging support.",
                    ),
                    &[
                        ("object_type", s("contacts, companies, deals, tickets, notes, tasks")),
                        (
                            "filter_groups",
                            json!({"type": "array", "items": {"type": "object"}, "description": "HubSpot filterGroups; Gruppen werden ODER-verknüpft, Filter einer Gruppe UND."}),
                        ),
                        ("query", s("Freitextsuche")),
                        ("properties", str_list("Zurückzugebende interne Property-Namen")),
                        ("sort_property_name", s("Interne Property zum Sortieren, z.B. createdate")),
                        (
                            "sort_direction",
                            json!({"type": "string", "enum": ["DESCENDING", "ASCENDING"], "default": "DESCENDING", "description": "Sortierrichtung"}),
                        ),
                        ("limit", int("1-200", 1, 200, 10)),
                        ("after", s("Paging-Cursor (paging.next.after)")),
                    ],
                    &["object_type"],
                    &[CONTACTS_R],
                    false,
                ),
                t(
                    "HUBSPOT_FIND_CONTACT",
                    ("Kontakt suchen", "Find contact"),
                    (
                        "Findet bis zu 200 Kontakte, die auf ein oder zwei Property-Werte passen.",
                        "Finds up to 200 contacts matching one or two property values.",
                    ),
                    &find_props("email, firstname, hs_object_id"),
                    &["value"],
                    &[CONTACTS_R],
                    false,
                ),
                t(
                    "HUBSPOT_FIND_COMPANY",
                    ("Firma suchen", "Find company"),
                    (
                        "Findet bis zu 200 Firmen, die auf ein oder zwei Property-Werte passen.",
                        "Finds up to 200 companies matching one or two property values.",
                    ),
                    &find_props("domain, name, hs_object_id"),
                    &["value"],
                    &[COMPANIES_R],
                    false,
                ),
                t(
                    "HUBSPOT_FIND_DEAL",
                    ("Deal suchen", "Find deal"),
                    (
                        "Findet bis zu 200 Deals, die auf ein oder zwei Property-Werte passen.",
                        "Finds up to 200 deals matching one or two property values.",
                    ),
                    &find_props("dealname, amount, hs_object_id"),
                    &["value"],
                    &[DEALS_R],
                    false,
                ),
                t(
                    "HUBSPOT_FIND_TICKET",
                    ("Ticket suchen", "Find ticket"),
                    (
                        "Findet bis zu 200 Tickets, die auf ein oder zwei Property-Werte passen.",
                        "Finds up to 200 tickets matching one or two property values.",
                    ),
                    &find_props("subject, hs_ticket_id, hs_object_id"),
                    &["value"],
                    &[TICKETS_R],
                    false,
                ),
                t(
                    "HUBSPOT_GET_CONTACT",
                    ("Kontakt lesen", "Get contact"),
                    (
                        "Liest einen Kontakt samt der Standard-Properties.",
                        "Reads a contact including the default properties.",
                    ),
                    &get_props("contact_id", "Contact ID, z.B. aus HUBSPOT_FIND_CONTACT"),
                    &["contact_id"],
                    &[CONTACTS_R],
                    false,
                ),
                t(
                    "HUBSPOT_GET_COMPANY",
                    ("Firma lesen", "Get company"),
                    (
                        "Liest eine Firma samt der Standard-Properties.",
                        "Reads a company including the default properties.",
                    ),
                    &get_props("company_id", "Company ID, z.B. aus HUBSPOT_FIND_COMPANY"),
                    &["company_id"],
                    &[COMPANIES_R],
                    false,
                ),
                t(
                    "HUBSPOT_GET_DEAL",
                    ("Deal lesen", "Get deal"),
                    (
                        "Liest einen Deal samt der Standard-Properties.",
                        "Reads a deal including the default properties.",
                    ),
                    &get_props("deal_id", "Deal ID, z.B. aus HUBSPOT_FIND_DEAL"),
                    &["deal_id"],
                    &[DEALS_R],
                    false,
                ),
                t(
                    "HUBSPOT_GET_TICKET",
                    ("Ticket lesen", "Get ticket"),
                    (
                        "Liest ein Ticket samt der Standard-Properties.",
                        "Reads a ticket including the default properties.",
                    ),
                    &get_props("ticket_id", "Ticket ID, z.B. aus HUBSPOT_FIND_TICKET"),
                    &["ticket_id"],
                    &[TICKETS_R],
                    false,
                ),
                t(
                    "HUBSPOT_LIST_OBJECT_PROPERTIES",
                    ("Objekt-Properties auflisten", "List object properties"),
                    (
                        "Listet die Properties eines CRM-Objekttyps; jede Property trägt `hubspotDefined`, um eigene Felder zu filtern.",
                        "Lists the properties of a CRM object type; each property carries `hubspotDefined` so custom fields can be filtered.",
                    ),
                    &[("object_type", s("contacts, companies, deals, tickets, notes, tasks"))],
                    &["object_type"],
                    SCHEMAS_R,
                    false,
                ),
                t(
                    "HUBSPOT_LIST_OWNERS",
                    ("Owner auflisten", "List owners"),
                    (
                        "Listet die CRM-Owner; optional nach E-Mail gefiltert.",
                        "Lists the CRM owners; optionally filtered by email.",
                    ),
                    &[
                        ("email", s("Nur der Owner mit dieser E-Mail")),
                        ("limit", int("1-500", 1, 500, 100)),
                        ("after", s("Paging-Cursor")),
                    ],
                    &[],
                    &[OWNERS_R],
                    false,
                ),
                t(
                    "HUBSPOT_GET_OWNER_BY_EMAIL",
                    ("Owner nach E-Mail", "Get owner by email"),
                    (
                        "Findet den HubSpot-Benutzer mit dieser E-Mail.",
                        "Finds the HubSpot user with this email address.",
                    ),
                    &[("email", s("E-Mail des Owners"))],
                    &["email"],
                    &[OWNERS_R],
                    false,
                ),
                t(
                    "HUBSPOT_GET_OWNER_BY_ID",
                    ("Owner nach ID", "Get owner by ID"),
                    (
                        "Liest einen Owner über seine `hubspot_owner_id`.",
                        "Reads an owner by its `hubspot_owner_id`.",
                    ),
                    &[("owner_id", s("Numerische Owner-ID"))],
                    &["owner_id"],
                    &[OWNERS_R],
                    false,
                ),
                t(
                    "HUBSPOT_LIST_PIPELINES",
                    ("Pipelines auflisten", "List pipelines"),
                    (
                        "Listet die Pipelines für Deals oder Tickets, inklusive Stages.",
                        "Lists the pipelines for deals or tickets, including their stages.",
                    ),
                    &[(
                        "object_type",
                        json!({"type": "string", "enum": ["deals", "tickets"], "default": "deals", "description": "Art der Pipeline"}),
                    )],
                    &["object_type"],
                    &[DEALS_R, TICKETS_R],
                    false,
                ),
                t(
                    "HUBSPOT_LIST_PIPELINE_STAGES",
                    ("Pipeline-Stages auflisten", "List pipeline stages"),
                    (
                        "Listet die Stages einer Pipeline in Anzeigereihenfolge.",
                        "Lists the stages of a single pipeline in display order.",
                    ),
                    &[
                        (
                            "object_type",
                            json!({"type": "string", "enum": ["deals", "tickets"], "default": "deals", "description": "Art der Pipeline"}),
                        ),
                        ("pipeline_id", s("Pipeline-ID aus HUBSPOT_LIST_PIPELINES")),
                    ],
                    &["object_type", "pipeline_id"],
                    &[DEALS_R, TICKETS_R],
                    false,
                ),
                t(
                    "HUBSPOT_GET_PIPELINE_STAGE",
                    ("Pipeline-Stage lesen", "Get pipeline stage"),
                    (
                        "Liest eine einzelne Stage (z.B. \"Closed Won\").",
                        "Reads a single pipeline stage (e.g. \"Closed Won\").",
                    ),
                    &[
                        (
                            "object_type",
                            json!({"type": "string", "enum": ["deals", "tickets"], "description": "Art der Pipeline"}),
                        ),
                        ("pipeline_id", s("Pipeline-ID")),
                        ("stage_id", s("Stage-ID aus HUBSPOT_LIST_PIPELINES")),
                    ],
                    &["object_type", "pipeline_id", "stage_id"],
                    &[DEALS_R, TICKETS_R],
                    false,
                ),
                t(
                    "HUBSPOT_LIST_ASSOCIATION_TYPES",
                    ("Assoziations-Typen auflisten", "List association types"),
                    (
                        "Listet die Typ-IDs, mit denen zwei Objekttypen verknüpft werden können.",
                        "Lists the type ids available for linking two object types.",
                    ),
                    &[
                        ("from_object_type", s("contact, company, deal, ticket")),
                        ("to_object_type", s("contact, company, deal, ticket")),
                    ],
                    &["from_object_type", "to_object_type"],
                    ASSOC_R,
                    false,
                ),
                t(
                    "HUBSPOT_FIND_ASSOCIATIONS",
                    ("Assoziationen auflisten", "Find associations"),
                    (
                        "Listet die Objekte, mit denen ein Datensatz verknüpft ist.",
                        "Lists the objects a record is associated with.",
                    ),
                    &[
                        ("from_object_id", s("ID des Ausgangsdatensatzes")),
                        ("from_object_type", s("contact, company, deal, ticket")),
                        ("to_object_type", s("contact, company, deal, ticket")),
                        ("after", s("Paging-Cursor")),
                    ],
                    &["from_object_id", "from_object_type", "to_object_type"],
                    ASSOC_R,
                    false,
                ),
                t(
                    "HUBSPOT_LIST_STATIC_LISTS",
                    ("Kontaktlisten auflisten", "List static lists"),
                    (
                        "Listet die Kontaktlisten des Accounts (Namensfilter optional).",
                        "Lists the contact lists in the account (optional name filter).",
                    ),
                    &[
                        ("query", s("Name enthält diesen Text")),
                        ("count", int("1-100", 1, 100, 20)),
                        ("offset", int("Offset aus einem vorherigen Aufruf", 0, 10_000, 0)),
                    ],
                    &[],
                    &[LISTS_R],
                    false,
                ),
                t(
                    "HUBSPOT_CREATE_CONTACT",
                    ("Kontakt anlegen", "Create contact"),
                    (
                        "Legt einen Kontakt an (schreibend, braucht --confirm).",
                        "Creates a contact (writes data; requires --confirm).",
                    ),
                    &[(
                        "properties",
                        json!({"type": "object", "description": "Interne Property-Namen zu Werten; Listen werden mit ';' verbunden."}),
                    )],
                    &[],
                    &[CONTACTS_W],
                    true,
                ),
                t(
                    "HUBSPOT_UPDATE_CONTACT",
                    ("Kontakt aktualisieren", "Update contact"),
                    (
                        "Aktualisiert die Properties eines Kontakts (braucht --confirm).",
                        "Updates the properties of a contact (requires --confirm).",
                    ),
                    &[
                        ("contact_id", s("Contact ID, z.B. aus HUBSPOT_FIND_CONTACT")),
                        (
                            "properties",
                            json!({"type": "object", "description": "Interne Property-Namen zu Werten; Listen werden mit ';' verbunden."}),
                        ),
                    ],
                    &["contact_id"],
                    &[CONTACTS_W],
                    true,
                ),
                t(
                    "HUBSPOT_UPSERT_CONTACT",
                    ("Kontakt anlegen oder aktualisieren", "Upsert contact"),
                    (
                        "Legt den Kontakt mit dieser E-Mail an oder aktualisiert ihn (Batch-Upsert, braucht --confirm).",
                        "Creates or updates the contact with this email (batch upsert; requires --confirm).",
                    ),
                    &[
                        ("email", s("E-Mail des Kontakts")),
                        (
                            "properties",
                            json!({"type": "object", "description": "Interne Property-Namen zu Werten; Listen werden mit ';' verbunden."}),
                        ),
                    ],
                    &["email"],
                    &[CONTACTS_W],
                    true,
                ),
                t(
                    "HUBSPOT_CREATE_COMPANY",
                    ("Firma anlegen", "Create company"),
                    (
                        "Legt eine Firma an (braucht --confirm).",
                        "Creates a company (requires --confirm).",
                    ),
                    &[(
                        "properties",
                        json!({"type": "object", "description": "Interne Property-Namen zu Werten; Listen werden mit ';' verbunden."}),
                    )],
                    &[],
                    &[COMPANIES_W],
                    true,
                ),
                t(
                    "HUBSPOT_CREATE_DEAL",
                    ("Deal anlegen", "Create deal"),
                    (
                        "Legt einen Deal in Pipeline und Stage an (braucht --confirm).",
                        "Creates a deal in a pipeline and stage (requires --confirm).",
                    ),
                    &[
                        ("dealname", s("Name des Deals")),
                        ("pipeline_id", s("Pipeline-ID, leer für die Standardpipeline")),
                        ("pipeline_stage_id", s("Stage-ID, leer für die Standardstage")),
                        (
                            "properties",
                            json!({"type": "object", "description": "Weitere Property-Namen zu Werten; Listen werden mit ';' verbunden."}),
                        ),
                    ],
                    &["dealname"],
                    &[DEALS_W],
                    true,
                ),
                t(
                    "HUBSPOT_UPDATE_DEAL",
                    ("Deal aktualisieren", "Update deal"),
                    (
                        "Ändert Name, Pipeline oder Stage eines Deals (braucht --confirm).",
                        "Changes the name, pipeline or stage of a deal (requires --confirm).",
                    ),
                    &[
                        ("deal_id", s("Deal ID, z.B. aus HUBSPOT_FIND_DEAL")),
                        ("dealname", s("Neuer Name; leer lässt den Namen unverändert")),
                        ("pipeline_id", s("Neue Pipeline-ID")),
                        ("pipeline_stage_id", s("Neue Stage-ID")),
                        (
                            "properties",
                            json!({"type": "object", "description": "Weitere Property-Namen zu Werten; Listen werden mit ';' verbunden."}),
                        ),
                    ],
                    &["deal_id"],
                    &[DEALS_W],
                    true,
                ),
                t(
                    "HUBSPOT_CREATE_TICKET",
                    ("Ticket anlegen", "Create ticket"),
                    (
                        "Legt ein Ticket in Pipeline und Stage an (braucht --confirm).",
                        "Creates a ticket in a pipeline and stage (requires --confirm).",
                    ),
                    &[
                        ("ticket_name", s("Betreff des Tickets")),
                        ("pipeline_id", s("Pipeline-ID, leer für die Standardpipeline")),
                        ("pipeline_stage_id", s("Stage-ID, leer für die Standardstage")),
                        (
                            "properties",
                            json!({"type": "object", "description": "Weitere Property-Namen zu Werten; Listen werden mit ';' verbunden."}),
                        ),
                    ],
                    &["ticket_name"],
                    &[TICKETS_W],
                    true,
                ),
                t(
                    "HUBSPOT_CREATE_NOTE",
                    ("Notiz anlegen", "Create note"),
                    (
                        "Hängt eine Notiz an einen Kontakt, eine Firma, einen Deal oder ein Ticket (braucht --confirm).",
                        "Attaches a note to a contact, company, deal or ticket (requires --confirm).",
                    ),
                    &[
                        ("note_body", s("Text der Notiz; einfaches HTML bleibt erhalten")),
                        (
                            "object_type",
                            json!({"type": "string", "enum": ["contact", "company", "deal", "ticket"], "default": "contact", "description": "Objekttyp, an den die Notiz gehängt wird"}),
                        ),
                        ("object_id", s("ID des Datensatzes")),
                        ("timestamp", s("ISO-8601-Zeitstempel, z.B. 2026-09-14T12:00:00Z; Standard: jetzt")),
                    ],
                    &["note_body", "object_type", "object_id"],
                    &[CONTACTS_W],
                    true,
                ),
                t(
                    "HUBSPOT_CREATE_TASK",
                    ("Aufgabe anlegen", "Create task"),
                    (
                        "Hängt eine Aufgabe mit Fälligkeit an einen Datensatz (braucht --confirm).",
                        "Attaches a task with a due date to a record (requires --confirm).",
                    ),
                    &[
                        ("subject", s("Titel der Aufgabe")),
                        (
                            "object_type",
                            json!({"type": "string", "enum": ["contact", "company", "deal", "ticket"], "default": "contact", "description": "Objekttyp, an den die Aufgabe gehängt wird"}),
                        ),
                        ("object_id", s("ID des Datensatzes")),
                        ("due_date", s("ISO-8601-Fälligkeit, z.B. 2026-09-20T09:00:00Z; Standard: jetzt")),
                        ("body", s("Optionaler Detailtext")),
                        (
                            "status",
                            json!({"type": "string", "enum": ["NOT_STARTED", "IN_PROGRESS", "WAITING", "COMPLETED", "DEFERRED"], "default": "NOT_STARTED", "description": "Status"}),
                        ),
                        (
                            "priority",
                            json!({"type": "string", "enum": ["LOW", "MEDIUM", "HIGH"], "default": "MEDIUM", "description": "Priorität"}),
                        ),
                        ("owner_id", s("Owner-ID aus HUBSPOT_LIST_OWNERS")),
                    ],
                    &["subject", "object_type", "object_id"],
                    &[CONTACTS_W],
                    true,
                ),
                t(
                    "HUBSPOT_ADD_CONTACT_TO_LIST",
                    ("Kontakt zur Liste hinzufügen", "Add contact to list"),
                    (
                        "Trägt eine E-Mail in eine statische Kontaktliste ein (braucht --confirm).",
                        "Adds an email address to a static contact list (requires --confirm).",
                    ),
                    &[
                        ("list_id", s("Listen-ID aus HUBSPOT_LIST_STATIC_LISTS")),
                        ("email", s("E-Mail des Kontakts")),
                    ],
                    &["list_id", "email"],
                    &[LISTS_W],
                    true,
                ),
                t(
                    "HUBSPOT_UNSUBSCRIBE_EMAIL",
                    ("E-Mail-Abmeldung setzen", "Unsubscribe email"),
                    (
                        "Meldet eine E-Mail-Adresse von allen Marketing-Mails ab (braucht --confirm).",
                        "Unsubscribes an email address from all marketing email (requires --confirm).",
                    ),
                    &[("email", s("E-Mail des Kontakts"))],
                    &["email"],
                    &[CONTACTS_W],
                    true,
                ),
                t(
                    "HUBSPOT_ADD_CONTACT_TO_WORKFLOW",
                    ("Kontakt in Workflow aufnehmen", "Enroll contact in workflow"),
                    (
                        "Nimmt einen Kontakt per E-Mail in einen Workflow auf (braucht --confirm).",
                        "Enrolls a contact by email into a workflow (requires --confirm).",
                    ),
                    &[
                        ("workflow_id", s("ID des aktiven Workflows")),
                        ("email", s("E-Mail des Kontakts")),
                    ],
                    &["workflow_id", "email"],
                    &[AUTOMATION],
                    true,
                ),
                t(
                    "HUBSPOT_CREATE_ASSOCIATIONS",
                    ("Assoziationen anlegen", "Create associations"),
                    (
                        "Verknüpft einen Datensatz mit weiteren Datensätzen (braucht --confirm).",
                        "Associates a record with other records (requires --confirm).",
                    ),
                    &[
                        ("from_object_id", s("ID des Ausgangsdatensatzes")),
                        ("from_object_type", s("contact, company, deal, ticket")),
                        ("to_object_type", s("contact, company, deal, ticket")),
                        ("association_type_id", s("Typ-ID aus HUBSPOT_LIST_ASSOCIATION_TYPES")),
                        (
                            "association_category",
                            json!({"type": "string", "enum": ["HUBSPOT_DEFINED", "USER_DEFINED"], "default": "HUBSPOT_DEFINED", "description": "Kategorie der Assoziation"}),
                        ),
                        ("to_object_ids", str_list("IDs der Ziel-Datensätze (max. 100)")),
                    ],
                    &["from_object_id", "from_object_type", "to_object_type", "association_type_id", "to_object_ids"],
                    ASSOC_R,
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

/// POST /crm/v3/objects/<objects>/search with the Find-action filter shape.
fn find_request(&self, objects: &str, defaults: &[&str], default_prop: &str, args: &Value) -> Result<HttpRequest> {
    let mut filters = vec![json!({"propertyName": opt_str(args, "property_name").unwrap_or(default_prop),
        "operator": "EQ", "value": req_str(args, "value")?})];
    if let (Some(p), Some(v)) = (opt_str(args, "property_name_2"), opt_str(args, "value_2")) {
        filters.push(json!({"propertyName": p, "operator": "EQ", "value": v}));
    }
    let body = json!({
        "limit": MAX_SEARCH_PAGE_SIZE,
        "properties": property_list(defaults, args).split(',').map(String::from).collect::<Vec<_>>(),
        "filterGroups": [{"filters": filters}],
    });
    Ok(HttpRequest::new(
        "POST",
        self.url(&format!("/crm/v3/objects/{objects}/search")),
    )
    .json(&body))
}

/// GET /crm/v3/objects/<objects>/<id> with the default + extra properties.
fn get_request(&self, objects: &str, defaults: &[&str], id_key: &str, args: &Value) -> Result<HttpRequest> {
    let id = path_segment(args, id_key)?;
    let props = query(&[("properties", Some(property_list(defaults, args)))]);
    Ok(HttpRequest::new("GET", self.url(&format!("/crm/v3/objects/{objects}/{id}{props}"))))
}
}

/// Percent-encode a value that appears inside a URL path (e.g. an email address).
fn path_encoded(args: &Value, key: &str) -> Result<String> {
    let v = req_str(args, key)?;
    if v.len() > 200 || v.contains('/') || v.contains('?') || v.contains('#') {
        bail!("invalid argument `{key}`");
    }
    Ok(url::form_urlencoded::byte_serialize(v.as_bytes()).collect())
}

/// `properties` argument -> HubSpot property map (arrays joined with `;`).
fn prop_map(args: &Value, extra: &[(&str, Option<&str>)]) -> Value {
    let mut m = Map::new();
    for (k, v) in extra {
        if let Some(v) = v {
            m.insert((*k).into(), json!(v));
        }
    }
    if let Some(Value::Object(obj)) = args.get("properties") {
        for (k, v) in obj {
            let v = match v {
                Value::Array(a) => a
                    .iter()
                    .map(|x| match x {
                        Value::String(s) => s.clone(),
                        other => other.to_string(),
                    })
                    .collect::<Vec<_>>()
                    .join(";"),
                Value::String(s) => s.clone(),
                Value::Null => continue,
                other => other.to_string(),
            };
            m.insert(k.clone(), json!(v));
        }
    }
    Value::Object(m)
}

/// Defaults + `properties` argument as a comma separated list.
fn property_list(defaults: &[&str], args: &Value) -> String {
    let mut out: Vec<String> = defaults.iter().map(|s| (*s).to_string()).collect();
    for extra in str_vec(args, "properties") {
        if !out.contains(&extra) {
            out.push(extra);
        }
    }
    out.join(",")
}

/// `associations` block used by note and task creation.
fn association(object_type: &str, object_id: &str, ids: &[(&str, i64)]) -> Result<Value> {
    let id = ids
        .iter()
        .find(|(t, _)| *t == object_type)
        .map(|(_, id)| *id)
        .ok_or_else(|| anyhow::anyhow!("unsupported object type `{object_type}`"))?;
    Ok(json!([{"to": {"id": object_id},
        "types": [{"associationCategory": "HUBSPOT_DEFINED", "associationTypeId": id}]}]))
}

/// UTC now as ISO-8601 (used for the default note timestamp / task due date).
fn now_iso8601() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let days = (secs / 86_400) as i64;
    let (h, mi, s) = (secs % 86_400 / 3600, secs % 3600 / 60, secs % 60);
    // days-from-civil, Howard Hinnant's algorithm.
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let (y, m, d) = (yoe + era * 400, if mp < 10 { mp + 3 } else { mp - 9 }, doy - (153 * mp + 2) / 5 + 1);
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02}T{h:02}:{mi:02}:{s:02}Z")
}

impl ToolkitAdapter for HubSpot {
    fn spec(&self) -> &ToolkitSpec {
        &self.spec
    }

    fn build_request(&self, tool: &str, args: &Value) -> Result<HttpRequest> {
        Ok(match tool {
            "HUBSPOT_SEARCH_OBJECTS" => {
                let objects = path_segment(args, "object_type")?;
                let mut body = Map::new();
                if let Some(fg) = args.get("filter_groups") {
                    if fg.is_array() {
                        body.insert("filterGroups".into(), fg.clone());
                    }
                }
                if let Some(q) = opt_str(args, "query") {
                    body.insert("query".into(), json!(q));
                }
                let props = str_vec(args, "properties");
                if !props.is_empty() {
                    body.insert("properties".into(), json!(props));
                }
                if let Some(p) = opt_str(args, "sort_property_name") {
                    let dir = opt_str(args, "sort_direction").unwrap_or("DESCENDING");
                    body.insert(
                        "sorts".into(),
                        json!([{"propertyName": p, "direction": dir}]),
                    );
                }
                if let Some(l) = args.get("limit").and_then(Value::as_u64) {
                    body.insert("limit".into(), json!(l.clamp(1, MAX_SEARCH_PAGE_SIZE)));
                }
                if let Some(a) = opt_str(args, "after") {
                    body.insert("after".into(), json!(a));
                }
                HttpRequest::new("POST", self.url(&format!("/crm/v3/objects/{objects}/search")))
                    .json(&Value::Object(body))
            }
            "HUBSPOT_FIND_CONTACT" => self.find_request("contacts", CONTACT_PROPS, "email", args)?,
            "HUBSPOT_FIND_COMPANY" => self.find_request("companies", COMPANY_PROPS, "domain", args)?,
            "HUBSPOT_FIND_DEAL" => self.find_request("deals", DEAL_PROPS, "dealname", args)?,
            "HUBSPOT_FIND_TICKET" => self.find_request("tickets", TICKET_PROPS, "subject", args)?,
            "HUBSPOT_GET_CONTACT" => self.get_request("contacts", CONTACT_PROPS, "contact_id", args)?,
            "HUBSPOT_GET_COMPANY" => self.get_request("companies", COMPANY_PROPS, "company_id", args)?,
            "HUBSPOT_GET_DEAL" => self.get_request("deals", DEAL_PROPS, "deal_id", args)?,
            "HUBSPOT_GET_TICKET" => self.get_request("tickets", TICKET_PROPS, "ticket_id", args)?,
            "HUBSPOT_LIST_OBJECT_PROPERTIES" => HttpRequest::new(
                "GET",
                self.url(&format!(
                    "/crm/v3/properties/{}",
                    path_segment(args, "object_type")?
                )),
            ),
            "HUBSPOT_LIST_OWNERS" => HttpRequest::new(
                "GET",
                self.url(&format!(
                    "/crm/v3/owners{}",
                    query(&[
                        ("email", opt_str(args, "email").map(String::from)),
                        ("limit", Some(opt_u64(args, "limit", 100, 1, 500).to_string())),
                        ("after", opt_str(args, "after").map(String::from)),
                    ])
                )),
            ),
            "HUBSPOT_GET_OWNER_BY_EMAIL" => HttpRequest::new(
                "GET",
                self.url(&format!(
                    "/crm/v3/owners{}",
                    query(&[("email", Some(req_str(args, "email")?.to_string()))])
                )),
            ),
            "HUBSPOT_GET_OWNER_BY_ID" => {
                let id = path_segment(args, "owner_id")?;
                if !id.bytes().all(|b| b.is_ascii_digit()) {
                    bail!("invalid argument `owner_id`");
                }
                HttpRequest::new("GET", self.url(&format!("/crm/v3/owners/{id}")))
            }
            "HUBSPOT_LIST_PIPELINES" => HttpRequest::new(
                "GET",
                self.url(&format!(
                    "/crm/v3/pipelines/{}",
                    path_segment(args, "object_type")?
                )),
            ),
            "HUBSPOT_LIST_PIPELINE_STAGES" => HttpRequest::new(
                "GET",
                self.url(&format!(
                    "/crm/v3/pipelines/{}/{}/stages",
                    path_segment(args, "object_type")?,
                    path_segment(args, "pipeline_id")?
                )),
            ),
            "HUBSPOT_GET_PIPELINE_STAGE" => HttpRequest::new(
                "GET",
                self.url(&format!(
                    "/crm/v3/pipelines/{}/{}/stages/{}",
                    path_segment(args, "object_type")?,
                    path_segment(args, "pipeline_id")?,
                    path_segment(args, "stage_id")?
                )),
            ),
            "HUBSPOT_LIST_ASSOCIATION_TYPES" => HttpRequest::new(
                "GET",
                self.url(&format!(
                    "/crm/v4/associations/{}/{}/labels",
                    path_segment(args, "from_object_type")?,
                    path_segment(args, "to_object_type")?
                )),
            ),
            "HUBSPOT_FIND_ASSOCIATIONS" => HttpRequest::new(
                "GET",
                self.url(&format!(
                    "/crm/v4/associations/{}/{}/{}{}",
                    path_segment(args, "from_object_type")?,
                    path_segment(args, "from_object_id")?,
                    path_segment(args, "to_object_type")?,
                    query(&[
                        ("limit", Some("100".into())),
                        ("after", opt_str(args, "after").map(String::from)),
                    ])
                )),
            ),
            "HUBSPOT_LIST_STATIC_LISTS" => {
                let mut body = Map::new();
                if let Some(q) = opt_str(args, "query") {
                    body.insert("query".into(), json!(q));
                }
                if let Some(c) = args.get("count").and_then(Value::as_u64) {
                    body.insert("count".into(), json!(c.clamp(1, 100)));
                }
                if let Some(o) = args.get("offset").and_then(Value::as_u64) {
                    body.insert("offset".into(), json!(o));
                }
                HttpRequest::new("POST", self.url("/crm/v3/lists/search"))
                    .json(&Value::Object(body))
            }
            "HUBSPOT_CREATE_CONTACT" => HttpRequest::new("POST", self.url("/crm/v3/objects/contacts"))
                .json(&json!({"properties": prop_map(args, &[])})),
            "HUBSPOT_CREATE_COMPANY" => HttpRequest::new("POST", self.url("/crm/v3/objects/companies"))
                .json(&json!({"properties": prop_map(args, &[])})),
            "HUBSPOT_UPDATE_CONTACT" => HttpRequest::new(
                "PATCH",
                self.url(&format!(
                    "/crm/v3/objects/contacts/{}",
                    path_segment(args, "contact_id")?
                )),
            )
            .json(&json!({"properties": prop_map(args, &[])})),
            "HUBSPOT_UPSERT_CONTACT" => {
                let email = req_str(args, "email")?;
                let mut props = prop_map(args, &[]);
                props["email"] = json!(email);
                HttpRequest::new("POST", self.url("/crm/v3/objects/contacts/batch/upsert")).json(
                    &json!({"idProperty": "email", "inputs": [{"id": email, "properties": props}]}),
                )
            }
            "HUBSPOT_CREATE_DEAL" => HttpRequest::new("POST", self.url("/crm/v3/objects/deals")).json(
                &json!({"properties": prop_map(args, &[
                    ("dealname", Some(req_str(args, "dealname")?)),
                    ("pipeline", opt_str(args, "pipeline_id")),
                    ("dealstage", opt_str(args, "pipeline_stage_id")),
                ])}),
            ),
            "HUBSPOT_UPDATE_DEAL" => HttpRequest::new(
                "PATCH",
                self.url(&format!(
                    "/crm/v3/objects/deals/{}",
                    path_segment(args, "deal_id")?
                )),
            )
            .json(&json!({"properties": prop_map(args, &[
                ("dealname", opt_str(args, "dealname")),
                ("pipeline", opt_str(args, "pipeline_id")),
                ("dealstage", opt_str(args, "pipeline_stage_id")),
            ])})),
            "HUBSPOT_CREATE_TICKET" => HttpRequest::new("POST", self.url("/crm/v3/objects/tickets")).json(
                &json!({"properties": prop_map(args, &[
                    ("subject", Some(req_str(args, "ticket_name")?)),
                    ("hs_pipeline", opt_str(args, "pipeline_id")),
                    ("hs_pipeline_stage", opt_str(args, "pipeline_stage_id")),
                ])}),
            ),
            "HUBSPOT_CREATE_NOTE" => {
                let ts = opt_str(args, "timestamp")
                    .map(String::from)
                    .unwrap_or_else(now_iso8601);
                let body = json!({
                    "properties": prop_map(args, &[
                        ("hs_timestamp", Some(&ts)),
                        ("hs_note_body", Some(req_str(args, "note_body")?)),
                    ]),
                    "associations": association(
                        req_str(args, "object_type")?,
                        req_str(args, "object_id")?,
                        NOTE_ASSOCIATION_IDS)?,
                });
                HttpRequest::new("POST", self.url("/crm/v3/objects/notes")).json(&body)
            }
            "HUBSPOT_CREATE_TASK" => {
                let due = opt_str(args, "due_date")
                    .map(String::from)
                    .unwrap_or_else(now_iso8601);
                let body = json!({
                    "properties": prop_map(args, &[
                        ("hs_timestamp", Some(&due)),
                        ("hs_task_subject", Some(req_str(args, "subject")?)),
                        ("hs_task_status", Some(opt_str(args, "status").unwrap_or("NOT_STARTED"))),
                        ("hs_task_priority", Some(opt_str(args, "priority").unwrap_or("MEDIUM"))),
                        ("hs_task_body", opt_str(args, "body")),
                        ("hubspot_owner_id", opt_str(args, "owner_id")),
                    ]),
                    "associations": association(
                        req_str(args, "object_type")?,
                        req_str(args, "object_id")?,
                        TASK_ASSOCIATION_IDS)?,
                });
                HttpRequest::new("POST", self.url("/crm/v3/objects/tasks")).json(&body)
            }
            "HUBSPOT_ADD_CONTACT_TO_LIST" => HttpRequest::new(
                "POST",
                self.url(&format!(
                    "/contacts/v1/lists/{}/add",
                    path_segment(args, "list_id")?
                )),
            )
            .json(&json!({"emails": [req_str(args, "email")?]})),
            "HUBSPOT_UNSUBSCRIBE_EMAIL" => HttpRequest::new(
                "PUT",
                self.url(&format!(
                    "/email/public/v1/subscriptions/{}",
                    path_encoded(args, "email")?
                )),
            )
            .json(&json!({"unsubscribeFromAll": true})),
            "HUBSPOT_ADD_CONTACT_TO_WORKFLOW" => HttpRequest::new(
                "POST",
                self.url(&format!(
                    "/automation/v2/workflows/{}/enrollments/contacts/{}",
                    path_segment(args, "workflow_id")?,
                    path_encoded(args, "email")?
                )),
            ),
            "HUBSPOT_CREATE_ASSOCIATIONS" => {
                let to_ids = str_vec(args, "to_object_ids");
                if to_ids.is_empty() {
                    bail!("missing argument `to_object_ids`");
                }
                if to_ids.len() > 100 {
                    bail!("at most 100 `to_object_ids` per call");
                }
                let from_id = path_segment(args, "from_object_id")?;
                let type_id = path_segment(args, "association_type_id")?;
                let category = opt_str(args, "association_category").unwrap_or("HUBSPOT_DEFINED");
                let inputs: Vec<Value> = to_ids
                    .iter()
                    .map(|id| {
                        json!({"_from": {"id": from_id}, "to": {"id": id},
                            "types": [{"associationCategory": category, "associationTypeId": type_id}]})
                    })
                    .collect();
                HttpRequest::new(
                    "POST",
                    self.url(&format!(
                        "/crm/v4/associations/{}/{}/batch/create",
                        path_segment(args, "from_object_type")?,
                        path_segment(args, "to_object_type")?
                    )),
                )
                .json(&json!({"inputs": inputs}))
            }
            other => bail!("unknown HubSpot tool `{other}`"),
        })
    }

    fn parse_response(&self, tool: &str, response: &HttpResponse) -> Result<Value> {
        let v = response.json()?;
        Ok(match tool {
            "HUBSPOT_SEARCH_OBJECTS"
            | "HUBSPOT_FIND_CONTACT"
            | "HUBSPOT_FIND_COMPANY"
            | "HUBSPOT_FIND_DEAL"
            | "HUBSPOT_FIND_TICKET" => json!({
                "total": v["total"],
                "results": v["results"].as_array().cloned().unwrap_or_default(),
                "paging": v["paging"].clone(),
            }),
            "HUBSPOT_LIST_OBJECT_PROPERTIES" => {
                let props = v["results"].as_array().cloned().unwrap_or_default();
                json!({"properties": props, "count": props.len()})
            }
            "HUBSPOT_LIST_OWNERS" | "HUBSPOT_GET_OWNER_BY_EMAIL" => {
                let owners = v["results"].as_array().cloned().unwrap_or_default();
                json!({"owners": owners, "count": owners.len(), "paging": v["paging"].clone()})
            }
            "HUBSPOT_GET_OWNER_BY_ID"
            | "HUBSPOT_GET_CONTACT"
            | "HUBSPOT_GET_COMPANY"
            | "HUBSPOT_GET_DEAL"
            | "HUBSPOT_GET_TICKET"
            | "HUBSPOT_GET_PIPELINE_STAGE"
            | "HUBSPOT_CREATE_CONTACT"
            | "HUBSPOT_CREATE_COMPANY"
            | "HUBSPOT_UPDATE_CONTACT"
            | "HUBSPOT_CREATE_DEAL"
            | "HUBSPOT_UPDATE_DEAL"
            | "HUBSPOT_CREATE_TICKET"
            | "HUBSPOT_CREATE_NOTE"
            | "HUBSPOT_CREATE_TASK"
            | "HUBSPOT_ADD_CONTACT_TO_LIST"
            | "HUBSPOT_ADD_CONTACT_TO_WORKFLOW"
            | "HUBSPOT_UNSUBSCRIBE_EMAIL" => v,
            "HUBSPOT_UPSERT_CONTACT" => {
                let first = v["results"]
                    .as_array()
                    .and_then(|r| r.first().cloned())
                    .unwrap_or(Value::Null);
                json!({"id": first["id"], "properties": first["properties"]})
            }
            "HUBSPOT_LIST_PIPELINES" => {
                let p = v["results"].as_array().cloned().unwrap_or_default();
                json!({"pipelines": p, "count": p.len()})
            }
            "HUBSPOT_LIST_PIPELINE_STAGES" => {
                let s = v["results"].as_array().cloned().unwrap_or_default();
                json!({"stages": s, "count": s.len()})
            }
            "HUBSPOT_LIST_ASSOCIATION_TYPES" => {
                let t = v["results"].as_array().cloned().unwrap_or_default();
                json!({"association_types": t, "count": t.len()})
            }
            "HUBSPOT_FIND_ASSOCIATIONS" => {
                let a = v["results"].as_array().cloned().unwrap_or_default();
                json!({"associations": a, "count": a.len(), "paging": v["paging"].clone()})
            }
            "HUBSPOT_LIST_STATIC_LISTS" => json!({
                "lists": v["lists"].as_array().cloned().unwrap_or_default(),
                "total": v["total"],
                "offset": v["offset"],
                "has_more": v["hasMore"],
            }),
            "HUBSPOT_CREATE_ASSOCIATIONS" => json!({
                "total_associations": v["results"].as_array().map(|r| r.len()).unwrap_or(0),
                "batch_count": 1,
                "status": v["status"],
            }),
            _ => v,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hs() -> HubSpot {
        HubSpot::with_api_base("https://hs.test")
    }

    #[test]
    fn find_get_and_guards() {
        let h = hs();
        let r = h
            .build_request(
                "HUBSPOT_FIND_CONTACT",
                &json!({"value": "a@b.de", "property_name_2": "firstname", "value_2": "Ada"}),
            )
            .unwrap();
        assert_eq!(r.method, "POST");
        assert_eq!(r.url, "https://hs.test/crm/v3/objects/contacts/search");
        let body: Value = serde_json::from_slice(r.body.as_deref().unwrap()).unwrap();
        assert_eq!(body["limit"], 200);
        assert_eq!(body["filterGroups"][0]["filters"][0]["operator"], "EQ");
        assert_eq!(body["filterGroups"][0]["filters"][1]["propertyName"], "firstname");

        let g = h
            .build_request("HUBSPOT_GET_DEAL", &json!({"deal_id": "123", "properties": ["amount"]}))
            .unwrap();
        assert!(g.url.starts_with("https://hs.test/crm/v3/objects/deals/123?properties=dealtype"));
        assert!(g.url.contains("amount"));

        assert!(h.build_request("HUBSPOT_GET_DEAL", &json!({"deal_id": "../x"})).is_err());
        assert!(h
            .build_request("HUBSPOT_GET_OWNER_BY_ID", &json!({"owner_id": "12a"}))
            .is_err());
        let u = h
            .build_request("HUBSPOT_UNSUBSCRIBE_EMAIL", &json!({"email": "a+b@x.de"}))
            .unwrap();
        assert_eq!(u.method, "PUT");
        assert_eq!(u.url, "https://hs.test/email/public/v1/subscriptions/a%2Bb%40x.de");
        assert!(h
            .build_request("HUBSPOT_UNSUBSCRIBE_EMAIL", &json!({"email": "a/../b"}))
            .is_err());
        let w = h
            .build_request(
                "HUBSPOT_ADD_CONTACT_TO_WORKFLOW",
                &json!({"workflow_id": "9", "email": "a@b.de"}),
            )
            .unwrap();
        assert_eq!(w.url, "https://hs.test/automation/v2/workflows/9/enrollments/contacts/a%40b.de");
    }

    #[test]
    fn writes_use_association_ids_and_defaults() {
        let h = hs();
        let n = h
            .build_request(
                "HUBSPOT_CREATE_NOTE",
                &json!({"note_body": "hi", "object_type": "deal", "object_id": "77"}),
            )
            .unwrap();
        let body: Value = serde_json::from_slice(n.body.as_deref().unwrap()).unwrap();
        assert_eq!(body["associations"][0]["types"][0]["associationTypeId"], 214);
        assert!(body["properties"]["hs_timestamp"].as_str().unwrap().ends_with('Z'));
        assert!(h
            .build_request(
                "HUBSPOT_CREATE_NOTE",
                &json!({"note_body": "hi", "object_type": "car", "object_id": "77"})
            )
            .is_err());

        let t = h
            .build_request(
                "HUBSPOT_CREATE_TASK",
                &json!({"subject": "call", "object_type": "contact", "object_id": "5"}),
            )
            .unwrap();
        let body: Value = serde_json::from_slice(t.body.as_deref().unwrap()).unwrap();
        assert_eq!(body["properties"]["hs_task_status"], "NOT_STARTED");
        assert_eq!(body["associations"][0]["types"][0]["associationTypeId"], 204);

        let d = h
            .build_request("HUBSPOT_CREATE_DEAL", &json!({"dealname": "ACV", "properties": {"amount": "100"}}))
            .unwrap();
        let body: Value = serde_json::from_slice(d.body.as_deref().unwrap()).unwrap();
        assert_eq!(body["properties"]["dealname"], "ACV");
        assert_eq!(body["properties"]["amount"], "100");

        let u = h
            .build_request("HUBSPOT_UPSERT_CONTACT", &json!({"email": "a@b.de", "properties": {"tags": ["x", "y"]}}))
            .unwrap();
        let body: Value = serde_json::from_slice(u.body.as_deref().unwrap()).unwrap();
        assert_eq!(body["idProperty"], "email");
        assert_eq!(body["inputs"][0]["properties"]["tags"], "x;y");
        assert_eq!(body["inputs"][0]["properties"]["email"], "a@b.de");
    }

    #[test]
    fn search_objects_body_and_timestamps() {
        let h = hs();
        let r = h
            .build_request(
                "HUBSPOT_SEARCH_OBJECTS",
                &json!({"object_type": "deals", "query": "acme", "sort_property_name": "amount",
                    "limit": 500, "after": "12", "properties": ["amount"],
                    "filter_groups": [{"filters": []}]}),
            )
            .unwrap();
        assert_eq!(r.url, "https://hs.test/crm/v3/objects/deals/search");
        let body: Value = serde_json::from_slice(r.body.as_deref().unwrap()).unwrap();
        assert_eq!(body["limit"], 200);
        assert_eq!(body["sorts"][0]["direction"], "DESCENDING");
        assert_eq!(body["after"], "12");
        assert!(h
            .build_request("HUBSPOT_SEARCH_OBJECTS", &json!({"object_type": "deals/../x"}))
            .is_err());
        let stamp = now_iso8601();
        assert_eq!(stamp.len(), 20, "{stamp}");
    }
}
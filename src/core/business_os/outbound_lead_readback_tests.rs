// Origin: CTOX
// License: AGPL-3.0-only

use super::*;

const COMPANY_FIELDS: &[&str] = &[
    "firma_name",
    "firma_fruehere_namen",
    "firma_aktivitaetsstatus",
    "firma_anschrift",
    "firma_besucheranschrift",
    "firma_postanschrift",
    "firma_postfach",
    "firma_plz",
    "firma_ort",
    "firma_land",
    "firma_email",
    "firma_domain",
    "firma_telefon",
    "firma_fax",
    "firma_geschaeftstaetigkeit",
    "firma_homepage_fact_sheet",
    "firma_geschaeftsfuehrung",
    "firma_prokura",
    "wz_code",
    "umsatz",
    "mitarbeiter",
];

const PERSON_FIELDS: &[&str] = &[
    "person_geschlecht",
    "person_titel",
    "person_vorname",
    "person_nachname",
    "person_funktion",
    "person_position",
    "person_email",
    "person_email_validation",
    "person_telefon",
    "person_linkedin",
    "person_xing",
];

fn long(text: &str, repeat: usize) -> String {
    format!("{text} ").repeat(repeat)
}

fn status(state: &str, value: Value, sources: &[(&str, &str)], person: Option<&str>) -> Value {
    json!({
        "status": state,
        "value": value,
        "sources": sources.iter().map(|(id, url)| json!({
            "source_id": id,
            "url": url,
            "quote": long("<div class=\"quote\">Wörtliches Zitat mit Kontext und HTML-Resten</div>", 3),
            "person_key": person,
            "requires_credential": false,
            "task_id": "",
            "command_id": "",
        })).collect::<Vec<_>>(),
        "attempts": (0..2).map(|n| json!({"source_id": format!("attempt-{n}.example"),
            "outcome": long("blocked or no result", 6)})).collect::<Vec<_>>(),
        "reason": if state == "verified" { String::new() } else { long("Begründung", 12) },
        "person_key": person,
        "review": null,
        "revision": {"writeback_id": "writeback_0123456789abcdefghijklmnopqrstuvwxyzABCDEF",
            "command_id": "leadgen-lead-research-00000000-0000-0000-0000-000000000000",
            "attempt": 3, "written_at_ms": 1_791_993_232_717_i64},
    })
}

/// A lead shaped like the customer-tenant lead of 14.10.2026: many contacts,
/// every person field per contact (also copied into the contact), long
/// quotes with HTML, attempts, revisions and a long evidence list.
pub(crate) fn large_lead_fixture(contact_count: usize) -> Value {
    let mut field_status = Map::new();
    for (index, field) in COMPANY_FIELDS.iter().enumerate() {
        let state = match index % 7 {
            5 => "no_match",
            6 => "action_required",
            _ => "verified",
        };
        let value = match *field {
            "firma_homepage_fact_sheet" | "firma_geschaeftstaetigkeit" => {
                json!(long("Systemanbieter für Oberflächenbehandlung", 60))
            }
            "firma_email" => json!("vertrieb@example-company.test"),
            _ if state == "no_match" => Value::Null,
            _ => json!(format!("Wert {field}")),
        };
        field_status.insert(
            (*field).to_string(),
            status(
                state,
                value,
                &[
                    ("register.example", "https://register.example/a"),
                    ("northdata.example", "https://northdata.example/b"),
                ],
                None,
            ),
        );
    }
    let mut contacts = Vec::new();
    let mut person_field_status = Map::new();
    for n in 0..contact_count {
        let key = format!("p-person-{n:02}");
        let email = format!("person.{n:02}@example-company.test");
        let mut fields = Map::new();
        for (index, field) in PERSON_FIELDS.iter().enumerate() {
            let state = match (n + index) % 5 {
                3 => "no_match",
                4 => "action_required",
                _ => "verified",
            };
            let value = match *field {
                "person_email" => json!(email),
                "person_linkedin" => json!(format!("https://www.linkedin.com/in/person-{n:02}")),
                "person_xing" => json!(format!("https://www.xing.com/profile/Person_{n:02}")),
                _ => json!(format!("{field} {n}")),
            };
            let mut entry = status(
                state,
                value,
                &[("company.example", "https://company.example/team")],
                Some(&key),
            );
            if *field == "person_email_validation" {
                entry["verdict"] = json!("unknown_catch_all");
                entry["validated_email"] = json!(email);
            }
            fields.insert((*field).to_string(), entry);
        }
        person_field_status.insert(key.clone(), Value::Object(fields.clone()));
        contacts.push(json!({
            "person_key": key,
            "id": format!("contact_{n:02}"),
            "name": format!("Vorname{n} Nachname{n}"),
            "person_vorname": format!("Vorname{n}"),
            "person_nachname": format!("Nachname{n}"),
            "person_funktion": "Einkauf",
            "person_position": format!("Leitung Bereich {n}"),
            "role": "Einkauf",
            "position": format!("Leitung Bereich {n}"),
            "email": email,
            "phone": format!("+49 5231 7604-{n:03}"),
            "person_xing": format!("https://www.xing.com/profile/Person_{n:02}"),
            "sources": [{"source_id": "company.example", "url": "https://company.example/team",
                "quote": long("Team page excerpt", 20)}],
            "field_status": Value::Object(fields),
        }));
    }
    // Lead-level person entries mirror the last written person, as on the
    // tenant lead.
    for field in PERSON_FIELDS {
        field_status.insert(
            (*field).to_string(),
            person_field_status["p-person-00"][*field].clone(),
        );
    }
    let evidence = (0..110)
        .map(|n| {
            json!({"field_key": "firma_name", "source_id": format!("source-{n}.example"),
            "url": format!("https://source-{n}.example/page"), "quote": long("evidence quote", 25),
            "person_key": null})
        })
        .collect::<Vec<_>>();
    let requested = COMPANY_FIELDS
        .iter()
        .chain(PERSON_FIELDS.iter())
        .map(|field| json!(field))
        .collect::<Vec<_>>();
    json!({
        "id": "lead_fixture",
        "name": "Example Company",
        "research_status": "needs_review",
        "updated_at_ms": 1_791_995_239_702_i64,
        "field_status": Value::Object(field_status),
        "person_field_status": Value::Object(person_field_status),
        "contacts": contacts,
        "evidence": evidence,
        "payload": {"native_research_requested_fields": requested},
    })
}

fn csv_items(value: &Value) -> Vec<String> {
    value
        .as_object()
        .into_iter()
        .flat_map(|object| object.values())
        .filter_map(Value::as_str)
        .flat_map(|text| text.split(','))
        .filter(|item| !item.is_empty())
        .map(str::to_string)
        .collect()
}

#[test]
fn compact_readback_of_a_large_lead_is_bounded_and_complete() {
    let lead = large_lead_fixture(14);
    let full = serde_json::to_vec(&lead).unwrap().len();
    assert!(
        full > 200_000,
        "fixture must be large like the tenant lead: {full}"
    );

    let view = compact_lead_readback(&lead);
    let size = serde_json::to_vec(&view).unwrap().len();
    assert!(
        size <= COMPACT_LEAD_READBACK_TARGET_BYTES,
        "compact view is {size} bytes"
    );
    assert_eq!(view["view"], COMPACT_LEAD_READBACK_VIEW);
    assert_eq!(view["complete"], true);
    let text = view.to_string();
    assert!(!text.contains("Wörtliches Zitat"), "quotes are dropped");
    assert!(!text.contains("<div"), "HTML is dropped");
    assert!(!text.contains("writeback_0123"), "revisions are dropped");

    for field in COMPANY_FIELDS {
        let entry = view["company"][*field].as_array().expect(field);
        assert!(!entry[0].as_str().unwrap().is_empty());
        assert_eq!(entry[2], "register.example,northdata.example");
    }
    assert_eq!(
        view["company"]["firma_email"][1],
        "vertrieb@example-company.test"
    );

    let contacts = view["contacts"].as_array().unwrap();
    assert_eq!(contacts.len(), 14);
    for (n, contact) in contacts.iter().enumerate() {
        assert_eq!(contact["key"], format!("p-person-{n:02}"));
        assert_eq!(contact["name"], format!("Vorname{n} Nachname{n}"));
        assert_eq!(contact["function"], "Einkauf");
        assert_eq!(contact["position"], format!("Leitung Bereich {n}"));
        assert_eq!(
            contact["email"],
            format!("person.{n:02}@example-company.test")
        );
        assert_eq!(contact["phone"], format!("+49 5231 7604-{n:03}"));
        assert_eq!(
            contact["xing"],
            format!("https://www.xing.com/profile/Person_{n:02}")
        );
        // No LinkedIn member on the contact: the value comes from the
        // person status, and only when that status is verified.
        let linkedin_index = PERSON_FIELDS
            .iter()
            .position(|field| *field == "person_linkedin")
            .unwrap();
        if (n + linkedin_index) % 5 < 3 {
            assert_eq!(
                contact["linkedin"],
                format!("https://www.linkedin.com/in/person-{n:02}")
            );
        } else {
            assert!(contact.get("linkedin").is_none());
        }
        assert_eq!(contact["email_check"], "unknown_catch_all");
        let mut fields = csv_items(&contact["status"]);
        fields.sort();
        let mut expected = PERSON_FIELDS
            .iter()
            .map(|field| field.trim_start_matches("person_").to_string())
            .collect::<Vec<_>>();
        expected.sort();
        assert_eq!(fields, expected, "every person field of contact {n}");
    }

    // Counts follow the server rule: verified and no_match answer a field.
    let mut company = (0, 0, 0);
    for (index, _) in COMPANY_FIELDS.iter().enumerate() {
        match index % 7 {
            5 => company.1 += 1,
            6 => company.2 += 1,
            _ => company.0 += 1,
        }
    }
    let mut persons = (0, 0, 0);
    for n in 0..14 {
        for index in 0..PERSON_FIELDS.len() {
            match (n + index) % 5 {
                3 => persons.1 += 1,
                4 => persons.2 += 1,
                _ => persons.0 += 1,
            }
        }
    }
    let counts = &view["counts"];
    assert_eq!(
        counts["company"],
        json!({"verified": company.0, "not_found": company.1, "open": company.2})
    );
    assert_eq!(
        counts["contacts"],
        json!({"verified": persons.0, "not_found": persons.1, "open": persons.2})
    );
    assert_eq!(counts["contacts_count"], 14);
    // Every person field is open for some contact, so all eleven are open at
    // field level, plus the three action_required company fields.
    assert_eq!(counts["fields_open"], 11 + company.2);
    assert_eq!(
        counts["fields_answered"].as_u64().unwrap() + counts["fields_open"].as_u64().unwrap(),
        (COMPANY_FIELDS.len() + PERSON_FIELDS.len()) as u64
    );
}

#[test]
fn compact_readback_stays_complete_for_many_contacts() {
    let lead = large_lead_fixture(40);
    let view = compact_lead_readback(&lead);
    let size = serde_json::to_vec(&view).unwrap().len();
    assert!(size < FULL_LEAD_RECORD_INLINE_MAX_BYTES, "{size} bytes");
    let contacts = view["contacts"].as_array().unwrap();
    assert_eq!(contacts.len(), 40);
    for contact in contacts {
        assert_eq!(csv_items(&contact["status"]).len(), PERSON_FIELDS.len());
        assert!(contact["email"].as_str().is_some());
    }
}

#[test]
fn report_built_from_the_readback_names_only_stored_addresses() {
    let lead = large_lead_fixture(14);
    let view = compact_lead_readback(&lead);
    let contacts = view["contacts"].as_array().unwrap();
    let mut report = format!(
        "Gespeichert: Firma {} belegt, {} nicht gefunden, {} offen. Firmen-E-Mail {}.\n",
        view["counts"]["company"]["verified"],
        view["counts"]["company"]["not_found"],
        view["counts"]["company"]["open"],
        view["company"]["firma_email"][1].as_str().unwrap(),
    );
    for contact in contacts {
        report.push_str(&format!(
            "- {} ({}), {}, Prüfung: {}\n",
            contact["name"].as_str().unwrap(),
            contact["function"].as_str().unwrap(),
            contact["email"].as_str().unwrap(),
            contact["email_check"].as_str().unwrap(),
        ));
    }
    assert!(report_emails_not_stored(&report, &view).is_empty());

    let contradicting = format!(
        "{report}- Weitere Person: anna.fremd@example-company.test (verifiziert)\n- Vorname3 Nachname3: P.Person.99@Example-Company.test"
    );
    assert_eq!(
        report_emails_not_stored(&contradicting, &view),
        vec![
            "anna.fremd@example-company.test".to_string(),
            "p.person.99@example-company.test".to_string()
        ]
    );
}

#[test]
fn missing_requested_fields_count_as_open() {
    let lead = json!({
        "id": "lead_small",
        "field_status": {
            "firma_fax": {"status": "no_match", "value": null, "sources": []},
        },
        "payload": {"native_research_requested_fields": ["firma_fax", "umsatz"]},
    });
    let view = compact_lead_readback(&lead);
    assert_eq!(view["company"]["umsatz"][0], "missing");
    assert_eq!(
        view["counts"]["company"],
        json!({"verified": 0, "not_found": 1, "open": 1})
    );
    assert_eq!(view["counts"]["open_fields"], json!(["umsatz"]));
    assert_eq!(view["contacts"], json!([]));
}

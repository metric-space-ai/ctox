//! Vetted spec catalog shipped with the release. Read-only by construction
//! (compiled in); activation still happens per instance through the gate.
pub const CATALOG: &[(&str, &str)] = &[
    ("airtable", include_str!("../../specs/airtable.json")),
    ("asana", include_str!("../../specs/asana.json")),
    ("calendly", include_str!("../../specs/calendly.json")),
    ("frankfurter", include_str!("../../specs/frankfurter.json")),
    ("github", include_str!("../../specs/github.json")),
    ("gitlab", include_str!("../../specs/gitlab.json")),
    ("hubspot", include_str!("../../specs/hubspot.json")),
    ("monday", include_str!("../../specs/monday.json")),
    ("nager_date", include_str!("../../specs/nager_date.json")),
    ("notion", include_str!("../../specs/notion.json")),
    ("open_meteo", include_str!("../../specs/open_meteo.json")),
    ("stripe", include_str!("../../specs/stripe.json")),
    ("todoist", include_str!("../../specs/todoist.json")),
    ("trello", include_str!("../../specs/trello.json")),
    ("zippopotam", include_str!("../../specs/zippopotam.json")),
];

pub fn get(slug: &str) -> Option<&'static str> {
    CATALOG.iter().find(|(s, _)| *s == slug).map(|(_, t)| *t)
}

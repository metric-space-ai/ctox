// ref: stalwart/src/directory/domain.rs:1-80
// ref: ctox-mailserver simple domain setting definitions

pub struct DomainSettings {
    pub name: String,
    pub spf: Option<String>,
    pub dmarc: Option<String>,
}

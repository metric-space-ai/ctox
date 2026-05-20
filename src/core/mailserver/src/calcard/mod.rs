// ref: stalwart/src/calcard/mod.rs:1-12
// ref: ctox-mailserver new code exposing ical and vcard parsers

pub mod ical;
pub mod vcard;

pub use ical::{ICalendar, ICalendarEvent};
pub use vcard::VCard;

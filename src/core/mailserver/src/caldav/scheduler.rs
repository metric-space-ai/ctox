// ref: stalwart/src/caldav/scheduler.rs:1-120
// ref: ctox-mailserver SQLite-backed scheduler with conflict checks and automatic email dispatch

use crate::store::SqliteStore;
use crate::calcard::ICalendar;
use crate::util::errors::{StalwartError, StalwartResult};
use tracing::{info, warn};

pub struct CalDavScheduler {
    store: SqliteStore,
}

impl CalDavScheduler {
    pub fn new(store: SqliteStore) -> Self {
        Self { store }
    }

    pub fn handle_schedule(
        &self,
        organizer: &str,
        attendees: &[String],
        event_ical: &str,
    ) -> StalwartResult<()> {
        let ical = ICalendar::parse(event_ical)?;
        let event = match ical.events.first() {
            Some(e) => e,
            None => return Err(StalwartError::General("No VEVENT found in iCalendar scheduling request".to_string())),
        };

        info!("Scheduling event: {} organized by {}", event.summary, organizer);

        for attendee in attendees {
            // 1. Check for scheduling conflicts for local users
            if self.is_local_user(attendee) {
                if self.has_conflict(attendee, &event.dtstart, &event.dtend)? {
                    warn!("Conflict detected for local attendee: {}", attendee);
                    return Err(StalwartError::CalDavConflict {
                        message: format!("Attendee {} has a scheduling conflict.", attendee),
                    });
                }

                // Auto-put the event into the local attendee's main calendar
                let calendar_id = format!("{}_main", attendee.replace("@", "_"));
                self.store.create_calendar(&calendar_id, attendee, "Main Calendar")?;
                self.store.put_event(&calendar_id, &event.uid, event_ical)?;
                info!("Automatically scheduled event {} in local attendee calendar {}", event.uid, calendar_id);
            } else {
                // 2. Dispatch SMTP invite email for external attendees
                let mail_subject = format!("Invitation: {}", event.summary);
                let mail_body = format!(
                    "Subject: {}\r\nFrom: {}\r\nTo: {}\r\nContent-Type: text/calendar; method=REQUEST; charset=UTF-8\r\n\r\n{}",
                    mail_subject, organizer, attendee, event_ical
                );
                self.store.queue_email(organizer, attendee, &mail_body)?;
                info!("Queued external SMTP invitation to {} for event {}", attendee, event.uid);
            }
        }

        Ok(())
    }

    fn is_local_user(&self, email: &str) -> bool {
        // Local domains typically end with the configured host or can be resolved
        email.ends_with("@localhost") || email.ends_with("@ctox.local")
    }

    fn has_conflict(&self, attendee: &str, dtstart: &str, dtend: &str) -> StalwartResult<bool> {
        let calendar_id = format!("{}_main", attendee.replace("@", "_"));
        let events = self.store.get_events(&calendar_id)?;
        for (_, _, ical_data, _) in events {
            if let Ok(existing_ical) = ICalendar::parse(&ical_data) {
                if let Some(existing_event) = existing_ical.events.first() {
                    // Basic overlap check:
                    // (StartA <= EndB) and (EndA >= StartB)
                    if existing_event.dtstart <= dtend.to_string() && existing_event.dtend >= dtstart.to_string() {
                        return Ok(true);
                    }
                }
            }
        }
        Ok(false)
    }
}

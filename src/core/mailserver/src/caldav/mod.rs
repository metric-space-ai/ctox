// ref: stalwart/src/caldav/mod.rs:1-350
// ref: ctox-mailserver SQLite-backed native CalDAV server using tiny_http

use crate::calcard::ICalendar;
use crate::config::CalDavConfig;
use crate::store::SqliteStore;
use crate::util::errors::{StalwartError, StalwartResult};
use std::sync::Arc;
use tracing::{error, info};

pub mod scheduler;

pub struct CalDavServer {
    store: SqliteStore,
    config: CalDavConfig,
}

impl CalDavServer {
    pub fn new(store: SqliteStore, config: CalDavConfig) -> Self {
        Self { store, config }
    }

    pub async fn start(self: Arc<Self>) -> StalwartResult<()> {
        let server_addr = self.config.bind_address;
        let server = tiny_http::Server::http(server_addr)
            .map_err(|e| StalwartError::General(format!("Failed to bind CalDAV server: {}", e)))?;

        info!("CalDAV Server running on http://{}", server_addr);

        let self_clone = Arc::clone(&self);
        tokio::task::spawn_blocking(move || {
            for request in server.incoming_requests() {
                if let Err(e) = self_clone.handle_request(request) {
                    error!("Error handling CalDAV request: {:?}", e);
                }
            }
        });

        Ok(())
    }

    fn handle_request(&self, mut request: tiny_http::Request) -> StalwartResult<()> {
        let url = request.url().to_string();
        let method = request.method().as_str();

        info!("CalDAV Request: {} {}", method, url);

        match method {
            "OPTIONS" => {
                let response = tiny_http::Response::empty(200)
                    .with_header(
                        tiny_http::Header::from_bytes(
                            &b"Allow"[..],
                            &b"OPTIONS, GET, HEAD, PUT, DELETE, PROPFIND, PROPPATCH, REPORT"[..],
                        )
                        .unwrap(),
                    )
                    .with_header(
                        tiny_http::Header::from_bytes(
                            &b"DAV"[..],
                            &b"1, 2, calendar-access, calendar-schedule"[..],
                        )
                        .unwrap(),
                    );
                request.respond(response)?;
            }
            "PROPFIND" => {
                // Return static XML describing the calendars for the user (owner)
                let xml_body = r#"<?xml version="1.0" encoding="utf-8" ?>
<d:multistatus xmlns:d="DAV:" xmlns:c="urn:ietf:params:xml:ns:caldav">
 <d:response>
  <d:href>/calendars/owner/main/</d:href>
  <d:propstat>
   <d:prop>
    <d:resourcetype>
     <d:collection/>
     <c:calendar/>
    </d:resourcetype>
    <d:displayname>Main Calendar</d:displayname>
   </d:prop>
   <d:status>HTTP/1.1 200 OK</d:status>
  </d:propstat>
 </d:response>
</d:multistatus>"#;
                let response = tiny_http::Response::from_string(xml_body)
                    .with_status_code(207)
                    .with_header(
                        tiny_http::Header::from_bytes(
                            &b"Content-Type"[..],
                            &b"application/xml; charset=utf-8"[..],
                        )
                        .unwrap(),
                    );
                request.respond(response)?;
            }
            "GET" => {
                // e.g. /calendars/owner/main/event_uid.ics
                if let Some(uid) = extract_uid_from_path(&url) {
                    if let Ok(Some((ical_data, _))) = self.store.get_event("main", &uid) {
                        let response = tiny_http::Response::from_string(ical_data)
                            .with_status_code(200)
                            .with_header(
                                tiny_http::Header::from_bytes(
                                    &b"Content-Type"[..],
                                    &b"text/calendar; charset=utf-8"[..],
                                )
                                .unwrap(),
                            );
                        request.respond(response)?;
                        return Ok(());
                    }
                }
                let response = tiny_http::Response::empty(404);
                request.respond(response)?;
            }
            "PUT" => {
                if let Some(uid) = extract_uid_from_path(&url) {
                    let mut body_bytes = Vec::new();
                    request.as_reader().read_to_end(&mut body_bytes)?;
                    let body_str = String::from_utf8_lossy(&body_bytes).to_string();

                    // Parse & validate iCal
                    if let Ok(ical) = ICalendar::parse(&body_str) {
                        if !ical.events.is_empty() {
                            self.store.put_event("main", &uid, &body_str)?;
                            let response = tiny_http::Response::empty(201);
                            request.respond(response)?;
                            return Ok(());
                        }
                    }
                }
                let response = tiny_http::Response::empty(400);
                request.respond(response)?;
            }
            "DELETE" => {
                if let Some(uid) = extract_uid_from_path(&url) {
                    self.store.delete_event("main", &uid)?;
                    let response = tiny_http::Response::empty(204);
                    request.respond(response)?;
                    return Ok(());
                }
                let response = tiny_http::Response::empty(404);
                request.respond(response)?;
            }
            _ => {
                let response = tiny_http::Response::empty(405);
                request.respond(response)?;
            }
        }
        Ok(())
    }
}

fn extract_uid_from_path(url: &str) -> Option<String> {
    // Expecting path to end with /calendars/owner/main/{uid}.ics or similar
    if url.ends_with(".ics") {
        let parts: Vec<&str> = url.split('/').collect();
        if let Some(filename) = parts.last() {
            return Some(filename.replace(".ics", ""));
        }
    }
    None
}

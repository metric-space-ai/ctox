// ref: internal/pluginstore/github_test.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use std::collections::BTreeMap;
use std::io::Cursor;
use std::sync::{Arc, Mutex};

use chrono::{TimeZone, Utc};

use crate::sdk::pluginstore::{
    Client, Release, ReleaseAsset, ResolvedAuthConfig, Secret, AUTH_TYPE_BEARER,
    REQUEST_KIND_REGISTRY,
};

use super::checksum::{parse_checksums, verify_checksum};
use super::github::{
    archive_name, select_release_assets, HttpRequest, HttpResponse, PluginStoreTransport,
    SafePluginStoreIo,
};
use super::UrlPolicy;

type FixtureResponse = (u16, BTreeMap<String, String>, Vec<u8>);

struct SequenceTransport {
    responses: Mutex<Vec<FixtureResponse>>,
    authorizations: Mutex<Vec<Option<String>>>,
}

impl PluginStoreTransport for SequenceTransport {
    fn get(&self, request: &HttpRequest) -> std::result::Result<HttpResponse, String> {
        self.authorizations
            .lock()
            .unwrap()
            .push(request.headers.get("Authorization").map(ToOwned::to_owned));
        let (status, headers, body) = self.responses.lock().unwrap().remove(0);
        Ok(HttpResponse {
            status,
            headers,
            body: Box::new(Cursor::new(body)),
        })
    }
}

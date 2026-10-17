// ref: internal/pluginstore/auth_test.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use std::collections::BTreeMap;

use chrono::{TimeZone, Utc};

use crate::sdk::pluginstore::{
    ResolvedAuthConfig, Secret, AUTH_TYPE_BEARER, REQUEST_KIND_ARTIFACT,
};

use super::auth::{apply_resolved_auth, request_url_allowed, UrlPolicy};

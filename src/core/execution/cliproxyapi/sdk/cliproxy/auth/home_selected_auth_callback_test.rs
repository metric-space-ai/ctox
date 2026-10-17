// ref: sdk/cliproxy/auth/home_selected_auth_callback_test.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// Port-Note: selected-auth notification is instance-injected and fires before execution
// License: MIT (upstream); modifications AGPL-3.0-only

use std::sync::{Arc, Mutex};

use super::home_execution_paths_test::{request, runtime, TestExecutor, TestHomeTransport};
use super::HomeSelectedAuthPublisher;

#[derive(Default)]
struct Selected(Mutex<Vec<(String, String)>>);
impl HomeSelectedAuthPublisher for Selected {
    fn selected(&self, auth_id: &str, auth_index: &str) {
        self.0
            .lock()
            .unwrap()
            .push((auth_id.to_owned(), auth_index.to_owned()));
    }
}

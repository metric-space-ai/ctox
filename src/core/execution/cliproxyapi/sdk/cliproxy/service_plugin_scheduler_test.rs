// ref: sdk/cliproxy/service_plugin_scheduler_test.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use std::sync::{Arc, Mutex};

use super::providers::LoadContext;
use super::service_plugins::{
    run_model_registration_tasks, ModelRegistrationPhase, ModelRegistrationTask,
};
use super::service_test_support::{runtime_fixture, TestPluginRuntime};
use super::usage::{Plugin, Record, UsageContext};

struct UsageRecorder(Arc<Mutex<usize>>);

impl Plugin for UsageRecorder {
    fn handle_usage(&self, _context: &UsageContext, _record: &Record) {
        *self.0.lock().expect("usage count") += 1;
    }
}

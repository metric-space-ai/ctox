// ref: internal/pluginhost/command_line_test.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// Port-Note: flags, output, and auth persistence use typed injected authorities
// License: MIT (upstream); modifications AGPL-3.0-only

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex};

use crate::sdk::pluginapi::{
    AuthData, CommandLineExecutionRequest, CommandLineExecutionResponse, CommandLineFlag,
    CommandLinePlugin, CommandLineRegistrationRequest, CommandLineRegistrationResponse,
    HostConfigSummary, Metadata, PluginFuture,
};

use super::command_line::{CommandLineAuthSink, CommandLinePluginRecord, CommandLineRegistry};

struct Plugin {
    flags: Vec<CommandLineFlag>,
    response: CommandLineExecutionResponse,
    received: Mutex<Vec<CommandLineExecutionRequest>>,
}

impl CommandLinePlugin for Plugin {
    fn register_command_line<'a>(
        &'a self,
        _request: CommandLineRegistrationRequest,
    ) -> PluginFuture<'a, CommandLineRegistrationResponse> {
        Box::pin(async move {
            Ok(CommandLineRegistrationResponse {
                flags: self.flags.clone(),
            })
        })
    }

    fn execute_command_line<'a>(
        &'a self,
        request: CommandLineExecutionRequest,
    ) -> PluginFuture<'a, CommandLineExecutionResponse> {
        Box::pin(async move {
            self.received
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .push(request);
            Ok(self.response.clone())
        })
    }
}

#[derive(Default)]
struct Sink(Mutex<Vec<String>>);

impl CommandLineAuthSink for Sink {
    fn save(&self, auth: &AuthData) -> Result<String, String> {
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(auth.id.clone());
        Ok(format!("/auths/{}.json", auth.id))
    }
}

fn flag(name: &str, kind: &str, default_value: &str) -> CommandLineFlag {
    CommandLineFlag {
        name: name.to_owned(),
        kind: kind.to_owned(),
        default_value: default_value.to_owned(),
        ..CommandLineFlag::default()
    }
}

fn record(id: &str, priority: i32, plugin: Arc<Plugin>) -> CommandLinePluginRecord {
    CommandLinePluginRecord {
        plugin_id: id.to_owned(),
        priority,
        metadata: Metadata {
            name: id.to_owned(),
            ..Metadata::default()
        },
        plugin,
    }
}

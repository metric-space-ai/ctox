// Origin: CTOX module graph for the upstream xAI auth package.
// License: AGPL-3.0-only

#[path = "xai.rs"]
mod flow;
mod token;
mod types;

pub use flow::*;
pub use token::*;
pub use types::*;

/// CTOX native host uses the gateway's already linked HTTP implementation.
/// No additional HTTP/TLS dependency is introduced into the daemon.
#[cfg(feature = "antigravity-http-transport")]
pub mod native_http {
    pub use wreq::{redirect, Client, Method, Response};
}
#[cfg(test)]
mod xai_auth_test;

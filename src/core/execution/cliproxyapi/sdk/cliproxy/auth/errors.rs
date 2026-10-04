// ref: sdk/cliproxy/auth/errors.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use std::error::Error;
use std::fmt;

use serde::{Deserialize, Serialize};

use crate::sdk::cliproxy::executor::{RequestScopedError, StatusError};

const REQUEST_SCOPED_ERROR_CODE: &str = "request_scoped";

fn is_zero(status: &u16) -> bool {
    *status == 0
}

/// Provider-independent authentication failure.
#[derive(Clone, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct AuthError {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub code: String,
    pub message: String,
    pub retryable: bool,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub http_status: u16,
}

impl AuthError {
    #[must_use]
    pub fn is_request_scoped(&self) -> bool {
        self.code == REQUEST_SCOPED_ERROR_CODE
            || crate::internal::clienterror::is_claude_thread_not_found(
                self.http_status,
                self.message.as_bytes(),
            )
    }

    #[must_use]
    pub fn status_code(&self) -> u16 {
        self.http_status
    }
}

impl fmt::Debug for AuthError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AuthError")
            .field("code", &self.code)
            .field("message_len", &self.message.len())
            .field("retryable", &self.retryable)
            .field("http_status", &self.http_status)
            .finish()
    }
}

impl fmt::Display for AuthError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.code.is_empty() {
            formatter.write_str(&self.message)
        } else {
            write!(formatter, "{}: {}", self.code, self.message)
        }
    }
}

impl Error for AuthError {}

impl StatusError for AuthError {
    fn status_code(&self) -> u16 {
        AuthError::status_code(self)
    }
}

impl RequestScopedError for AuthError {
    fn is_request_scoped(&self) -> bool {
        AuthError::is_request_scoped(self)
    }
}

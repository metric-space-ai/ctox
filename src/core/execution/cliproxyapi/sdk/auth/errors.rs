// ref: sdk/auth/errors.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use std::fmt;

pub const DEFAULT_EMAIL_REQUIRED_MESSAGE: &str = "cliproxy auth: email is required";

/// Indicates that an authenticator requires an email address or alias.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct EmailRequiredError {
    pub prompt: String,
}

impl EmailRequiredError {
    pub fn new(prompt: impl Into<String>) -> Self {
        Self {
            prompt: prompt.into(),
        }
    }
}

impl fmt::Display for EmailRequiredError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(if self.prompt.is_empty() {
            DEFAULT_EMAIL_REQUIRED_MESSAGE
        } else {
            &self.prompt
        })
    }
}

impl std::error::Error for EmailRequiredError {}

/// Preserves the observable nil-receiver branch of the Go `Error` method.
pub fn email_required_message(error: Option<&EmailRequiredError>) -> &str {
    match error {
        Some(error) if !error.prompt.is_empty() => &error.prompt,
        _ => DEFAULT_EMAIL_REQUIRED_MESSAGE,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
}

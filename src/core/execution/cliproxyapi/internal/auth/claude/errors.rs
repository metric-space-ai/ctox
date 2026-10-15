// ref: internal/auth/claude/errors.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use std::error::Error;
use std::fmt;
use std::sync::LazyLock;

use serde::{Deserialize, Serialize};

/// OAuth error returned by Anthropic's authorization endpoint.
///
/// `status_code` is intentionally excluded from JSON, matching Go's `json:"-"`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OAuthError {
    #[serde(rename = "error")]
    pub code: String,
    #[serde(
        rename = "error_description",
        default,
        skip_serializing_if = "String::is_empty"
    )]
    pub description: String,
    #[serde(
        rename = "error_uri",
        default,
        skip_serializing_if = "String::is_empty"
    )]
    pub uri: String,
    #[serde(skip)]
    pub status_code: i32,
}

impl OAuthError {
    pub fn new(code: impl Into<String>, description: impl Into<String>, status_code: i32) -> Self {
        Self {
            code: code.into(),
            description: description.into(),
            uri: String::new(),
            status_code,
        }
    }
}

impl fmt::Display for OAuthError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.description.is_empty() {
            write!(formatter, "OAuth error: {}", self.code)
        } else {
            write!(formatter, "OAuth error {}: {}", self.code, self.description)
        }
    }
}

impl Error for OAuthError {}

pub fn new_oauth_error(
    code: impl Into<String>,
    description: impl Into<String>,
    status_code: i32,
) -> OAuthError {
    OAuthError::new(code, description, status_code)
}

/// Authentication failure plus its optional underlying error.
///
/// As in the Go source, `cause` affects the display text but is not serialized
/// and is not exposed as an `Error::source`/Go `Unwrap` chain.
pub struct AuthenticationError {
    pub error_type: String,
    pub message: String,
    pub code: i32,
    pub cause: Option<Box<dyn Error + Send + Sync + 'static>>,
}

impl AuthenticationError {
    pub fn from_base<E>(base: &Self, cause: E) -> Self
    where
        E: Error + Send + Sync + 'static,
    {
        Self {
            error_type: base.error_type.clone(),
            message: base.message.clone(),
            code: base.code,
            cause: Some(Box::new(cause)),
        }
    }

    fn base(error_type: impl Into<String>, message: impl Into<String>, code: i32) -> Self {
        Self {
            error_type: error_type.into(),
            message: message.into(),
            code,
            cause: None,
        }
    }
}

impl fmt::Debug for AuthenticationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AuthenticationError")
            .field("error_type", &self.error_type)
            .field("message", &self.message)
            .field("code", &self.code)
            .field("cause", &self.cause.as_ref().map(|_| "[REDACTED]"))
            .finish()
    }
}

impl fmt::Display for AuthenticationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.cause {
            Some(cause) => write!(
                formatter,
                "{}: {} (caused by: {cause})",
                self.error_type, self.message
            ),
            None => write!(formatter, "{}: {}", self.error_type, self.message),
        }
    }
}

impl Error for AuthenticationError {}

impl Serialize for AuthenticationError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        #[derive(Serialize)]
        struct Wire<'a> {
            #[serde(rename = "type")]
            error_type: &'a str,
            message: &'a str,
            code: i32,
        }

        Wire {
            error_type: &self.error_type,
            message: &self.message,
            code: self.code,
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for AuthenticationError {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct Wire {
            #[serde(rename = "type")]
            error_type: String,
            message: String,
            code: i32,
        }

        let wire = Wire::deserialize(deserializer)?;
        Ok(Self {
            error_type: wire.error_type,
            message: wire.message,
            code: wire.code,
            cause: None,
        })
    }
}

pub static ERR_INVALID_STATE: LazyLock<AuthenticationError> = LazyLock::new(|| {
    AuthenticationError::base("invalid_state", "OAuth state parameter is invalid", 400)
});

pub static ERR_CODE_EXCHANGE_FAILED: LazyLock<AuthenticationError> = LazyLock::new(|| {
    AuthenticationError::base(
        "code_exchange_failed",
        "Failed to exchange authorization code for tokens",
        400,
    )
});

pub static ERR_SERVER_START_FAILED: LazyLock<AuthenticationError> = LazyLock::new(|| {
    AuthenticationError::base(
        "server_start_failed",
        "Failed to start OAuth callback server",
        500,
    )
});

pub static ERR_PORT_IN_USE: LazyLock<AuthenticationError> = LazyLock::new(|| {
    AuthenticationError::base("port_in_use", "OAuth callback port is already in use", 13)
});

pub static ERR_CALLBACK_TIMEOUT: LazyLock<AuthenticationError> = LazyLock::new(|| {
    AuthenticationError::base(
        "callback_timeout",
        "Timeout waiting for OAuth callback",
        408,
    )
});

pub fn new_authentication_error<E>(base: &AuthenticationError, cause: E) -> AuthenticationError
where
    E: Error + Send + Sync + 'static,
{
    AuthenticationError::from_base(base, cause)
}

pub fn is_authentication_error(error: &(dyn Error + 'static)) -> bool {
    find_in_chain::<AuthenticationError>(error).is_some()
}

pub fn is_oauth_error(error: &(dyn Error + 'static)) -> bool {
    find_in_chain::<OAuthError>(error).is_some()
}

pub fn get_user_friendly_message(error: &(dyn Error + 'static)) -> String {
    if let Some(error) = find_in_chain::<AuthenticationError>(error) {
        return match error.error_type.as_str() {
            "token_expired" => "Your authentication has expired. Please log in again.",
            "token_invalid" => "Your authentication is invalid. Please log in again.",
            "authentication_required" => "Please log in to continue.",
            "port_in_use" => "The required port is already in use. Please close any applications using port 3000 and try again.",
            "callback_timeout" => "Authentication timed out. Please try again.",
            "browser_open_failed" => "Could not open your browser automatically. Please copy and paste the URL manually.",
            _ => "Authentication failed. Please try again.",
        }
        .to_owned();
    }

    if let Some(error) = find_in_chain::<OAuthError>(error) {
        return match error.code.as_str() {
            "access_denied" => "Authentication was cancelled or denied.".to_owned(),
            "invalid_request" => "Invalid authentication request. Please try again.".to_owned(),
            "server_error" => "Authentication server error. Please try again later.".to_owned(),
            _ => format!("Authentication failed: {}", error.description),
        };
    }

    "An unexpected error occurred. Please try again.".to_owned()
}

fn find_in_chain<'a, T>(mut error: &'a (dyn Error + 'static)) -> Option<&'a T>
where
    T: Error + 'static,
{
    loop {
        if let Some(found) = error.downcast_ref::<T>() {
            return Some(found);
        }
        error = error.source()?;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug)]
    struct Cause(&'static str);

    impl fmt::Display for Cause {
        fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str(self.0)
        }
    }

    impl Error for Cause {}

#[test]
    fn authentication_error_copies_base_and_formats_cause() {
        let error = new_authentication_error(&ERR_INVALID_STATE, Cause("state mismatch"));
        assert_eq!(error.error_type, "invalid_state");
        assert_eq!(error.code, 400);
        assert_eq!(
            error.to_string(),
            "invalid_state: OAuth state parameter is invalid (caused by: state mismatch)"
        );
        assert_eq!(
            serde_json::to_value(&error).unwrap(),
            serde_json::json!({
                "type": "invalid_state",
                "message": "OAuth state parameter is invalid",
                "code": 400
            })
        );
        assert!(!format!("{error:?}").contains("state mismatch"));
    }

}

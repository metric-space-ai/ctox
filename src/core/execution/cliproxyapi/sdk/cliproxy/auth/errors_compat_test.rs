// ref: sdk/cliproxy/auth/errors_compat_test.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use crate::sdk::cliproxy::executor::{RequestScopedError, StatusError};

use super::{AuthError, AuthStatus};

#[test]
fn request_scoped_code_implements_the_shared_executor_trait() {
    let error = AuthError {
        code: "request_scoped".to_owned(),
        message: "request shape is invalid".to_owned(),
        retryable: false,
        http_status: 400,
    };

    assert!(error.is_request_scoped());
    assert!(RequestScopedError::is_request_scoped(&error));
    assert!(!format!("{error:?}").contains("request shape is invalid"));
}

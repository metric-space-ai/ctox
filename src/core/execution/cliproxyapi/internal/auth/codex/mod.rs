// Origin: CTOX
// License: AGPL-3.0-only

pub mod errors;
pub mod html_templates;
pub mod jwt_parser;
pub mod oauth_server;
pub mod openai;
pub mod openai_auth;
pub mod pkce;
pub mod token;
#[cfg(feature = "codex-http-transport")]
pub mod transport;

#[cfg(test)]
mod filename_test;
#[cfg(test)]
mod openai_auth_test;

pub use errors::{
    get_user_friendly_message, is_authentication_error, is_oauth_error, new_authentication_error,
    new_oauth_error, AuthenticationError, OAuthError, ERR_BROWSER_OPEN_FAILED,
    ERR_CALLBACK_TIMEOUT, ERR_CODE_EXCHANGE_FAILED, ERR_INVALID_STATE, ERR_PORT_IN_USE,
    ERR_SERVER_START_FAILED,
};
pub use html_templates::{
    render_login_success_html, HtmlTemplateError, LOGIN_SUCCESS_HTML, SETUP_NOTICE_HTML,
};
pub use jwt_parser::{parse_jwt_token, CodexAuthInfo, JwtClaims, Organization};
pub use oauth_server::{OAuthResult, OAuthServer, OAuthServerError};
pub use openai::{CodexAuthBundle, CodexTokenData, PkceCodes};
pub use openai_auth::{
    generate_auth_url, CodexAuth, CodexCodeExchangeTransport, CodexExchangeError,
    CodexExchangeHttpResponse, CodexExchangeRequest, CodexRefreshCoordinator, CodexRefreshError,
    CodexRefreshHttpResponse, CodexRefreshRequest, CodexRefreshTransport,
    CodexRefreshTransportFailure, RefreshClock, SystemRefreshClock, AUTH_URL, CLIENT_ID,
    EXCHANGE_TIMEOUT, REDIRECT_URI, REFRESH_TIMEOUT, TOKEN_URL,
};
pub use pkce::{generate_code_challenge, generate_pkce_codes};
pub use token::{
    CodexCredentialHandles, CodexRefreshSnapshot, CodexSecretHandle, CodexSecretKind,
    CodexSecretStore, CodexStoredCredentials, CodexTokenError, CodexTokenStorage, SecretStoreError,
    SecretString,
};
#[cfg(feature = "codex-http-transport")]
pub use transport::{
    new_codex_transport_with_proxy, CodexHttpTransport, CodexProxyMode, CodexProxyOverride,
    CodexTransportBuildError,
};

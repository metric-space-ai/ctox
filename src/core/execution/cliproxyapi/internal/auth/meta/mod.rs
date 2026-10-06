// Origin: CTOX module graph for the frozen upstream Meta authentication package.
// License: AGPL-3.0-only
#[path = "meta.rs"]
mod flow;
mod token;
pub use flow::{
    DeviceCodeResponse, MetaAuth, MetaAuthBundle, MetaAuthError, MetaClock, MintedKeyResponse,
    SystemMetaClock, TokenData, CLIENT_ID, DEFAULT_API_BASE_URL, DEFAULT_POLL_INTERVAL,
    DEVICE_AUTHORIZATION_ENDPOINT, DEVICE_CODE_GRANT_TYPE, HTTP_TIMEOUT, MAX_POLL_DURATION,
    MINT_ENDPOINT, TOKEN_ENDPOINT,
};
pub use token::{credential_file_name, MetaTokenStorage};
#[cfg(test)]
mod meta_auth_test;

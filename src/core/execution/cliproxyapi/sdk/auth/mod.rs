// Origin: CTOX
// License: AGPL-3.0-only

mod antigravity;
mod claude;
mod codex;
mod codex_device;
mod devin;
mod errors;
mod filestore;
mod interfaces;
mod kimi;
mod manager;
mod refresh_registry;
mod store_registry;
mod xai;

pub use devin::{
    DevinAuthenticator, DevinClock, DevinLoginPresentation, DevinLoginPresenter, DevinPromptFuture,
    DevinStateGenerator, RandomDevinStateGenerator, SystemDevinClock, DEVIN_LOGIN_TIMEOUT,
    DEVIN_MANUAL_PROMPT_DELAY,
};
pub use errors::{email_required_message, EmailRequiredError, DEFAULT_EMAIL_REQUIRED_MESSAGE};
pub use filestore::InjectedTokenStore;
pub use interfaces::{
    Authenticator, AuthenticatorError, AuthenticatorErrorKind, LoginCancellation, LoginConfig,
    LoginFuture, LoginOptions, PromptCallback, PromptError,
};
pub use kimi::{KimiAuthenticator, KimiDevicePresentation, KimiHandleFactory, KimiLoginPresenter};
pub use manager::{Manager, ManagerError, ManagerErrorKind};
pub use refresh_registry::{AuthenticatorFactory, RefreshLeadRegistry};
pub use store_registry::TokenStoreRegistry;
pub use xai::{XaiAuthenticator, XaiDevicePresentation, XaiHandleFactory, XaiLoginPresenter};

#[cfg(test)]
mod filestore_disabled_test;
#[cfg(test)]
mod filestore_test;
#[cfg(test)]
mod xai_test;
pub use antigravity::{
    ActiveAntigravityCallbackSession, AntigravityAuthenticator, AntigravityCallbackError,
    AntigravityCallbackErrorKind, AntigravityCallbackFuture, AntigravityCallbackResult,
    AntigravityCallbackSession, AntigravityCallbackStartFuture, AntigravityClock,
    AntigravityHandleFactory, AntigravityLoginPresentation, AntigravityLoginPresenter,
    AntigravityStateError, AntigravityStateGenerator, RandomAntigravityStateGenerator,
    SystemAntigravityClock, ANTIGRAVITY_CALLBACK_TIMEOUT, ANTIGRAVITY_MANUAL_PROMPT_DELAY,
};
pub use claude::{
    ActiveClaudeCallbackSession, ClaudeAuthenticator, ClaudeCallbackError, ClaudeCallbackErrorKind,
    ClaudeCallbackFuture, ClaudeCallbackResult, ClaudeCallbackSession, ClaudeCallbackStartFuture,
    ClaudeExchangeFuture, ClaudeHandleFactory, ClaudeLoginPresentation, ClaudeLoginPresenter,
    ClaudeOAuthService, ClaudeStateGenerator, RandomClaudeStateGenerator, CLAUDE_CALLBACK_PORT,
    CLAUDE_CALLBACK_TIMEOUT, CLAUDE_MANUAL_PROMPT_DELAY,
};
pub use codex::{
    ActiveCodexCallbackSession, CodexAuthenticator, CodexBrowserPresentation, CodexCallbackError,
    CodexCallbackErrorKind, CodexCallbackFuture, CodexCallbackResult, CodexCallbackSession,
    CodexCallbackStartFuture, CodexClock, CodexDevicePresentation, CodexExchangeError,
    CodexExchangeFuture, CodexHandleFactory, CodexLoginPresenter, CodexOAuthService,
    CodexStateGenerator, RandomCodexStateGenerator, SystemCodexClock, CODEX_CALLBACK_PORT,
    CODEX_CALLBACK_TIMEOUT, CODEX_MANUAL_PROMPT_DELAY, CODEX_REFRESH_LEAD,
};
pub use codex_device::{
    codex_device_is_success_status, parse_codex_device_poll_interval, poll_codex_device_token,
    request_codex_device_user_code, should_use_codex_device_flow, CodexDeviceTokenResponse,
    CodexDeviceTransport, CodexDeviceUserCodeResponse, DeviceFlowError, DeviceFlowErrorKind,
    DeviceHttpResponse, DevicePollRuntime, TokioDevicePollRuntime,
    CODEX_DEVICE_DEFAULT_POLL_INTERVAL, CODEX_DEVICE_TIMEOUT,
    CODEX_DEVICE_TOKEN_EXCHANGE_REDIRECT_URI, CODEX_DEVICE_TOKEN_URL, CODEX_DEVICE_USER_CODE_URL,
    CODEX_DEVICE_VERIFICATION_URL, CODEX_LOGIN_MODE_DEVICE, CODEX_LOGIN_MODE_METADATA_KEY,
};

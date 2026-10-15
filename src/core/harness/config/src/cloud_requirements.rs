use crate::config_requirements::ConfigRequirementsToml;
use futures::future::BoxFuture;
use futures::future::FutureExt;
use futures::future::Shared;
use std::fmt;
use std::future::Future;
use thiserror::Error;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CloudRequirementsLoadErrorCode {
    Auth,
    Timeout,
    Parse,
    RequestFailed,
    Internal,
}

#[derive(Clone, Debug, Eq, Error, PartialEq)]
#[error("{message}")]
pub struct CloudRequirementsLoadError {
    code: CloudRequirementsLoadErrorCode,
    message: String,
    status_code: Option<u16>,
}

impl CloudRequirementsLoadError {
    pub fn new(
        code: CloudRequirementsLoadErrorCode,
        status_code: Option<u16>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            code,
            message: message.into(),
            status_code,
        }
    }

    pub fn code(&self) -> CloudRequirementsLoadErrorCode {
        self.code
    }

    pub fn status_code(&self) -> Option<u16> {
        self.status_code
    }
}

#[derive(Clone)]
pub struct CloudRequirementsLoader {
    fut: Shared<
        BoxFuture<'static, Result<Option<ConfigRequirementsToml>, CloudRequirementsLoadError>>,
    >,
}

impl CloudRequirementsLoader {
    pub fn new<F>(fut: F) -> Self
    where
        F: Future<Output = Result<Option<ConfigRequirementsToml>, CloudRequirementsLoadError>>
            + Send
            + 'static,
    {
        Self {
            fut: fut.boxed().shared(),
        }
    }

    pub async fn get(&self) -> Result<Option<ConfigRequirementsToml>, CloudRequirementsLoadError> {
        self.fut.clone().await
    }
}

impl fmt::Debug for CloudRequirementsLoader {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CloudRequirementsLoader").finish()
    }
}

impl Default for CloudRequirementsLoader {
    fn default() -> Self {
        Self::new(async { Ok(None) })
    }
}

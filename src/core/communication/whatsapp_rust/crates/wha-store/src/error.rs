use thiserror::Error;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum StoreError {
    #[error("store: not found")]
    NotFound,
    #[error("store: device deleted")]
    DeviceDeleted,
    #[error("store: not implemented in this backend ({0})")]
    NotImplemented(&'static str),
    #[error("store: backend error: {0}")]
    Backend(String),
}

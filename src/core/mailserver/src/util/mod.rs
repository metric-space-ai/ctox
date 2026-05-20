// ref: stalwart/src/utils/mod.rs:1-15
pub mod errors;

pub use errors::{StalwartError, StalwartResult};

// ref: stalwart/src/utils/mod.rs:18-35
pub fn generate_unique_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

pub fn now_utc_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

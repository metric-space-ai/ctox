//! Thread-safe last-error slot. Both backends call into this to record
//! loader / graph / driver failures in a single place so the public
//! `last_error()` accessor at the crate root stays backend-agnostic.
//!
//! ref (CUDA side): `lucebox/dflash/src/errors.cpp:1-27`

use std::sync::{Mutex, OnceLock};

fn last_error_slot() -> &'static Mutex<String> {
    static SLOT: OnceLock<Mutex<String>> = OnceLock::new();
    SLOT.get_or_init(|| Mutex::new(String::new()))
}

/// Record a new last-error message.
pub fn set_last_error(msg: impl Into<String>) {
    let mut g = last_error_slot().lock().unwrap();
    *g = msg.into();
}

/// Read the most recent error. Next `set_last_error` overwrites the slot,
/// matching the reference's `const char *` semantics — callers just get
/// an owned `String`.
pub fn last_error() -> String {
    last_error_slot().lock().unwrap().clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn last_error_set_and_read() {
        set_last_error("boom");
        assert_eq!(last_error(), "boom");
        set_last_error("different");
        assert_eq!(last_error(), "different");
    }
}

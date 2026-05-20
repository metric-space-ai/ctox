//! Time utilities.
//!
//! Monotonic `now()` returning a `f64` of milliseconds with two decimals,
//! mirroring upstream behaviour of never returning the same value twice.

use std::sync::LazyLock;
use std::time::{SystemTime, UNIX_EPOCH};

use parking_lot::Mutex;

// ref: rxdb/src/plugins/utils/utils-time.ts:14
static LAST_NOW: LazyLock<Mutex<f64>> = LazyLock::new(|| Mutex::new(0.0));

// ref: rxdb/src/plugins/utils/utils-time.ts:1-36
/// Returns the current unix time in milliseconds (with two decimals!)
/// Because the accuracy of getTime() in javascript is bad,
/// and we cannot rely on performance.now() on all platforms,
/// this method implements a way to never return the same value twice.
/// This ensures that when now() is called often, we do not loose the information
/// about which call came first and which came after.
///
/// We had to move from having no decimals, to having two decimals
/// because it turned out that some storages are such fast that
/// calling this method too often would return 'the future'.
pub fn now() -> f64 {
    let mut last_now = LAST_NOW.lock();
    let mut ret = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs_f64() * 1000.0)
        .unwrap_or(0.0);
    ret += 0.01;
    if ret <= *last_now {
        ret = *last_now + 0.01;
    }
    // Strip the returned number to max two decimals.
    // In theory we would not need this but
    // in practice JavaScript has no such good number precision
    // so rounding errors could add another decimal place.
    let two_decimals = (ret * 100.0).round() / 100.0;
    *last_now = two_decimals;
    two_decimals
}

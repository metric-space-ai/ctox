//! Number utilities.

use rand::Rng;

// ref: rxdb/src/plugins/utils/utils-number.ts:7-9
/// returns a random number
/// `min` is inclusive (default 0), `max` is inclusive (default 1000).
pub fn random_number(min: Option<i64>, max: Option<i64>) -> i64 {
    let min = min.unwrap_or(0);
    let max = max.unwrap_or(1000);
    rand::thread_rng().gen_range(min..=max)
}

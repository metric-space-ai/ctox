//! Regex constants used across the codebase.

use std::sync::LazyLock;

use regex::Regex;

// ref: rxdb/src/plugins/utils/utils-regex.ts:1
pub static REGEX_ALL_DOTS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\.").unwrap());

// ref: rxdb/src/plugins/utils/utils-regex.ts:2
pub static REGEX_ALL_PIPES: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\|").unwrap());

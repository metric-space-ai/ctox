//! String utilities.

use rand::Rng;

// ref: rxdb/src/plugins/utils/utils-string.ts:1
const COUCH_NAME_CHARS: &[u8] = b"abcdefghijklmnopqrstuvwxyz";

// ref: rxdb/src/plugins/utils/utils-string.ts:10-18
/// Get a random string which can be used for many things in RxDB.
/// The returned string is guaranteed to be a valid database name or collection name
/// and also to be a valid JavaScript variable name.
pub fn random_token(length: Option<usize>) -> String {
    let length = length.unwrap_or(10);
    let mut rng = rand::thread_rng();
    let mut text = String::with_capacity(length);
    for _ in 0..length {
        let idx = rng.gen_range(0..COUCH_NAME_CHARS.len());
        text.push(COUCH_NAME_CHARS[idx] as char);
    }
    text
}

// ref: rxdb/src/plugins/utils/utils-string.ts:24
/// A random string that is never inside of any storage
pub const RANDOM_STRING: &str = "Fz7SZXPmYJujkzjY1rpXWvlWBqoGAfAX";

// ref: rxdb/src/plugins/utils/utils-string.ts:29-34
/// uppercase first char
pub fn ucfirst(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
    }
}

// ref: rxdb/src/plugins/utils/utils-string.ts:39-51
/// removes trailing and leading dots from the string
pub fn trim_dots(s: &str) -> String {
    let s = s.trim_start_matches('.');
    let s = s.trim_end_matches('.');
    s.to_string()
}

// ref: rxdb/src/plugins/utils/utils-string.ts:56-58
pub fn last_char_of_string(s: &str) -> char {
    s.chars().last().unwrap_or('\0')
}

// ref: rxdb/src/plugins/utils/utils-string.ts:63-73
/// returns true if the given name is likely a folder path
pub fn is_folder_path(name: &str) -> bool {
    name.contains('/') || name.contains('\\')
}

// ref: rxdb/src/plugins/utils/utils-string.ts:80-82
pub fn array_buffer_to_string(buf: &[u8]) -> String {
    String::from_utf8_lossy(buf).into_owned()
}

// ref: rxdb/src/plugins/utils/utils-string.ts:84-86
pub fn string_to_array_buffer(s: &str) -> Vec<u8> {
    s.as_bytes().to_vec()
}

// ref: rxdb/src/plugins/utils/utils-string.ts:89-91
pub fn normalize_string(s: &str) -> String {
    let trimmed = s.trim();
    // upstream: replace `/[\n\s]+/g` (newline OR whitespace, repeated) with empty
    let mut out = String::with_capacity(trimmed.len());
    for ch in trimmed.chars() {
        if !ch.is_whitespace() {
            out.push(ch);
        }
    }
    out
}

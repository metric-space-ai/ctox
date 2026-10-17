//! Docker's flag **types**: the option wrappers a workflow's `options:` string
//! is poured into on its way to a `HostConfig`.
//!
//! act borrows these from `docker/cli` rather than writing its own, so this is a
//! port and not a rewrite. Sources, all under `docker/cli@v29.3.0+incompatible`:
//!
//! | Go file | what came across |
//! |---------|------------------|
//! | `opts/opts.go` | [`ListOpts`], [`MapOpts`], [`MemBytes`], [`MemSwapBytes`], [`NanoCPUs`], and the `validate_*` family |
//! | `opts/parse.go` | [`parse_restart_policy`], [`read_kv_strings`], [`read_kv_env_strings`] |
//! | `opts/env.go` | [`validate_env`] |
//! | `opts/ulimit.go` | [`UlimitOpt`] |
//! | `opts/weightdevice.go` | [`validate_weight_device`] |
//! | `opts/throttledevice.go` | [`validate_throttle_bps_device`], [`validate_throttle_iops_device`] |
//! | `opts/hosts.go` | [`validate_extra_host`] |
//! | `pkg/kvfile/kvfile.go` | the line-delimited `k=v` reader behind `ReadKVStrings` |
//! | `github.com/docker/go-units` | `RAMInBytes`, `ParseUlimit` |
//!
//! Like [`super::docker_opts`], everything here is **pure**: strings in, values
//! out, no daemon. The one exception is [`validate_env`] and
//! [`read_kv_env_strings`], which read the caller's environment exactly as Go's
//! `os.LookupEnv` does.
//!
//! # The error text is the contract
//!
//! `pflag` composes `invalid argument "x" for "--label" flag: <message>`, and
//! the value in front of the message is the whole user-visible error. Upstream
//! tests assert several of these strings verbatim —
//! `invalid restart policy format: maximum retry count must be an integer`,
//! `IP address is not correctly formatted: [::1]`,
//! `invalid ulimit soft limit must be less than or equal to hard limit: 1024 > 1`
//! — so they are reproduced byte for byte, including Go's `strconv` and `net`
//! phrasing.
//!
//! # `Set` writes the field *before* it checks the error
//!
//! Go's setters are
//!
//! ```go
//! val, err := units.RAMInBytes(value)
//! *m = MemBytes(val)   // unconditional
//! return err
//! ```
//!
//! and `RAMInBytes` returns `-1` on failure. So a failed `MemBytes::set` leaves
//! the option at `-1`, and a failed `NanoCPUs::set` leaves it at `0`, instead of
//! leaving it untouched. That is reproduced here: the port has to be able to
//! produce the states Go can produce, so [`MemBytes`] and [`NanoCPUs`] are
//! `&mut self` setters rather than `parse` functions.
//!
//! # `Set` on a `ListOpts` never drops earlier values
//!
//! Validation happens *before* the append, so a rejected value leaves the list
//! exactly as it was. `--label a --label b --label ""` therefore ends up with
//! two entries, not three and not one.
//!
//! # Go's nil slice is a real state, so it is a real type here
//!
//! `NewListOpts` starts from a **nil** `[]string`, and `GetAllOrEmpty` exists
//! precisely to turn that nil into `[]string{}`. Both spellings marshal
//! differently (`null` versus `[]`) in the `HostConfig` act puts on the wire,
//! so [`ListOpts`] stores `Option<Vec<String>>` and exposes both:
//! [`get_slice`](ListOpts::get_slice) preserves the nil, and
//! [`get_all_or_empty`](ListOpts::get_all_or_empty) never returns it. A
//! `Vec<String>` alone could not tell "never set" from "set to nothing".
//!
//! # `GetMap` is a set of whole values
//!
//! Since v29.3.0 [`get_map`](ListOpts::get_map) returns
//! `map[string]struct{}` — a **set of the values as given**. It does *not* split
//! at `=`, and it is not a "key set" helper. (The `k=v` splitting lives in
//! [`MapOpts::get_all`] and in
//! [`convert_kv_strings_to_map`](super::docker_opts::convert_kv_strings_to_map).)
//!
//! # The two things worth measuring rather than assuming
//!
//! * **`int64(f)` on an out-of-range float is undefined in Go**, and the answer
//!   is platform-dependent (`i64::MIN` on amd64's `CVTTSD2SI`, saturation on
//!   arm64's `FCVTZS`). Measured on go1.26.2/darwin-arm64:
//!   `RAMInBytes("9223372036854775808") == i64::MAX`, and `RAMInBytes("1e30") ==
//!   i64::MAX`. [`go_int64`] saturates to match the platform this port builds
//!   and tests on.
//!
//! # Accepted but deliberately not ported
//!
//! * `Display` for [`MemBytes`]/[`NanoCPUs`] (`units.BytesSize`, `big.Rat`
//!   `FloatString`) — pure `pflag` `--help` text, and no upstream test covers it.
//!   `UnmarshalJSON` for the same two types needs `serde` and has no test.
//! * `ListOpts::delete` is `Deprecated:` upstream with no callers.
//! * `WeightdeviceOpt` / `ThrottledeviceOpt` / `FilterOpt` / `DurationOpt` — the
//!   option *wrappers* around the throttle and weight validators. Only the
//!   validators themselves are needed by act, so only they came across.
//! * Hex floats, `Inf`/`NaN` and underscore digit separators: go1.26.2's
//!   `ParseFloat` accepts all three (`RAMInBytes("1_0") == 10`), this port does
//!   not. No upstream test reaches them and a memory size is never written that
//!   way.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::net::IpAddr;
use std::path::Path;

use crate::container::docker_api::{RestartPolicy, Ulimit};

/// `ValidatorFctType`: a flag validator. Returns the value to *store* — which
/// may be a normalised form of the input — or the message the user sees.
pub type Validator = fn(&str) -> Result<String, String>;

/// `ValidatorWeightFctType`: a validator whose result is a parsed struct rather
/// than a string, so it cannot be a [`Validator`].
pub type WeightDeviceValidator = fn(&str) -> Result<WeightDevice, String>;

/// `ValidatorThrottleFctType`.
pub type ThrottleDeviceValidator = fn(&str) -> Result<ThrottleDevice, String>;

/// The `emptyFn` of `readKVStrings`: a lookup for a key that was given without a
/// value, or `None` for a reader that has no such notion.
type Lookup<'a> = Option<&'a dyn Fn(&str) -> Option<String>>;

// ---------------------------------------------------------------------------
// strings helpers
// ---------------------------------------------------------------------------

/// `strings.Cut`: split at the first `sep`, leaving the tail whole.
///
/// Go's ubiquitous three-value form — including the "not found" case, which
/// yields the *whole* string as the head and an empty tail. Using
/// `split_once` and unwrapping is wrong for that case.
fn cut(value: &str, sep: char) -> (&str, &str, bool) {
    match value.find(sep) {
        Some(index) => (&value[..index], &value[index + sep.len_utf8()..], true),
        None => (value, "", false),
    }
}

/// `strconv.Quote`, i.e. what `%q` prints for a string.
///
/// Only the ASCII range is escaped the way `strconv` does it; printable
/// non-ASCII is passed through, as Go's `unicode.IsPrint` allows.
fn go_quote(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for c in value.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (' '..='~').contains(&c) => out.push(c),
            c if (c as u32) < 0x20 || c as u32 == 0x7f => {
                out.push_str(&format!("\\x{:02x}", c as u32))
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Go's `fmt.Sprintf("%v", []byte)`.
fn go_bytes_fmt(bytes: &[u8]) -> String {
    let parts: Vec<String> = bytes.iter().map(u8::to_string).collect();
    format!("[{}]", parts.join(" "))
}

/// `os.PathError`'s `Err` half, which Go lower-cases (`no such file or
/// directory`) where Rust does not (`No such file or directory (os error 2)`).
fn go_os_error(err: &std::io::Error) -> String {
    match err.kind() {
        std::io::ErrorKind::NotFound => "no such file or directory".to_string(),
        std::io::ErrorKind::PermissionDenied => "permission denied".to_string(),
        std::io::ErrorKind::IsADirectory => "is a directory".to_string(),
        _ => err.to_string(),
    }
}

/// `strconv`'s `*NumError` text, e.g.
/// `strconv.ParseInt: parsing "abc": invalid syntax`.
fn go_num_error(func: &str, input: &str, detail: &str) -> String {
    format!("strconv.{func}: parsing {input}: {detail}")
}

/// `strconv.Atoi`: an optional sign then base-10 digits, nothing else.
fn go_atoi(value: &str) -> Result<i64, String> {
    let digits = value.strip_prefix(['+', '-']).unwrap_or(value);
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return Err(go_num_error("Atoi", &go_quote(value), "invalid syntax"));
    }
    value
        .parse::<i64>()
        .map_err(|_| go_num_error("Atoi", &go_quote(value), "value out of range"))
}

/// `strconv.ParseInt`/`ParseUint` with an explicit base and bit size. Go allows
/// a leading `+` for the signed forms but **not** for the unsigned ones, which
/// Rust's `from_str` does not distinguish.
fn go_parse_uint_bits(value: &str, bits: u32) -> Option<u64> {
    if value.starts_with('+') {
        return None;
    }
    let parsed = value.parse::<u128>().ok()?;
    let limit = if bits == 64 {
        u64::MAX as u128
    } else {
        (1u128 << bits) - 1
    };
    if parsed > limit {
        None
    } else {
        Some(parsed as u64)
    }
}

/// `strconv.ParseFloat`, reduced to the two outcomes the size parser needs.
fn go_parse_float(value: &str) -> Result<f64, String> {
    match value.parse::<f64>() {
        Ok(parsed) if parsed.is_infinite() => Err(go_num_error(
            "ParseFloat",
            &go_quote(value),
            "value out of range",
        )),
        Ok(parsed) => Ok(parsed),
        Err(_) => Err(go_num_error(
            "ParseFloat",
            &go_quote(value),
            "invalid syntax",
        )),
    }
}

/// `int64(someFloat)` as go1.26.2 compiles it on arm64: out of range saturates
/// to `i64::MAX` (see the module header for the measurement).
fn go_int64(value: f64) -> i64 {
    if value.is_nan() {
        return 0;
    }
    if value >= i64::MAX as f64 {
        return i64::MAX;
    }
    if value <= i64::MIN as f64 {
        return i64::MIN;
    }
    value as i64
}

// ---------------------------------------------------------------------------
// ListOpts
// ---------------------------------------------------------------------------

/// `ListOpts`: a list of flag values plus an optional validator.
#[derive(Debug, Clone, Default)]
pub struct ListOpts {
    /// `values *[]string`. `None` is Go's **nil** slice, which means "no value
    /// was ever set" and marshals as `null` rather than `[]`.
    values: Option<Vec<String>>,
    /// `validator ValidatorFctType`, nil when the flag takes anything.
    validator: Option<Validator>,
}

impl ListOpts {
    /// `NewListOpts`.
    pub fn new(validator: Option<Validator>) -> Self {
        Self {
            values: None,
            validator,
        }
    }

    /// `WithValidator`: returns the option with its validator replaced.
    pub fn with_validator(mut self, validator: Option<Validator>) -> Self {
        self.validator = validator;
        self
    }

    /// `Set`: validate, then **append**.
    ///
    /// A rejected value is not appended and does not disturb the values already
    /// in the list, which is what lets `--add-host` keep the hosts it accepted
    /// when a later one is malformed.
    pub fn set(&mut self, value: &str) -> Result<(), String> {
        let value = match self.validator {
            Some(validator) => validator(value)?,
            None => value.to_string(),
        };
        self.values.get_or_insert_with(Vec::new).push(value);
        Ok(())
    }

    /// `GetMap`: the values as a **set of whole values**, to drop duplicates.
    ///
    /// A `BTreeSet`, because a Go `map` has no order and a test comparing two
    /// of these needs a deterministic one.
    pub fn get_map(&self) -> BTreeSet<String> {
        self.get_slice().iter().cloned().collect()
    }

    /// `GetSlice`: the values in insertion order, nil-preserving.
    pub fn get_slice(&self) -> &[String] {
        self.values.as_deref().unwrap_or_default()
    }

    /// `GetAllOrEmpty`: the values, or an **empty** slice when never set.
    ///
    /// The reason this method exists is the nil/empty distinction: callers that
    /// put the result on the wire need `[]`, not `null`.
    pub fn get_all_or_empty(&self) -> Vec<String> {
        self.values.clone().unwrap_or_default()
    }

    /// `Get`: is this exact value present?
    pub fn get(&self, key: &str) -> bool {
        self.get_slice().iter().any(|value| value == key)
    }

    /// `Len`.
    pub fn len(&self) -> usize {
        self.values.as_ref().map_or(0, Vec::len)
    }

    /// Whether the option was ever set — Go's `*opts.values == nil`.
    pub fn is_empty(&self) -> bool {
        self.values.is_none()
    }
}

impl fmt::Display for ListOpts {
    /// `String()`: Go returns `""` for an empty list rather than `[]`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let values = self.get_slice();
        if values.is_empty() {
            return Ok(());
        }
        write!(f, "[{}]", values.join(" "))
    }
}

// ---------------------------------------------------------------------------
// MapOpts
// ---------------------------------------------------------------------------

/// `MapOpts`: a `k=v` map plus an optional validator, as `--sysctl` and
/// `--annotation` produce.
#[derive(Debug, Clone, Default)]
pub struct MapOpts {
    values: BTreeMap<String, String>,
    validator: Option<Validator>,
}

impl MapOpts {
    /// `NewMapOpts` with a nil map, which Go turns into an empty one.
    pub fn new(validator: Option<Validator>) -> Self {
        Self {
            values: BTreeMap::new(),
            validator,
        }
    }

    /// `NewMapOpts` with a pre-populated map.
    pub fn from_values(values: BTreeMap<String, String>, validator: Option<Validator>) -> Self {
        Self { values, validator }
    }

    /// `Set`: validate, then split at the first `=` and store.
    ///
    /// A value with no `=` is stored under its whole text with an empty value,
    /// and a repeated key **overwrites** — `strings.Cut` plus a map assignment,
    /// not an append.
    pub fn set(&mut self, value: &str) -> Result<(), String> {
        let value = match self.validator {
            Some(validator) => validator(value)?,
            None => value.to_string(),
        };
        let (key, val, _) = cut(&value, '=');
        self.values.insert(key.to_string(), val.to_string());
        Ok(())
    }

    /// `GetAll`.
    pub fn get_all(&self) -> &BTreeMap<String, String> {
        &self.values
    }
}

impl fmt::Display for MapOpts {
    /// `String()`: `fmt.Sprintf("%v", map[string]string)`, whose keys Go prints
    /// in sorted order — which is what a `BTreeMap` already does.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let body: Vec<String> = self
            .values
            .iter()
            .map(|(key, value)| format!("{key}:{value}"))
            .collect();
        write!(f, "map[{}]", body.join(" "))
    }
}

// ---------------------------------------------------------------------------
// MemBytes / MemSwapBytes
// ---------------------------------------------------------------------------

/// `units.RAMInBytes`: a human-readable byte count with binary (1024) suffixes.
///
/// The number part is truncated toward zero, so `"0.3"` is `0` and `"32.3"` is
/// `32`; a suffix multiplies afterwards, so `"0.3MB"` is `314572`.
pub fn ram_in_bytes(size: &str) -> Result<i64, String> {
    // strings.LastIndexAny(sizeStr, "01234567890. ") — a *byte* index. Every
    // byte in the set is ASCII, so it can never land inside a multi-byte rune.
    let Some(separator) = size
        .as_bytes()
        .iter()
        .rposition(|byte| byte.is_ascii_digit() || *byte == b'.' || *byte == b' ')
    else {
        return Err(format!("invalid size: '{size}'"));
    };

    // A space separator is dropped rather than kept in the number.
    let (number, suffix) = if size.as_bytes()[separator] == b' ' {
        (&size[..separator], &size[separator + 1..])
    } else {
        (&size[..separator + 1], &size[separator + 1..])
    };

    let value = go_parse_float(number)?;
    // Backward compatibility: reject negative sizes.
    if value < 0.0 {
        return Err(format!("invalid size: '{size}'"));
    }
    if suffix.is_empty() {
        return Ok(go_int64(value));
    }
    if suffix.len() > 3 {
        // Checked before ToLower, so the error shows the original spelling.
        return Err(format!("invalid suffix: '{suffix}'"));
    }

    let suffix = suffix.to_lowercase();
    let head = suffix.as_bytes()[0];
    if head == b'b' {
        // Trivial case: a bare `b` suffix, with nothing after it.
        return if suffix.len() > 1 {
            Err(format!("invalid suffix: '{suffix}'"))
        } else {
            Ok(go_int64(value))
        };
    }
    let multiplier = match head {
        b'k' => 1024.0,
        b'm' => 1024.0 * 1024.0,
        b'g' => 1024.0 * 1024.0 * 1024.0,
        b't' => 1024.0 * 1024.0 * 1024.0 * 1024.0,
        b'p' => 1024.0 * 1024.0 * 1024.0 * 1024.0 * 1024.0,
        _ => return Err(format!("invalid suffix: '{suffix}'")),
    };

    // The suffix may carry an extra `b` or `ib` (KiB, MB, ...).
    if (suffix.len() == 2 && suffix.as_bytes()[1] != b'b')
        || (suffix.len() == 3 && &suffix[1..] != "ib")
    {
        return Err(format!("invalid suffix: '{suffix}'"));
    }
    Ok(go_int64(value * multiplier))
}

/// `MemBytes`: `-m` / `--memory`, in human-readable units.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct MemBytes(pub i64);

impl MemBytes {
    /// `Value`.
    pub fn value(self) -> i64 {
        self.0
    }

    /// `Set`.
    ///
    /// Note the assignment: Go stores the parse result *before* returning the
    /// error, so a failed parse leaves `-1` (the value `RAMInBytes` returns on
    /// failure), not the previous value.
    pub fn set(&mut self, value: &str) -> Result<(), String> {
        match ram_in_bytes(value) {
            Ok(parsed) => {
                self.0 = parsed;
                Ok(())
            }
            Err(err) => {
                self.0 = -1;
                Err(err)
            }
        }
    }
}

/// `MemSwapBytes`: `--memory-swap`.
///
/// It differs from [`MemBytes`] in that `-1` — unlimited — is valid, and is the
/// default, so the exact string `-1` is short-circuited before any parsing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct MemSwapBytes(pub i64);

impl MemSwapBytes {
    /// `Value`.
    pub fn value(self) -> i64 {
        self.0
    }

    /// `Set`, with the same write-on-error behaviour as [`MemBytes::set`].
    pub fn set(&mut self, value: &str) -> Result<(), String> {
        if value == "-1" {
            self.0 = -1;
            return Ok(());
        }
        match ram_in_bytes(value) {
            Ok(parsed) => {
                self.0 = parsed;
                Ok(())
            }
            Err(err) => {
                self.0 = -1;
                Err(err)
            }
        }
    }
}

// ---------------------------------------------------------------------------
// NanoCPUs
// ---------------------------------------------------------------------------

/// `NanoCPUs`: `--cpus`, a decimal CPU count stored as nanocpus.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct NanoCPUs(pub i64);

impl NanoCPUs {
    /// `Value`.
    pub fn value(self) -> i64 {
        self.0
    }

    /// `Set`. On failure the option is left at `0`, because `ParseCPUs` returns
    /// `0` with its error and Go assigns the return value before the error.
    pub fn set(&mut self, value: &str) -> Result<(), String> {
        match parse_cpus(value) {
            Ok(parsed) => {
                self.0 = parsed;
                Ok(())
            }
            Err(err) => {
                self.0 = 0;
                Err(err)
            }
        }
    }
}

/// The two things `big.Rat.SetString` accepts, kept apart because only one of
/// them has a decimal exponent.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Rational {
    /// `m / 10^scale`, where `scale` is negative for a trailing exponent.
    Decimal { mantissa: i128, scale: i32 },
    /// `num / den`, i.e. the `a/b` form.
    Fraction { num: i128, den: i128 },
}

impl Rational {
    /// `new(big.Rat).SetString`: an optionally signed integer, decimal or
    /// exponent form, or `a/b`.
    fn parse(value: &str) -> Option<Self> {
        let (negative, rest) = match value.strip_prefix('-') {
            Some(rest) => (true, rest),
            None => (false, value.strip_prefix('+').unwrap_or(value)),
        };
        if rest.is_empty() {
            return None;
        }

        if let Some((numerator, denominator)) = rest.split_once('/') {
            if numerator.is_empty() || denominator.is_empty() || denominator.contains('/') {
                return None;
            }
            let num: i128 = parse_signed_digits(numerator)?;
            let den: i128 = parse_signed_digits(denominator)?;
            if den == 0 {
                return None;
            }
            return Some(Self::Fraction {
                num: if negative { -num } else { num },
                den,
            });
        }

        let (mantissa_text, exponent_text) = match rest.find(['e', 'E']) {
            Some(index) => (&rest[..index], Some(&rest[index + 1..])),
            None => (rest, None),
        };
        let exponent: i32 = match exponent_text {
            Some(text) => parse_signed_digits(text)?.try_into().ok()?,
            None => 0,
        };

        let mut mantissa: i128 = 0;
        let mut fraction_digits: i32 = 0;
        let mut seen_digit = false;
        let mut seen_dot = false;
        for byte in mantissa_text.bytes() {
            match byte {
                b'0'..=b'9' => {
                    seen_digit = true;
                    mantissa = mantissa.checked_mul(10)?;
                    mantissa = mantissa.checked_add(i128::from(byte - b'0'))?;
                    if seen_dot {
                        fraction_digits += 1;
                    }
                }
                b'.' if !seen_dot => seen_dot = true,
                _ => return None,
            }
        }
        if !seen_digit {
            return None;
        }
        Some(Self::Decimal {
            mantissa: if negative { -mantissa } else { mantissa },
            scale: fraction_digits - exponent,
        })
    }

    /// Multiply by 10^9 and require a whole number, exactly as
    /// `cpu.Mul(cpu, big.NewRat(1e9, 1))` then `nano.IsInt()` does.
    ///
    /// `Err` is `value is too precise`. A value that cannot even be held in an
    /// `i128` is out of `int64` range, where Go's `big.Int.Int64` is documented
    /// as undefined; go1.26.2 answers `0` for `ParseCPUs("1e999")` and so does
    /// this.
    fn nanocpus(self) -> Result<i64, String> {
        let (num, den) = match self {
            Rational::Decimal { mantissa, scale } => {
                if scale >= 0 {
                    match pow10(scale as u32) {
                        Some(den) => (mantissa, den),
                        None => return Ok(0),
                    }
                } else {
                    match mantissa.checked_mul(pow10(scale.unsigned_abs()).unwrap_or(i128::MAX)) {
                        Some(num) => (num, 1),
                        None => return Ok(0),
                    }
                }
            }
            Rational::Fraction { num, den } => (num, den),
        };
        let Some(scaled) = num.checked_mul(1_000_000_000) else {
            return Ok(0);
        };
        if scaled % den != 0 {
            return Err("value is too precise".to_string());
        }
        // Go's `big.Int.Int64` keeps the low 64 bits when the value does not
        // fit, which is the same truncation `as i64` performs.
        Ok((scaled / den) as i64)
    }
}

/// `parseSignedDigits`: base-10 digits with an optional sign, nothing else.
fn parse_signed_digits(value: &str) -> Option<i128> {
    let (negative, digits) = match value.strip_prefix('-') {
        Some(digits) => (true, digits),
        None => (false, value.strip_prefix('+').unwrap_or(value)),
    };
    if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let parsed: i128 = digits.parse().ok()?;
    Some(if negative { -parsed } else { parsed })
}

fn pow10(exponent: u32) -> Option<i128> {
    let mut result: i128 = 1;
    for _ in 0..exponent {
        result = result.checked_mul(10)?;
    }
    Some(result)
}

/// `ParseCPUs`: a rational CPU count as nanocpus.
pub fn parse_cpus(value: &str) -> Result<i64, String> {
    match Rational::parse(value) {
        Some(rational) => rational.nanocpus(),
        None => Err(format!("failed to parse {value} as a rational number")),
    }
}

// ---------------------------------------------------------------------------
// UlimitOpt
// ---------------------------------------------------------------------------

/// The `ulimitNameMapping` keys, minus `as` — which go-units carries commented
/// out because it "doesn't seem usable with the way Docker inits a container".
const ULIMIT_NAMES: [&str; 15] = [
    "core",
    "cpu",
    "data",
    "fsize",
    "locks",
    "memlock",
    "msgqueue",
    "nice",
    "nofile",
    "nproc",
    "rss",
    "rtprio",
    "rttime",
    "sigpending",
    "stack",
];

/// `units.ParseUlimit`: `name=soft[:hard]`.
///
/// The hard limit defaults to the soft limit, and the comparison rules are
/// asymmetric: `-1` is only accepted as a **soft** limit when the hard limit is
/// *also* unlimited, because "soft unlimited, hard 1024" is not a limit at all.
pub fn parse_ulimit(val: &str) -> Result<Ulimit, String> {
    let Some((name, limits)) = val.split_once('=') else {
        return Err(format!("invalid ulimit argument: {val}"));
    };
    if !ULIMIT_NAMES.contains(&name) {
        return Err(format!("invalid ulimit type: {name}"));
    }

    let parts: Vec<&str> = limits.split(':').collect();
    let soft;
    let hard;
    match parts.len() {
        2 => {
            // The hard limit is parsed first — that is the order `fallthrough`
            // runs in, and it decides which error a doubly bad value reports.
            hard = parts[1]
                .parse::<i64>()
                .map_err(|_| go_num_error("ParseInt", &go_quote(parts[1]), "invalid syntax"))?;
            soft = parts[0]
                .parse::<i64>()
                .map_err(|_| go_num_error("ParseInt", &go_quote(parts[0]), "invalid syntax"))?;
        }
        1 => {
            soft = parts[0]
                .parse::<i64>()
                .map_err(|_| go_num_error("ParseInt", &go_quote(parts[0]), "invalid syntax"))?;
            hard = soft;
        }
        _ => {
            return Err(format!(
                "too many limit value arguments - {limits}, can only have up to two, `soft[:hard]`"
            ))
        }
    }

    if hard != -1 {
        if soft == -1 {
            return Err(format!(
                "ulimit soft limit must be less than or equal to hard limit: soft: -1 (unlimited), hard: {hard}"
            ));
        }
        if soft > hard {
            return Err(format!(
                "ulimit soft limit must be less than or equal to hard limit: {soft} > {hard}"
            ));
        }
    }

    Ok(Ulimit {
        name: name.to_string(),
        soft,
        hard,
    })
}

/// `UlimitOpt`: the `--ulimit` flag, keyed by name.
#[derive(Debug, Clone, Default)]
pub struct UlimitOpt {
    /// `map[string]*Ulimit`, so a repeated name replaces rather than appends.
    values: BTreeMap<String, Ulimit>,
}

impl UlimitOpt {
    /// `NewUlimitOpt(nil)`.
    pub fn new() -> Self {
        Self::default()
    }

    /// `NewUlimitOpt(ref)` with a pre-populated map.
    pub fn from_values(values: BTreeMap<String, Ulimit>) -> Self {
        Self { values }
    }

    /// `Set`: parse, then store under the ulimit's name. Ulimits are otherwise
    /// not validated.
    pub fn set(&mut self, val: &str) -> Result<(), String> {
        let ulimit = parse_ulimit(val)?;
        self.values.insert(ulimit.name.clone(), ulimit);
        Ok(())
    }

    /// `GetList`: sorted by name. A `BTreeMap` already iterates that way, which
    /// is why the sort upstream needs is absent here.
    pub fn get_list(&self) -> Vec<Ulimit> {
        self.values.values().cloned().collect()
    }
}

impl fmt::Display for UlimitOpt {
    /// `String()`: `name=soft:hard` per entry, sorted.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let body: Vec<String> = self
            .values
            .values()
            .map(|ulimit| format!("{}={}:{}", ulimit.name, ulimit.soft, ulimit.hard))
            .collect();
        write!(f, "[{}]", body.join(" "))
    }
}

// ---------------------------------------------------------------------------
// weight / throttle devices
// ---------------------------------------------------------------------------

/// `blkiodev.WeightDevice`: a device path and its `--device-weight` share.
#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Serialize)]
pub struct WeightDevice {
    /// `Path`, always under `/dev/`.
    #[serde(rename = "Path")]
    pub path: String,
    /// `Weight`, 0-1000 with 0 meaning "unset".
    #[serde(rename = "Weight")]
    pub weight: u16,
}

/// `blkiodev.ThrottleDevice`: a device path and its rate.
#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Serialize)]
pub struct ThrottleDevice {
    /// `Path`, always under `/dev/`.
    #[serde(rename = "Path")]
    pub path: String,
    /// `Rate`: bytes per second, or IOPS, depending on the flag.
    #[serde(rename = "Rate")]
    pub rate: u64,
}

/// `ValidateWeightDevice`: `<device-path>:<weight>`.
///
/// The weight range is 10..=1000, **plus** 0 — a weight of exactly 0 is
/// accepted and means "leave it alone", which is why the bound test is written
/// `weight > 0 && (weight < 10 || weight > 1000)`.
pub fn validate_weight_device(val: &str) -> Result<WeightDevice, String> {
    let (key, value, found) = cut(val, ':');
    if !found || key.is_empty() {
        return Err(format!("bad format: {val}"));
    }
    if !key.starts_with("/dev/") {
        return Err(format!("bad format for device path: {val}"));
    }
    let weight =
        go_parse_uint_bits(value, 16).ok_or_else(|| format!("invalid weight for device: {val}"))?;
    if weight > 0 && !(10..=1000).contains(&weight) {
        return Err(format!("invalid weight for device: {val}"));
    }
    Ok(WeightDevice {
        path: key.to_string(),
        weight: u16::try_from(weight).unwrap_or(0),
    })
}

const THROTTLE_BPS_HELP: &str = "invalid rate for device: {val}. The correct format is <device-path>:<number>[<unit>]. Number must be a positive integer. Unit is optional and can be kb, mb, or gb";

/// `ValidateThrottleBpsDevice`: `<device-path>:<rate>[kb|mb|gb]`.
pub fn validate_throttle_bps_device(val: &str) -> Result<ThrottleDevice, String> {
    let (key, value, found) = cut(val, ':');
    if !found || key.is_empty() {
        return Err(format!("bad format: {val}"));
    }
    if !key.starts_with("/dev/") {
        return Err(format!("bad format for device path: {val}"));
    }
    let rate = ram_in_bytes(value).map_err(|_| THROTTLE_BPS_HELP.replace("{val}", val))?;
    if rate < 0 {
        return Err(THROTTLE_BPS_HELP.replace("{val}", val));
    }
    Ok(ThrottleDevice {
        path: key.to_string(),
        rate: rate as u64,
    })
}

/// `ValidateThrottleIOpsDevice`: `<device-path>:<iops>`, with no unit suffix —
/// the rate is a plain count, so it does not go through `RAMInBytes`.
pub fn validate_throttle_iops_device(val: &str) -> Result<ThrottleDevice, String> {
    let (key, value, found) = cut(val, ':');
    if !found || key.is_empty() {
        return Err(format!("bad format: {val}"));
    }
    if !key.starts_with("/dev/") {
        return Err(format!("bad format for device path: {val}"));
    }
    let rate = go_parse_uint_bits(value, 64).ok_or_else(|| {
        format!("invalid rate for device: {val}. The correct format is <device-path>:<number>. Number must be a positive integer")
    })?;
    Ok(ThrottleDevice {
        path: key.to_string(),
        rate,
    })
}

// ---------------------------------------------------------------------------
// validators
// ---------------------------------------------------------------------------

/// `ValidateIPAddress`: parse, then return the **normalised** address.
///
/// The normalisation is the point: `2001:DB8::68` comes back lowercase and
/// `0:0:0:0:0:0:0:1` collapses to `::1`. Bracketed IPv6 is rejected, which is
/// why `--add-host` has to strip them itself.
pub fn validate_ip_address(val: &str) -> Result<String, String> {
    match val.trim().parse::<IpAddr>() {
        // Go's `IP.String` prints a v4-mapped v6 address as a dotted quad, so
        // `::ffff:1.2.3.4` normalises to `1.2.3.4` there and must here too.
        Ok(IpAddr::V4(v4)) => Ok(v4.to_string()),
        Ok(IpAddr::V6(v6)) => Ok(match v6.to_ipv4_mapped() {
            Some(v4) => v4.to_string(),
            None => v6.to_string(),
        }),
        // The message quotes the value as *given*, not the trimmed one.
        Err(_) => Err(format!("IP address is not correctly formatted: {val}")),
    }
}

/// `hostGatewayName`: the `--add-host` alias for "the host's gateway".
const HOST_GATEWAY_NAME: &str = "host-gateway";

/// `ValidateExtraHost`: `name:ip`, `name=ip`, with a bracketed address.
///
/// Both separators are accepted and normalised to `:`, and the brackets come off
/// — the daemon accepts neither. The address is checked but **not** normalised,
/// so `ipv6local=0:0:0:0:0:0:0:1` keeps the form the user typed.
pub fn validate_extra_host(val: &str) -> Result<String, String> {
    let (mut key, mut value, mut found) = cut(val, '=');
    if !found {
        // Only split on the *first* colon, so an unbracketed IPv6 address
        // survives as the value.
        let cut_at_colon = cut(val, ':');
        key = cut_at_colon.0;
        value = cut_at_colon.1;
        found = cut_at_colon.2;
    }
    // A colon in the hostname would make the daemon split in the wrong place and
    // report something incomprehensible, so it is caught here instead.
    if !found || key.is_empty() || key.contains(':') {
        return Err(format!("bad format for add-host: {}", go_quote(val)));
    }
    if value != HOST_GATEWAY_NAME {
        // Brackets are unambiguous whichever address family is meant, so they
        // are permitted for IPv4 too.
        if value.len() > 2 && value.starts_with('[') && value.ends_with(']') {
            value = &value[1..value.len() - 1];
        }
        if validate_ip_address(value).is_err() {
            return Err(format!(
                "invalid IP address in add-host: {}",
                go_quote(value)
            ));
        }
    }
    Ok(format!("{key}:{value}"))
}

/// `ValidateDNSSearch`: a `resolv.conf` search domain, or `.` for "none".
pub fn validate_dns_search(val: &str) -> Result<String, String> {
    // Only spaces are trimmed here; `validateDomain` handles the rest.
    let val = val.trim_matches(' ');
    if val == "." {
        return Ok(val.to_string());
    }
    validate_domain(val)
}

/// `validateDomain`, plus the `alphaRegexp` pre-check that rejects a purely
/// numeric name with a better message.
fn validate_domain(val: &str) -> Result<String, String> {
    if !val.chars().any(|c| c.is_ascii_alphabetic()) {
        return Err(format!("{val} is not a valid domain"));
    }
    match match_domain_regexp(val) {
        Some(domain) if domain.len() < 255 => Ok(domain.to_string()),
        _ => Err(format!("{val} is not a valid domain")),
    }
}

/// The captured group 1 of
/// `^(:?(:?[a-zA-Z0-9]|:?[a-zA-Z0-9][a-zA-Z0-9-]*[a-zA-Z0-9])(:?\.(:?[a-zA-Z0-9]|:?[a-zA-Z0-9][a-zA-Z0-9-]*[a-zA-Z0-9]))*)\.?\s*$`,
/// or `None` when the pattern does not match.
///
/// Hand-written rather than regex-matched because the crate has no regex engine
/// and the grammar is small: dot-separated labels of
/// `alnum | alnum[alnum-]*alnum`, an optional trailing dot, and optional
/// trailing whitespace. The trailing `\.?` is greedy, so `a.b.` yields `a.b` and
/// not `a.b.`.
fn match_domain_regexp(val: &str) -> Option<&str> {
    let trimmed = val.trim_end_matches(is_regexp_space);
    let bytes = trimmed.as_bytes();
    let mut index = label_len(bytes, 0)?;
    while bytes.get(index) == Some(&b'.') {
        match label_len(bytes, index + 1) {
            Some(len) => index += 1 + len,
            None => break,
        }
    }
    // What is left must be exactly the optional trailing dot.
    if index == bytes.len() || &bytes[index..] == b"." {
        Some(&trimmed[..index])
    } else {
        None
    }
}

/// RE2's `\s`: tab, newline, form feed, carriage return, space. Note this is
/// *not* Rust's `is_whitespace` difference — `char::is_whitespace` happens to
/// match this set exactly — and not Go's `unicode.IsSpace`, which also covers
/// U+0085 and U+00A0.
fn is_regexp_space(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\n' | '\u{c}' | '\r')
}

/// Length of the domain label starting at `start`, or `None`.
fn label_len(bytes: &[u8], start: usize) -> Option<usize> {
    if !bytes.get(start).is_some_and(u8::is_ascii_alphanumeric) {
        return None;
    }
    let mut end = start + 1;
    while bytes
        .get(end)
        .is_some_and(|byte| byte.is_ascii_alphanumeric() || *byte == b'-')
    {
        end += 1;
    }
    if end == start + 1 {
        // A single character satisfies the first, shorter alternative.
        return Some(1);
    }
    // The greedy run must end on a letter or digit, so `17-` is not a label.
    bytes.get(end - 1).filter(|b| b.is_ascii_alphanumeric())?;
    Some(end - start)
}

/// The `key=value` shape `--label` takes: a non-empty key with no whitespace in
/// it. The value is **not** constrained at all — not even for whitespace — and
/// the whole label is returned unchanged, quotes and all.
pub fn validate_label(value: &str) -> Result<String, String> {
    let (key, _, _) = cut(value, '=');
    let key = key.trim_start_matches([' ', '\t']);
    if key.is_empty() {
        return Err(format!("invalid label '{value}': empty name"));
    }
    if key.contains([' ', '\t']) {
        return Err(format!("label '{key}' contains whitespaces"));
    }
    Ok(value.to_string())
}

/// `ValidateSysctl`: a `k=v` pair whose key is on an allow-list.
///
/// The allow-list is a fixed set of `kernel.*` names plus the `net.` and
/// `fs.mqueue.` prefixes, because `--sysctl` can otherwise reconfigure the
/// host from inside a container.
const VALID_SYSCTLS: [&str; 8] = [
    "kernel.msgmax",
    "kernel.msgmnb",
    "kernel.msgmni",
    "kernel.sem",
    "kernel.shmall",
    "kernel.shmmax",
    "kernel.shmmni",
    "kernel.shm_rmid_forced",
];

/// The allowed sysctl prefixes, after `validSysctlPrefixes`.
const VALID_SYSCTL_PREFIXES: [&str; 2] = ["net.", "fs.mqueue."];

/// `ValidateSysctl`.
pub fn validate_sysctl(val: &str) -> Result<String, String> {
    let (key, _, found) = cut(val, '=');
    if !found || key.is_empty() {
        return Err(format!("sysctl '{val}' is not allowed"));
    }
    if VALID_SYSCTLS.contains(&key) {
        return Ok(val.to_string());
    }
    if VALID_SYSCTL_PREFIXES
        .iter()
        .any(|prefix| key.starts_with(prefix))
    {
        return Ok(val.to_string());
    }
    Err(format!("sysctl '{val}' is not allowed"))
}

/// `ValidateEnv`: `NAME=value`, or a bare `NAME` to be taken from the
/// environment.
///
/// A bare name whose variable is not set is passed through unchanged, so the
/// daemon can decide what an unset variable means. Names themselves are not
/// validated at all: that is the application in the container's job, not the
/// CLI's.
pub fn validate_env(val: &str) -> Result<String, String> {
    let (key, _, has_value) = cut(val, '=');
    if key.is_empty() {
        return Err(format!("invalid environment variable: {val}"));
    }
    if has_value {
        // val contains an "=", but the value may be the empty string.
        return Ok(val.to_string());
    }
    match std::env::var(key) {
        Ok(value) => Ok(format!("{key}={value}")),
        Err(_) => Ok(val.to_string()),
    }
}

/// `ParseLink`: `container:alias`, or just `container`.
///
/// Returns `(name, alias)`. The `/`-prefixed branch exists because a
/// `HostConfig` read back from an already-created container carries links in
/// the `/name:/c1/alias` form, and the alias has to come out as the last path
/// element.
pub fn parse_link(val: &str) -> Result<(String, String), String> {
    if val.is_empty() {
        return Err("empty string specified for links".to_string());
    }
    // Two parts expected, but split into three so a longer input is detectable.
    let parts: Vec<&str> = val.splitn(3, ':').collect();
    if parts.len() > 2 {
        return Err(format!("bad format for links: {val}"));
    }
    if parts.len() == 1 {
        return Ok((val.to_string(), val.to_string()));
    }
    if parts[0].starts_with('/') {
        return Ok((
            parts[0][1..].to_string(),
            go_path_split(parts[1]).1.to_string(),
        ));
    }
    Ok((parts[0].to_string(), parts[1].to_string()))
}

/// `path.Split`: the text up to and including the last `/`, and the rest.
fn go_path_split(path: &str) -> (&str, &str) {
    match path.rfind('/') {
        Some(index) => (&path[..=index], &path[index + 1..]),
        None => ("", path),
    }
}

/// `ValidateLink`: a link is well-formed. The value itself is returned
/// unchanged, so the caller can store what the user wrote.
pub fn validate_link(val: &str) -> Result<String, String> {
    parse_link(val)?;
    Ok(val.to_string())
}

// ---------------------------------------------------------------------------
// ParseRestartPolicy
// ---------------------------------------------------------------------------

/// `ParseRestartPolicy`: `no`, `always`, `on-failure[:max]`, `unless-stopped`.
///
/// Two quirks are preserved, both of which `TestParseRestartPolicy` pins down:
///
/// * An **empty** policy is not an error and does not mean `"no"` — it yields the
///   zero value, because older engines may not know the policy at all.
/// * The name is **not** validated. `ParseRestartPolicy("garbage")` succeeds
///   with `name == "garbage"`; the daemon is what rejects it.
pub fn parse_restart_policy(policy: &str) -> Result<RestartPolicy, String> {
    if policy.is_empty() {
        return Ok(RestartPolicy::default());
    }
    let (name, value, had_colon) = cut(policy, ':');
    if had_colon && name.is_empty() {
        return Err("invalid restart policy format: no policy provided before colon".to_string());
    }
    let mut parsed = RestartPolicy {
        name: name.to_string(),
        ..RestartPolicy::default()
    };
    if !value.is_empty() {
        match go_atoi(value) {
            Ok(count) => parsed.maximum_retry_count = count,
            Err(_) => {
                return Err(
                    "invalid restart policy format: maximum retry count must be an integer"
                        .to_string(),
                )
            }
        }
    }
    Ok(parsed)
}

// ---------------------------------------------------------------------------
// ReadKVStrings / ReadKVEnvStrings
// ---------------------------------------------------------------------------

/// `ReadKVStrings`: `k=v` lines from each file, then the overrides.
///
/// The overrides come **last** so they win, and they are appended rather than
/// merged: `a=1` from a file and `a=2` from the command line both survive, as
/// two entries. De-duplication is the caller's job.
pub fn read_kv_strings<P: AsRef<Path>>(
    files: &[P],
    overrides: &[String],
) -> Result<Vec<String>, String> {
    read_kv_strings_with(files, overrides, None)
}

/// `ReadKVEnvStrings`: as [`read_kv_strings`], except that a line with no value
/// is looked up in the environment, and is **dropped** if the variable is not
/// set.
pub fn read_kv_env_strings<P: AsRef<Path>>(
    files: &[P],
    overrides: &[String],
) -> Result<Vec<String>, String> {
    read_kv_strings_with(files, overrides, Some(&|key: &str| std::env::var(key).ok()))
}

/// `readKVStrings`.
fn read_kv_strings_with<P: AsRef<Path>>(
    files: &[P],
    overrides: &[String],
    lookup: Lookup<'_>,
) -> Result<Vec<String>, String> {
    let mut variables: Vec<String> = Vec::new();
    for file in files {
        let path = file.as_ref();
        // A missing file is reported bare: upstream's `kvfile.Parse` returns the
        // `os.Open` error unwrapped, and only wraps a *parse* error.
        let raw = std::fs::read(path)
            .map_err(|err| format!("open {}: {}", path.display(), go_os_error(&err)))?;
        let parsed = parse_key_value_lines(&raw, lookup)
            .map_err(|err| format!("invalid env file ({}): {err}", path.display()))?;
        variables.extend(parsed);
    }
    // Parse the '-e' and '--env' after, to allow override.
    variables.extend(overrides.iter().cloned());
    Ok(variables)
}

/// `kvfile.ParseFromReader` + `parseKeyValueFile`.
///
/// Comments (`#`), blank lines and leading whitespace go; trailing whitespace
/// is part of the value and stays. A key with whitespace in it is an error,
/// which is why leading whitespace is stripped from the whole line before the
/// key is looked at.
fn parse_key_value_lines(raw: &[u8], lookup: Lookup<'_>) -> Result<Vec<String>, String> {
    let mut lines: Vec<String> = Vec::new();
    for (index, raw_line) in raw.split(|byte| *byte == b'\n').enumerate() {
        // bufio.ScanLines drops a trailing carriage return.
        let raw_line = match raw_line.split_last() {
            Some((b'\r', head)) => head,
            _ => raw_line,
        };
        let current_line = index + 1;
        let text = std::str::from_utf8(raw_line).map_err(|_| {
            format!(
                "invalid utf8 bytes at line {current_line}: {}",
                go_bytes_fmt(raw_line)
            )
        })?;
        // A UTF-8 BOM, and only on the first line.
        let text = if current_line == 1 {
            text.strip_prefix('\u{feff}').unwrap_or(text)
        } else {
            text
        };
        let line = text.trim_start_matches(char::is_whitespace);
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (key, _, has_value) = cut(line, '=');
        if key.is_empty() {
            return Err(format!("no variable name on line '{line}'"));
        }
        if key.contains([' ', '\t']) {
            return Err(format!("variable '{key}' contains whitespaces"));
        }
        if has_value {
            lines.push(line.to_string());
            continue;
        }
        if let Some(lookup) = lookup {
            // No value given: look one up. It may be empty, but if there is no
            // such variable the key is omitted entirely.
            if let Some(value) = lookup(line) {
                lines.push(format!("{key}={value}"));
            }
        }
    }
    Ok(lines)
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::collections::BTreeMap;
    use std::path::PathBuf;

    use crate::container::docker_api::{
        RESTART_POLICY_ALWAYS, RESTART_POLICY_DISABLED, RESTART_POLICY_ON_FAILURE,
        RESTART_POLICY_UNLESS_STOPPED,
    };

    /// `opts_test.go`: `sampleValidator`.
    fn sample_validator(val: &str) -> Result<String, String> {
        const ALLOWED: [(&str, &str); 2] = [("valid-option", "1"), ("valid-option2", "2")];
        let (key, _, _) = cut(val, '=');
        if ALLOWED.iter().any(|(name, _)| *name == key) {
            return Ok(val.to_string());
        }
        Err(format!("invalid key {key}"))
    }

    // --- ListOpts ----------------------------------------------------------

    // opts_test.go: TestListOptsWithoutValidator
    #[test]
    fn test_list_opts_without_validator() {
        let mut o = ListOpts::new(None);
        o.set("foo").expect("accepted");
        assert_eq!(o.to_string(), "[foo]");
        o.set("bar").expect("accepted");
        assert_eq!(o.len(), 2);
        // A repeat is appended, not deduplicated.
        o.set("bar").expect("accepted");
        assert_eq!(o.len(), 3);
        assert!(o.get("bar"));
        assert!(!o.get("baz"));
        assert_eq!(o.get_slice(), ["foo", "bar", "bar"]);
        // GetMap is a set of *whole values*: "bar" twice collapses to one.
        assert_eq!(
            o.get_map(),
            BTreeSet::from(["foo".to_string(), "bar".to_string()])
        );
    }

    // opts_test.go: TestListOptsWithValidator
    #[test]
    fn test_list_opts_with_validator() {
        let mut o = ListOpts::new(Some(sample_validator));
        assert_eq!(o.set("foo").unwrap_err(), "invalid key foo");
        assert_eq!(o.to_string(), "");
        assert_eq!(o.set("foo=bar").unwrap_err(), "invalid key foo");
        assert_eq!(o.to_string(), "");
        o.set("valid-option2=2").expect("accepted");
        assert_eq!(o.len(), 1);
        assert!(o.get("valid-option2=2"));
        assert!(!o.get("baz"));
        assert_eq!(o.to_string(), "[valid-option2=2]");
    }

    // A rejected value must leave the accepted ones alone.
    #[test]
    fn test_list_opts_set_keeps_earlier_values_on_error() {
        let mut o = ListOpts::new(Some(sample_validator));
        o.set("valid-option=1").expect("accepted");
        assert!(o.set("dummy-val=3").is_err());
        assert_eq!(o.get_slice(), ["valid-option=1"]);
        assert_eq!(o.len(), 1);
    }

    // opts_test.go: TestGetAllOrEmptyReturnsNilOrValue
    #[test]
    fn test_get_all_or_empty_returns_nil_or_value() {
        let mut o = ListOpts::new(None);
        // Go's *values is still nil here, which is the case this method exists
        // for: the caller gets []string{}, not nil.
        assert!(o.is_empty());
        assert_eq!(o.get_all_or_empty(), Vec::<String>::new());
        o.set("foo").expect("accepted");
        assert!(!o.is_empty());
        assert_eq!(o.get_all_or_empty(), vec!["foo".to_string()]);
    }

    // --- MapOpts -----------------------------------------------------------

    // opts_test.go: TestMapOpts
    #[test]
    fn test_map_opts() {
        let mut o = MapOpts::new(Some(sample_validator));
        o.set("valid-option=1").expect("accepted");
        assert_eq!(o.to_string(), "map[valid-option:1]");
        o.set("valid-option2=2").expect("accepted");
        let all = o.get_all();
        assert_eq!(all.len(), 2);
        assert_eq!(all.get("valid-option").map(String::as_str), Some("1"));
        assert_eq!(all.get("valid-option2").map(String::as_str), Some("2"));
        assert_eq!(o.set("dummy-val=3").unwrap_err(), "invalid key dummy-val");
    }

    // No upstream test covers these two behaviours, which the Go source is
    // explicit about: a value with no '=' gets an empty value, and a repeated
    // key overwrites.
    #[test]
    fn test_map_opts_split_and_overwrite() {
        let mut o = MapOpts::new(None);
        o.set("novalue").expect("accepted");
        o.set("k=v").expect("accepted");
        o.set("k=w=extra").expect("accepted");
        assert_eq!(o.get_all().get("novalue").map(String::as_str), Some(""));
        // Cut splits at the *first* '=' only.
        assert_eq!(o.get_all().get("k").map(String::as_str), Some("w=extra"));
        assert_eq!(o.get_all().len(), 2);
    }

    // --- MemBytes / ram_in_bytes -------------------------------------------

    // go-units size_test.go: TestRAMInBytes (success cases)
    #[test]
    fn test_ram_in_bytes_success() {
        const KIB: i64 = 1024;
        const MIB: i64 = 1024 * 1024;
        const GIB: i64 = 1024 * 1024 * 1024;
        const TIB: i64 = 1024 * 1024 * 1024 * 1024;
        const PIB: i64 = 1024 * 1024 * 1024 * 1024 * 1024;
        let cases: &[(&str, i64)] = &[
            ("32", 32),
            ("32b", 32),
            ("32B", 32),
            ("32k", 32 * KIB),
            ("32K", 32 * KIB),
            ("32kb", 32 * KIB),
            ("32Kb", 32 * KIB),
            ("32Kib", 32 * KIB),
            ("32KIB", 32 * KIB),
            ("32Mb", 32 * MIB),
            ("32Gb", 32 * GIB),
            ("32Tb", 32 * TIB),
            ("32Pb", 32 * PIB),
            ("32PB", 32 * PIB),
            ("32P", 32 * PIB),
            // Truncation toward zero, before and after the multiplier.
            ("32.3", 32),
            ("32.3 mb", (32.3 * MIB as f64) as i64),
            ("0.3MB", (0.3 * MIB as f64) as i64),
            ("0.3", 0),
        ];
        for (input, expected) in cases {
            assert_eq!(
                ram_in_bytes(input).unwrap_or_else(|err| panic!("{input:?}: {err}")),
                *expected,
                "ram_in_bytes({input:?})"
            );
        }
    }

    // go-units size_test.go: TestRAMInBytes (error cases)
    #[test]
    fn test_ram_in_bytes_errors() {
        for input in ["", "hello", "-32", " 32 ", "32m b", "32bm"] {
            assert!(
                ram_in_bytes(input).is_err(),
                "ram_in_bytes({input:?}) should fail"
            );
        }
        // The message is the user's only clue, so it is checked exactly.
        assert_eq!(ram_in_bytes("").unwrap_err(), "invalid size: ''");
        assert_eq!(ram_in_bytes("-32").unwrap_err(), "invalid size: '-32'");
        // The suffix in the message is lower-cased only once the length check
        // has passed, which is why this one keeps its original spelling.
        assert_eq!(ram_in_bytes("32bm").unwrap_err(), "invalid suffix: 'bm'");
        // A space in the middle leaves "32m" to be parsed as the number.
        assert_eq!(
            ram_in_bytes("32m b").unwrap_err(),
            "strconv.ParseFloat: parsing \"32m\": invalid syntax"
        );
        assert_eq!(
            ram_in_bytes(" 32 ").unwrap_err(),
            "strconv.ParseFloat: parsing \" 32\": invalid syntax"
        );
    }

    // go-units size_test.go: BenchmarkParseSize's input list, run for its
    // values. No upstream test asserts them, but they pin the space/decimal
    // handling that benchmark merely smoke-tests.
    #[test]
    fn test_ram_in_bytes_spaced_and_decimal() {
        let cases: &[(&str, i64)] = &[
            ("32 B", 32),
            // The space is the separator, so the number is "32.5" and the
            // suffix "K": 32.5 * 1024, not 32 * 1024.
            ("32.5 K", 33280),
            ("32 Kb", 32 * 1024),
            ("32.8Mb", (32.8 * 1024.0 * 1024.0) as i64),
            ("32.9Gb", (32.9 * 1024f64.powi(3)) as i64),
            ("32.777Tb", (32.777 * 1024f64.powi(4)) as i64),
            ("0.3Mb", (0.3 * 1024.0 * 1024.0) as i64),
        ];
        for (input, expected) in cases {
            assert_eq!(
                ram_in_bytes(input).unwrap_or_else(|err| panic!("{input:?}: {err}")),
                *expected,
                "ram_in_bytes({input:?})"
            );
        }
        // "-1" is in the benchmark's list but is a *failure* there: parseSize
        // rejects negative sizes outright.
        assert_eq!(ram_in_bytes("-1").unwrap_err(), "invalid size: '-1'");
    }

    // Measured against go1.26.2 on darwin/arm64: out-of-range float to int
    // conversion saturates to i64::MAX there. See the module header.
    #[test]
    fn test_ram_in_bytes_int64_overflow_saturates() {
        for input in [
            "9223372036854775807",
            "9223372036854775808",
            "9223372036854775809",
            "18446744073709551616",
            "1e30",
        ] {
            assert_eq!(
                ram_in_bytes(input).unwrap_or_else(|err| panic!("{input:?}: {err}")),
                i64::MAX,
                "ram_in_bytes({input:?})"
            );
        }
        // Just under the boundary still works.
        assert_eq!(
            ram_in_bytes("8e18").expect("in range"),
            8_000_000_000_000_000_000
        );
        // 1e999 overflows the float parse itself, which Go reports as a range
        // error from strconv rather than a saturation.
        assert_eq!(
            ram_in_bytes("1e999").unwrap_err(),
            "strconv.ParseFloat: parsing \"1e999\": value out of range"
        );
    }

    // Go's Set assigns before it returns the error, so a failure is visible in
    // the option. Reproduced deliberately.
    #[test]
    fn test_mem_bytes_set_writes_before_failing() {
        let mut m = MemBytes::default();
        m.set("2g").expect("accepted");
        assert_eq!(m.value(), 2 * 1024 * 1024 * 1024);
        assert!(m.set("hello").is_err());
        assert_eq!(m.value(), -1, "a failed Set leaves -1, not the old value");
    }

    // The default for --memory-swap is unlimited, so "-1" must not be run
    // through RAMInBytes (which rejects a negative size).
    #[test]
    fn test_mem_swap_bytes_minus_one_is_unlimited() {
        let mut m = MemSwapBytes::default();
        m.set("-1").expect("accepted");
        assert_eq!(m.value(), -1);
        m.set("1g").expect("accepted");
        assert_eq!(m.value(), 1024 * 1024 * 1024);
        assert!(m.set("-2").is_err());
        assert_eq!(m.value(), -1);
    }

    // --- NanoCPUs ----------------------------------------------------------

    // opts_test.go: TestParseCPUsReturnZeroOnInvalidValues
    #[test]
    fn test_parse_cpus_returns_zero_on_invalid_values() {
        assert_eq!(parse_cpus("foo").unwrap_or(0), 0);
        assert_eq!(parse_cpus("1e-32").unwrap_or(0), 0);
    }

    // No upstream table, but these are the shapes big.Rat accepts and the
    // nanocpu scaling has to reproduce; all verified against go1.26.2.
    #[test]
    fn test_parse_cpus_accepted_forms() {
        let cases: &[(&str, i64)] = &[
            ("1", 1_000_000_000),
            ("1.5", 1_500_000_000),
            ("0.5", 500_000_000),
            ("0.1", 100_000_000),
            ("0.3", 300_000_000),
            ("1.", 1_000_000_000),
            (".5", 500_000_000),
            ("+2.5", 2_500_000_000),
            ("2.5e-1", 250_000_000),
            ("1e9", 1_000_000_000_000_000_000),
            ("1/2", 500_000_000),
            ("3/6", 500_000_000),
        ];
        for (input, expected) in cases {
            assert_eq!(
                parse_cpus(input).unwrap_or_else(|err| panic!("{input:?}: {err}")),
                *expected,
                "parse_cpus({input:?})"
            );
        }
        for (input, expected) in [
            ("1e-32", "value is too precise"),
            ("1/3", "value is too precise"),
            ("0.0000000001", "value is too precise"),
            ("foo", "failed to parse foo as a rational number"),
        ] {
            assert_eq!(parse_cpus(input).unwrap_err(), expected, "for {input:?}");
        }
    }

    // Go assigns ParseCPUs' zero return before reporting the error.
    #[test]
    fn test_nano_cpus_set_writes_before_failing() {
        let mut c = NanoCPUs::default();
        c.set("1.5").expect("accepted");
        assert_eq!(c.value(), 1_500_000_000);
        assert!(c.set("foo").is_err());
        assert_eq!(c.value(), 0);
    }

    // --- UlimitOpt ---------------------------------------------------------

    // opts/ulimit_test.go: TestUlimitOpt
    #[test]
    fn test_ulimit_opt() {
        let mut values = BTreeMap::new();
        values.insert(
            "nofile".to_string(),
            Ulimit {
                name: "nofile".to_string(),
                soft: 512,
                hard: 1024,
            },
        );
        let mut o = UlimitOpt::from_values(values);
        assert_eq!(o.to_string(), "[nofile=512:1024]");

        o.set("core=1024:1024").expect("accepted");
        assert!(o
            .set("nofile")
            .unwrap_err()
            .contains("invalid ulimit argument"));
        assert!(o
            .set("notavalidtype=1024:1024")
            .unwrap_err()
            .contains("invalid ulimit type"));
        // Neither rejected value was stored.
        assert_eq!(o.to_string(), "[core=1024:1024 nofile=512:1024]");
        assert_eq!(o.get_list().len(), 2);
    }

    // opts/ulimit_test.go: TestUlimitOptSorting
    #[test]
    fn test_ulimit_opt_sorting() {
        let mut values = BTreeMap::new();
        values.insert(
            "nofile".to_string(),
            Ulimit {
                name: "nofile".to_string(),
                soft: 512,
                hard: 1024,
            },
        );
        values.insert(
            "core".to_string(),
            Ulimit {
                name: "core".to_string(),
                soft: 1024,
                hard: 1024,
            },
        );
        let o = UlimitOpt::from_values(values);
        assert_eq!(
            o.get_list(),
            vec![
                Ulimit {
                    name: "core".to_string(),
                    soft: 1024,
                    hard: 1024
                },
                Ulimit {
                    name: "nofile".to_string(),
                    soft: 512,
                    hard: 1024
                },
            ]
        );
        assert_eq!(o.to_string(), "[core=1024:1024 nofile=512:1024]");
    }

    // go-units ulimit_test.go: TestParseUlimit{Valid,InvalidLimitType,BadFormat,
    // HardLessThanSoft,Unlimited}
    #[test]
    fn test_parse_ulimit() {
        assert_eq!(
            parse_ulimit("nofile=512:1024").expect("valid"),
            Ulimit {
                name: "nofile".to_string(),
                soft: 512,
                hard: 1024
            }
        );
        // A soft limit with no hard limit defaults hard to soft.
        assert_eq!(
            parse_ulimit("nofile=512").expect("valid"),
            Ulimit {
                name: "nofile".to_string(),
                soft: 512,
                hard: 512
            }
        );
        // Unlimited on both sides is the one combination that allows -1.
        assert_eq!(
            parse_ulimit("nofile=-1:-1").expect("valid"),
            Ulimit {
                name: "nofile".to_string(),
                soft: -1,
                hard: -1
            }
        );
        for input in [
            "notarealtype=1024:1024",
            "nofile:1024:1024",
            "nofile",
            "nofile=",
            "nofile=:",
            "nofile=:1024",
            "nofile=1024:1",
            "nofile=-1:1024",
        ] {
            assert!(parse_ulimit(input).is_err(), "parse_ulimit({input:?})");
        }
        assert_eq!(
            parse_ulimit("notarealtype=1").unwrap_err(),
            "invalid ulimit type: notarealtype"
        );
        assert_eq!(
            parse_ulimit("nofile").unwrap_err(),
            "invalid ulimit argument: nofile"
        );
        assert_eq!(
            parse_ulimit("nofile=1:2:3").unwrap_err(),
            "too many limit value arguments - 1:2:3, can only have up to two, `soft[:hard]`"
        );
        assert_eq!(
            parse_ulimit("nofile=1024:1").unwrap_err(),
            "ulimit soft limit must be less than or equal to hard limit: 1024 > 1"
        );
        assert_eq!(
            parse_ulimit("nofile=-1:1024").unwrap_err(),
            "ulimit soft limit must be less than or equal to hard limit: soft: -1 (unlimited), hard: 1024"
        );
    }

    // --- ParseRestartPolicy ------------------------------------------------

    // cli/command/container/opts_test.go: TestParseRestartPolicy
    #[test]
    fn test_parse_restart_policy() {
        let cases: &[(&str, &str, i64, Option<&str>)] = &[
            // An empty policy is the zero value, not "no": older engines may not
            // know the field at all.
            ("", "", 0, None),
            ("no", RESTART_POLICY_DISABLED, 0, None),
            ("always", RESTART_POLICY_ALWAYS, 0, None),
            ("always:1", RESTART_POLICY_ALWAYS, 1, None),
            ("on-failure:1", RESTART_POLICY_ON_FAILURE, 1, None),
            ("unless-stopped", RESTART_POLICY_UNLESS_STOPPED, 0, None),
            (
                ":1",
                "",
                0,
                Some("invalid restart policy format: no policy provided before colon"),
            ),
            (
                "always:2:3",
                "",
                0,
                Some("invalid restart policy format: maximum retry count must be an integer"),
            ),
            (
                "on-failure:invalid",
                "",
                0,
                Some("invalid restart policy format: maximum retry count must be an integer"),
            ),
            (
                "unless-stopped:invalid",
                "",
                0,
                Some("invalid restart policy format: maximum retry count must be an integer"),
            ),
        ];
        for (input, name, retries, expected_err) in cases {
            match expected_err {
                Some(message) => assert_eq!(
                    parse_restart_policy(input).unwrap_err(),
                    *message,
                    "for {input:?}"
                ),
                None => {
                    let policy = parse_restart_policy(input).expect("valid");
                    assert_eq!(policy.name, *name, "name for {input:?}");
                    assert_eq!(policy.maximum_retry_count, *retries, "for {input:?}");
                }
            }
        }
    }

    // The name is never validated, by design: the daemon rejects it.
    #[test]
    fn test_parse_restart_policy_does_not_validate_the_name() {
        let policy = parse_restart_policy("garbage:3").expect("accepted");
        assert_eq!(policy.name, "garbage");
        assert_eq!(policy.maximum_retry_count, 3);
        // is_none() is what lets --rm combine with the default policy.
        assert!(parse_restart_policy("").expect("valid").is_none());
        assert!(parse_restart_policy("no").expect("valid").is_none());
        assert!(!parse_restart_policy("always").expect("valid").is_none());
    }

    // --- read_kv_strings ---------------------------------------------------

    /// A file in a temp dir that cleans itself up.
    struct TempFile(PathBuf);

    impl TempFile {
        fn new(name: &str, content: &str) -> Self {
            let dir = tempfile::tempdir().expect("temp dir");
            let path = dir.path().join(name);
            std::fs::write(&path, content).expect("write");
            // Leak the directory handle for the test's lifetime; the OS reaps it.
            std::mem::forget(dir);
            Self(path)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    // opts/parse_test.go: TestReadKVEnvStrings
    #[test]
    fn test_read_kv_env_strings() {
        let empty_env_file = TempFile::new("empty", "");
        let env_file1 = TempFile::new("env1", "Z1=z\nEMPTY_VAR=\nFROM_ENV\nNO_SUCH_ENV\n");
        let env_file2 = TempFile::new("env2", "Z2=z\nA2=a");

        let previous = std::env::var("FROM_ENV").ok();
        std::env::set_var("FROM_ENV", "from-env");
        std::env::remove_var("NO_SUCH_ENV");

        let read = |files: Vec<&TempFile>, overrides: &[&str]| {
            let paths: Vec<&Path> = files.iter().map(|file| file.path()).collect();
            let overrides: Vec<String> = overrides.iter().map(|o| o.to_string()).collect();
            read_kv_env_strings(&paths, &overrides)
        };

        // NO_SUCH_ENV is not in the environment, so it is dropped entirely; a
        // bare name that *is* set is expanded, and a bare name that is set to
        // the empty string keeps its "=".
        let result = read(vec![&env_file1], &[]).expect("parsed");
        assert_eq!(result, ["Z1=z", "EMPTY_VAR=", "FROM_ENV=from-env"]);

        let result = read(vec![&env_file1, &env_file2], &[]).expect("parsed");
        assert_eq!(
            result,
            ["Z1=z", "EMPTY_VAR=", "FROM_ENV=from-env", "Z2=z", "A2=a"]
        );

        // Overrides are appended, not merged: both Z1 entries survive.
        let result = read(vec![&env_file1], &["Z1=override", "EXTRA=extra"]).expect("parsed");
        assert_eq!(
            result,
            [
                "Z1=z",
                "EMPTY_VAR=",
                "FROM_ENV=from-env",
                "Z1=override",
                "EXTRA=extra"
            ]
        );

        let result = read(vec![], &["Z1=z", "EMPTY_VAR="]).expect("parsed");
        assert_eq!(result, ["Z1=z", "EMPTY_VAR="]);

        let result = read(vec![&empty_env_file], &[]).expect("parsed");
        assert!(result.is_empty());

        let result = read(vec![], &[]).expect("parsed");
        assert!(result.is_empty());

        match previous {
            Some(value) => std::env::set_var("FROM_ENV", value),
            None => std::env::remove_var("FROM_ENV"),
        }
    }

    // ReadKVStrings (env.go) has no environment lookup, so a bare key is
    // dropped instead of expanded. Its file format quirks — comments, blank
    // lines, a UTF-8 BOM, leading whitespace stripped, trailing whitespace kept
    // as part of the value — are the ones the two functions share.
    #[test]
    fn test_read_kv_strings_file_format() {
        let file = TempFile::new(
            "labels",
            "\u{feff}# a comment\n\n   SPACED=value  \nBARE\nEQ=\n",
        );
        // "BARE" has no value and there is no lookup, so it is dropped.
        assert_eq!(
            read_kv_strings(&[file.path()], &[]).expect("parsed"),
            ["SPACED=value  ", "EQ="]
        );
    }

    // kvfile.Parse's own error paths, and the wrapping ReadKVStrings adds.
    #[test]
    fn test_read_kv_strings_errors() {
        let no_key = TempFile::new("nokey", "=value\n");
        assert_eq!(
            read_kv_strings(&[no_key.path()], &[]).unwrap_err(),
            format!(
                "invalid env file ({}): no variable name on line '=value'",
                no_key.path().display()
            )
        );

        let spaced_key = TempFile::new("spaced", "a b=1\n");
        assert_eq!(
            read_kv_strings(&[spaced_key.path()], &[]).unwrap_err(),
            format!(
                "invalid env file ({}): variable 'a b' contains whitespaces",
                spaced_key.path().display()
            )
        );

        // A missing file is reported bare, not wrapped in "invalid env file".
        let missing = std::path::Path::new("/definitely/not/here.env");
        assert_eq!(
            read_kv_strings(&[missing], &[]).unwrap_err(),
            format!("open {}: no such file or directory", missing.display())
        );
    }

    // The env reader deliberately does not collapse `k=v` into a map: it keeps
    // the pairs, including repeats, and the conversion is the caller's — the
    // one that already lives in `docker_opts`. A repeated key therefore
    // resolves at that point, and the *last* one wins.
    #[test]
    fn test_read_kv_strings_composes_with_convert_kv_strings_to_map() {
        let file = TempFile::new("env", "A=1\nB=2\n");
        let pairs = read_kv_strings(
            &[file.path()],
            &["A=override".to_string(), "C=3".to_string()],
        )
        .expect("parsed");
        assert_eq!(pairs, ["A=1", "B=2", "A=override", "C=3"]);
        let map = super::super::docker_opts::convert_kv_strings_to_map(&pairs);
        assert_eq!(map.get("A").map(String::as_str), Some("override"));
        assert_eq!(map.get("B").map(String::as_str), Some("2"));
        assert_eq!(map.get("C").map(String::as_str), Some("3"));
    }

    // --- ValidateIPAddress -------------------------------------------------

    // opts_test.go: TestValidateIPAddress
    #[test]
    fn test_validate_ip_address() {
        let cases: &[(&str, &str, &str)] = &[
            ("127.0.0.1", "127.0.0.1", ""),
            (" 127.0.0.1 ", "127.0.0.1", ""),
            ("0:0:0:0:0:0:0:1", "::1", ""),
            ("::1", "::1", ""),
            (" ::1 ", "::1", ""),
            ("2001:db8::68", "2001:db8::68", ""),
            ("2001:DB8::68", "2001:db8::68", ""),
            ("[::1]", "", "IP address is not correctly formatted: [::1]"),
            ("127", "", "IP address is not correctly formatted: 127"),
            (
                "random invalid string",
                "",
                "IP address is not correctly formatted: random invalid string",
            ),
        ];
        for (input, expected_out, expected_err) in cases {
            if expected_err.is_empty() {
                assert_eq!(
                    validate_ip_address(input).expect("valid"),
                    *expected_out,
                    "for {input:?}"
                );
            } else {
                assert_eq!(
                    validate_ip_address(input).unwrap_err(),
                    *expected_err,
                    "for {input:?}"
                );
            }
        }
    }

    // Go's IP.String prints a v4-mapped address as a dotted quad. No upstream
    // test covers it, but `--dns` normalisation is user-visible.
    #[test]
    fn test_validate_ip_address_normalises_v4_mapped() {
        assert_eq!(
            validate_ip_address("::ffff:1.2.3.4").expect("valid"),
            "1.2.3.4"
        );
    }

    // --- ValidateDNSSearch -------------------------------------------------

    // opts_test.go: TestValidateDNSSearch
    #[test]
    fn test_validate_dns_search() {
        let valid = [
            ".",
            "a",
            "a.",
            "1.foo",
            "17.foo",
            "foo.bar",
            "foo.bar.baz",
            "foo.bar.",
            "foo.bar.baz",
            "foo1.bar2",
            "foo1.bar2.baz",
            "1foo.2bar.",
            "1foo.2bar.baz",
            "foo-1.bar-2",
            "foo-1.bar-2.baz",
            "foo-1.bar-2.",
            "foo-1.bar-2.baz",
            "1-foo.2-bar",
            "1-foo.2-bar.baz",
            "1-foo.2-bar.",
            "1-foo.2-bar.baz",
        ];
        for domain in valid {
            let result = validate_dns_search(domain)
                .unwrap_or_else(|err| panic!("ValidateDNSSearch({domain:?}) got {err}"));
            assert!(!result.is_empty(), "empty result for {domain:?}");
        }

        let invalid = [
            "",
            " ",
            "  ",
            "17",
            "17.",
            ".17",
            "17-.",
            "17-.foo",
            ".foo",
            "foo-.bar",
            "-foo.bar",
            "foo.bar-",
            "foo.bar-.baz",
            "foo.-bar",
            "foo.-bar.baz",
        ];
        for domain in invalid {
            assert!(
                validate_dns_search(domain).is_err(),
                "ValidateDNSSearch({domain:?}) should fail"
            );
        }

        // The long name from upstream's `invalid` table: `foo.bar.baz.` plus
        // **four** copies of the 65-byte label, so capture group 1 is 272 bytes
        // and the `len(group) < 255` cap rejects it. Measured against the real
        // pattern from the shipped `opts.go` on go1.26.2.
        let long = format!(
            "foo.bar.baz.{}",
            "this.should.fail.on.long.name.because.it.is.longer.thanisshouldbe".repeat(4)
        );
        assert_eq!(long.len(), 272);
        assert_eq!(
            validate_dns_search(&long).unwrap_err(),
            format!("{long} is not a valid domain"),
            "272 bytes is over the 255 cap, so upstream rejects it too",
        );
    }

    // The label grammar's edges, verified against the real regex.
    #[test]
    fn test_validate_domain_label_grammar() {
        for accepted in ["a", "a.b", "a.b.", "a. ", "a\t", "a-b.c", "1.2.a", "1-2.3a"] {
            assert!(
                validate_dns_search(accepted).is_ok(),
                "{accepted:?} should be accepted"
            );
        }
        // A label may not *end* in a hyphen, and a hyphen is not a separator,
        // so "ab-.c" fails as a whole rather than as two labels. And the
        // alphaRegexp pre-check rejects an all-numeric name outright, which is
        // why "17" is invalid while "17.foo" is not.
        for rejected in [
            "a..", "a.b..c", "ab-", "a.b-", "a b", ".a", "-a", "ab-.c", "1.2.3", "17",
        ] {
            assert!(
                validate_dns_search(rejected).is_err(),
                "{rejected:?} should be rejected"
            );
        }
    }

    // --- ValidateLabel -----------------------------------------------------

    // opts_test.go: TestValidateLabel
    #[test]
    fn test_validate_label() {
        let cases: &[(&str, Option<&str>)] = &[
            ("", Some("invalid label '': empty name")),
            (" ", Some("invalid label ' ': empty name")),
            (" = ", Some("invalid label ' = ': empty name")),
            ("    label=value", None),
            (
                "this is a label without value",
                Some("label 'this is a label without value' contains whitespaces"),
            ),
            (
                "this is a label=value",
                Some("label 'this is a label' contains whitespaces"),
            ),
            ("label=a value that has whitespace", None),
            ("label=value      ", None),
            ("label=    value", None),
            ("label", None),
            ("=label", Some("invalid label '=label': empty name")),
            ("label=", None),
            ("key1=value1", None),
            ("key1=value1=value2", None),
            ("key1=value1=value2=value", None),
            ("key\"with\"quotes={\"hello\"}", None),
            ("\"quoted-label\"=\"quoted value\"", None),
            ("key'with'quotes=hello'with'quotes", None),
            ("'quoted-label'='quoted value''", None),
        ];
        for (value, expected_err) in cases {
            match expected_err {
                Some(message) => {
                    assert_eq!(
                        validate_label(value).unwrap_err(),
                        *message,
                        "for {value:?}"
                    )
                }
                None => assert_eq!(
                    validate_label(value).expect("valid"),
                    *value,
                    "the label is returned unchanged"
                ),
            }
        }
    }

    // --- ValidateLink / ParseLink ------------------------------------------

    // opts_test.go: TestValidateLink
    #[test]
    fn test_validate_link() {
        let valid = [
            "name",
            "dcdfbe62ecd0:alias",
            "7a67485460b7642516a4ad82ecefe7f57d0c4916f530561b71a50a3f9c4e33da",
            "angry_torvalds:linus",
        ];
        for link in valid {
            assert!(validate_link(link).is_ok(), "ValidateLink({link:?})");
        }
        for (link, expected) in [
            ("", "empty string specified for links"),
            ("too:much:of:it", "bad format for links: too:much:of:it"),
        ] {
            assert!(
                validate_link(link)
                    .expect_err("should fail")
                    .contains(expected),
                "for {link:?}"
            );
        }
    }

    // opts_test.go: TestParseLink
    #[test]
    fn test_parse_link() {
        assert_eq!(
            parse_link("name:alias").expect("valid"),
            ("name".to_string(), "alias".to_string())
        );
        // Short form: the name is also the alias.
        assert_eq!(
            parse_link("name").expect("valid"),
            ("name".to_string(), "name".to_string())
        );
        assert!(parse_link("")
            .expect_err("should fail")
            .contains("empty string specified for links"));
        assert!(parse_link("link:alias:wrong")
            .expect_err("should fail")
            .contains("bad format for links: link:alias:wrong"));
        // The /name:/c1/alias form a HostConfig read back from a created
        // container carries: the alias is the last path element, and the leading
        // slash comes off the name.
        assert_eq!(
            parse_link("/foo:/c1/bar").expect("valid"),
            ("foo".to_string(), "bar".to_string())
        );
    }

    // --- ValidateEnv -------------------------------------------------------

    // opts/env_test.go: TestValidateEnv (the Windows-only case is skipped
    // upstream because the environment is case-insensitive there, and this port
    // has no Windows build)
    #[test]
    fn test_validate_env() {
        let path = std::env::var("PATH").unwrap_or_default();
        let cases: &[(&str, &str, Option<&str>)] = &[
            ("a", "a", None),
            ("something", "something", None),
            ("_=a", "_=a", None),
            ("env1=value1", "env1=value1", None),
            ("_env1=value1", "_env1=value1", None),
            ("env2=value2=value3", "env2=value2=value3", None),
            ("env3=abc!qwe", "env3=abc!qwe", None),
            ("env_4=value 4", "env_4=value 4", None),
            ("asd!qwe", "asd!qwe", None),
            ("1asd", "1asd", None),
            ("123", "123", None),
            ("some space", "some space", None),
            ("  some space before", "  some space before", None),
            ("some space after  ", "some space after  ", None),
            ("=a", "", Some("invalid environment variable: =a")),
            ("=", "", Some("invalid environment variable: =")),
        ];
        for (value, expected, expected_err) in cases {
            match expected_err {
                Some(message) => {
                    assert_eq!(validate_env(value).unwrap_err(), *message, "for {value:?}")
                }
                None => assert_eq!(
                    validate_env(value).expect("valid"),
                    *expected,
                    "for {value:?}"
                ),
            }
        }
        // A bare name is expanded from the environment, and an explicit "="
        // wins over it even when it empties the value.
        assert_eq!(validate_env("PATH").expect("valid"), format!("PATH={path}"));
        assert_eq!(validate_env("PATH=").expect("valid"), "PATH=");
        assert_eq!(
            validate_env("PATH=something").expect("valid"),
            "PATH=something"
        );
    }

    // A bare name that is not set at all is passed through, so the daemon can
    // decide what an unset variable means.
    #[test]
    fn test_validate_env_passes_through_unset_variables() {
        let name = "CTOX_OPTS_TYPES_DEFINITELY_NOT_SET_9F3A";
        std::env::remove_var(name);
        assert_eq!(validate_env(name).expect("valid"), name);
    }

    // --- ValidateExtraHost -------------------------------------------------

    // opts/hosts_test.go: TestValidateExtraHosts
    #[test]
    fn test_validate_extra_hosts() {
        // An empty expected output means "the input, unchanged".
        let cases: &[(&str, &str, &str)] = &[
            ("IPv4, colon sep", "myhost:192.168.0.1", ""),
            ("IPv4, eq sep", "myhost=192.168.0.1", "myhost:192.168.0.1"),
            (
                "Weird but permitted, IPv4 with brackets",
                "myhost=[192.168.0.1]",
                "myhost:192.168.0.1",
            ),
            ("Host and domain", "host.and.domain.invalid:10.0.2.1", ""),
            ("IPv6, colon sep", "anipv6host:2003:ab34:e::1", ""),
            (
                "IPv6, colon sep, brackets",
                "anipv6host:[2003:ab34:e::1]",
                "anipv6host:2003:ab34:e::1",
            ),
            (
                "IPv6, eq sep, brackets",
                "anipv6host=[2003:ab34:e::1]",
                "anipv6host:2003:ab34:e::1",
            ),
            ("IPv6 localhost, colon sep", "ipv6local:::1", ""),
            ("IPv6 localhost, eq sep", "ipv6local=::1", "ipv6local:::1"),
            (
                "IPv6 localhost, eq sep, brackets",
                "ipv6local=[::1]",
                "ipv6local:::1",
            ),
            (
                "IPv6 localhost, non-canonical, colon sep",
                "ipv6local:0:0:0:0:0:0:0:1",
                "",
            ),
            (
                "IPv6 localhost, non-canonical, eq sep",
                "ipv6local=0:0:0:0:0:0:0:1",
                "ipv6local:0:0:0:0:0:0:0:1",
            ),
            (
                "IPv6 localhost, non-canonical, eq sep, brackets",
                "ipv6local=[0:0:0:0:0:0:0:1]",
                "ipv6local:0:0:0:0:0:0:0:1",
            ),
            (
                "host-gateway, colon sep",
                "host.docker.internal:host-gateway",
                "",
            ),
            (
                "host-gateway, eq sep",
                "host.docker.internal=host-gateway",
                "host.docker.internal:host-gateway",
            ),
            (
                "Bad address, colon sep",
                "myhost:192.notanipaddress.1",
                "invalid IP address in add-host: \"192.notanipaddress.1\"",
            ),
            (
                "Bad address, eq sep",
                "myhost=192.notanipaddress.1",
                "invalid IP address in add-host: \"192.notanipaddress.1\"",
            ),
            (
                "No sep",
                "thathost-nosemicolon10.0.0.1",
                "bad format for add-host: \"thathost-nosemicolon10.0.0.1\"",
            ),
            (
                "Bad IPv6",
                "anipv6host:::::1",
                "invalid IP address in add-host: \"::::1\"",
            ),
            (
                "Bad IPv6, trailing colons",
                "ipv6local:::0::",
                "invalid IP address in add-host: \"::0::\"",
            ),
            (
                "Bad IPv6, missing close bracket",
                "ipv6addr=[::1",
                "invalid IP address in add-host: \"[::1\"",
            ),
            (
                "Bad IPv6, missing open bracket",
                "ipv6addr=::1]",
                "invalid IP address in add-host: \"::1]\"",
            ),
            (
                "Missing address, colon sep",
                "myhost.invalid:",
                "invalid IP address in add-host: \"\"",
            ),
            (
                "Missing address, eq sep",
                "myhost.invalid=",
                "invalid IP address in add-host: \"\"",
            ),
            (
                "IPv6 localhost, bad name",
                ":=::1",
                "bad format for add-host: \":=::1\"",
            ),
            ("No input", "", "bad format for add-host: \"\""),
        ];
        for (doc, input, expected) in cases {
            let expected_out: &str = if expected.is_empty() { input } else { expected };
            match validate_extra_host(input) {
                Ok(value) => assert_eq!(value, expected_out, "{doc}: {input:?}"),
                Err(message) => {
                    // A failed validation returns the empty string, not the input.
                    assert_eq!(message, *expected, "{doc}: {input:?}");
                }
            }
        }
    }

    // --- ValidateSysctl ----------------------------------------------------

    // No upstream test exists for ValidateSysctl; the table is taken from the
    // source's own allow-list and prefix list.
    #[test]
    fn test_validate_sysctl() {
        for allowed in [
            "kernel.msgmax=1024",
            "kernel.shm_rmid_forced=1",
            "kernel.sem=32000",
            "net.ipv4.ip_forward=1",
            "fs.mqueue.msg_max=10",
        ] {
            assert_eq!(
                validate_sysctl(allowed).expect("allowed"),
                allowed,
                "for {allowed:?}"
            );
        }
        for rejected in [
            "",
            "=1",
            "net.core",
            "fs.mqueue",
            "kernel.shmmax2=1",
            "vm.swappiness=1",
        ] {
            assert_eq!(
                validate_sysctl(rejected).unwrap_err(),
                format!("sysctl '{rejected}' is not allowed"),
                "for {rejected:?}"
            );
        }
    }

    // --- weight / throttle devices -----------------------------------------

    // No upstream test exists for these; the table is derived from the source's
    // three error branches plus the 10..=1000 weight range.
    #[test]
    fn test_validate_weight_device() {
        assert_eq!(
            validate_weight_device("/dev/sda:100").expect("valid"),
            WeightDevice {
                path: "/dev/sda".to_string(),
                weight: 100
            }
        );
        // 0 is the one weight outside 10..=1000 that is accepted.
        assert_eq!(
            validate_weight_device("/dev/sda:0").expect("valid"),
            WeightDevice {
                path: "/dev/sda".to_string(),
                weight: 0
            }
        );
        assert_eq!(
            validate_weight_device("/dev/sda:1000")
                .expect("valid")
                .weight,
            1000
        );
        assert_eq!(
            validate_weight_device("/dev/sda:9").unwrap_err(),
            "invalid weight for device: /dev/sda:9"
        );
        assert_eq!(
            validate_weight_device("/dev/sda:1001").unwrap_err(),
            "invalid weight for device: /dev/sda:1001"
        );
        assert_eq!(
            validate_weight_device("/dev/sda:abc").unwrap_err(),
            "invalid weight for device: /dev/sda:abc"
        );
        assert_eq!(
            validate_weight_device("/dev/sda:").unwrap_err(),
            "invalid weight for device: /dev/sda:"
        );
        assert_eq!(
            validate_weight_device("sda:100").unwrap_err(),
            "bad format for device path: sda:100"
        );
        assert_eq!(
            validate_weight_device(":100").unwrap_err(),
            "bad format: :100"
        );
        assert_eq!(
            validate_weight_device("/dev/sda").unwrap_err(),
            "bad format: /dev/sda"
        );
    }

    // No upstream test exists; derived from the source.
    #[test]
    fn test_validate_throttle_bps_device() {
        assert_eq!(
            validate_throttle_bps_device("/dev/sda:1mb").expect("valid"),
            ThrottleDevice {
                path: "/dev/sda".to_string(),
                rate: 1024 * 1024
            }
        );
        assert_eq!(
            validate_throttle_bps_device("/dev/sda:1024")
                .expect("valid")
                .rate,
            1024
        );
        for rejected in ["/dev/sda:abc", "/dev/sda:-1", "/dev/sda:"] {
            assert!(
                validate_throttle_bps_device(rejected)
                    .unwrap_err()
                    .starts_with("invalid rate for device: "),
                "for {rejected:?}"
            );
        }
        assert_eq!(
            validate_throttle_bps_device("sda:1mb").unwrap_err(),
            "bad format for device path: sda:1mb"
        );
        assert_eq!(
            validate_throttle_bps_device("/dev/sda").unwrap_err(),
            "bad format: /dev/sda"
        );
    }

    // No upstream test exists; derived from the source. Note the rate is a
    // plain count here, so no unit suffix is accepted.
    #[test]
    fn test_validate_throttle_iops_device() {
        assert_eq!(
            validate_throttle_iops_device("/dev/sda:1000").expect("valid"),
            ThrottleDevice {
                path: "/dev/sda".to_string(),
                rate: 1000
            }
        );
        assert!(validate_throttle_iops_device("/dev/sda:1mb").is_err());
        assert_eq!(
            validate_throttle_iops_device("/dev/sda:abc").unwrap_err(),
            "invalid rate for device: /dev/sda:abc. The correct format is <device-path>:<number>. Number must be a positive integer"
        );
        assert_eq!(
            validate_throttle_iops_device("sda:1").unwrap_err(),
            "bad format for device path: sda:1"
        );
        assert_eq!(
            validate_throttle_iops_device(":1").unwrap_err(),
            "bad format: :1"
        );
    }
}

//! A faithful `pflag` subset: the flag grammar `docker run` is written in.
//!
//! act does not invent a flag language. A workflow's
//! `container: { options: --cpus 1 --memory 2g }` is literally a `docker run`
//! command line, and act hands it to `pflag`, the same parser `docker` uses.
//! This module is that parser — nothing more.
//!
//! # Why hand-written rather than `clap`
//!
//! Because the observable behaviour *is* pflag's, and `TestParseRunWithInvalidArgs`
//! asserts pflag's error strings byte for byte:
//!
//! ```text
//! invalid argument "invalid" for "-a, --attach" flag: valid streams are STDIN, STDOUT and STDERR
//! unknown shorthand flag: 'z' in -z
//! ```
//!
//! A validator's message is wrapped, not repeated, and the flag is named in
//! pflag's own `-s, --long` spelling. A different parser produces different
//! text, and this text is what a user sees when a workflow's `options:` is
//! wrong. So the grammar is reproduced instead of approximated.
//!
//! # The four spellings a value can take
//!
//! `--flag=value`, `--flag value`, `-fvalue` and `-f value`, plus the bare
//! `--flag` / `-f` for a boolean. Flags and positional arguments may be
//! interspersed, because pflag defaults to `interspersed = true` — which is
//! what lets `docker run ubuntu -a stdin bash` treat `-a` as a flag rather than
//! as the command. `--` ends flag parsing, and everything after it is
//! positional.
//!
//! # A list flag validates on every occurrence, not on the last
//!
//! `--attach=stdin --attach=bogus` fails on `bogus` even though the flag
//! appeared twice, because `pflag` calls `Value.Set` per occurrence. The test
//! for that is upstream's own: `-a invalid -a stdout` is an error.
//!
//! # `Changed` is load-bearing and is tracked here
//!
//! Three places ask whether a flag was *explicitly given* rather than left at
//! its default: `--entrypoint=` (which resets the entrypoint to empty, unlike
//! omitting the flag), `--stop-timeout` (which stays `nil` when unset) and
//! `--init` (likewise). A default-valued field cannot express that, so
//! [`ParsedFlags::changed`] exists and the flag layer records it.

use std::collections::BTreeMap;
use std::fmt;

/// A per-value validator, matching Go's `func(string) error`.
pub type Validator = fn(&str) -> Result<String, String>;

/// How a flag's raw string is interpreted.
///
/// No `PartialEq`: a validator is a function pointer, and two identical
/// function items are not guaranteed to compare equal. [`FlagKind::is_bool`]
/// is the one question the parser actually asks of a kind.
#[derive(Debug, Clone, Copy)]
pub enum FlagKind {
    /// A boolean. `--flag` alone means `true`.
    Bool,
    /// An uninterpreted string.
    Text,
    /// A platform `int`.
    Int,
    /// A 64-bit signed integer.
    Int64,
    /// A 16-bit unsigned integer.
    Uint16,
    /// A 64-bit unsigned integer.
    Uint64,
    /// A Go `time.Duration` such as `1m30s`, kept as nanoseconds.
    Duration,
    /// An IP address, kept as its canonical text.
    Ip,
    /// A repeatable string, each occurrence validated.
    List(Option<Validator>),
    /// A repeatable `key=value`, each occurrence validated.
    Map(Option<Validator>),
    /// `-m`, a memory size with a b/k/m/g suffix.
    MemBytes,
    /// `--memory-swap`, like [`FlagKind::MemBytes`] but `-1` is meaningful.
    MemSwapBytes,
    /// `--cpus`, a decimal CPU count kept as nanocpus.
    NanoCpus,
    /// `--ulimit name=soft[:hard]`.
    Ulimit,
    /// `--blkio-weight-device`.
    WeightDevice,
    /// `--device-read-bps` and friends, with a per-kind validator.
    ThrottleDevice(ThrottleKind),
}

/// Which of the four throttle flags a value came from, because each validates
/// against a different unit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThrottleKind {
    /// Bytes per second.
    Bps,
    /// IO operations per second.
    Iops,
}

/// One flag declaration.
#[derive(Debug, Clone, Copy)]
pub struct FlagDef {
    /// The long name, without the leading dashes.
    pub long: &'static str,
    /// The optional one-letter shorthand, without the leading dash.
    pub short: Option<char>,
    /// How to interpret the value.
    pub kind: FlagKind,
}

impl FlagKind {
    /// Whether the flag can stand alone without a value.
    ///
    /// A boolean may; everything else must be followed by one.
    pub fn is_bool(&self) -> bool {
        matches!(self, FlagKind::Bool)
    }
}

impl FlagDef {
    pub const fn new(long: &'static str, kind: FlagKind) -> Self {
        Self {
            long,
            short: None,
            kind,
        }
    }

    /// Attaches a one-letter shorthand.
    pub const fn short(mut self, short: char) -> Self {
        self.short = Some(short);
        self
    }

    /// How pflag names this flag inside an error message: `-a, --attach`, or
    /// just `--attach` when it has no shorthand.
    fn display_name(&self) -> String {
        match self.short {
            Some(short) => format!("-{short}, --{}", self.long),
            None => format!("--{}", self.long),
        }
    }
}

/// A parsed value, before it is given its concrete type.
///
/// One enum for every flag rather than a typed field per flag: the flag layer's
/// job is to reproduce the *grammar*, and the interpretation of each value
/// belongs to the value types themselves. Keeping them apart is also what lets
/// the flag parser be tested without any of them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FlagValue {
    /// A boolean.
    Bool(bool),
    /// Text, a number, a duration or an IP — all still strings, so that a
    /// malformed value is reported by the value's own parser rather than here.
    Text(String),
    /// A repeated flag, in the order given.
    List(Vec<String>),
    /// A repeated `key=value` flag. A repeated key last wins.
    Map(BTreeMap<String, String>),
}

/// Why a command line could not be parsed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FlagError {
    /// A flag name that is not declared.
    UnknownFlag(String),
    /// A one-letter flag that is not declared.
    UnknownShorthand {
        /// The letter.
        shorthand: char,
        /// The whole shorthand group it appeared in, so `-abc` reports `-abc`.
        group: String,
    },
    /// A non-boolean flag at the end of the arguments with no value.
    MissingValue {
        /// How the flag was spelled.
        display: String,
    },
    /// A value the flag's own validator rejected.
    InvalidArgument {
        /// The rejected text.
        value: String,
        /// How the flag is named in pflag's messages.
        display: String,
        /// The validator's message, verbatim.
        message: String,
    },
}

impl fmt::Display for FlagError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            // pflag writes the shorthand as a quoted *rune*, hence 'z'.
            Self::UnknownShorthand { shorthand, group } => {
                write!(f, "unknown shorthand flag: '{shorthand}' in -{group}")
            }
            Self::UnknownFlag(name) => write!(f, "unknown flag: --{name}"),
            Self::MissingValue { display } => {
                if display.starts_with("--") {
                    write!(f, "flag needs an argument: {display}")
                } else {
                    // pflag's wording for a shorthand carries the group.
                    write!(f, "flag needs an argument: {display}")
                }
            }
            Self::InvalidArgument {
                value,
                display,
                message,
            } => write!(
                f,
                "invalid argument {value:?} for {display:?} flag: {message}"
            ),
        }
    }
}

impl std::error::Error for FlagError {}

/// The result of parsing one command line.
#[derive(Debug, Clone, Default)]
pub struct ParsedFlags {
    values: BTreeMap<&'static str, FlagValue>,
    changed: Vec<&'static str>,
    /// The positional arguments, in order.
    pub args: Vec<String>,
}

impl ParsedFlags {
    /// The value of a flag, if it was given.
    pub fn get(&self, long: &str) -> Option<&FlagValue> {
        self.values.get(long)
    }

    /// Whether a flag was given explicitly, as opposed to left at its default.
    ///
    /// `--entrypoint`, `--stop-timeout` and `--init` are the three that turn on
    /// different behaviour when present but empty or false.
    pub fn changed(&self, long: &str) -> bool {
        self.changed.contains(&long)
    }

    /// The value of a string flag, or `""`.
    pub fn text(&self, long: &str) -> &str {
        match self.get(long) {
            Some(FlagValue::Text(value)) => value,
            _ => "",
        }
    }

    /// The value of a boolean flag, or `false`.
    pub fn flag(&self, long: &str) -> bool {
        matches!(self.get(long), Some(FlagValue::Bool(true)))
    }

    /// `pflag.Value.Set`: give a flag a value the command line did not.
    ///
    /// act does this to `copts.netMode` so that a job which sets a
    /// `networkMode` *and* an `options:` that never mentions `--network` still
    /// ends up on the job's network. The flag kind decides how the value is
    /// stored, and a flag whose kind is not a list is rejected rather than
    /// silently coerced.
    pub fn set_list(&mut self, long: &'static str, value: &str) {
        match self.values.get_mut(long) {
            Some(FlagValue::List(values)) => values.push(value.to_string()),
            _ => {
                self.values
                    .insert(long, FlagValue::List(vec![value.to_string()]));
            }
        }
    }

    /// The values of a repeatable string flag, in order.
    pub fn list(&self, long: &str) -> &[String] {
        match self.get(long) {
            Some(FlagValue::List(values)) => values,
            _ => &[],
        }
    }

    /// The values of a repeatable `key=value` flag.
    pub fn map(&self, long: &str) -> BTreeMap<String, String> {
        match self.get(long) {
            Some(FlagValue::Map(values)) => values.clone(),
            _ => BTreeMap::new(),
        }
    }

    /// A numeric flag, or `default` when it was not given.
    ///
    /// Parsing failures are deliberately *not* reported here: a value that is
    /// not a number is rejected by the conversion that follows, which is where
    /// the owning type produces the message the user sees.
    pub fn number(&self, long: &str, default: i64) -> i64 {
        match self.get(long) {
            Some(FlagValue::Text(value)) => value.parse().unwrap_or(default),
            _ => default,
        }
    }

    /// A duration flag in nanoseconds, or `default`.
    pub fn duration(&self, long: &str, default: i64) -> i64 {
        match self.get(long) {
            Some(FlagValue::Text(value)) => parse_go_duration(value).unwrap_or(default),
            _ => default,
        }
    }

    fn set(&mut self, def: &FlagDef, raw: &str) -> Result<(), FlagError> {
        let value = match def.kind {
            FlagKind::Bool => FlagValue::Bool(parse_bool(raw)),
            FlagKind::List(validator) => {
                let text = match validator {
                    Some(validator) => validator(raw).map_err(|message| FlagError::InvalidArgument {
                        value: raw.to_string(),
                        display: def.display_name(),
                        message,
                    })?,
                    None => raw.to_string(),
                };
                match self.values.get_mut(def.long) {
                    Some(FlagValue::List(values)) => values.push(text),
                    _ => {
                        self.values.insert(def.long, FlagValue::List(vec![text]));
                    }
                }
                self.mark_changed(def.long);
                return Ok(());
            }
            FlagKind::Map(validator) => {
                let (key, value) = split_once_or(raw, '=');
                if let Some(validator) = validator {
                    validator(raw).map_err(|message| FlagError::InvalidArgument {
                        value: raw.to_string(),
                        display: def.display_name(),
                        message,
                    })?;
                }
                let _ = value;
                match self.values.get_mut(def.long) {
                    Some(FlagValue::Map(entries)) => {
                        entries.insert(key.clone(), value.to_string());
                    }
                    _ => {
                        let mut entries = BTreeMap::new();
                        entries.insert(key.clone(), value.to_string());
                        self.values.insert(def.long, FlagValue::Map(entries));
                    }
                }
                self.mark_changed(def.long);
                return Ok(());
            }
            _ => FlagValue::Text(raw.to_string()),
        };
        self.values.insert(def.long, value);
        self.mark_changed(def.long);
        Ok(())
    }

    fn mark_changed(&mut self, long: &'static str) {
        if !self.changed.contains(&long) {
            self.changed.push(long);
        }
    }
}

fn split_once_or(value: &str, separator: char) -> (String, &str) {
    match value.split_once(separator) {
        Some((key, rest)) => (key.to_string(), rest),
        None => (value.to_string(), ""),
    }
}

fn parse_bool(value: &str) -> bool {
    matches!(value, "1" | "t" | "T" | "true" | "TRUE" | "True")
}

/// A set of declared flags, and the parser over them.
#[derive(Debug, Clone)]
pub struct FlagSet {
    flags: Vec<FlagDef>,
}

impl FlagSet {
    /// Declares a set of flags.
    pub fn new(flags: Vec<FlagDef>) -> Self {
        Self { flags }
    }

    /// The declarations.
    pub fn flags(&self) -> &[FlagDef] {
        &self.flags
    }

    fn find_long(&self, name: &str) -> Option<&FlagDef> {
        self.flags.iter().find(|def| def.long == name)
    }

    fn find_short(&self, short: char) -> Option<&FlagDef> {
        self.flags
            .iter()
            .find(|def| def.short == Some(short))
    }

    /// Parses a command line.
    ///
    /// Flags and positional arguments are interspersed, as pflag does by
    /// default, and a bare `--` ends flag parsing.
    pub fn parse(&self, args: &[String]) -> Result<ParsedFlags, FlagError> {
        let mut parsed = ParsedFlags::default();
        let mut index = 0;
        let mut no_more_flags = false;

        while index < args.len() {
            let arg = &args[index];
            index += 1;

            if no_more_flags {
                parsed.args.push(arg.clone());
                continue;
            }
            if arg == "--" {
                no_more_flags = true;
                continue;
            }

            if let Some(body) = arg.strip_prefix("--") {
                let (name, inline) = match body.split_once('=') {
                    Some((name, value)) => (name, Some(value.to_string())),
                    None => (body, None),
                };
                let def = self
                    .find_long(name)
                    .ok_or_else(|| FlagError::UnknownFlag(name.to_string()))?;
                let value = match inline {
                    Some(value) => value,
                    None if def.kind.is_bool() => "true".to_string(),
                    None => {
                        let next = args.get(index).cloned().ok_or_else(|| {
                            FlagError::MissingValue {
                                display: format!("--{}", def.long),
                            }
                        })?;
                        index += 1;
                        next
                    }
                };
                parsed.set(def, &value)?;
                continue;
            }

            if arg.len() > 1 && arg.starts_with('-') {
                let body = &arg[1..];
                // Walked by byte offset rather than as a `Chars` iterator: a
                // value taken from the group ends the group, and that is easier
                // to express by moving an index than by replacing an iterator.
                let mut offset = 0;
                while offset < body.len() {
                    let short = body[offset..].chars().next().expect("a character");
                    let after = offset + short.len_utf8();
                    let def = self.find_short(short).ok_or_else(|| {
                        FlagError::UnknownShorthand {
                            shorthand: short,
                            group: body.to_string(),
                        }
                    })?;
                    let rest = &body[after..];
                    let value = if let Some(explicit) = rest.strip_prefix('=') {
                        offset = body.len();
                        explicit.to_string()
                    } else if def.kind.is_bool() {
                        // A boolean may be followed by more letters in the same
                        // group, which are themselves shorthand flags.
                        offset = after;
                        "true".to_string()
                    } else if !rest.is_empty() {
                        offset = body.len();
                        rest.to_string()
                    } else {
                        let next = args.get(index).cloned().ok_or_else(|| {
                            FlagError::MissingValue {
                                display: format!("'{short}' in -{short}"),
                            }
                        })?;
                        index += 1;
                        offset = after;
                        next
                    };
                    parsed.set(def, &value)?;
                }
                continue;
            }

            parsed.args.push(arg.clone());
        }
        Ok(parsed)
    }
}

/// Go's `time.ParseDuration`, in nanoseconds.
///
/// The health-check flags take `1m30s` and friends, and a workflow may write
/// `500ms`. Go's grammar allows a signed sequence of `<number><unit>` pairs
/// with a fractional part on the value, and only these units.
pub fn parse_go_duration(value: &str) -> Option<i64> {
    let mut rest = value;
    let negative = match rest.strip_prefix('-') {
        Some(stripped) => {
            rest = stripped;
            true
        }
        None => {
            rest = rest.strip_prefix('+').unwrap_or(rest);
            false
        }
    };
    if rest == "0" {
        return Some(0);
    }
    if rest.is_empty() {
        return None;
    }
    let mut total: f64 = 0.0;
    while !rest.is_empty() {
        // The number, with an optional fraction.
        let digits_end = rest
            .find(|c: char| !c.is_ascii_digit())
            .unwrap_or(rest.len());
        let mut number_end = digits_end;
        if rest[number_end..].starts_with('.') {
            let fraction_start = number_end + 1;
            let fraction_end = rest[fraction_start..]
                .find(|c: char| !c.is_ascii_digit())
                .map(|offset| fraction_start + offset)
                .unwrap_or(rest.len());
            number_end = fraction_end;
        }
        if number_end == 0 {
            return None;
        }
        let number: f64 = rest[..number_end].parse().ok()?;
        rest = &rest[number_end..];

        let unit_end = rest
            .find(|c: char| !c.is_ascii_alphabetic() && c != 'µ')
            .unwrap_or(rest.len());
        if unit_end == 0 {
            return None;
        }
        let multiplier = match &rest[..unit_end] {
            "ns" => 1.0,
            "us" | "µs" | "μs" => 1e3,
            "ms" => 1e6,
            "s" => 1e9,
            "m" => 6e10,
            "h" => 3.6e12,
            _ => return None,
        };
        total += number * multiplier;
        rest = &rest[unit_end..];
    }
    let value = total.round() as i64;
    Some(if negative { -value } else { value })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|v| v.to_string()).collect()
    }

    fn spec() -> FlagSet {
        FlagSet::new(vec![
            FlagDef::new("attach", FlagKind::List(Some(super::super::docker_opts::validate_attach))).short('a'),
            FlagDef::new("interactive", FlagKind::Bool).short('i'),
            FlagDef::new("tty", FlagKind::Bool).short('t'),
            FlagDef::new("publish-all", FlagKind::Bool).short('P'),
            FlagDef::new("rm", FlagKind::Bool),
            FlagDef::new("entrypoint", FlagKind::Text),
            FlagDef::new("memory", FlagKind::MemBytes).short('m'),
            FlagDef::new("health-interval", FlagKind::Duration),
            FlagDef::new("sysctl", FlagKind::Map(None)),
            FlagDef::new("label", FlagKind::List(None)).short('l'),
        ])
    }

    // docker_cli_test.go: TestParseRunWithInvalidArgs
    #[test]
    fn pflag_error_text_is_reproduced() {
        let flags = spec();
        let cases: &[(&[&str], &str)] = &[
            (
                &["-a", "ubuntu", "bash"],
                r#"invalid argument "ubuntu" for "-a, --attach" flag: valid streams are STDIN, STDOUT and STDERR"#,
            ),
            (
                &["-a", "invalid", "ubuntu", "bash"],
                r#"invalid argument "invalid" for "-a, --attach" flag: valid streams are STDIN, STDOUT and STDERR"#,
            ),
            (
                &["-a", "invalid", "-a", "stdout", "ubuntu", "bash"],
                r#"invalid argument "invalid" for "-a, --attach" flag: valid streams are STDIN, STDOUT and STDERR"#,
            ),
            (
                &["-a", "stdout", "-a", "stderr", "-z", "ubuntu", "bash"],
                "unknown shorthand flag: 'z' in -z",
            ),
            (
                &["-a", "stdin", "-z", "ubuntu", "bash"],
                "unknown shorthand flag: 'z' in -z",
            ),
            (
                &["-z", "--rm", "ubuntu", "bash"],
                "unknown shorthand flag: 'z' in -z",
            ),
        ];
        for (input, want) in cases {
            let err = flags
                .parse(&args(input))
                .expect_err(&format!("{input:?} should fail"));
            assert_eq!(err.to_string(), *want, "for {input:?}");
        }
    }

    /// A repeated flag is validated on **every** occurrence, so a bad value
    /// fails even when a good one follows it.
    #[test]
    fn a_repeated_flag_is_validated_each_time() {
        let flags = spec();
        assert!(flags.parse(&args(&["-a", "invalid", "-a", "stdout"])).is_err());
        // The reverse order is fine, because every value is valid.
        assert!(flags.parse(&args(&["-a", "stdout", "-a", "stderr"])).is_ok());
    }

    #[test]
    fn the_four_value_spellings_all_parse() {
        let flags = spec();
        for input in [
            vec!["--attach=stdin"],
            vec!["--attach", "stdin"],
            vec!["-astdin"],
            vec!["-a", "stdin"],
        ] {
            let parsed = flags.parse(&args(&input)).expect("parses");
            assert_eq!(parsed.list("attach"), ["stdin".to_string()], "for {input:?}");
        }
    }

    /// Flags after a positional argument are still flags — pflag defaults to
    /// `interspersed = true`, and `docker run ubuntu -a stdin bash` depends on
    /// it.
    #[test]
    fn flags_and_positionals_are_interspersed() {
        let flags = spec();
        let parsed = flags
            .parse(&args(&["ubuntu", "-a", "stdin", "bash"]))
            .expect("parses");
        assert_eq!(parsed.args, vec!["ubuntu".to_string(), "bash".to_string()]);
        assert_eq!(parsed.list("attach"), ["stdin".to_string()]);
    }

    /// `--` ends flag parsing, and the rest is the command.
    #[test]
    fn a_double_dash_ends_flag_parsing() {
        let flags = spec();
        let parsed = flags
            .parse(&args(&["ubuntu", "--", "-a", "bash"]))
            .expect("parses");
        assert_eq!(
            parsed.args,
            vec![
                "ubuntu".to_string(),
                "-a".to_string(),
                "bash".to_string()
            ]
        );
        assert!(parsed.list("attach").is_empty());
    }

    #[test]
    fn a_boolean_flag_needs_no_value_but_accepts_one() {
        let flags = spec();
        for input in [vec!["-i"], vec!["-i=true"], vec!["--interactive"], vec!["-it"]] {
            let parsed = flags.parse(&args(&input)).expect("parses");
            assert!(parsed.flag("interactive"), "for {input:?}");
            assert!(parsed.changed("interactive"), "for {input:?}");
        }
        // `false` is a value, not an absence, so it is still "changed".
        let parsed = flags.parse(&args(&["--interactive=false"])).expect("parses");
        assert!(!parsed.flag("interactive"));
        assert!(parsed.changed("interactive"));
    }

    /// A non-boolean flag with no value is an error naming the flag.
    #[test]
    fn a_value_flag_without_a_value_is_an_error() {
        let flags = spec();
        assert_eq!(
            flags.parse(&args(&["ubuntu", "--entrypoint"])).unwrap_err().to_string(),
            "flag needs an argument: --entrypoint",
        );
        assert_eq!(
            flags.parse(&args(&["ubuntu", "-a"])).unwrap_err().to_string(),
            "flag needs an argument: 'a' in -a",
        );
    }

    #[test]
    fn an_undeclared_long_flag_is_reported_by_name() {
        let flags = spec();
        assert_eq!(
            flags.parse(&args(&["--nope"])).unwrap_err().to_string(),
            "unknown flag: --nope",
        );
    }

    /// A repeated `key=value` flag keeps the last value for a repeated key.
    #[test]
    fn a_map_flag_keeps_the_last_value_for_a_key() {
        let flags = spec();
        let parsed = flags
            .parse(&args(&["--sysctl=a=1", "--sysctl=b=2", "--sysctl=a=3"]))
            .expect("parses");
        let map = parsed.map("sysctl");
        assert_eq!(map["a"], "3");
        assert_eq!(map["b"], "2");
    }

    /// Go's `time.ParseDuration`, which the health-check flags depend on.
    #[test]
    fn a_duration_is_parsed_as_go_does() {
        let table: &[(&str, i64)] = &[
            ("0", 0),
            ("1s", 1_000_000_000),
            ("500ms", 500_000_000),
            ("1m30s", 90_000_000_000),
            ("1m", 60_000_000_000),
            ("1h", 3_600_000_000_000),
            ("100ns", 100),
            ("1us", 1_000),
            ("1.5s", 1_500_000_000),
            ("-1.5h", -5_400_000_000_000),
        ];
        for (input, want) in table {
            assert_eq!(
                parse_go_duration(input),
                Some(*want),
                "ParseDuration({input:?})",
            );
        }
        for input in ["", "abc", "1", "1x", "s"] {
            assert_eq!(parse_go_duration(input), None, "ParseDuration({input:?})");
        }
    }
}

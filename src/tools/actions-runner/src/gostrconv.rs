//! Port of the two `strconv` integer parsers act calls whose **return value**
//! is load-bearing, rather than their error.
//!
//! Both `Job.GetMaxParallel`/`GetFailFast` and `setActionRuntimeVars` are
//! written in the same Go shape, which reads like "keep the default unless the
//! option is unreadable":
//!
//! ```go
//! maxParallel := 4
//! if s.MaxParallelString != "" {
//!     var err error
//!     if maxParallel, err = strconv.Atoi(s.MaxParallelString); err != nil {
//!         log.Errorf("Failed to parse 'max-parallel' option: %v", err)
//!     }
//! }
//! ```
//!
//! The `=` on `maxParallel, err = …` is the whole story: it **overwrites** the
//! default with whatever `Atoi` returned, and `Atoi` returns **0** on a syntax
//! error. So `max-parallel: lots` yields `0`, not `4`. `log.Errorf` records the
//! problem; it does not restore the default. The same block appears in
//! `setActionRuntimeVars` for `GITHUB_RUN_ID`, and the same mistake in the port
//! — `unwrap_or(1)`, `unwrap_or(4)` — is invisible in the happy path, because
//! the default only becomes observable in the error path.
//!
//! So the default lives in the *caller* (`if the string is absent or empty, use
//! the default`) and the parse lives here, where it can only return what Go
//! returns. That ordering is the port; getting it backwards is how a Rust
//! `unwrap_or(default)` looks reasonable and behaves differently.
//!
//! # The three return values, and which is which
//!
//! `strconv.ParseInt` never returns an error *and* leaves the destination
//! alone. It returns a value, and on a range error that value is **clamped**,
//! not zeroed. Measured against go1.26.2 (see [`tests`]):
//!
//! | input | Go | why |
//! |---|---|---|
//! | `"45"` | `45` | valid |
//! | `"+45"`, `"007"` | `45`, `7` | a sign and leading zeros are accepted at base 10 |
//! | `"-1"` | `-1` | valid |
//! | `""`, `" 45"`, `"45 "`, `"45\n"` | `0` | syntax error → `ErrSyntax`, value `0` |
//! | `"0x2d"`, `"lots"`, `"4.5"` | `0` | base 10 is explicit: no prefix, no float |
//! | `"9223372036854775807"` | `i64::MAX` | the boundary is in range |
//! | `"9223372036854775808"` | `i64::MAX` | **`ErrRange` clamps**, it does not zero |
//! | `"-9223372036854775809"` | `i64::MIN` | same, negative side |
//!
//! The last two rows are the reason [`parse_int`] exists instead of
//! `str::parse`. Rust's `i64::from_str` returns `Err` on overflow, so
//! `s.parse().unwrap_or(0)` — the obvious one-liner — is right on all seven
//! syntax rows and wrong on both range rows. It cannot be fixed by changing
//! the fallback, because the two failures need *different* answers.
//!
//! Go's `Atoi` is `ParseInt(s, 10, 0)`, and on a 64-bit build `0` bits means
//! `int`, which is 64 bits — the same two rows act sees, measured on both call
//! sites against the real `pkg/model`.
//!
//! # What is deliberately not here
//!
//! * `ParseBool`. Two different `strconv` booleans are in play in act and they
//!   are *not* interchangeable: `strconv.ParseBool` accepts `t`, `T`, `TRUE`,
//!   `True`, `1`; the flag parser's own accepts a different set. Both are ported
//!   at their own call sites, in [`crate::model`] and
//!   [`crate::container::docker_opts_mounts`], where the accepted set is
//!   asserted verbatim by an upstream test.
//! * `ParseUint`. Only reachable through go-connections' `nat` port grammar,
//!   which is ported as [`crate::container::docker_specs::nat`], because there
//!   the error *text* is part of the contract and the value is not.
//! * `ParseFloat` and `FormatInt`. act never parses a float from a workflow.
//!
//! Upstream is a fork of the Go standard library (BSD-style licence, see the
//! `LICENSE` file in the act tree).

/// Go's `strconv.ParseInt(s, 10, 64)` — base 10, 64-bit — **as act uses it**:
/// the returned value, with the error folded in.
///
/// Returns `0` for a syntax error and the clamped bound for a range error, so
/// that `runID, _ = strconv.ParseInt(rid, 10, 64)` and
/// `maxParallel, err = strconv.Atoi(s)` both land on the value their Go
/// counterpart would. See the module docs for the measured table.
pub fn parse_int(s: &str) -> i64 {
    if let Ok(value) = s.parse::<i64>() {
        return value;
    }
    // `s.parse` failed, and Go's `ParseInt` fails for two different reasons
    // that need two different answers, so they are separated here by hand.
    // (`ParseIntError::PosOverflow` would name the range case, but matching on
    // it is still unstable, so the syntax rule is written out instead — which
    // is the clearer of the two anyway.)
    //
    // `strconv.ParseInt(s, 10, 64)` accepts an optional leading `+`/`-`
    // followed by ASCII digits, and nothing else: no spaces, no `0x`, no
    // `_` separators, no non-ASCII digits. Rust's `from_str` accepts exactly
    // that grammar, so having got here, the only two possibilities left are
    // "not a number" and "a number too large for i64".
    let digits = s
        .strip_prefix('-')
        .or_else(|| s.strip_prefix('+'))
        .unwrap_or(s);
    if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        // Syntax error: Go returns 0 alongside `ErrSyntax`.
        return 0;
    }
    // A well-formed integer that did not fit is a *range* error, and Go clamps
    // to the bound rather than zeroing. The sign is all that is needed to know
    // which bound.
    if s.starts_with('-') {
        i64::MIN
    } else {
        i64::MAX
    }
}

#[cfg(test)]
mod tests {
    use super::parse_int;

    /// The table from the module docs, one row per measured Go result. Each
    /// pair is a Go probe output and the value this port must produce, so a
    /// disagreement names the row rather than just failing.
    const MEASURED: &[(&str, i64)] = &[
        ("45", 45),
        ("+45", 45),
        ("007", 7),
        ("-1", -1),
        ("", 0),
        (" 45", 0),
        ("45 ", 0),
        ("45\n", 0),
        ("0x2d", 0),
        ("lots", 0),
        ("4.5", 0),
        ("9223372036854775807", i64::MAX),
        ("9223372036854775808", i64::MAX),
        ("-9223372036854775808", i64::MIN),
        ("-9223372036854775809", i64::MIN),
        ("99999999999999999999", i64::MAX),
        ("-99999999999999999999", i64::MIN),
        ("\t45", 0),
        ("45\r", 0),
        ("4 5", 0),
        ("  ", 0),
        ("-", 0),
        ("+", 0),
        ("0b101", 0),
        ("1_0", 0),
        ("٣", 0),
    ];

    #[test]
    fn every_measured_row_reproduces() {
        for (input, expected) in MEASURED {
            assert_eq!(parse_int(input), *expected, "input {input:?}");
        }
    }

    /// The row that separates this from `s.parse().unwrap_or(0)`: a range
    /// error clamps. Both `str::parse` and a zero fallback are wrong here, and
    /// only the signed direction tells the two clamps apart.
    #[test]
    fn a_range_error_clamps_rather_than_zeroing() {
        assert_ne!(parse_int("9223372036854775808"), 0);
        assert_ne!(parse_int("-92233720368599999999"), 0);
        assert_eq!(parse_int("99999999999999999999"), i64::MAX);
        assert_eq!(parse_int("-99999999999999999999"), i64::MIN);
    }

    /// The row that separates it from a trimming port: Go's `Atoi` does not
    /// trim, so a stray space is a syntax error and the value is 0, not the
    /// number. Measured, and the reason `max-parallel: " 3 "` is 0 upstream.
    #[test]
    fn surrounding_whitespace_is_a_syntax_error() {
        assert_eq!(parse_int(" 45"), 0);
        assert_eq!(parse_int("45 "), 0);
        assert_eq!(parse_int("\t45"), 0);
        // ... and the empty string, which is the case `Atoi` reports as
        // "invalid syntax" rather than "value out of range".
        assert_eq!(parse_int(""), 0);
    }
}

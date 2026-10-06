// ref: sdk/cliproxy/usage/accounting_test.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use super::*;

#[test]
fn subset_avoids_cache_and_reasoning_double_count() {
    let value = new_subset_token_breakdown(100, 40, 10, 30, 12, 130);
    assert!(value.valid());
    assert_eq!(value.input.uncached_tokens, 50);
    assert_eq!(value.output.non_reasoning_tokens, 18);
    assert_eq!(value.total_tokens, 130);
}

#[test]
fn partial_subset_preserves_known_buckets() {
    let value = new_partial_subset_token_breakdown(10, 4, 0, 0, 0, 15);
    assert!(value.valid());
    assert_eq!(value.quality, TokenAccountingQuality::Unclassified);
    assert_eq!(value.input.total_tokens, 10);
    assert_eq!(value.unclassified_tokens, 5);
}

#[test]
fn independent_keeps_claude_cache_buckets_independent() {
    let value = new_independent_token_breakdown(30, 7, 13, 5, 0, 55);
    assert!(value.valid());
    assert_eq!(value.input.total_tokens, 50);
    assert_eq!(value.total_tokens, 55);
}

#[test]
fn separate_reasoning_adds_reasoning_to_output() {
    let value = new_separate_reasoning_token_breakdown(20, 5, 0, 7, 3, 30);
    assert!(value.valid());
    assert_eq!(value.output.total_tokens, 10);
    assert_eq!(value.total_tokens, 30);
}

#[test]
fn contradictory_parents_are_inconsistent_but_valid() {
    let value = new_subset_token_breakdown(10, 4, 0, 3, 1, 20);
    assert!(value.valid());
    assert_eq!(value.quality, TokenAccountingQuality::Inconsistent);
    assert_eq!(value.unclassified_tokens, 20);
}

#[test]
fn unclassified_does_not_guess_buckets() {
    let value = new_unclassified_token_breakdown(42);
    assert!(value.valid());
    assert_eq!(value.quality, TokenAccountingQuality::Unclassified);
    assert_eq!(value.unclassified_tokens, 42);
}

#[test]
fn provider_semantics_match_upstream() {
    for (provider, executor, total, input, output) in [
        ("openai", "", 130, 100, 30),
        ("anthropic", "OpenAICompatExecutor", 130, 100, 30),
        ("gemini", "", 142, 100, 42),
        ("anthropic", "", 192, 150, 42),
    ] {
        let detail = ensure_token_breakdown_for_provider(
            Detail {
                input_tokens: 100,
                output_tokens: 30,
                reasoning_tokens: 12,
                cache_read_tokens: 40,
                cache_creation_tokens: 10,
                ..Detail::default()
            },
            provider,
            executor,
        );
        assert!(detail.token_breakdown.valid());
        assert_eq!(
            detail.token_breakdown.quality,
            TokenAccountingQuality::Complete
        );
        assert_eq!(detail.total_tokens, total);
        assert_eq!(detail.token_breakdown.input.total_tokens, input);
        assert_eq!(detail.token_breakdown.output.total_tokens, output);
    }
}

#[test]
fn unknown_provider_preserves_auxiliary_usage_without_guessing() {
    let detail = ensure_token_breakdown_for_provider(
        Detail {
            reasoning_tokens: 12,
            cache_read_tokens: 7,
            ..Detail::default()
        },
        "plugin-provider",
        "",
    );
    assert_eq!(detail.total_tokens, 19);
    assert_eq!(
        detail.token_breakdown.quality,
        TokenAccountingQuality::Unclassified
    );
    assert_eq!(detail.token_breakdown.unclassified_tokens, 19);
}

#[test]
fn gemini_reasoning_only_is_complete() {
    let detail = ensure_token_breakdown_for_provider(
        Detail {
            reasoning_tokens: 12,
            ..Detail::default()
        },
        "gemini",
        "",
    );
    assert_eq!(detail.total_tokens, 12);
    assert_eq!(
        detail.token_breakdown.quality,
        TokenAccountingQuality::Complete
    );
    assert_eq!(detail.token_breakdown.output.reasoning_tokens, 12);
}

#[test]
fn legacy_cached_only_and_canonical_zero_cache_read_are_distinct() {
    let legacy = ensure_token_breakdown_for_provider(
        Detail {
            cached_tokens: 13,
            ..Detail::default()
        },
        "openai",
        "",
    );
    assert_eq!(legacy.cache_read_tokens, 13);
    assert_eq!(legacy.total_tokens, 13);
    assert_eq!(
        legacy.token_breakdown.quality,
        TokenAccountingQuality::Unclassified
    );

    let canonical = ensure_token_breakdown_for_provider(
        Detail {
            cached_tokens: 13,
            cache_creation_tokens: 13,
            ..Detail::default()
        },
        "openai",
        "",
    );
    assert_eq!(canonical.cache_read_tokens, 0);
}

#[test]
fn negative_and_overflow_inputs_fail_closed() {
    let negative = new_independent_token_breakdown(-1, 0, 0, 0, 0, 0);
    assert_eq!(negative.quality, TokenAccountingQuality::Inconsistent);
    assert!(negative.valid());
    let overflow = new_independent_token_breakdown(i64::MAX, 1, 0, 0, 0, 0);
    assert_eq!(overflow.quality, TokenAccountingQuality::Inconsistent);
    assert!(overflow.valid());
}

#[test]
fn valid_rejects_component_sums_that_overflow_int64() {
    let input = TokenBreakdown {
        schema_version: TOKEN_ACCOUNTING_SCHEMA_VERSION,
        quality: TokenAccountingQuality::Complete,
        total_tokens: 0,
        input: TokenInputBreakdown {
            total_tokens: 0,
            uncached_tokens: i64::MAX,
            cache_read_tokens: i64::MAX,
            cache_write_tokens: 2,
        },
        ..TokenBreakdown::default()
    };
    assert!(!input.valid());

    let output = TokenBreakdown {
        schema_version: TOKEN_ACCOUNTING_SCHEMA_VERSION,
        quality: TokenAccountingQuality::Complete,
        total_tokens: 0,
        output: TokenOutputBreakdown {
            total_tokens: -2,
            non_reasoning_tokens: i64::MAX,
            reasoning_tokens: i64::MAX,
        },
        ..TokenBreakdown::default()
    };
    assert!(!output.valid());
}

#[test]
fn subset_and_separate_reasoning_reject_cache_sum_overflow() {
    let subset = new_subset_token_breakdown(i64::MAX, i64::MAX, i64::MAX, 0, 0, i64::MAX);
    assert_ne!(subset.quality, TokenAccountingQuality::Complete);
    assert!(subset.input.uncached_tokens >= 0);

    let separate =
        new_separate_reasoning_token_breakdown(i64::MAX, i64::MAX, i64::MAX, 0, 0, i64::MAX);
    assert_ne!(separate.quality, TokenAccountingQuality::Complete);
    assert!(separate.input.uncached_tokens >= 0);
}

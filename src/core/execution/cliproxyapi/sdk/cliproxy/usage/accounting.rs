// ref: sdk/cliproxy/usage/accounting.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: ported
// License: MIT (upstream); modifications AGPL-3.0-only

use serde::{Deserialize, Serialize};

pub const TOKEN_ACCOUNTING_SCHEMA_VERSION: u8 = 2;

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TokenAccountingQuality {
    Complete,
    Inconsistent,
    #[default]
    Unclassified,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct TokenInputBreakdown {
    pub total_tokens: i64,
    pub uncached_tokens: i64,
    pub cache_read_tokens: i64,
    pub cache_write_tokens: i64,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct TokenOutputBreakdown {
    pub total_tokens: i64,
    pub non_reasoning_tokens: i64,
    pub reasoning_tokens: i64,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct TokenBreakdown {
    pub schema_version: u8,
    pub quality: TokenAccountingQuality,
    pub total_tokens: i64,
    pub input: TokenInputBreakdown,
    pub output: TokenOutputBreakdown,
    pub unclassified_tokens: i64,
}

impl TokenBreakdown {
    pub fn valid(&self) -> bool {
        self.schema_version == TOKEN_ACCOUNTING_SCHEMA_VERSION
            && [
                self.total_tokens,
                self.unclassified_tokens,
                self.input.total_tokens,
                self.input.uncached_tokens,
                self.input.cache_read_tokens,
                self.input.cache_write_tokens,
                self.output.total_tokens,
                self.output.non_reasoning_tokens,
                self.output.reasoning_tokens,
            ]
            .into_iter()
            .all(|value| value >= 0)
            && checked_sum(&[
                self.input.uncached_tokens,
                self.input.cache_read_tokens,
                self.input.cache_write_tokens,
            ]) == Some(self.input.total_tokens)
            && checked_sum(&[
                self.output.non_reasoning_tokens,
                self.output.reasoning_tokens,
            ]) == Some(self.output.total_tokens)
            && checked_sum(&[
                self.input.total_tokens,
                self.output.total_tokens,
                self.unclassified_tokens,
            ]) == Some(self.total_tokens)
            && (self.quality != TokenAccountingQuality::Complete || self.unclassified_tokens == 0)
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Detail {
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub reasoning_tokens: i64,
    pub cached_tokens: i64,
    pub cache_read_tokens: i64,
    pub cache_creation_tokens: i64,
    pub total_tokens: i64,
    pub token_breakdown: TokenBreakdown,
    pub response_service_tier: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum AccountingSemantics {
    Unknown,
    Subset,
    Independent,
    SeparateReasoning,
}

pub fn new_subset_token_breakdown(
    input_total: i64,
    cache_read: i64,
    cache_write: i64,
    output_total: i64,
    reasoning: i64,
    total: i64,
) -> TokenBreakdown {
    let Some(expected) = checked_sum(&[input_total, output_total]) else {
        return inconsistent(total, 0);
    };
    let Some(cache_total) = checked_sum(&[cache_read, cache_write]) else {
        return inconsistent(total, expected);
    };
    if cache_total > input_total || reasoning < 0 || reasoning > output_total {
        return inconsistent(total, expected);
    }
    let Some(resolved) = resolve_total(total, expected) else {
        return inconsistent(total, expected);
    };
    complete(
        resolved,
        TokenInputBreakdown {
            total_tokens: input_total,
            uncached_tokens: input_total - cache_total,
            cache_read_tokens: cache_read,
            cache_write_tokens: cache_write,
        },
        TokenOutputBreakdown {
            total_tokens: output_total,
            non_reasoning_tokens: output_total - reasoning,
            reasoning_tokens: reasoning,
        },
    )
}

pub fn new_partial_subset_token_breakdown(
    input_total: i64,
    cache_read: i64,
    cache_write: i64,
    output_total: i64,
    reasoning: i64,
    total: i64,
) -> TokenBreakdown {
    let (Some(cache_total), Some(expected)) = (
        checked_sum(&[cache_read, cache_write]),
        checked_sum(&[input_total, output_total]),
    ) else {
        return inconsistent(total, 0);
    };
    if reasoning < 0 || cache_total > input_total || reasoning > output_total || total < 0 {
        return inconsistent(total, expected);
    }
    let resolved = if total == 0 { expected } else { total };
    if resolved < expected {
        return inconsistent(total, expected);
    }
    let unclassified = resolved - expected;
    TokenBreakdown {
        schema_version: TOKEN_ACCOUNTING_SCHEMA_VERSION,
        quality: if unclassified == 0 {
            TokenAccountingQuality::Complete
        } else {
            TokenAccountingQuality::Unclassified
        },
        total_tokens: resolved,
        input: TokenInputBreakdown {
            total_tokens: input_total,
            uncached_tokens: input_total - cache_total,
            cache_read_tokens: cache_read,
            cache_write_tokens: cache_write,
        },
        output: TokenOutputBreakdown {
            total_tokens: output_total,
            non_reasoning_tokens: output_total - reasoning,
            reasoning_tokens: reasoning,
        },
        unclassified_tokens: unclassified,
    }
}

pub fn new_independent_token_breakdown(
    uncached_input: i64,
    cache_read: i64,
    cache_write: i64,
    non_reasoning_output: i64,
    reasoning: i64,
    total: i64,
) -> TokenBreakdown {
    let (Some(input_total), Some(output_total)) = (
        checked_sum(&[uncached_input, cache_read, cache_write]),
        checked_sum(&[non_reasoning_output, reasoning]),
    ) else {
        return inconsistent(total, 0);
    };
    let Some(expected) = checked_sum(&[input_total, output_total]) else {
        return inconsistent(total, 0);
    };
    let Some(resolved) = resolve_total(total, expected) else {
        return inconsistent(total, expected);
    };
    complete(
        resolved,
        TokenInputBreakdown {
            total_tokens: input_total,
            uncached_tokens: uncached_input,
            cache_read_tokens: cache_read,
            cache_write_tokens: cache_write,
        },
        TokenOutputBreakdown {
            total_tokens: output_total,
            non_reasoning_tokens: non_reasoning_output,
            reasoning_tokens: reasoning,
        },
    )
}

pub fn new_separate_reasoning_token_breakdown(
    input_total: i64,
    cache_read: i64,
    cache_write: i64,
    non_reasoning_output: i64,
    reasoning: i64,
    total: i64,
) -> TokenBreakdown {
    let Some(cache_total) = checked_sum(&[cache_read, cache_write]) else {
        return inconsistent(total, 0);
    };
    if cache_total > input_total {
        return inconsistent(total, 0);
    }
    let Some(output_total) = checked_sum(&[non_reasoning_output, reasoning]) else {
        return inconsistent(total, 0);
    };
    let Some(expected) = checked_sum(&[input_total, output_total]) else {
        return inconsistent(total, 0);
    };
    let Some(resolved) = resolve_total(total, expected) else {
        return inconsistent(total, expected);
    };
    complete(
        resolved,
        TokenInputBreakdown {
            total_tokens: input_total,
            uncached_tokens: input_total - cache_total,
            cache_read_tokens: cache_read,
            cache_write_tokens: cache_write,
        },
        TokenOutputBreakdown {
            total_tokens: output_total,
            non_reasoning_tokens: non_reasoning_output,
            reasoning_tokens: reasoning,
        },
    )
}

pub fn new_unclassified_token_breakdown(total: i64) -> TokenBreakdown {
    if total <= 0 {
        return TokenBreakdown {
            schema_version: TOKEN_ACCOUNTING_SCHEMA_VERSION,
            quality: if total < 0 {
                TokenAccountingQuality::Inconsistent
            } else {
                TokenAccountingQuality::Complete
            },
            ..TokenBreakdown::default()
        };
    }
    TokenBreakdown {
        schema_version: TOKEN_ACCOUNTING_SCHEMA_VERSION,
        quality: TokenAccountingQuality::Unclassified,
        total_tokens: total,
        unclassified_tokens: total,
        ..TokenBreakdown::default()
    }
}

pub fn ensure_token_breakdown(detail: Detail) -> Detail {
    ensure_token_breakdown_for_provider(detail, "", "")
}

pub fn ensure_token_breakdown_for_provider(
    mut detail: Detail,
    provider: &str,
    executor_type: &str,
) -> Detail {
    if !detail.token_breakdown.valid() {
        let semantics = semantics_for(provider, executor_type);
        if detail.cache_read_tokens == 0
            && detail.cached_tokens > 0
            && detail.input_tokens == 0
            && detail.output_tokens == 0
            && detail.reasoning_tokens == 0
            && detail.cache_creation_tokens == 0
            && detail.total_tokens == 0
            && matches!(
                semantics,
                AccountingSemantics::Subset | AccountingSemantics::SeparateReasoning
            )
        {
            detail.cache_read_tokens = detail.cached_tokens;
        }
        detail.token_breakdown = breakdown_for_semantics(&detail, semantics);
    }
    if detail.total_tokens == 0 {
        detail.total_tokens = detail.token_breakdown.total_tokens;
    }
    detail
}

fn breakdown_for_semantics(detail: &Detail, semantics: AccountingSemantics) -> TokenBreakdown {
    if detail.total_tokens == 0 && detail.input_tokens == 0 && detail.output_tokens == 0 {
        let Some(total) = unclassified_lower_bound(detail) else {
            return inconsistent(detail.total_tokens, 0);
        };
        if total > 0
            && (semantics == AccountingSemantics::Unknown
                || semantics == AccountingSemantics::Subset
                || (semantics == AccountingSemantics::SeparateReasoning
                    && (detail.cache_read_tokens > 0
                        || detail.cache_creation_tokens > 0
                        || detail.cached_tokens > 0)))
        {
            return new_unclassified_token_breakdown(total);
        }
    }
    match semantics {
        AccountingSemantics::Subset => new_subset_token_breakdown(
            detail.input_tokens,
            detail.cache_read_tokens,
            detail.cache_creation_tokens,
            detail.output_tokens,
            detail.reasoning_tokens,
            detail.total_tokens,
        ),
        AccountingSemantics::Independent => new_independent_token_breakdown(
            detail.input_tokens,
            detail.cache_read_tokens,
            detail.cache_creation_tokens,
            detail.output_tokens,
            detail.reasoning_tokens,
            detail.total_tokens,
        ),
        AccountingSemantics::SeparateReasoning => new_separate_reasoning_token_breakdown(
            detail.input_tokens,
            detail.cache_read_tokens,
            detail.cache_creation_tokens,
            detail.output_tokens,
            detail.reasoning_tokens,
            detail.total_tokens,
        ),
        AccountingSemantics::Unknown => unclassified_lower_bound(detail)
            .map(new_unclassified_token_breakdown)
            .unwrap_or_else(|| inconsistent(detail.total_tokens, 0)),
    }
}

fn unclassified_lower_bound(detail: &Detail) -> Option<i64> {
    let cache = checked_sum(&[detail.cache_read_tokens, detail.cache_creation_tokens])?;
    if [
        detail.input_tokens,
        detail.output_tokens,
        detail.reasoning_tokens,
        detail.cached_tokens,
    ]
    .into_iter()
    .any(|value| value < 0)
    {
        return None;
    }
    checked_sum(&[
        detail.input_tokens.max(cache).max(detail.cached_tokens),
        detail.output_tokens.max(detail.reasoning_tokens),
    ])
}

fn semantics_for(provider: &str, executor_type: &str) -> AccountingSemantics {
    let provider = provider.trim().to_ascii_lowercase();
    let executor = executor_type.trim().to_ascii_lowercase();
    let value = format!("{provider} {executor}");
    let value = value.trim();
    if value.is_empty() || value == "unknown" || value == "unknown unknown" {
        return AccountingSemantics::Unknown;
    }
    if executor == "openaicompatexecutor"
        || provider == "openai-compatibility"
        || provider.starts_with("openai-compatible-")
    {
        return AccountingSemantics::Subset;
    }
    if value.contains("claude") || value.contains("anthropic") {
        return AccountingSemantics::Independent;
    }
    if ["gemini", "aistudio", "antigravity", "vertex", "interaction"]
        .into_iter()
        .any(|marker| value.contains(marker))
    {
        return AccountingSemantics::SeparateReasoning;
    }
    if [
        "openai",
        "codex",
        "xai",
        "grok",
        "kimi",
        "qwen",
        "deepseek",
        "openrouter",
    ]
    .into_iter()
    .any(|marker| value.contains(marker))
    {
        return AccountingSemantics::Subset;
    }
    AccountingSemantics::Unknown
}

fn complete(
    total: i64,
    input: TokenInputBreakdown,
    output: TokenOutputBreakdown,
) -> TokenBreakdown {
    TokenBreakdown {
        schema_version: TOKEN_ACCOUNTING_SCHEMA_VERSION,
        quality: TokenAccountingQuality::Complete,
        total_tokens: total,
        input,
        output,
        unclassified_tokens: 0,
    }
}

fn inconsistent(total: i64, fallback: i64) -> TokenBreakdown {
    let resolved = if total > 0 { total } else { fallback.max(0) };
    TokenBreakdown {
        schema_version: TOKEN_ACCOUNTING_SCHEMA_VERSION,
        quality: TokenAccountingQuality::Inconsistent,
        total_tokens: resolved,
        unclassified_tokens: resolved,
        ..TokenBreakdown::default()
    }
}

fn resolve_total(total: i64, expected: i64) -> Option<i64> {
    if total < 0 || expected < 0 {
        None
    } else if total == 0 {
        Some(expected)
    } else if total == expected {
        Some(total)
    } else {
        None
    }
}

/// Matches upstream `nonNegativeSum`: any negative operand or int64 overflow
/// rejects the sum. A wrapped positive total must not validate.
fn checked_sum(values: &[i64]) -> Option<i64> {
    values.iter().try_fold(0_i64, |total, value| {
        if *value < 0 {
            return None;
        }
        total.checked_add(*value)
    })
}

// Origin: CTOX; License: AGPL-3.0-only
//! Only provider-reported shared quota windows may rank a fresh account selection.
use std::collections::BTreeMap;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QuotaWindowKind { FiveHour, SevenDay }

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReportedQuotaStatus { Allowed, Rejected }

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ReportedQuotaWindow {
    pub kind: QuotaWindowKind,
    pub status: ReportedQuotaStatus,
    pub reset_at_ms: i64,
    /// Absent means the provider did not supply utilization; never invent a percentage.
    pub utilization: Option<f64>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ReportedQuotaSnapshot {
    pub observed_at_ms: i64,
    pub windows: Vec<ReportedQuotaWindow>,
}
impl ReportedQuotaSnapshot {
    pub fn validate(&self) -> Result<(), AccountQuotaStoreError> {
        if self.observed_at_ms <= 0 || self.windows.is_empty() || self.windows.len() > 2 {
            return Err(AccountQuotaStoreError::InvalidRecord);
        }
        for (i, window) in self.windows.iter().enumerate() {
            if window.reset_at_ms <= self.observed_at_ms
                || window.reset_at_ms.saturating_sub(self.observed_at_ms) > 608_400_000
                || self.windows[..i].iter().any(|prior| prior.kind == window.kind)
                || window.utilization.is_some_and(|v| !v.is_finite() || !(0.0..=1.0).contains(&v))
            {
                return Err(AccountQuotaStoreError::InvalidRecord);
            }
        }
        Ok(())
    }
    /// A known future reset with reported usable capacity precedes an unknown
    /// window. Exhaustion/cooldowns remain the existing conductor's authority.
    pub fn usable_reset_at(&self, now_ms: i64) -> Option<i64> {
        if self.validate().is_err() || self.observed_at_ms > now_ms { return None; }
        self.windows.iter().filter(|window| {
            window.reset_at_ms > now_ms && window.status == ReportedQuotaStatus::Allowed
                && window.utilization.is_none_or(|used| used < 1.0)
        }).map(|window| window.reset_at_ms).min()
    }
}

/// No credentials, client labels, prompts, raw headers or provider error text.
/// The host serializes observations and rejects an older snapshot replacing a newer one.
pub trait AccountQuotaStore: Send + Sync {
    fn load(&self, provider: &str, auth_ids: &[String])
        -> Result<BTreeMap<String, ReportedQuotaSnapshot>, AccountQuotaStoreError>;
    fn save(&self, provider: &str, auth_id: &str, snapshot: &ReportedQuotaSnapshot)
        -> Result<(), AccountQuotaStoreError>;
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AccountQuotaStoreError { Read, Write, InvalidRecord, Capacity }
impl std::fmt::Display for AccountQuotaStoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Read => "reported quota read failed", Self::Write => "reported quota write failed",
            Self::InvalidRecord => "reported quota is invalid", Self::Capacity => "reported quota storage is full",
        })
    }
}
impl std::error::Error for AccountQuotaStoreError {}

#[cfg(test)]
#[path = "account_quota_test.rs"]
mod tests;

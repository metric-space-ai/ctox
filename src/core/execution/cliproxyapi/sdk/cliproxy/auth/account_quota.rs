// Origin: CTOX; License: AGPL-3.0-only
//! Only provider-reported shared quota windows may rank a fresh account selection.
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QuotaWindowKind {
    FiveHour,
    SevenDay,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReportedQuotaStatus {
    Allowed,
    Rejected,
}

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
                || self.windows[..i]
                    .iter()
                    .any(|prior| prior.kind == window.kind)
                || window
                    .utilization
                    .is_some_and(|v| !v.is_finite() || !(0.0..=1.0).contains(&v))
            {
                return Err(AccountQuotaStoreError::InvalidRecord);
            }
        }
        Ok(())
    }
    /// A known future reset with reported usable capacity precedes an unknown
    /// window. Exhaustion/cooldowns remain the existing conductor's authority.
    pub fn usable_reset_at(&self, now_ms: i64) -> Option<i64> {
        if self.validate().is_err() || self.observed_at_ms > now_ms {
            return None;
        }
        self.windows
            .iter()
            .filter(|window| {
                window.reset_at_ms > now_ms
                    && window.status == ReportedQuotaStatus::Allowed
                    && window.utilization.is_none_or(|used| used < 1.0)
            })
            .map(|window| window.reset_at_ms)
            .min()
    }
}

/// No credentials, client labels, prompts, raw headers or provider error text.
/// The host serializes observations and rejects an older snapshot replacing a newer one.
/// A generation is holder-private: never serialize it into public quota metadata.
#[derive(Clone, PartialEq, Eq)]
pub struct AccountQuotaGeneration([u8; 32]);
impl AccountQuotaGeneration {
    pub fn from_host_digest(digest: [u8; 32]) -> Self {
        Self(digest)
    }
    pub fn host_digest(&self) -> &[u8; 32] {
        &self.0
    }
}

/// Captured before transport execution and consumed after an actual response.
/// No Debug/Serialize: account selectors and generations remain holder-private.
pub struct ClaudeQuotaObservation {
    pub(super) auth_id: String,
    pub(super) generation: Option<AccountQuotaGeneration>,
}

pub trait AccountQuotaStore: Send + Sync {
    fn load(
        &self,
        provider: &str,
        auth_ids: &[String],
    ) -> Result<BTreeMap<String, ReportedQuotaSnapshot>, AccountQuotaStoreError>;
    fn save(
        &self,
        provider: &str,
        auth_id: &str,
        snapshot: &ReportedQuotaSnapshot,
    ) -> Result<(), AccountQuotaStoreError>;

    fn capture_generation(
        &self,
        _provider: &str,
        _auth_id: &str,
    ) -> Result<Option<AccountQuotaGeneration>, AccountQuotaStoreError> {
        Ok(None)
    }

    fn save_if_generation(
        &self,
        provider: &str,
        auth_id: &str,
        snapshot: &ReportedQuotaSnapshot,
        generation: Option<&AccountQuotaGeneration>,
    ) -> Result<(), AccountQuotaStoreError> {
        if generation.is_some() {
            return Err(AccountQuotaStoreError::InvalidRecord);
        }
        self.save(provider, auth_id, snapshot)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AccountQuotaStoreError {
    Read,
    Write,
    InvalidRecord,
    Capacity,
}
impl std::fmt::Display for AccountQuotaStoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Read => "reported quota read failed",
            Self::Write => "reported quota write failed",
            Self::InvalidRecord => "reported quota is invalid",
            Self::Capacity => "reported quota storage is full",
        })
    }
}
impl std::error::Error for AccountQuotaStoreError {}

#[cfg(test)]
#[path = "account_quota_test.rs"]
mod tests;

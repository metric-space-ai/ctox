//! Refresh scheduling: `next_refresh_after = expires_at - lead`, clamped to now.
use crate::model::{ConnectedAccount, ConnectionStatus};

/// Default lead time before expiry (cliproxyapi `RefreshLeadRegistry` pattern).
pub const DEFAULT_REFRESH_LEAD_SECS: u64 = 300;

pub fn expires_at(now: u64, expires_in: Option<u64>) -> Option<u64> {
    expires_in.map(|s| now.saturating_add(s))
}

pub fn next_refresh_after(
    now: u64,
    expires_at: Option<u64>,
    lead: u64,
    refreshable: bool,
) -> Option<u64> {
    if !refreshable {
        return None;
    }
    expires_at.map(|e| e.saturating_sub(lead).max(now))
}

/// Accounts the scheduler should refresh now.
pub fn due<'a>(
    accounts: &'a [ConnectedAccount],
    now: u64,
) -> impl Iterator<Item = &'a ConnectedAccount> {
    accounts.iter().filter(move |a| {
        matches!(
            a.status,
            ConnectionStatus::Active | ConnectionStatus::Expired
        ) && a.next_refresh_after.is_some_and(|t| t <= now)
    })
}

/// Whether the access token should be treated as expired at `now`.
pub fn is_expired(account: &ConnectedAccount, now: u64) -> bool {
    account.expires_at.is_some_and(|e| e <= now)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schedule_math() {
        assert_eq!(next_refresh_after(100, Some(1_000), 300, true), Some(700));
        assert_eq!(next_refresh_after(900, Some(1_000), 300, true), Some(900));
        assert_eq!(next_refresh_after(100, Some(1_000), 300, false), None);
        assert_eq!(next_refresh_after(100, None, 300, true), None);
        assert_eq!(expires_at(10, Some(5)), Some(15));
    }
}

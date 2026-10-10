// Origin: CTOX; License: AGPL-3.0-only
//! HTTP-request-scoped account pin. No credentials or cross-request affinity live here.
use crate::sdk::cliproxy::auth::AccountCandidate;
use std::sync::Mutex;
#[derive(Default)]
pub(crate) struct AccountSelection {
    requested: Option<String>,
    selected: Option<String>,
    error_class: Option<&'static str>,
    upstream_status: Option<u16>,
}
tokio::task_local! { pub(crate) static ACCOUNT_SELECTION: Mutex<AccountSelection>; }
pub fn requested_account() -> Option<String> {
    ACCOUNT_SELECTION
        .try_with(|state| state.lock().unwrap().requested.clone())
        .ok()
        .flatten()
}
pub(crate) fn request_account(id: Option<String>) {
    let _ = ACCOUNT_SELECTION.try_with(|state| state.lock().unwrap().requested = id);
}
pub fn candidates(accounts: &[AccountCandidate]) -> Vec<AccountCandidate> {
    let requested = requested_account();
    accounts
        .iter()
        .filter(|account| requested.as_deref().is_none_or(|id| id == account.auth_id))
        .cloned()
        .collect()
}
pub fn record_selected(id: &str) {
    // Only configured, scheduler-selected account IDs may be acknowledged.
    if valid_account_id(id) {
        let _ = ACCOUNT_SELECTION
            .try_with(|state| state.lock().unwrap().selected = Some(id.to_owned()));
    }
}
pub(crate) fn acknowledgement() -> String {
    ACCOUNT_SELECTION
        .try_with(|state| {
            let state = state.lock().unwrap();
            let mut headers = state
                .selected
                .as_ref()
                .map(|id| format!("X-CTOX-Account-Selected: {id}\r\n"))
                .unwrap_or_default();
            if let Some(class) = state.error_class {
                headers.push_str(&format!("X-CTOX-Error-Class: {class}\r\n"));
            }
            if let Some(status) = state.upstream_status {
                headers.push_str(&format!("X-CTOX-Upstream-Status: {status}\r\n"));
            }
            headers
        })
        .unwrap_or_default()
}

pub fn valid_account_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && id
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"-_.:".contains(&c))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn pin_is_exact_and_request_scoped() {
        let accounts = vec![
            AccountCandidate {
                auth_id: "healthy".into(),
                ..Default::default()
            },
            AccountCandidate {
                auth_id: "broken".into(),
                ..Default::default()
            },
        ];
        ACCOUNT_SELECTION
            .scope(Mutex::new(Default::default()), async {
                request_account(Some("broken".into()));
                assert_eq!(candidates(&accounts), vec![accounts[1].clone()]);
                record_selected("broken");
                assert_eq!(acknowledgement(), "X-CTOX-Account-Selected: broken\r\n");
            })
            .await;
        assert_eq!(candidates(&accounts), accounts);
        assert!(acknowledgement().is_empty());
    }
    #[test]
    fn selector_is_bounded_and_header_safe() {
        for id in ["", "a/b", "a\r\nSecret: value", "é"] {
            assert!(!valid_account_id(id));
        }
        assert!(!valid_account_id(&"a".repeat(129)));
        assert!(valid_account_id("account-opaque_1.2:3"));
    }
}

/// Retain only a closed error class for pinned probes; raw upstream data stays native.
pub fn record_upstream_status(status: u16, body: &[u8]) {
    if requested_account().is_none() {
        return;
    }
    let class = if (200..300).contains(&status) {
        None
    } else if matches!(status, 401 | 403) {
        Some("auth")
    } else if matches!(status, 402 | 429) {
        Some("quota-rate-limit")
    } else {
        let json = serde_json::from_slice::<serde_json::Value>(body).ok();
        let unknown_model = json.as_ref().is_some_and(|json| {
            ["/error/code", "/error/type", "/code", "/error/status"]
                .iter()
                .any(|path| {
                    json.pointer(path)
                        .and_then(serde_json::Value::as_str)
                        .is_some_and(|code| {
                            matches!(
                                code,
                                "model_not_found"
                                    | "unknown_model"
                                    | "invalid_model"
                                    | "unsupported_model"
                                    | "MODEL_NOT_FOUND"
                                    | "UNKNOWN_MODEL"
                                    | "UNSUPPORTED_MODEL"
                            )
                        })
                })
                || json
                    .pointer("/error/param")
                    .and_then(serde_json::Value::as_str)
                    == Some("model")
        });
        Some(if unknown_model {
            "unknown-model"
        } else {
            "network-provider"
        })
    };
    let _ = ACCOUNT_SELECTION.try_with(|state| {
        let mut state = state.lock().unwrap();
        state.error_class = class;
        state.upstream_status = Some(status);
    });
}

#[cfg(test)]
mod upstream_witness_tests {
    use super::*;

    #[tokio::test]
    async fn only_actual_upstream_io_can_supply_a_status_witness() {
        ACCOUNT_SELECTION
            .scope(Mutex::new(Default::default()), async {
                request_account(Some("exact-account".into()));
                record_selected("exact-account");
                assert!(!acknowledgement().contains("X-CTOX-Upstream-Status"));
                record_upstream_status(429, b"private provider body");
                assert!(acknowledgement().contains("X-CTOX-Upstream-Status: 429\r\n"));
                assert!(acknowledgement().contains("X-CTOX-Error-Class: quota-rate-limit\r\n"));
                assert!(!acknowledgement().contains("private provider body"));
                record_upstream_status(200, b"private generated text");
                assert!(acknowledgement().contains("X-CTOX-Upstream-Status: 200\r\n"));
                assert!(!acknowledgement().contains("X-CTOX-Error-Class"));
                assert!(!acknowledgement().contains("private generated text"));
            })
            .await;
        assert!(acknowledgement().is_empty());
    }

    #[tokio::test]
    async fn unpinned_requests_never_publish_probe_status_or_error_class() {
        ACCOUNT_SELECTION
            .scope(Mutex::new(Default::default()), async {
                record_selected("regular-account");
                record_upstream_status(401, b"private credential error");
                assert_eq!(
                    acknowledgement(),
                    "X-CTOX-Account-Selected: regular-account\r\n"
                );
            })
            .await;
    }
}

#[cfg(test)]
mod isolation_tests {
    use super::*;
    #[tokio::test]
    async fn simultaneous_pin_and_unpinned_requests_do_not_share_error_or_selection() {
        let accounts = vec![
            AccountCandidate {
                auth_id: "healthy".into(),
                ..Default::default()
            },
            AccountCandidate {
                auth_id: "broken".into(),
                ..Default::default()
            },
        ];
        let pinned = ACCOUNT_SELECTION.scope(Mutex::new(Default::default()), async {
            request_account(Some("broken".into()));
            tokio::task::yield_now().await;
            assert_eq!(candidates(&accounts).len(), 1);
            record_selected("broken");
            record_upstream_status(429, b"secret");
            assert!(acknowledgement().contains("quota-rate-limit"));
            assert_eq!(
                tokio::spawn(async { requested_account() }).await.unwrap(),
                None
            );
        });
        let unpinned = ACCOUNT_SELECTION.scope(Mutex::new(Default::default()), async {
            tokio::task::yield_now().await;
            assert_eq!(candidates(&accounts).len(), 2);
            record_selected("healthy");
            record_upstream_status(401, b"secret");
            assert_eq!(acknowledgement(), "X-CTOX-Account-Selected: healthy\r\n");
        });
        tokio::join!(pinned, unpinned);
    }
}

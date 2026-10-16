// ref: internal/config/credential_concurrency_test.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: ported
// License: MIT (upstream); modifications AGPL-3.0-only

use std::time::Duration;

use super::credential_concurrency::*;

#[test]
fn explicit_invalid_values_are_not_defaulted() {
    for source in [
        "lifecycle-config-revision: 0\n",
        "lifecycle-config-revision: 1\ncpa-heartbeat-timeout: 0s\n",
        "lifecycle-config-revision: 1\ncpa-heartbeat-timeout: null\n",
        "lifecycle-config-revision: 1\nobservation-barrier-revision: -1\n",
    ] {
        let config = serde_yaml::from_str::<CredentialConcurrencyConfig>(source)
            .unwrap()
            .with_defaults();
        assert!(
            validate_credential_concurrency_lifecycle(Duration::from_secs(20), &config).is_err()
        );
    }
}

fn valid() -> CredentialConcurrencyConfig {
    CredentialConcurrencyConfig {
        cpa_heartbeat_timeout: Duration::from_secs(3),
        cpa_cancel_bound: Duration::from_secs(5),
        reclaim_grace: Duration::from_secs(5),
        cleanup_interval: Duration::from_secs(5),
        release_flush_interval: Duration::from_millis(250),
        release_max_backoff: Duration::from_secs(2),
        busy_retry_min: Duration::from_millis(250),
        busy_retry_max: Duration::from_secs(1),
        max_limit: MAX_CREDENTIAL_CONCURRENCY_LIMIT,
        ..CredentialConcurrencyConfig::default()
    }
}

#[test]
fn rejects_invalid_limiter_and_go_duration_overflow() {
    let invalid = [
        CredentialConcurrencyConfig {
            release_flush_interval: Duration::from_secs(1),
            release_max_backoff: Duration::from_millis(500),
            ..valid()
        },
        CredentialConcurrencyConfig {
            busy_retry_min: Duration::from_micros(1500),
            busy_retry_max: Duration::from_millis(2),
            ..valid()
        },
        CredentialConcurrencyConfig {
            max_limit: 1_000_001,
            ..valid()
        },
    ];
    assert!(invalid
        .iter()
        .all(
            |config| validate_credential_concurrency_lifecycle(Duration::from_secs(20), config)
                .is_err()
        ));

    let overflow = CredentialConcurrencyConfig {
        lifecycle_config_revision: 1,
        cpa_heartbeat_timeout: Duration::from_nanos(i64::MAX as u64),
        cpa_cancel_bound: Duration::from_nanos(1),
        reclaim_grace: Duration::from_secs(1),
        cleanup_interval: Duration::from_secs(1),
        release_flush_interval: Duration::from_nanos(1),
        release_max_backoff: Duration::from_nanos(1),
        busy_retry_min: Duration::from_millis(1),
        busy_retry_max: Duration::from_millis(1),
        max_limit: 1,
        ..CredentialConcurrencyConfig::default()
    };
    assert!(validate_credential_concurrency_lifecycle(Duration::from_secs(1), &overflow).is_err());
}

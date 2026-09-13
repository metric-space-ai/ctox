use super::*;
use std::cell::Cell;
use std::future::{pending, ready, Future, Ready};
use std::task::{Context, Poll};

fn options(reconnect: bool, max_reconnect_delay_ms: u64) -> BusinessOsMcpGatewayConnectOptions {
    BusinessOsMcpGatewayConnectOptions {
        url: "wss://unused.invalid/connect/test".into(),
        token: None,
        reconnect,
        max_reconnect_delay_ms,
        heartbeat_interval_ms: DEFAULT_GATEWAY_HEARTBEAT_INTERVAL_MS,
        max_connection_age_ms: DEFAULT_GATEWAY_MAX_CONNECTION_AGE_MS,
    }
}

// Drive the production retry loop through a finite sequence, then cancel its
// pending connection. Ready sleeps record exactly which backoffs it requests.
fn retry_delays(
    outcomes: impl IntoIterator<Item = anyhow::Result<ManagedGatewayConnectionExit>>,
    max_delay_ms: u64,
) -> Vec<u64> {
    let options = options(true, max_delay_ms);
    let mut outcomes = outcomes.into_iter();
    let mut delays = Vec::new();
    let mut connector = Box::pin(run_managed_gateway_connector(
        &options,
        || {
            let outcome = outcomes.next();
            async move {
                match outcome {
                    Some(outcome) => outcome,
                    None => pending().await,
                }
            }
        },
        |delay| {
            delays.push(delay);
            ready(())
        },
    ));
    let waker = futures_util::task::noop_waker();
    assert!(connector
        .as_mut()
        .poll(&mut Context::from_waker(&waker))
        .is_pending());
    drop(connector);
    delays
}

#[test]
fn healthy_rotations_never_sleep_or_accumulate_failure_backoff() {
    let outcomes = (0..12)
        .map(|_| Ok(ManagedGatewayConnectionExit::MaxAgeReached))
        .chain([Err(anyhow::anyhow!("connection refused"))]);
    assert_eq!(retry_delays(outcomes, 30_000), vec![500]);
}

#[test]
fn healthy_rotation_resets_saturated_failure_backoff() {
    let failures = (0..9).map(|_| Err(anyhow::anyhow!("connection refused")));
    let outcomes = failures.chain([
        Ok(ManagedGatewayConnectionExit::MaxAgeReached),
        Err(anyhow::anyhow!("websocket read failed")),
        Ok(ManagedGatewayConnectionExit::StreamEnded),
    ]);
    assert_eq!(
        retry_delays(outcomes, 30_000),
        vec![500, 1_000, 2_000, 4_000, 8_000, 16_000, 30_000, 30_000, 30_000, 500, 1_000]
    );
}

#[test]
fn stream_ends_and_errors_share_bounded_failure_backoff() {
    for (configured, expected) in [
        (0, vec![250; 10]),
        (
            2_500,
            vec![
                500, 1_000, 2_000, 2_500, 2_500, 2_500, 2_500, 2_500, 2_500, 2_500,
            ],
        ),
        (
            u64::MAX,
            vec![
                500, 1_000, 2_000, 4_000, 8_000, 16_000, 30_000, 30_000, 30_000, 30_000,
            ],
        ),
    ] {
        let outcomes = (0..10).map(|index| {
            if index % 2 == 0 {
                Ok(ManagedGatewayConnectionExit::StreamEnded)
            } else {
                Err(anyhow::anyhow!("handshake rejected"))
            }
        });
        assert_eq!(retry_delays(outcomes, configured), expected);
    }
}

#[test]
fn reconnect_disabled_returns_each_outcome_without_retry_or_sleep() {
    for outcome in [
        Ok(ManagedGatewayConnectionExit::MaxAgeReached),
        Ok(ManagedGatewayConnectionExit::StreamEnded),
        Err(anyhow::anyhow!("handshake rejected")),
    ] {
        let expect_error = outcome.is_err();
        let options = options(false, 30_000);
        let mut outcome = Some(outcome);
        let mut connector = Box::pin(run_managed_gateway_connector(
            &options,
            || ready(outcome.take().expect("must connect only once")),
            |_| -> Ready<()> { panic!("reconnect disabled must not sleep") },
        ));
        let waker = futures_util::task::noop_waker();
        let Poll::Ready(result) = connector.as_mut().poll(&mut Context::from_waker(&waker)) else {
            panic!("single connection must return its outcome");
        };
        assert_eq!(result.is_err(), expect_error);
        if let Err(error) = result {
            assert_eq!(error.to_string(), "handshake rejected");
        }
    }
}

struct Dropped<'a>(&'a Cell<bool>);

impl Drop for Dropped<'_> {
    fn drop(&mut self) {
        self.0.set(true);
    }
}

#[test]
fn cancellation_drops_active_connection_after_rotation() {
    let options = options(true, 30_000);
    let dropped = Cell::new(false);
    let mut attempts = 0;
    let mut connector = Box::pin(run_managed_gateway_connector(
        &options,
        || {
            attempts += 1;
            let rotate = attempts == 1;
            let dropped = &dropped;
            async move {
                if rotate {
                    return Ok(ManagedGatewayConnectionExit::MaxAgeReached);
                }
                let _guard = Dropped(dropped);
                pending().await
            }
        },
        |_| -> Ready<()> { panic!("rotation must not sleep") },
    ));
    let waker = futures_util::task::noop_waker();
    assert!(connector
        .as_mut()
        .poll(&mut Context::from_waker(&waker))
        .is_pending());
    assert!(!dropped.get());
    drop(connector);
    assert!(dropped.get());
    assert_eq!(attempts, 2);
}

#[test]
fn cancellation_drops_failure_backoff_without_another_attempt() {
    let options = options(true, 30_000);
    let dropped = Cell::new(false);
    let mut attempts = 0;
    let mut connector = Box::pin(run_managed_gateway_connector(
        &options,
        || {
            attempts += 1;
            ready(Err(anyhow::anyhow!("connection refused")))
        },
        |delay| {
            assert_eq!(delay, 500);
            let dropped = &dropped;
            async move {
                let _guard = Dropped(dropped);
                pending().await
            }
        },
    ));
    let waker = futures_util::task::noop_waker();
    assert!(connector
        .as_mut()
        .poll(&mut Context::from_waker(&waker))
        .is_pending());
    assert!(!dropped.get());
    drop(connector);
    assert!(dropped.get());
    assert_eq!(attempts, 1);
}

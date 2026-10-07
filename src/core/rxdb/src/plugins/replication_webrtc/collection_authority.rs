//! Bounded collection admission shared by demand queries and replication RPCs.

use std::time::Duration;

use crate::rx_error::RxResult;

use super::webrtc_types::WebRTCConnectionHandler;

pub(super) const COLLECTION_AUTHORITY_ATTEMPTS: usize = 8;
const COLLECTION_AUTHORITY_RETRY_DELAY: Duration = Duration::from_millis(10);

pub(super) async fn authorize_collection_for_peer<H: WebRTCConnectionHandler>(
    handler: &H,
    peer: &H::Peer,
    collection: &str,
) -> RxResult<bool> {
    for attempt in 0..COLLECTION_AUTHORITY_ATTEMPTS {
        // The handler checks the current peer/token generation on every attempt.
        // A policy denial or any other error is final; only issuer/store
        // availability can recover inside this finite admission window.
        match handler.collection_authorization_for_peer(peer, collection) {
            Err(error)
                if error.code() == "COLLECTION_AUTHORITY_UNAVAILABLE"
                    && attempt + 1 < COLLECTION_AUTHORITY_ATTEMPTS =>
            {
                tokio::time::sleep(COLLECTION_AUTHORITY_RETRY_DELAY).await;
            }
            result => return result,
        }
    }
    unreachable!("bounded authority loop always returns on its final attempt")
}

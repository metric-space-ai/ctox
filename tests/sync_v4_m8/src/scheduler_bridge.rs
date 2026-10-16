//! Test-composition access to the SAME private constructor used by the native
//! lane4 real-WebRTC test. Appended as a child of the pristine scheduler module
//! only in the runner's disposable component source. No transport behavior changes.
use super::{Framer, NativeSendScheduler, RxResult, TransportSelection};
use std::sync::Arc;
use webrtc::data_channel::DataChannel;
pub async fn accept(
    channels: Vec<Arc<dyn DataChannel>>,
    transport: TransportSelection,
) -> RxResult<NativeSendScheduler> {
    let framer = Framer::new(transport).map_err(|e| super::failure(&e.to_string()))?;
    NativeSendScheduler::from_channels_session(channels, framer, None).await
}

# CTOX transport patches

Pinned published sources:
- rtc 0.20.0-alpha.1, workspace commit b808b74f712ed379312a114b848ede133880d58a (path rtc).
- webrtc 0.20.0-alpha.1, commit b899593a5c525e88098ce9f5326fe29b4478832d.

Both declare MIT OR Apache-2.0. webrtc's published licenses are retained.
rtc's MIT notice is taken from the exact workspace commit above; the published
rtc crate omitted its license files. The local dependency links webrtc to this
rtc patch so standalone tests cannot accidentally select a later registry rtc.

CTOX deltas:
- rtc/src/peer_connection/handler/mod.rs: DataChannel writes run through the
  DataChannel and SCTP handlers synchronously. The caller receives handoff errors.
  Media RTP/RTCP and downstream DTLS/ICE pumping retain their existing path.
- rtc/src/peer_connection/handler/sctp.rs: missing associations and non-writable
  streams return errors; missing associations are not reported as size errors.
- webrtc/src/peer_connection/driver.rs: DataChannel message/state event queues
  await capacity without holding the core or channel registry lock. Track media
  queues retain upstream loss behavior; they are not reliable DataChannels.
- webrtc/src/data_channel/mod.rs: coalesce a full WriteNotify queue after SCTP
  acceptance so a successful handoff is not reported as a failed send.
- Unit tests cover synchronous SCTP rejection, queue backpressure and ordered
  delivery of 1024 events through a bounded queue of capacity two.

Remove the patches only when an upstream version provides these same guarantees
and the no-loss/error tests pass. No runtime fetch or dependency service added.

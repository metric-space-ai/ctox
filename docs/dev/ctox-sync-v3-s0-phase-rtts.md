# S0: native scale phase and round-trip accounting

This measurement-only slice builds on #597 and retains its fixture, actual native
artifact, loopback datagram relay and 0/300/600 ms RTT cases. It does not change
replication, scheduling, acknowledgements, authority or runtime configuration.

The existing `measure-sync-v3-native.mjs` release check now records sanitized
metadata at actual RTCDataChannel send/message boundaries from before navigation.
Request IDs bind responses across interleaving. Framed payloads are reconstructed
asynchronously for method/collection correlation; documents, credentials, protocol
payloads and frame contents are not written to the trace. Trace events, concurrent
transfers and reconstruction bytes have hard bounds; overflow fails measurement.

Disjoint navigation phases are boot to shell hook, hook to complete-health barrier,
health to fixture setup, setup to demand query, query to rows returned, and paint.
RTC creation through first open channel is an overlapping subphase, not an extra
cost to add. This retains #597's boundary: offline seed and native process startup
are excluded; the existing full-shell health barrier is included.

Writes are split at the actual request invocation and response: local enqueue,
first masterWrite through conflict, reconciliation through retry, retry through
accepted native ACK. Independent SQLite verification follows the ACK. Each phase
must conserve the original visible-data/ACK elapsed time within 1 ms.

Reports distinguish:

- delay slopes `(phase600-phase0)/600` and `(phase600-phase300)/300`, expressed as
  observed RTT equivalents, not asserted integer protocol handshake counts;
- total completed RPCs versus the longest non-overlapping temporal RPC chain
  (a lower bound only; source await sites identify actual causal dependencies);
- browser outbound frame windows with actual last-chunk-to-ACK wait intervals;
- native inbound ACK-window counts, which are pipelined and never summed as
  sequential RTTs. Native keeps up to 24 four-frame windows open; browser sends
  one four-frame window then waits. Frame ACKs and application ACKs are separate.

One native case per RTT and five writes provide diagnostic evidence, not a
statistical SLO or installed customer acceptance. Timings include observer
metadata/reconstruction overhead. SCTP retransmission/congestion and native send
queue time cannot be attributed to an integer handshake solely from JavaScript
RTC events; unexplained residuals must remain explicit. No WELSCH/THESEN/default
prefix, production command execution or new native baseline build is used.

Run through the normal gpu3 gate using `scripts/sync-v3/run-native-lane.sh` and
its pinned shared archive/Playwright inputs. Bounded JSON, screenshots and owned
process lifecycle evidence are retained outside disposable fixture databases.

/**
 * Runs inside the installed acceptance page through Playwright evaluate.
 * Keep this function closure-free; export only bounded counters and known states.
 */
export function browserTransportSnapshot() {
  const session = globalThis.__installedAcceptance;
  // A watchdog restart replaces the replication state; never inspect only its retired predecessor.
  const peer = (session?.latestReplicationState || session?.state)?.peer;
  if (!peer?.getTransportStatus) return { available: false, reason: 'transport-unavailable' };
  try {
    const raw = peer.getTransportStatus({ includeDiagnostics: true });
    const count = value => Number.isFinite(value) && value >= 0 ? value : null;
    const pick = (value, allowed) => allowed.includes(value) ? value : null;
    const rtcStates = ['new', 'connecting', 'connected', 'disconnected', 'failed', 'closed'];
    const iceStates = ['new', 'checking', 'connected', 'completed', 'disconnected', 'failed', 'closed'];
    const signalingStates = ['stable', 'have-local-offer', 'have-remote-offer',
      'have-local-pranswer', 'have-remote-pranswer', 'closed'];
    const channelStates = ['connecting', 'open', 'closing', 'closed'];
    const candidateTypes = ['host', 'srflx', 'prflx', 'relay'];
    const events = ['created', 'handshake-timeout', 'local-candidates-complete',
      'ice-connection-state', 'ice-gathering-state', 'connection-state',
      'selected-candidate-pair'];
    const expectedConfigured = Boolean(peer.options?.expectedNativePeerId);
    const matches = id => {
      if (!expectedConfigured || typeof peer.peerMatchesExpectedNativePeerId !== 'function') return null;
      return Boolean(peer.peerMatchesExpectedNativePeerId(id, peer.peerMetadata?.get?.(id)));
    };
    let metadataScanned = 0, matchedNativeDescriptors = 0;
    for (const [id] of peer.peerMetadata?.entries?.() || []) {
      if (metadataScanned >= 64) break;
      metadataScanned++;
      if (matches(id)) matchedNativeDescriptors++;
    }
    const candidates = value => Object.fromEntries(candidateTypes.map(key => [key, count(value?.[key])]));
    const signalCounters = ['offerSent', 'offerReceived', 'answerSent', 'answerReceived',
      'candidateSent', 'candidateReceived', 'lastSignalAtMs'];
    const connections = Array.isArray(raw.rtcConnections) ? raw.rtcConnections : [];
    const recent = Array.isArray(raw.recentRtcEvents) ? raw.recentRtcEvents : [];
    return {
      available: true,
      expectedNativeConfigured: expectedConfigured,
      metadataCount: count(peer.peerMetadata?.size),
      metadataScanned, matchedNativeDescriptors,
      activePeerCount: count(raw.activePeerCount),
      pendingRequests: count(raw.pendingRequests),
      pendingAcks: count(raw.pendingAcks),
      connectionsTruncated: connections.length > 8,
      connections: connections.slice(-8).map(value => ({
        expectedNative: matches(value.peerId),
        ageMs: count(value.ageMs),
        connectionState: pick(value.connectionState, rtcStates),
        iceConnectionState: pick(value.iceConnectionState, iceStates),
        iceGatheringState: pick(value.iceGatheringState, ['new', 'gathering', 'complete']),
        signalingState: pick(value.signalingState, signalingStates),
        channelReadyState: pick(value.channelReadyState, channelStates),
        pendingCandidates: count(value.pendingCandidates),
        hasLocalDescription: typeof value.hasLocalDescription === 'boolean' ? value.hasLocalDescription : null,
        hasRemoteDescription: typeof value.hasRemoteDescription === 'boolean' ? value.hasRemoteDescription : null,
        localCandidateTypes: candidates(value.localCandidateTypes),
        remoteCandidateTypes: candidates(value.remoteCandidateTypes),
        signal: {
          ...Object.fromEntries(signalCounters.map(key => [key, count(value.signal?.[key])])),
          localCandidateComplete: typeof value.signal?.localCandidateComplete === 'boolean'
            ? value.signal.localCandidateComplete : null,
          selectedLocalCandidateType: pick(value.signal?.selectedLocalCandidateType, candidateTypes),
          selectedRemoteCandidateType: pick(value.signal?.selectedRemoteCandidateType, candidateTypes),
          selectedCandidateProtocol: pick(value.signal?.selectedCandidateProtocol, ['udp', 'tcp']),
        },
      })),
      recentRtcEvents: recent.slice(-24).filter(value => events.includes(value.event)).map(value => ({
        atMs: count(value.atMs), event: value.event, expectedNative: matches(value.peerId),
        ageMs: count(value.ageMs),
        state: pick(value.state, [...rtcStates, ...iceStates, ...signalingStates, ...channelStates, 'gathering', 'complete']),
        connectionState: pick(value.connectionState, rtcStates),
        iceConnectionState: pick(value.iceConnectionState, iceStates),
        iceGatheringState: pick(value.iceGatheringState, ['new', 'gathering', 'complete']),
        signalingState: pick(value.signalingState, signalingStates),
        localCandidateType: pick(value.localCandidateType, candidateTypes),
        remoteCandidateType: pick(value.remoteCandidateType, candidateTypes),
        protocol: pick(value.protocol, ['udp', 'tcp']),
      })),
    };
  } catch {
    // Native/DOM errors can contain signaling URLs or capability material.
    return { available: false, reason: 'transport-snapshot-failed' };
  }
}

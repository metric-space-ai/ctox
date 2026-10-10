// Interval accounting: counts observed wire barriers without summing concurrent RPCs.
export function unionDuration(intervals, start = -Infinity, end = Infinity) {
  const ranges = intervals.map(item => [Math.max(start, item.startAt), Math.min(end, item.endAt)])
    .filter(([a, b]) => b > a).sort((a, b) => a[0] - b[0]);
  let total = 0, right = -Infinity;
  for (const [a, b] of ranges) { total += Math.max(0, b - Math.max(a, right)); right = Math.max(right, b); }
  return total;
}
export function serialRpcChain(requests) {
  // A temporal lower bound only. An interval ending before another starts is
  // not proof that the application awaited it; source await sites supply that cause.
  const ordered = [...requests].sort((a, b) => a.endAt - b.endAt);
  const chain = []; let end = -Infinity;
  for (const request of ordered) if (request.startAt >= end) { chain.push(request); end = request.endAt; }
  return chain;
}
export function correlate(trace) {
  const events = [...trace.events].sort((a, b) => a.at - b.at), requests = [], windows = [], transfers = [];
  const messages = events.filter(event => event.kind === 'logical');
  for (const request of messages.filter(event => event.direction === 'out' && event.id && event.method)) {
    const response = messages.find(event => event.direction === 'in' && event.id === request.id && !event.method && event.at >= request.at);
    if (response) requests.push({ id: request.id, method: request.method, collection: request.collection,
      streamId: request.streamId, writeMarker: request.writeMarker, startAt: request.startedAt,
      endAt: response.at, responseRows: response.responseRows, resultDocuments: response.resultDocuments,
      requestTransferId: request.transferId, responseTransferId: response.transferId });
  }
  for (const start of events.filter(event => event.kind === 'frame' && event.frameKind === 'start')) {
    const chunks = events.filter(event => event.kind === 'frame' && event.channel === start.channel
      && event.direction === start.direction && event.transferId === start.transferId && event.frameKind === 'chunk'
      && event.attempt === start.attempt && event.at >= start.at);
    const acks = events.filter(event => event.kind === 'frame' && event.channel === start.channel
      && event.direction !== start.direction && event.transferId === start.transferId && event.frameKind === 'ack' && event.at >= start.at);
    const endAt = chunks.at(-1)?.at ?? start.at;
    transfers.push({ id: start.transferId, direction: start.direction, startAt: start.at, endAt,
      frames: start.totalFrames, windowSize: start.windowSize, ackCount: acks.length, encoding: start.encoding });
    if (start.direction === 'out') for (const ack of acks) {
      const chunk = chunks.find(event => event.seq === ack.ackSeq && event.at <= ack.at);
      if (chunk) windows.push({ transferId: start.transferId, ackSeq: ack.ackSeq,
        startAt: chunk.at, endAt: ack.at, final: ack.final });
    }
  }
  return { requests, windows, transfers, logicalMessages: messages };
}
const median = numbers => [...numbers].sort((a, b) => a - b)[Math.floor(numbers.length / 2)];
export function analyzeCase(measurement) {
  const trace = measurement.phaseTrace;
  if (!trace || trace.errors.length || !trace.events.some(event => event.kind === 'channel-open')) throw Error('Incomplete phase trace');
  const correlated = correlate(trace);
  const boundaries = ['boot', 'smoke-hook', 'health-ready', 'fixture-setup', 'fixture-query', 'rows-ready', 'visible'];
  const phases = boundaries.slice(0, -1).map((name, index) => {
    const startAt = trace.marks[name], endAt = trace.marks[boundaries[index + 1]];
    if (!Number.isFinite(startAt) || !Number.isFinite(endAt) || endAt < startAt) throw Error(`Invalid phase ${name}`);
    const requests = correlated.requests.filter(item => item.startAt >= startAt && item.endAt <= endAt);
    const overlappingRequests = correlated.requests.filter(item => item.startAt < endAt && item.endAt > startAt);
    const windows = correlated.windows.filter(item => item.startAt >= startAt && item.endAt <= endAt);
    const transfers = correlated.transfers.filter(item => item.startAt >= startAt && item.endAt <= endAt);
    return { phase: `${name} → ${boundaries[index + 1]}`, startAt, endAt, elapsedMs: endAt - startAt,
      completedRpcCount: requests.length, overlappingRpcCount: overlappingRequests.length,
      rpcActiveUnionMs: unionDuration(overlappingRequests, startAt, endAt),
      temporalSerialRpcLowerBound: serialRpcChain(requests).length,
      temporalChain: serialRpcChain(requests).map(item => ({ method: item.method, collection: item.collection, startAt: item.startAt, endAt: item.endAt })),
      outboundStopAndWaitWindows: windows.length, outboundWindowWaitUnionMs: unionDuration(windows),
      inboundPipelineWindows: transfers.filter(item => item.direction === 'in').reduce((n, item) => n + item.ackCount, 0),
      requests, transfers };
  });
  const rtcCreated = trace.events.find(event => event.kind === 'rtc-created');
  const opened = trace.events.find(event => event.kind === 'channel-open');
  const transport = { phase: 'RTC create → first DataChannel open (nested, do not add to top-level phases)',
    elapsedMs: opened.at - rtcCreated.at, startAt: rtcCreated.at, endAt: opened.at,
    events: trace.events.filter(event => event.at >= rtcCreated.at && event.at <= opened.at && !['frame', 'logical'].includes(event.kind)) };
  const writeSamples = measurement.writes.map(sample => {
    const startAt = trace.marks[`write-${sample.sample}-start`];
    const attempts = sample.attempts;
    if (attempts.length !== 2 || attempts[0].conflicts !== 1 || attempts[1].conflicts !== 0) throw Error('Write phase decomposition requires observed conflict→accepted chain');
    const parts = [
      ['local enqueue → first request', startAt, attempts[0].startAt],
      ['first masterWrite → conflict', attempts[0].startAt, attempts[0].endAt],
      ['conflict reconciliation → retry', attempts[0].endAt, attempts[1].startAt],
      ['retry masterWrite → accepted ACK', attempts[1].startAt, attempts[1].endAt],
    ].map(([phase, startAt, endAt]) => ({ phase, startAt, endAt, elapsedMs: endAt - startAt,
      windows: correlated.windows.filter(item => item.startAt >= startAt && item.endAt <= endAt) }));
    const errorMs = Math.abs(parts.reduce((sum, part) => sum + part.elapsedMs, 0) - sample.nativeAckMs);
    if (errorMs > 1) throw Error('Write phase conservation failed');
    return { sample: sample.sample, nativeAckMs: sample.nativeAckMs, parts,
      requests: correlated.requests.filter(item => item.startAt >= startAt && item.endAt <= attempts[1].endAt + 1), errorMs };
  });
  const writePhases = writeSamples[0].parts.map((part, index) => ({ phase: part.phase,
    medianMs: median(writeSamples.map(sample => sample.parts[index].elapsedMs)),
    minMs: Math.min(...writeSamples.map(sample => sample.parts[index].elapsedMs)),
    maxMs: Math.max(...writeSamples.map(sample => sample.parts[index].elapsedMs)),
    outboundStopAndWaitWindowsMedian: median(writeSamples.map(sample => sample.parts[index].windows.length)) }));
  const visibilityConservationErrorMs = Math.abs(phases.reduce((sum, phase) => sum + phase.elapsedMs, 0) - measurement.coldPageToVisibleMs);
  if (visibilityConservationErrorMs > 1) throw Error('Visibility phase conservation failed');
  return { phases, transport, writeSamples, writePhases, visibilityConservationErrorMs,
    correlated, traceDefinition: 'sanitized observer at actual RTCDataChannel send/message; compressed frame reconstruction is asynchronous and observer-only',
    countingDefinition: 'temporal RPC chain is a lower bound, not a causal assertion; outbound frame windows are observed stop-and-wait barriers; inbound ACK windows are pipelined and never counted as serial RTTs' };
}
export function compareCases(cases) {
  const [zero, three, six] = [0, 300, 600].map(rtt => cases.find(item => item.requestedRttMs === rtt));
  if (![zero, three, six].every(item => item?.phaseAnalysis)) throw Error('Three phase cases required');
  const compare = (kind, key) => zero.phaseAnalysis[kind].map((phase, index) => ({ phase: phase.phase,
    at0Ms: phase[key], at300Ms: three.phaseAnalysis[kind][index][key], at600Ms: six.phaseAnalysis[kind][index][key],
    rttEquivalent0To300: (three.phaseAnalysis[kind][index][key] - phase[key]) / 300,
    rttEquivalent0To600: (six.phaseAnalysis[kind][index][key] - phase[key]) / 600,
    rttEquivalent300To600: (six.phaseAnalysis[kind][index][key] - three.phaseAnalysis[kind][index][key]) / 300 }));
  return { visibility: compare('phases', 'elapsedMs'), writes: compare('writePhases', 'medianMs'),
    transport: { at0Ms: zero.phaseAnalysis.transport.elapsedMs, at300Ms: three.phaseAnalysis.transport.elapsedMs,
      at600Ms: six.phaseAnalysis.transport.elapsedMs,
      rttEquivalent0To600: (six.phaseAnalysis.transport.elapsedMs - zero.phaseAnalysis.transport.elapsedMs) / 600 },
    interpretation: 'delay slopes are observed RTT-equivalents, not integer handshake counts; residual scheduling/SCTP/server work remains explicit, one case per RTT' };
}

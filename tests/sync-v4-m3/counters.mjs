export const CLASSES = ['control', 'command', 'interactive', 'live', 'initial', 'bulk'];
export function quantiles(values) {
  const sorted = values.filter(Number.isFinite).sort((a, b) => a - b);
  const at = p => sorted.length ? sorted[Math.max(0, Math.ceil(sorted.length * p) - 1)] : null;
  return { n: sorted.length, p50: at(.5), p95: at(.95), p99: at(.99), max: sorted.at(-1) ?? null };
}
export function reconcile(sent, received, durationSeconds) {
  const result = {};
  for (const cls of CLASSES) {
    const attempts = sent.filter(e => e.kind === 'attempt' && e.class === cls);
    const success = sent.filter(e => e.kind === 'send_result' && e.class === cls && e.ok);
    const failed = sent.filter(e => e.kind === 'send_result' && e.class === cls && !e.ok);
    const delivered = received.filter(e => e.kind === 'received' && e.class === cls);
    const expected = new Set(success.map(e => e.id)), actual = new Set(delivered.map(e => e.id));
    const acknowledged = new Set(sent.filter(e => e.kind === 'ack' && e.class === cls && !e.error).map(e => e.id));

    result[cls] = { attempted: attempts.length, sendAccepted: success.length, explicitSendFailures: failed.length,
      sendResultMissing: attempts.length - success.length - failed.length, received: delivered.length, receivedUnique: actual.size,
      duplicateDeliveries: delivered.length - actual.size, acceptedButNotReceived: [...expected].filter(id => !actual.has(id)),
      receivedWithoutAcceptedSend: [...actual].filter(id => !expected.has(id)),
      receivedBodyBytes: delivered.reduce((n, e) => n + e.bytes, 0),
      usefulMBps: delivered.reduce((n, e) => n + e.bytes, 0) / 1e6 / durationSeconds,
      reliableCountersEqual: expected.size === actual.size && [...expected].every(id => actual.has(id)) && delivered.length === actual.size,
      sendCompletionTimeouts: sent.filter(e => e.kind === 'send_completion_timeout' && e.class === cls).length,
      acknowledged: acknowledged.size,

      acceptedWithoutAck: [...expected].filter(id => !acknowledged.has(id)),
      ackTimeouts: sent.filter(e => e.kind === 'ack_timeout' && e.class === cls).length,

      ackErrors: sent.filter(e => e.kind === 'ack' && e.class === cls && e.error).length,
      pingMs: quantiles(sent.filter(e => e.kind === 'ack' && e.class === cls && !e.error).map(e => e.latencyMs)) };

  }
  return result;
}

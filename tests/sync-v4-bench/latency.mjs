import { quantiles } from './counters.mjs';

// Derive offer->ACK from raw timestamps, including synchronous queue admission.
// Each list has its own monotonic clock; never subtract across peers/processes.
export function controlLatency(events) {
  const start = events.find(e => e.kind === 'measurement_start')?.at ?? -Infinity;
  const stop = events.find(e => e.kind === 'measurement_stop')?.at ?? Infinity;
  const attempts = new Map(events.filter(e => e.kind === 'attempt' && e.class === 'control'
    && e.at >= start && e.at <= stop).map(e => [e.id, e]));
  const seen = new Set(), samples = [];
  let completedDuringDrain = Number.isFinite(stop) ? 0 : null, errors = 0, invalid = 0;
  for (const ack of events.filter(e => e.kind === 'ack' && e.class === 'control')) {
    const offer = attempts.get(ack.id);
    if (!offer || seen.has(ack.id)) continue;
    seen.add(ack.id);
    if (ack.error) { errors++; continue; }
    const elapsed = ack.at - offer.at;
    if (!Number.isFinite(elapsed) || elapsed < 0) { invalid++; continue; }
    samples.push(elapsed);
    if (ack.at > stop) completedDuringDrain++;
  }
  return { samples, completedDuringDrain, errors, invalid, offered: attempts.size,
    noSuccessfulAck: attempts.size - samples.length, pingMs: quantiles(samples) };
}
export function mergeLatency(rows) {
  const samples = rows.flatMap(r => r.samples);
  return { pingMs: quantiles(samples), offered: rows.reduce((n, r) => n + r.offered, 0),
    noSuccessfulAck: rows.reduce((n, r) => n + r.noSuccessfulAck, 0),
    completedDuringDrain: rows.some(r => r.completedDuringDrain === null) ? null : rows.reduce((n, r) => n + r.completedDuringDrain, 0),
    errors: rows.reduce((n, r) => n + r.errors, 0), invalid: rows.reduce((n, r) => n + r.invalid, 0) };
}

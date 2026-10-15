#!/usr/bin/env node
import fs from 'node:fs';
import path from 'node:path';
import { controlLatency, mergeLatency } from '../../tests/sync-v4-bench/latency.mjs';
const dir = path.resolve(process.argv[2]);
const matrix = JSON.parse(fs.readFileSync(path.join(dir, 'matrix.json'), 'utf8'));
const fmt = value => Number.isFinite(value) ? value.toFixed(2) : '–';
const lines = ['# Transport-Baseline – Messwerte', '',
  `Quelle: ${matrix.sourceHead}. Dauer je Profil: ${matrix.durationSeconds} s. Browserpeers: ${matrix.peers}.`];
for (const profile of matrix.profiles) {
  lines.push('', `## Profil ${profile.rttMs} ms`, '',
    '| Ziel-RTT ms | Richtung | Control n | p50 ms | p95 ms | p99 ms | Bulk MB/s | angen. ohne Empfang | explizit abgelehnt |',
    '|---:|---|---:|---:|---:|---:|---:|---:|---:|');
  const file = path.join(dir, `rtt-${profile.rttMs}`, 'result.json');
  if (!fs.existsSync(file)) { lines.push(`| ${profile.rttMs} | NICHT GEMESSEN | – | – | – | – | – | – | – |`); continue; }
  const result = JSON.parse(fs.readFileSync(file, 'utf8'));
  if (!result.measured || !result.terminal) { lines.push(`| ${profile.rttMs} | FEHLER: ${result.error || 'unvollständig'} | – | – | – | – | – | – | – |`); continue; }
  const m = result.measurements;
  const derived = {
    browserToNative: mergeLatency(result.browsers.map(b => controlLatency(b.events))),
    nativeToBrowser: mergeLatency([controlLatency(result.nativeTx)]),
  };
  fs.writeFileSync(path.join(dir, `rtt-${profile.rttMs}`, 'derived-latency.json'), JSON.stringify({
    schema: 'ctox.transport.offer-to-ack.v1', method: 'offer timestamps; includes queue admission and bounded drain', ...derived,
  }, null, 2));
  for (const [key, label] of [['browserToNative', 'Browser → Native'], ['nativeToBrowser', 'Native → Browser']]) {
    const p = derived[key].pingMs; const counters = Object.values(m[key]);
    lines.push(`| ${profile.rttMs} | ${label} | ${p.n} | ${fmt(p.p50)} | ${fmt(p.p95)} | ${fmt(p.p99)} | ${fmt(m.bulkReceivedUnderLoadMBps[key])} | ${counters.reduce((n, c) => n + c.acceptedButNotReceived.length, 0)} | ${counters.reduce((n, c) => n + c.explicitSendFailures, 0)} |`);
  }
  lines.push('', `Profil ${profile.rttMs}: ICE-RTT p50/p95/p99 ${fmt(m.rttMs.p50)}/${fmt(m.rttMs.p95)}/${fmt(m.rttMs.p99)} ms; Relay durchgehend beobachtet: ${m.relayAlwaysObserved}; Abbrüche je Browser: ${result.injectedDisconnectsPerBrowser}.`, '',
    `Control ohne erfolgreichen ACK B→N/N→B: ${derived.browserToNative.noSuccessfulAck}/${derived.nativeToBrowser.noSuccessfulAck}; erst im Drain abgeschlossen: ${derived.browserToNative.completedDuringDrain}/${derived.nativeToBrowser.completedDuringDrain ?? "unbekannt"}.`, '',
    '| Richtung / Klasse | Versuche | Send angenommen | eindeutig empfangen | fehlt | Duplikate | Send-Ergebnis fehlt | ACK-Timeouts |',
    '|---|---:|---:|---:|---:|---:|---:|---:|');
  for (const [key, label] of [['browserToNative', 'B→N'], ['nativeToBrowser', 'N→B']]) for (const [cls, c] of Object.entries(m[key])) {
    lines.push(`| ${label} ${cls} | ${c.attempted} | ${c.sendAccepted} | ${c.receivedUnique} | ${c.acceptedButNotReceived.length} | ${c.duplicateDeliveries} | ${c.sendResultMissing} | ${c.ackTimeouts} |`);
  }
  lines.push('');
}
lines.push('', 'Pings: Angebot→ACK auf derselben monotonen Uhr, einschließlich synchroner Aufnahme und begrenztem Drain. Fehlende ACKs werden separat gezählt. MB/s sind dezimale Nutzlast-MB im Lastintervall. Kein M3/M4-Erfolg wird aus Exit 0 abgeleitet.');
process.stdout.write(lines.join('\n') + '\n');

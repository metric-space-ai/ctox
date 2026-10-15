#!/usr/bin/env node
import fs from 'node:fs';
import path from 'node:path';
const dir = path.resolve(process.argv[2]);
const matrix = JSON.parse(fs.readFileSync(path.join(dir, 'matrix.json'), 'utf8'));
const fmt = value => Number.isFinite(value) ? value.toFixed(2) : '–';
const lines = ['# Transport-Baseline – Messwerte', '',
  `Quelle: ${matrix.sourceHead}. Dauer je Profil: ${matrix.durationSeconds} s. Browserpeers: ${matrix.peers}.`, '',
  '| Ziel-RTT ms | Richtung | Control n | p50 ms | p95 ms | p99 ms | Bulk MB/s | angen. ohne Empfang | explizit abgelehnt |',
  '|---:|---|---:|---:|---:|---:|---:|---:|---:|'];
for (const profile of matrix.profiles) {
  const file = path.join(dir, `rtt-${profile.rttMs}`, 'result.json');
  if (!fs.existsSync(file)) { lines.push(`| ${profile.rttMs} | NICHT GEMESSEN | – | – | – | – | – | – | – |`); continue; }
  const result = JSON.parse(fs.readFileSync(file, 'utf8'));
  if (!result.measured || !result.terminal) { lines.push(`| ${profile.rttMs} | FEHLER: ${result.error || 'unvollständig'} | – | – | – | – | – | – | – |`); continue; }
  const m = result.measurements;
  for (const [key, label] of [['browserToNative', 'Browser → Native'], ['nativeToBrowser', 'Native → Browser']]) {
    const p = m.controlPingUnderLoadMs[key]; const counters = Object.values(m[key]);
    lines.push(`| ${profile.rttMs} | ${label} | ${p.n} | ${fmt(p.p50)} | ${fmt(p.p95)} | ${fmt(p.p99)} | ${fmt(m.bulkReceivedUnderLoadMBps[key])} | ${counters.reduce((n, c) => n + c.acceptedButNotReceived.length, 0)} | ${counters.reduce((n, c) => n + c.explicitSendFailures, 0)} |`);
  }
  lines.push('', `Profil ${profile.rttMs}: ICE-RTT p50/p95/p99 ${fmt(m.rttMs.p50)}/${fmt(m.rttMs.p95)}/${fmt(m.rttMs.p99)} ms; Relay durchgehend beobachtet: ${m.relayAlwaysObserved}; Abbrüche je Browser: ${result.injectedDisconnectsPerBrowser}.`, '',
    '| Richtung / Klasse | Versuche | Send angenommen | eindeutig empfangen | fehlt | Duplikate | Send-Ergebnis fehlt | ACK-Timeouts |',
    '|---|---:|---:|---:|---:|---:|---:|---:|');
  for (const [key, label] of [['browserToNative', 'B→N'], ['nativeToBrowser', 'N→B']]) for (const [cls, c] of Object.entries(m[key])) {
    lines.push(`| ${label} ${cls} | ${c.attempted} | ${c.sendAccepted} | ${c.receivedUnique} | ${c.acceptedButNotReceived.length} | ${c.duplicateDeliveries} | ${c.sendResultMissing} | ${c.ackTimeouts} |`);
  }
  lines.push('');
}
lines.push('', 'MB/s sind dezimale Nutzlast-MB im Lastintervall. Erfolgsperzentile sind zusammen mit Fehlern und fehlenden Nachrichten zu lesen. Kein M3/M4-Erfolg wird aus Exit 0 abgeleitet.');
process.stdout.write(lines.join('\n') + '\n');

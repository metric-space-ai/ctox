// Test-only SDP/ICE signalling over stdio; application bytes use real WebRTC.
import { createRequire } from 'node:module';
import { createInterface } from 'node:readline';
import path from 'node:path';
const require = createRequire(path.join(process.env.CTOX_DCEP_BROWSER_MODULES, 'package.json'));
const { chromium } = require('playwright');
const browser = await chromium.launch({ headless: true,
  ...(process.env.CTOX_DCEP_CHROME ? { executablePath: process.env.CTOX_DCEP_CHROME } : {}) });
const emit = row => process.stdout.write(JSON.stringify(row) + '\n');
const page = await browser.newPage();
await page.exposeFunction('nativeSignal', emit);
await page.evaluate(() => {
  const pc = window.pc = new RTCPeerConnection({ iceServers: [] });
  window.channels = [];
  pc.onicecandidate = e => { if (e.candidate) window.nativeSignal({ type: 'candidate', candidate: e.candidate.toJSON() }); };
  pc.ondatachannel = e => {
    const channel = e.channel;
    window.channels.push(channel);
    channel.onopen = () => window.nativeSignal({ type: 'open', label: channel.label });
    channel.onmessage = e => channel.send(e.data);
    channel.onerror = () => window.nativeSignal({ type: 'error', message: 'data channel failed' });
  };
});
const lines = createInterface({ input: process.stdin });
const candidates = [];
try {
  for await (const line of lines) {
    const row = JSON.parse(line);
    if (row.type === 'offer') {
      const answer = await page.evaluate(async offer => {
        await pc.setRemoteDescription(offer);
        await pc.setLocalDescription(await pc.createAnswer());
        return pc.localDescription.toJSON();
      }, row.description);
      emit({ type: 'answer', description: answer });
      for (const candidate of candidates.splice(0)) await page.evaluate(c => pc.addIceCandidate(c), candidate);
    } else if (row.type === 'candidate') {
      if (await page.evaluate(() => !!pc.remoteDescription)) await page.evaluate(c => pc.addIceCandidate(c), row.candidate);
      else candidates.push(row.candidate);
    } else if (row.type === 'close') break;
    else throw new Error('unknown fixture signal');
  }
} finally {
  await page.evaluate(() => pc.close()).catch(() => {});
  await browser.close();
}

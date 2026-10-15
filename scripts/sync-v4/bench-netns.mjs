// Root only inside a NEW disposable network namespace. Never changes host qdiscs/routes.
import fs from 'node:fs';
import { execFileSync, spawn } from 'node:child_process';
const file = process.argv[2], config = JSON.parse(fs.readFileSync(file, 'utf8'));
if (process.getuid() !== 0 || fs.readlinkSync('/proc/self/ns/net') === config.hostNetworkNamespace) {
  throw new Error('Refusing network setup outside a distinct root-owned test namespace');
}
const command = (bin, args) => execFileSync(bin, args, { stdio: 'inherit' });
command('ip', ['link', 'set', 'lo', 'up']);
command('ip', ['link', 'add', 'bench0', 'type', 'dummy']);
command('ip', ['addr', 'add', '192.0.2.1/32', 'dev', 'bench0']);
command('ip', ['link', 'set', 'bench0', 'up']);
command('ip', ['route', 'add', 'default', 'dev', 'bench0']);

// Only the browser<->TURN socket is shaped. Native is host-only in this namespace.
command('tc', ['qdisc', 'add', 'dev', 'lo', 'root', 'handle', '1:', 'prio', 'bands', '3']);
command('tc', ['qdisc', 'add', 'dev', 'lo', 'parent', '1:3', 'handle', '30:', 'netem',
  'delay', `${config.rttMs / 2}ms`, 'loss', `${config.lossPercent}%`, 'limit', '10000']);
for (const direction of ['dport', 'sport']) command('tc', ['filter', 'add', 'dev', 'lo', 'parent', '1:', 'protocol', 'ip', 'prio', '1',
  'u32', 'match', 'ip', 'protocol', '17', '0xff', 'match', 'ip', direction, String(config.turnPort), '0xffff', 'flowid', '1:3']);
config.netem = { isolated: true, networkNamespace: fs.readlinkSync('/proc/self/ns/net'),
  requestedRttMs: config.rttMs, perDirectionDelayMs: config.rttMs / 2, perDirectionLossPercent: config.lossPercent,
  shapedSocket: `udp port ${config.turnPort} (browser/TURN)`, hostQdiscChanged: false };
fs.writeFileSync(file, JSON.stringify(config, null, 2));
fs.chownSync(file, config.uid, config.gid);
const child = spawn('setpriv', [`--reuid=${config.uid}`, `--regid=${config.gid}`, '--init-groups', '--reset-env', process.execPath,
  config.runner, file], { stdio: 'inherit', detached: true });
const terminate = signal => { try { process.kill(-child.pid, signal); } catch {} };
const timer = setTimeout(() => { terminate('SIGTERM'); setTimeout(() => terminate('SIGKILL'), 5000).unref(); }, (config.durationSeconds + 150) * 1000);
process.on('SIGTERM', () => terminate('SIGTERM'));
child.on('exit', (code, signal) => { clearTimeout(timer); terminate('SIGKILL'); process.exitCode = code ?? (signal ? 143 : 1); });

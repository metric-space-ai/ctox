#!/usr/bin/env python3
"""Real proxy traffic must reject foreign names before sending DNS queries."""
import json
from pathlib import Path
import subprocess
import sys

out = Path(sys.argv[1])
out.mkdir(parents=True, exist_ok=True)
with (out / 'cargo.log').open('w') as log:
    result = subprocess.run(
        ['cargo', 'test', '-p', 'ctox-process-network', '--test', 'egress',
         '--no-run', '--message-format=json'],
        check=True, text=True, stdout=subprocess.PIPE, stderr=log,
    )
artifacts = [json.loads(line) for line in result.stdout.splitlines()]
executable = next(a['executable'] for a in artifacts
                  if a.get('reason') == 'compiler-artifact'
                  and a.get('target', {}).get('name') == 'egress'
                  and a.get('executable'))
trace = out / 'network.trace'
traced = subprocess.run(
    ['strace', '-f', '-xx', '-s', '8192', '-e', 'trace=network', '-o', str(trace),
     executable, '--exact', 'e20_net_domain_escape', '--nocapture'], check=False,
)
# DNS wire labels contain lengths, unlike the HTTP request's dotted host.
# The test includes a per-process nonce beneath example.org, defeating caches.
wire = ''.join('\\x%02x' % byte for byte in b'\x07example\x03org')
assert wire not in trace.read_text(), 'E20-NET DNS ESCAPE: foreign name was resolved'
assert traced.returncode == 0, 'E20-NET traffic invariant failed under trace'
print('E20-NET foreign DNS rejected before resolution')

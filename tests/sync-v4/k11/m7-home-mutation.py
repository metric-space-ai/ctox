#!/usr/bin/env python3
"""Causal real-namespace proof: omit the private passwd bind, reject before CTOX."""
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys


def main():
    assert os.getenv('BUILD_LANE_BIN')
    output = Path(sys.argv[1]).resolve()
    allowed = (Path(os.environ['BUILD_LANE_BIN']) / '../artifacts').resolve()
    assert allowed in output.parents
    output.mkdir(mode=0o700)
    source = Path(__file__).with_name('m7-manager-probe.py').read_text()
    needle = "run('mount', '--bind', str(file), '/etc/' + name)"
    assert source.count(needle) == 1
    helper = output / 'probe.py'
    def run(label, code):
        helper.write_text(code)
        p = subprocess.run([sys.executable, str(helper), str(output / label)],
                           capture_output=True, text=True, timeout=40)
        (output / (label + '.log')).write_text(p.stdout + p.stderr)
        return p
    before = run('before', source)
    mutated = run('mutated', source.replace(needle, "if name != 'passwd': " + needle))
    restored = run('restored', source)
    passed = before.returncode == restored.returncode == 0 and mutated.returncode != 0 and 'Private NSS home does not match owned profile' in mutated.stderr
    config = json.loads((output / 'mutated/config.json').read_text())
    removed = not Path(config['privateCgroup']).exists()
    result = {'mutation': 'omit only the private /etc/passwd bind',
              'sourceSha256': hashlib.sha256(source.encode()).hexdigest(),
              'before': before.returncode, 'mutated': mutated.returncode,
              'restored': restored.returncode, 'failedBeforeCtoxStart': 'Private NSS home does not match owned profile' in mutated.stderr,
              'mutatedCgroupRemoved': removed, 'pass': passed and removed}
    (output / 'result.json').write_text(json.dumps(result, indent=2))
    print(json.dumps(result))
    return 0 if result['pass'] else 1


if __name__ == '__main__':
    raise SystemExit(main())

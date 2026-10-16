#!/usr/bin/env python3
"""E29 custody: public process configuration/open DBs, SQLite-API snapshots.

No live DB byte hash, environment dump, secret read or raw DB copy. Invoke while
the private V3 owner is alive, then quiesce it before the separate snapshot.
"""
import argparse
import json
import os
from pathlib import Path
import time

OWNER = '01a087a0-169a-7e23-ba3d-71352256cbfb'


def process_identity(pid, root):
    root = root.resolve()
    marker = json.loads((root/'.sync-v4-e2e.json').read_text())
    assert marker.get('owner') == OWNER and marker.get('synthetic') is True
    proc = Path('/proc')/str(pid)
    stat = (proc/'stat').read_text().rsplit(')', 1)[1].split()
    start = stat[19]  # field 22, after pid and parenthesized comm
    allowed = {'CTOX_ROOT', 'CTOX_STATE_ROOT', 'CTOX_INSTALL_ROOT'}
    config = {}
    for entry in (proc/'environ').read_bytes().split(b'\0'):
        name, sep, value = entry.partition(b'=')
        if sep and name.decode(errors='replace') in allowed:
            config[name.decode()] = value.decode()
    cwd = (proc/'cwd').resolve()
    roots = [Path(value).resolve() for value in config.values()]
    assert root == cwd or root in cwd.parents or root in roots, 'PID is not this private owner'
    opened = []
    for fd in (proc/'fd').iterdir():
        try:
            value = fd.resolve(strict=True)
        except FileNotFoundError:
            continue
        if value.suffix in ('.sqlite3', '.sqlite', '.db'):
            assert root in value.parents, 'Unexpected database outside owned profile'
            opened.append({'fd': int(fd.name), 'path': str(value)})
    assert (proc/'stat').read_text().rsplit(')', 1)[1].split()[19] == start, 'PID reused'
    return dict(pid=pid, startTicks=start, cwd=str(cwd), config=config,
                databaseFiles=sorted(opened, key=lambda x: x['fd']),
                measuredAtMs=int(time.time()*1000), method='/proc/PID/fd + allowlisted process configuration')


if __name__ == '__main__':
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--pid', type=int, required=True)
    p.add_argument('--root', type=Path, required=True)
    p.add_argument('--output', type=Path, required=True)
    a = p.parse_args()
    assert os.environ.get('BUILD_LANE_BIN') and os.uname().sysname == 'Linux'
    base = (Path(os.environ['BUILD_LANE_BIN'])/'../artifacts').resolve()
    assert base in a.root.resolve().parents and base in a.output.resolve().parents
    result = process_identity(a.pid, a.root)
    a.output.write_text(json.dumps(result, indent=2)+'\n')
    print(json.dumps(result))

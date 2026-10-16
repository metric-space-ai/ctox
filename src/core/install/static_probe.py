#!/usr/bin/env python3
"""Private real-daemon static/bootstrap diagnosis; never touches host services."""
import argparse
import concurrent.futures
import hashlib
import http.client
import json
import os
from pathlib import Path
import shutil
import signal
import socket
import sqlite3
import subprocess
import time
from datetime import datetime, timezone

OWNER = '01a087a0-169a-7e23-ba3d-71352256cbfb'


def stamp():
    return datetime.now(timezone.utc).isoformat()


def main():
    def interrupted(signum, _frame):
        raise RuntimeError('Owned probe interrupted by signal ' + str(signum))
    signal.signal(signal.SIGTERM, interrupted)
    parser = argparse.ArgumentParser()
    for name in ['profile', 'binary', 'sha256', 'source', 'slot', 'out']:
        parser.add_argument('--' + name, required=True)
    parser.add_argument('--mode', choices=['gdb', 'strace', 'plain'], default='gdb')
    parser.add_argument('--interrupt-ms', type=int, default=5000)
    args = parser.parse_args()
    assert os.environ.get('BUILD_LANE_BIN'), 'Canonical Linux lane required'
    source_profile = Path(args.profile).resolve()
    marker = json.loads((source_profile / '.sync-v4-e2e.json').read_text())
    assert marker['owner'] == OWNER and marker['synthetic'] is True
    binary = Path(args.binary).resolve()
    digest = hashlib.sha256()
    with binary.open('rb') as image:
        for chunk in iter(lambda: image.read(1024 * 1024), b''):
            digest.update(chunk)
    assert digest.hexdigest() == args.sha256
    out = Path(args.out).resolve()
    lane_artifacts = (Path(os.environ['BUILD_LANE_BIN']) / '../artifacts').resolve()
    assert out.is_relative_to(lane_artifacts) and not out.exists()
    out.mkdir(mode=0o700, parents=True)
    root = out / 'profile'
    root.mkdir()
    (root / '.sync-v4-e2e.json').write_text(json.dumps(marker))
    runtime = root / 'runtime'
    runtime.mkdir()
    # SQLite API snapshots even for this terminal owned fixture. No raw DB copy.
    for item in (source_profile / 'runtime').iterdir():
        if item.name.endswith(('-wal', '-shm', '-journal')):
            continue
        dest = runtime / item.name
        if item.name.endswith(('.sqlite3', '.sqlite', '.db')):
            with sqlite3.connect('file:' + str(item) + '?mode=ro', uri=True) as src:
                with sqlite3.connect(dest) as dst:
                    src.backup(dst)
        elif item.is_dir() and item.name != 'home':
            shutil.copytree(item, dest, symlinks=True)
        elif item.is_file():
            shutil.copy2(item, dest)
    home = runtime / 'home'
    home.mkdir(exist_ok=True)
    for name in ['src', 'contracts', 'Cargo.toml']:
        (root / name).symlink_to(Path(args.source).resolve() / name)
    (root / 'bin').mkdir()
    (root / 'bin/ctox').symlink_to(binary)
    slot = Path(args.slot).resolve()
    dest = runtime / 'business-os-shell/slots' / slot.name
    dest.parent.mkdir(parents=True, exist_ok=True)
    shutil.copytree(slot, dest)
    with sqlite3.connect(runtime / 'business-os.sqlite3') as db:
        db.execute('CREATE TABLE IF NOT EXISTS business_os_shell_update_state(singleton INTEGER PRIMARY KEY CHECK(singleton=1), state_json TEXT NOT NULL, updated_at TEXT NOT NULL)')
        state = json.loads((slot.parent / 'fixture-state.json').read_text())
        db.execute("INSERT OR REPLACE INTO business_os_shell_update_state VALUES(1,?,datetime('now'))", (json.dumps(state),))
    sockets = [socket.socket(), socket.socket()]
    for sock in sockets:
        sock.bind(('127.0.0.1', 0))
    port, signaling_port = [sock.getsockname()[1] for sock in sockets]
    for sock in sockets:
        sock.close()
    env = dict(os.environ, HOME=str(home), CTOX_ROOT=str(root), CTOX_STATE_ROOT=str(runtime),
               CTOX_INSTALL_ROOT=str(root), CTOX_BUSINESS_OS_SIGNALING_URLS='ws://127.0.0.1:' + str(signaling_port),
               CTOX_BUSINESS_OS_ENABLE_SMOKE_CONTROLS='1',
               CTOX_WEBRTC_UDP_BIND_ADDR='127.0.0.1:0', CTOX_BUSINESS_OS_DISABLE_BACKGROUND_FILE_INDEX='1')
    command = [str(binary), 'business-os', 'serve', '--addr', '127.0.0.1:' + str(port)]
    if args.mode == 'gdb':
        command = ['/usr/bin/gdb', '-q', '-batch', '-ex', 'set pagination off', '-ex', 'set confirm off',
                   '-ex', 'set print frame-arguments none', '-ex', 'handle SIGPIPE nostop noprint pass',
                   '-ex', 'run', '-ex', 'thread apply all bt 45', '-ex', 'quit', '--args'] + command
    elif args.mode == 'strace':
        command = ['/usr/bin/strace', '-f', '-tt', '-T', '-s', '0', '-e', 'trace=network,read,write,openat,futex',
                   '-o', str(out / 'syscalls.log')] + command
    result = dict(owner=OWNER, startedAt=stamp(), mode=args.mode, binarySha256=args.sha256,
                  profileSource=str(source_profile), root=str(root), snapshots='SQLite backup API', rows=[], pass_=False)
    result['pass'] = False
    child = None
    signaller = None
    inferior_pids = []
    def save():
        (out / 'result.json').write_text(json.dumps(result, indent=2))
    def request(path, timeout=60):
        row = dict(path=path, startedAt=stamp(), headersMs=None, bodyMs=None, bytes=0)
        started = time.monotonic()
        connection = http.client.HTTPConnection('127.0.0.1', port, timeout=timeout)
        try:
            connection.request('GET', path, headers={'Accept-Encoding': 'gzip', 'Connection': 'close'})
            response = connection.getresponse()
            row.update(status=response.status, headersMs=(time.monotonic()-started)*1000)
            row['bytes'] = len(response.read())
            row['bodyMs'] = (time.monotonic()-started)*1000
        except Exception as error:
            row['error'] = str(error)
        finally:
            connection.close()
            row['totalMs'] = (time.monotonic()-started)*1000
        return row
    try:
        with (out / 'signaling.log').open('w') as signaling_log:
            signaller = subprocess.Popen(['node', str(Path(args.source) / 'src/core/rxdb/tools/local_signaling_server.js'), str(signaling_port)],
                                        cwd=root, env=dict(env, SIGNALING_HOST='127.0.0.1', SIGNALING_PORT=str(signaling_port)),
                                        stdout=signaling_log, stderr=signaling_log, start_new_session=True)
        result['signaling'] = dict(pid=signaller.pid, pgid=signaller.pid, stop='probe finally', url='loopback synthetic only')
        with (out / 'daemon.log').open('w') as log:
            child = subprocess.Popen(command, cwd=root, env=env, stdout=log, stderr=log, start_new_session=True)
            result.update(pid=child.pid, pgid=child.pid, stop='finally or 180-second lane timeout')
            save()
            deadline = time.monotonic()+45
            while True:
                try:
                    with socket.create_connection(('127.0.0.1', port), timeout=.2):
                        break
                except OSError:
                    if time.monotonic() > deadline or child.poll() is not None:
                        raise RuntimeError('Owned static listener did not start')
                    time.sleep(.1)
            result['assetBefore'] = request('/app.js?v=m7-phase', 5)
            save()
            if args.mode == 'gdb':
                with concurrent.futures.ThreadPoolExecutor(max_workers=1) as pool:
                    future = pool.submit(request, '/', 12)
                    try:
                        row = future.result(timeout=args.interrupt_ms / 1000)
                    except concurrent.futures.TimeoutError:
                        result['interruptedAt'] = stamp()
                        children = Path('/proc') / str(child.pid) / 'task' / str(child.pid) / 'children'
                        inferior_pids = [int(pid) for pid in children.read_text().split()]
                        result['inferiorPids'] = inferior_pids
                        os.killpg(child.pid, signal.SIGINT)
                        row = future.result(timeout=15)
                    result['rows'].append(row)
                try:
                    child.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    pass
            else:
                for _ in range(20 if args.mode == 'plain' else 1):
                    row = request('/')
                    result['rows'].append(row)
                    save()
                    if 'error' in row:
                        break
                result['assetAfter'] = request('/app.js?v=m7-phase', 5)
            rows = result['rows']
            result['p99Ms'] = max(r['bodyMs'] for r in rows) if len(rows) == 20 and all(r['bodyMs'] is not None for r in rows) else None
            result['pass'] = len(rows) == 20 and all(r.get('status') == 200 and r['bodyMs'] is not None and r['bodyMs'] < 500 for r in rows)
    except Exception as error:
        result['error'] = str(error)
    finally:
        if child:
            for sig in [signal.SIGTERM, signal.SIGKILL]:
                try:
                    os.killpg(child.pid, sig)
                except ProcessLookupError:
                    pass
                try:
                    child.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    continue
                break
            result['exit'] = child.poll()
            try:
                os.killpg(child.pid, 0)
                result['groupAbsent'] = False
            except ProcessLookupError:
                result['groupAbsent'] = True
            result['inferiorsAbsent'] = all(not Path('/proc', str(pid)).exists() for pid in inferior_pids)
        if signaller:
            try:
                os.killpg(signaller.pid, signal.SIGTERM)
            except ProcessLookupError:
                pass
            try:
                signaller.wait(timeout=5)
            except subprocess.TimeoutExpired:
                os.killpg(signaller.pid, signal.SIGKILL)
                signaller.wait()
            try:
                os.killpg(signaller.pid, 0)
                result['signaling']['groupAbsent'] = False
            except ProcessLookupError:
                result['signaling']['groupAbsent'] = True
        result['finishedAt'] = stamp()
        result.pop('pass_', None)
        save()
    print(json.dumps({k: v for k, v in result.items() if k != 'rows'}))
    clean = result.get('groupAbsent') and result.get('inferiorsAbsent') and result.get('signaling', {}).get('groupAbsent')
    passed = args.mode != 'plain' or result.get('pass')
    return 0 if clean and passed and 'error' not in result else 1


if __name__ == '__main__':
    raise SystemExit(main())

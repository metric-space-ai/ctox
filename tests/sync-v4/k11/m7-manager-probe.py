#!/usr/bin/env python3
"""Try a real user manager with no writable host cgroup or visible host bus.

Run only through the admitted lane. The process/PID namespace is the lifetime
boundary; no replacement systemctl and no success inferred from exit suppression.
"""
import json
import os
import pwd
from pathlib import Path
import signal
import select
import subprocess
import sys
import time


def setup(config, delegated=False):
    c = json.loads(Path(config).read_text())
    assert os.getuid() == 0 and (c['uid'] > 0 or c.get('mappedUser'))
    for kind, old in c['host'].items():
        if kind == 'cgroup' or (kind == 'user' and not c.get('mappedUser')):
            continue
        assert os.readlink('/proc/self/ns/' + kind) != old
    run = lambda *args: subprocess.run(args, check=True)
    run('mount', '--make-rprivate', '/')
    if not delegated:
        # A child of THIS admitted systemd scope only. The lane's CPU/memory
        # ancestor limits still apply. Never mount a writable host cgroup root.
        current = Path('/proc/self/cgroup').read_text().strip().split('::', 1)[1]
        assert current.endswith('.scope') and '/app.slice/run-' in current
        parent = Path('/sys/fs/cgroup') / current.lstrip('/')
        owned = parent / ('m7-manager-' + str(c['uid']) + '-' + str(os.getpid()))
        owned.mkdir()
        (owned / 'cgroup.procs').write_text('0')
        for path in [owned, owned / 'cgroup.procs', owned / 'cgroup.subtree_control', owned / 'cgroup.threads']:
            os.chown(path, c['uid'], c['gid'])
        c['privateCgroup'] = str(owned)
        Path(config).write_text(json.dumps(c))
        try:
            run('unshare', '--cgroup', '--mount', sys.executable, __file__, '--delegated', config)
        finally:
            (parent / 'cgroup.procs').write_text('0')
            # Only empty child groups below our exact newly-created root.
            for path in sorted(owned.rglob('*'), key=lambda p: len(p.parts), reverse=True):
                if path.is_dir():
                    path.rmdir()
            owned.rmdir()
            receipt = Path(c['output']) / 'service-manager.json'
            if receipt.exists():
                result = json.loads(receipt.read_text())
                result['privateCgroupRemoved'] = not owned.exists()
                receipt.write_text(json.dumps(result, indent=2))
        return
    run('mount', '-t', 'tmpfs', '-o', 'mode=755,size=32m', 'tmpfs', '/run')
    run('mount', '-t', 'tmpfs', '-o', 'mode=1777,size=256m', 'tmpfs', '/dev/shm')
    run('mount', '--bind', c['privateCgroup'], '/sys/fs/cgroup')
    run('ip', 'link', 'set', 'lo', 'up')
    assert Path('/proc/self/cgroup').read_text().strip() == '0::/'
    runtime = Path('/run/user') / str(c['uid'])
    runtime.mkdir(parents=True, mode=0o700)
    runtime.chmod(0o700)
    os.chown(runtime, c['uid'], c['gid'])
    Path('/run/systemd/system').mkdir(parents=True)
    out = Path(c['output'])
    home = Path(c['root']) / 'installer-home' if c.get('runner') else out
    home.mkdir(mode=0o700, exist_ok=True)
    os.chown(home, c['uid'], c['gid'])
    # HOME alone does not change getpwuid(). Keep libc, systemd specifiers and
    # the fixture process on the same private identity. No host passwd/group
    # contents or supplemental host privileges are copied into this namespace.
    nss = out / 'nss'
    nss.mkdir(mode=0o700)
    entries = {
        'passwd': (f'root:x:0:0:M7 fixture:{home}:/bin/sh\n' if c.get('mappedUser') else
                   'root:x:0:0:root:/nonexistent-m7-root:/usr/sbin/nologin\n'
                   + f"m7-fixture:x:{c['uid']}:{c['gid']}:M7 fixture:{home}:/bin/sh\n"),
        'group': 'root:x:0:\n' + ('' if c.get('mappedUser') else f"m7-fixture:x:{c['gid']}:m7-fixture\n"),
        'nsswitch.conf': 'passwd: files\ngroup: files\nshadow: files\nhosts: files dns\nnetworks: files\nservices: files\nprotocols: files\n',
    }
    if c.get('dressSmoke'):
        entries['resolv.conf'] = 'nameserver 10.0.2.3\n'
    for name, content in entries.items():
        file = nss / name
        file.write_text(content)
        file.chmod(0o644)
        target = Path('/etc') / name
        if name == 'resolv.conf' and not target.exists():
            resolved = target.resolve()
            assert Path('/run') in resolved.parents, 'Only private masked-run resolver target may be created'
            resolved.parent.mkdir(parents=True, exist_ok=True)
            resolved.touch()
        run('mount', '--bind', str(file), str(target))
    if pwd.getpwuid(c['uid']).pw_dir != str(home):
        raise RuntimeError('Private NSS home does not match owned profile')
    if c.get('dressSmoke'):
        ack = os.open(out / 'network-ack', os.O_RDWR | os.O_NONBLOCK)
        try:
            (out / 'network-request.json').write_text(json.dumps({'privateNet': True}))
            if not select.select([ack], [], [], 30)[0] or os.read(ack, 1) != b'1':
                raise RuntimeError('Private rootless network not ready')
        finally:
            os.close(ack)
    env = {'PATH': c.get('path', '/usr/bin:/bin'), 'HOME': str(home),
           'XDG_RUNTIME_DIR': str(runtime), 'DBUS_SESSION_BUS_ADDRESS': 'unix:path=' + str(runtime / 'bus'),
           'XDG_CONFIG_HOME': str(home / '.config'), 'XDG_CACHE_HOME': str(home / '.cache'),
           'TMPDIR': c.get('tmpdir', '/run'), 'BUILD_LANE_BIN': c.get('laneBin', ''),
           'LANG': 'C.UTF-8'}
    prefix = [] if c.get('mappedUser') else ['setpriv', '--reuid=' + str(c['uid']), '--regid=' + str(c['gid']), '--init-groups']
    children = []
    def spawn(name, command):
        with (out / (name + '.log')).open('w') as log:
            p = subprocess.Popen(prefix + command, env=env, stdout=log, stderr=log)
        children.append(p)
        return p
    result = {'owner': c['owner'], 'startedAtMs': int(time.time() * 1000), 'privateCgroup': c['privateCgroup'], 'hostCgroupHidden': True, 'hostBusHidden': True, 'pass': False}
    try:
        bus = spawn('dbus', ['dbus-daemon', '--session', '--address=' + env['DBUS_SESSION_BUS_ADDRESS'], '--nofork'])
        manager = spawn('manager', ['/usr/lib/systemd/systemd', '--user', '--unit=basic.target'])
        time.sleep(2)
        check = subprocess.run(prefix + ['systemctl', '--user', 'show', '-p', 'Version'], env=env, capture_output=True, text=True, timeout=5)
        result.update(managerExit=manager.poll(), busExit=bus.poll(), checkExit=check.returncode,
                      stdout=check.stdout, stderr=check.stderr, pass_=check.returncode == 0)
        result['pass'] = check.returncode == 0
        unit_dir = home / '.config/systemd/user'
        unit_dir.mkdir(parents=True, exist_ok=True, mode=0o700)
        for path in [home / '.config', home / '.config/systemd', unit_dir]:
            os.chown(path, c['uid'], c['gid'])
        (unit_dir / 'm7-home-probe.service').write_text('[Service]\nType=oneshot\nWorkingDirectory=%h\nExecStart=/usr/bin/true\n')
        subprocess.run(prefix + ['systemctl', '--user', 'daemon-reload'], env=env, capture_output=True, timeout=5, check=True)
        resolved = subprocess.run(prefix + ['systemctl', '--user', 'show', 'm7-home-probe.service', '--property=WorkingDirectory', '--value'], env=env, capture_output=True, text=True, timeout=5, check=True)
        result['homeAuthority'] = {'environment': str(home), 'nss': pwd.getpwuid(c['uid']).pw_dir, 'unitSpecifier': resolved.stdout.strip()}
        if any(value != str(home) for value in result['homeAuthority'].values()):
            result['pass'] = False
            raise RuntimeError('Private home authorities diverged')
        if c.get('runner'):
            if not result['pass']:
                raise RuntimeError('Real isolated user manager is unavailable')
            c['proof'] = {'host': c['host'], 'runtimeMasked': True, 'mappedUser': c.get('mappedUser', False),
                          'namespaces': {n: os.readlink('/proc/self/ns/' + n) for n in ('net', 'mnt', 'pid', 'cgroup')},
                          'serviceManager': {'runtime': str(runtime), 'bus': env['DBUS_SESSION_BUS_ADDRESS'],
                                             'privateCgroup': c['privateCgroup'], 'managerPid': manager.pid}}
            Path(config).write_text(json.dumps(c))
            os.chown(config, c['uid'], c['gid'])
            child = spawn('installer', [c['node'], c['runner'], config])
            result['installerExit'] = child.wait(timeout=540)
            result['pass'] = result['installerExit'] == 0
    finally:
        if 'manager' in locals() and manager.poll() is None:
            try:
                stopped = subprocess.run(prefix + ['systemctl', '--user', 'stop', 'ctox-watchdog.timer', 'ctox-watchdog.service', 'ctox.service'], env=env, capture_output=True, text=True, timeout=20)
                result['stopExit'] = stopped.returncode
            except subprocess.TimeoutExpired:
                result['stopTimedOut'] = True
                result['pass'] = False
        for p in reversed(children):
            if p.poll() is None:
                p.terminate()
                try:
                    p.wait(timeout=3)
                except subprocess.TimeoutExpired:
                    p.kill()
                    p.wait()
        result['finishedAtMs'] = int(time.time() * 1000)
        result['childrenTerminal'] = all(p.poll() is not None for p in children)
        (out / 'service-manager.json').write_text(json.dumps(result, indent=2))
    if not result['pass']:
        raise RuntimeError('Private manager/installer did not complete; see service-manager.json')


def main():
    assert os.name == 'posix' and os.getenv('BUILD_LANE_BIN')
    output = Path(sys.argv[1]).resolve()
    allowed = (Path(os.environ['BUILD_LANE_BIN']) / '../artifacts').resolve()
    assert allowed in output.parents
    output.mkdir(mode=0o700)
    config = output / 'config.json'
    config.write_text(json.dumps({'owner': '01a087a0-169a', 'uid': os.getuid(), 'gid': os.getgid(),
                                 'output': str(output), 'host': {n: os.readlink('/proc/self/ns/' + n) for n in ('pid', 'net', 'mnt')}}))
    p = subprocess.run(['sudo', '-n', 'unshare', '--net', '--mount', '--pid', '--fork', '--mount-proc', sys.executable, __file__, '--setup', str(config)], timeout=30)
    assert p.returncode == 0
    result = json.loads((output / 'service-manager.json').read_text())
    print(json.dumps(result))
    return 0 if result['pass'] else 2


if __name__ == '__main__':
    if len(sys.argv) > 1 and sys.argv[1] == '--delegated':
        setup(sys.argv[2], True)
    elif len(sys.argv) > 1 and sys.argv[1] == '--setup':
        setup(sys.argv[2])
    else:
        sys.exit(main())

#!/usr/bin/env python3
"""Bounded preflight of an actually installed private K24 target.

This does not invent voters, users, grants, policy or a fake browser transport.
Only a usable real target can be handed to Shell; otherwise all children retire.
"""
import argparse
import http.client
import json
import os
from pathlib import Path
import signal
import socket
import subprocess
import time
import importlib.util


def main(a):
    base = (Path(os.environ['BUILD_LANE_BIN'])/'../artifacts').resolve()
    root, out = a.root.resolve(), a.output.resolve()
    assert base in root.parents and base in out.parents and not out.exists()
    marker = json.loads((root/'.sync-v4-e2e.json').read_text())
    assert marker.get('owner') == '01a087a0-169a-7e23-ba3d-71352256cbfb' and marker.get('synthetic') is True
    current = (root/'managed/current').resolve(strict=True)
    binary = current/'bin/ctox'
    assert root in binary.resolve().parents
    out.mkdir(mode=0o700)
    spec = importlib.util.spec_from_file_location('adapter', Path(__file__).with_name('core-adapter.py'))
    adapter = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(adapter)
    release = json.loads(a.release.read_text())
    assert adapter.digest(binary) == release['binarySha256']
    home = root/'installer-home'
    env = dict(PATH=os.environ['PATH'],HOME=str(home),TMPDIR=os.environ['TMPDIR'],LANG='C.UTF-8',
               CTOX_ROOT=str(current),CTOX_STATE_ROOT=str(root/'runtime'),CTOX_INSTALL_ROOT=str(root/'managed'),
               XDG_CONFIG_HOME=str(home/'.config'),XDG_CACHE_HOME=str(home/'.cache'),
               XDG_RUNTIME_DIR=str(out/'run'))
    Path(env['XDG_RUNTIME_DIR']).mkdir(mode=0o700)
    public = root/'k24-target.public.json'
    public.write_text(json.dumps(dict(revision=release['revision'],binarySha256=release['binarySha256'],
                                     root=str(root),purpose='isolated K24 preflight; no LLM credentials')))
    result = dict(owner=marker['owner'],synthetic=True,startedAtMs=int(time.time()*1000),
                  processes=[],targetReady=False,criterion8='not_run(T4-welsch)',terminal=False,
                  adminUrl=None,nonAdminUrl=None)
    children = []
    def spawn(name, args):
        with (out/(name+'.log')).open('w') as log:
            p = subprocess.Popen([str(x) for x in args],cwd=current,env=env,stdout=log,stderr=log,start_new_session=True)
        children.append(p)
        row = dict(name=name,pid=p.pid,pgid=p.pid,stopAfterSeconds=60)
        result['processes'].append(row)
        return p
    try:
        native = spawn('foreground',[binary,'service','--foreground','--root',current])
        until = time.monotonic()+15
        while time.monotonic()<until and native.poll() is None:
            if 'ctox v4 store daemon ready:' in (out/'foreground.log').read_text(): break
            time.sleep(.1)
        result['nativeReady'] = native.poll() is None and 'ctox v4 store daemon ready:' in (out/'foreground.log').read_text()
        if not result['nativeReady']: raise RuntimeError('Installed native owner did not become ready')
        result['fingerprintBefore'] = adapter.fingerprint(root,native.pid,[public,root/'managed/install_manifest.json'])
        with socket.socket() as sock:
            sock.bind(('127.0.0.1',0)); port = sock.getsockname()[1]
        web = spawn('http',[binary,'business-os','serve','--addr','127.0.0.1:'+str(port),'--root',current])
        until = time.monotonic()+15
        while time.monotonic()<until and web.poll() is None:
            try:
                with socket.create_connection(('127.0.0.1',port),timeout=.1): break
            except OSError: time.sleep(.1)
        at = time.monotonic()
        connection = http.client.HTTPConnection('127.0.0.1',port,timeout=5)
        try:
            connection.request('GET','/')
            response = connection.getresponse()
            result['httpRoot'] = dict(status=response.status,durationMs=(time.monotonic()-at)*1000)
            response.read()
        except Exception as error:
            result['httpRoot'] = dict(error=str(error),durationMs=(time.monotonic()-at)*1000)
        finally: connection.close()
        result['v4ShellSourcePresent'] = (current/'src/apps/business-os/v4/boot/boot.js').is_file()
        result['coreRead'] = dict(status='not_run',owner='phase1 / Architecture')
        os.environ['K24_QUERY'] = 'core-view'
        try:
            result['coreRead'] = dict(status='pass',value=adapter.query(root/'runtime/business-os-rxdb.sqlite3'))
        except Exception as error:
            result['coreRead']['reason'] = str(error)
        result['fingerprintAfter'] = adapter.fingerprint(root,native.pid,[public,root/'managed/install_manifest.json'])
        result['noNativeRestartOrConfigChange'] = result['fingerprintBefore']==result['fingerprintAfter']
        result['blockers'] = [dict(owner='Architecture CUTOVER-6',reason='No native-backed non-admin browser principal binding supplied; never create a role via URL parameters'),
                              dict(owner='Shell v4 / Kernel',reason='A real V4 shell and native browser transport must bind this profile; V3 HTTP bootstrap is not K24 acceptance')]
    except Exception as error:
        result['error'] = str(error)
    finally:
        for p in reversed(children):
            try: os.killpg(p.pid,signal.SIGTERM)
            except ProcessLookupError: pass
            try: p.wait(timeout=5)
            except subprocess.TimeoutExpired:
                os.killpg(p.pid,signal.SIGKILL);p.wait()
            try: os.killpg(p.pid,signal.SIGKILL)
            except ProcessLookupError: pass
            row = next(r for r in result['processes'] if r['pid']==p.pid)
            row['exit'] = p.returncode
            try: os.killpg(p.pid,0);row['groupAbsent']=False
            except ProcessLookupError: row['groupAbsent']=True
        result.update(terminal=True,finishedAtMs=int(time.time()*1000),handoff='not_run: preflight retired; no unusable URL advertised as live')
        (out/'target.json').write_text(json.dumps(result,indent=2)+'\n')
    print(json.dumps(result))
    return 0 if result['targetReady'] else 2


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--root',type=Path,required=True)
    parser.add_argument('--release',type=Path,required=True)
    parser.add_argument('--output',type=Path,required=True)
    raise SystemExit(main(parser.parse_args()))

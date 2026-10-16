#!/usr/bin/env python3
"""Offline synthetic fixture configuration, never a tenant/runtime repair.

Use the existing persisted runtime key, not an ambient environment toggle. A
zero port asks the kernel for a private socket without racing a shared listener.
"""
import argparse
import errno
import json
import os
from pathlib import Path
import socket
import sqlite3
import time


def configure(root, address='127.0.0.1:0'):
    assert address in ('127.0.0.1:0', '127.0.0.1:8765')
    assert os.uname().sysname=='Linux' and os.environ.get('BUILD_LANE_BIN')
    root=Path(root).resolve()
    base=(Path(os.environ['BUILD_LANE_BIN'])/'../artifacts').resolve()
    assert base in root.parents
    marker=json.loads((root/'.sync-v4-e2e.json').read_text())
    assert marker.get('owner')=='01a087a0-169a-7e23-ba3d-71352256cbfb' and marker.get('synthetic') is True
    # Only offline fixture seeding: a live owner is never bypassed by SQLite.
    endpoint=root/'runtime/store/store.sock'
    with socket.socket(socket.AF_UNIX,socket.SOCK_STREAM) as sock:
        sock.settimeout(.3)
        try:sock.connect(str(endpoint))
        except OSError as error:
            if error.errno not in (errno.ENOENT,errno.ECONNREFUSED):raise
        else:raise RuntimeError('Private fixture still has a live Store owner')
    runtime=root/'runtime';runtime.mkdir(exist_ok=True,mode=0o700)
    database=runtime/'ctox-runtime.sqlite3'
    existed=database.exists()
    with sqlite3.connect(database,timeout=3) as db:
        db.execute('BEGIN IMMEDIATE')
        # This is the product runtime config schema, seeded before the real
        # daemon owns/migrates it, just like the other synthetic profile data.
        db.execute('CREATE TABLE IF NOT EXISTS runtime_env_kv (env_key TEXT PRIMARY KEY, env_value TEXT NOT NULL)')
        assert [r[1] for r in db.execute('PRAGMA table_info(runtime_env_kv)')]==['env_key','env_value']
        key='CTOX_BUSINESS_OS_WEB_ADDR'
        old=db.execute('SELECT env_value FROM runtime_env_kv WHERE env_key=?',(key,)).fetchone()
        db.execute('INSERT INTO runtime_env_kv(env_key,env_value) VALUES (?,?) ON CONFLICT(env_key) DO UPDATE SET env_value=excluded.env_value',(key,address))
    return dict(owner=marker['owner'],synthetic=True,fixtureSetup=True,productionChange=False,
        key=key,value=address,priorPublicAddress=old[0] if old else None,
        existingDatabase=existed,measuredAtMs=int(time.time()*1000),root=str(root))


if __name__=='__main__':
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--root',type=Path,required=True)
    p.add_argument('--output',type=Path,required=True);a=p.parse_args()
    r=configure(a.root);a.output.write_text(json.dumps(r,indent=2)+'\n');print(json.dumps(r))

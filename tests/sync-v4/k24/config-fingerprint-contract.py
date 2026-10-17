#!/usr/bin/env python3
"""Configuration observer invariant on actual SQLite, not a sync acceptance."""
import importlib.util
import json
import os
from pathlib import Path
import sqlite3
import tempfile

spec=importlib.util.spec_from_file_location('adapter',Path(__file__).with_name('core-adapter.py'))
m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m)
assert os.uname().sysname=='Linux' and os.environ.get('BUILD_LANE_BIN')
with tempfile.TemporaryDirectory(prefix='k24-config-',dir=os.environ['TMPDIR']) as path:
    root=Path(path)
    (root/'.sync-v4-e2e.json').write_text(json.dumps(dict(owner=m.OWNER,synthetic=True)))
    (root/'runtime').mkdir()
    file=root/'runtime/ctox-runtime.sqlite3'
    with sqlite3.connect(file) as db:
        db.execute('CREATE TABLE runtime_env_kv(env_key TEXT PRIMARY KEY,env_value TEXT NOT NULL)')
        db.execute("INSERT INTO runtime_env_kv VALUES('synthetic-setting','first')")
    before=m.runtime_configuration(root)
    assert before==m.runtime_configuration(root),'Read-only observation changed its own result'
    with sqlite3.connect(file) as db:
        db.execute("UPDATE runtime_env_kv SET env_value='changed'")
    after=m.runtime_configuration(root)
    assert before!=after,'An actual configuration value change was invisible'
    with sqlite3.connect(file) as db:
        db.execute("UPDATE runtime_env_kv SET env_value='first'")
    assert before==m.runtime_configuration(root),'Restored configuration fingerprint differs'
    print(json.dumps(dict(stableRead=True,valueChangeDetected=True,restorationDetected=True,
                         limit='SQLite observer component; not installed K24/browser acceptance')))

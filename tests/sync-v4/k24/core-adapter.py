#!/usr/bin/env python3
"""Read-only K24 evidence adapter. Missing native bindings are errors, not zeros."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import sqlite3

OWNER = '01a087a0-169a-7e23-ba3d-71352256cbfb'


def quote(name):
    return '"'+name.replace('"', '""')+'"'


def digest(file):
    value = hashlib.sha256()
    with file.open('rb') as source:
        for chunk in iter(lambda: source.read(1024*1024), b''):
            value.update(chunk)
    return value.hexdigest()


def query(database):
    app = os.environ.get('K24_APP', 'k24-probe')
    if app != 'k24-probe':
        raise ValueError('Only the disposable k24-probe app may be inspected')
    kind = os.environ.get('K24_QUERY')
    with sqlite3.connect(database.as_uri()+'?mode=ro', uri=True, timeout=3) as db:
        db.execute('PRAGMA query_only=ON')
        db.execute('BEGIN')
        if kind == 'audit' and db.execute("SELECT 1 FROM sqlite_master WHERE type='table' AND name='business_events'").fetchone():
            record = os.environ.get('K24_RECORD')
            if record and not record.startswith('cmd:'):
                raise ValueError('K24_RECORD must be cmd:<command_id>')
            # cap.audit.event persists these native fields (caps/audit.rs).
            # App identity is the native _cap provenance, never a test actor.
            sql = "SELECT event_id,json_extract(payload_json,'$._cap.app_id'),command_type,record_id,collection,json_extract(payload_json,'$.label') FROM business_events WHERE json_extract(payload_json,'$._cap.app_id')=?"
            params = [app]
            if record:
                sql += ' AND record_id=?'
                params.append(record)
            sql += ' ORDER BY observed_at_ms,event_id'
            return [dict(zip(('id','app_id','event_type','record_id','collection','label'), row)) for row in db.execute(sql, params)]
        registry = {r[0]: r for r in db.execute(
            "SELECT name,table_name,gen,app_id FROM _collections WHERE state='active'")}
        def documents(name):
            if name not in registry:
                raise ValueError('Native collection is not registered: '+name)
            table = registry[name][1]
            return [json.loads(r[0]) for r in db.execute('SELECT data FROM '+quote(table)+' WHERE deleted=0 ORDER BY id')]
        if kind == 'audit':
            record = os.environ.get('K24_RECORD')
            if record and not record.startswith('cmd:'):
                raise ValueError('K24_RECORD must be cmd:<command_id>')
            # Native _audit is a collection, never a browser/fake-core oracle.
            if '_audit' in registry:
                rows = documents('_audit')
            else:
                columns = {r[1] for r in db.execute('PRAGMA table_info(_audit)')}
                wanted = {'id','app_id','event_type','record_id','collection','label'}
                if not wanted <= columns:
                    raise ValueError('Native _audit read binding unavailable')
                rows = [dict(zip(sorted(wanted), row)) for row in db.execute(
                    'SELECT '+','.join(quote(c) for c in sorted(wanted))+' FROM _audit WHERE app_id=?', (app,))]
            return [{key: row.get(key) for key in ('id','app_id','event_type','record_id','collection','label')}
                    for row in rows if row.get('app_id') == app and (not record or row.get('record_id') == record)]
        if kind != 'core-view':
            raise ValueError('K24_QUERY must be core-view or audit')
        apps = [row for row in documents('business_apps') if row.get('id') == app]
        if len(apps) > 1:
            raise ValueError('Ambiguous active app')
        versions = [row.get('id') for row in documents('business_app_versions') if row.get('app_id') == app]
        active = apps[0].get('active_ver') if apps and apps[0].get('state') == 'active' else None
        archive = []
        # Return native recorded removal metadata, never infer removed_by from a
        # test actor or fabricate an app version from the active pointer.
        for ident, version, body in db.execute('SELECT id,app_ver,data FROM _archive WHERE app_id=? ORDER BY archive_id', (app,)):
            row = json.loads(body)
            archive.append(dict(id=ident,app_ver=version,removed_by=row.get('removed_by'),app_active_ver=row.get('app_active_ver')))
        counts = {}
        for name, gen in (('k24_ticks', None), ('k24_ticks', 2), ('k24_runs', None)):
            key = name+('@g2' if gen == 2 else '')
            sql = 'SELECT table_name FROM _collections WHERE name=? AND app_id=?'
            params = [name, app]
            if gen is None:
                sql += " AND state='active'"
            else:
                sql += ' AND gen=?'
                params.append(gen)
            tables = db.execute(sql, params).fetchall()
            if len(tables) > 1:
                raise ValueError('Ambiguous collection generation')
            counts[key] = db.execute('SELECT count(*) FROM '+quote(tables[0][0])+' WHERE deleted=0').fetchone()[0] if tables else 0
        return dict(app=active, versions=versions, archive=archive, data=counts)


def runtime_configuration(root):
    """Hash live configuration through SQLite, never the live DB file.

    Only the owned synthetic profile is eligible. No secret-store or protected
    desktop file is opened; values remain in memory and only digests leave.
    """
    root = root.resolve()
    marker = json.loads((root/'.sync-v4-e2e.json').read_text())
    if marker.get('owner') != OWNER or marker.get('synthetic') is not True:
        raise ValueError('Only the owned synthetic runtime configuration')
    database = (root/'runtime/ctox-runtime.sqlite3').resolve(strict=True)
    if root not in database.parents:
        raise ValueError('Runtime configuration outside private profile')
    result = {}
    with sqlite3.connect(database.as_uri()+'?mode=ro', uri=True, timeout=3) as db:
        db.execute('PRAGMA query_only=ON')
        db.execute('BEGIN')
        tables = {r[0] for r in db.execute("SELECT name FROM sqlite_master WHERE type='table'")}
        if 'runtime_env_kv' not in tables:
            raise ValueError('Actual runtime configuration binding unavailable')
        for table in ('runtime_env_kv', 'runtime_state_store', 'ctox_sync_host'):
            if table not in tables:
                result[table] = dict(present=False)
                continue
            columns = [r[1] for r in db.execute('PRAGMA table_info('+quote(table)+')')]
            rows = []
            for row in db.execute('SELECT * FROM '+quote(table)):
                cells = [dict(blobSha256=hashlib.sha256(c).hexdigest()) if isinstance(c, bytes) else c for c in row]
                rows.append(json.dumps(cells, ensure_ascii=False, separators=(',',':')).encode())
            value = hashlib.sha256(json.dumps(columns,separators=(',',':')).encode()+b'\0')
            for row in sorted(rows):
                value.update(len(row).to_bytes(8,'big'));value.update(row)
            result[table] = dict(present=True, rows=len(rows), sha256=value.hexdigest())
    return dict(method='sqlite-ro-query_only', database=str(database), tables=result)


def fingerprint(root, pid, configs):
    proc = Path('/proc')/str(pid)
    before = (proc/'stat').read_text().rsplit(')', 1)[1].split()[19]
    binary = (proc/'exe').resolve(strict=True)
    if root not in binary.parents:
        raise ValueError('Running executable is not installed in the private profile')
    # Bind the observer to the daemon's actual state root. Never fingerprint a
    # convenient shadow DB while the process is using another runtime tree.
    roots = {}
    for field in (proc/'environ').read_bytes().split(b'\0'):
        key, _, value = field.partition(b'=')
        if key in (b'CTOX_ROOT', b'CTOX_STATE_ROOT', b'CTOX_INSTALL_ROOT'):
            roots[key.decode()] = value.decode()
    state = roots.get('CTOX_STATE_ROOT')
    if not state or Path(state).resolve() != (root/'runtime').resolve():
        raise ValueError('Daemon runtime configuration binding is not established')
    files = {}
    for file in configs:
        file = file.resolve(strict=True)
        if root not in file.parents or file.suffix not in ('.json', '.toml', '.conf'):
            raise ValueError('Only explicitly supplied private public config files')
        files[str(file)] = digest(file)
    if not files:
        raise ValueError('Explicit profile configuration inventory required')
    result = dict(binarySha256=digest(proc/'exe'), configs=files, runtimeConfiguration=runtime_configuration(root),
                  pid=pid, startTicks=before, roots=roots, bootId=Path('/proc/sys/kernel/random/boot_id').read_text().strip())
    if (proc/'stat').read_text().rsplit(')', 1)[1].split()[19] != before:
        raise ValueError('PID reused')
    return result


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('operation', choices=('query','fingerprint'))
    parser.add_argument('--root', required=True, type=Path)
    parser.add_argument('--database', type=Path)
    parser.add_argument('--audit-database', type=Path)
    parser.add_argument('--pid', type=int)
    parser.add_argument('--config', action='append', type=Path, default=[])
    args = parser.parse_args()
    root = args.root.resolve()
    marker = json.loads((root/'.sync-v4-e2e.json').read_text())
    if marker.get('owner') != OWNER or marker.get('synthetic') is not True:
        raise ValueError('Only the owned synthetic profile')
    if args.operation == 'query':
        file = args.audit_database if os.environ.get('K24_QUERY') == 'audit' and args.audit_database else args.database
        database = file.resolve(strict=True)
        if root not in database.parents:
            raise ValueError('Database outside private profile')
        result = query(database)
    else:
        result = fingerprint(root, args.pid, args.config)
    print(json.dumps(result, separators=(',',':')))

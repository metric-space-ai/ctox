#!/usr/bin/env python3
"""Full-row readback of every owned synthetic app table, including tombstones.

No limit, no customer table, no payload in the receipt. This is not whole-tenant
coverage: the phase1 source/destination report is a separate M7 criterion.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import sqlite3


def readback(root):
    marker = json.loads((root/'.sync-v4-e2e.json').read_text())
    assert marker.get('owner') == '01a087a0-169a-7e23-ba3d-71352256cbfb' and marker.get('synthetic') is True
    database = root/'runtime/business-os-rxdb.sqlite3'
    with sqlite3.connect(database.as_uri()+'?mode=ro', uri=True) as db:
        db.execute('PRAGMA query_only=ON')
        db.execute('BEGIN')
        result = {}
        for (name,) in db.execute("SELECT name FROM sqlite_master WHERE type='table' AND name GLOB 'ctox_business_os__e2e_*__v[0-9]*' ORDER BY name").fetchall():
            quoted = '"'+name.replace('"','""')+'"'
            rows = []
            for row in db.execute('SELECT * FROM '+quoted):
                encoded = json.dumps(row, separators=(',',':'), ensure_ascii=False,
                                     default=lambda value: {'blob_hex': value.hex()}).encode()
                rows.append(hashlib.sha256(encoded).digest())
            schema = list(db.execute('PRAGMA table_info('+quoted+')'))
            result[name] = dict(rows=len(rows), sha256=hashlib.sha256(b''.join(sorted(rows))).hexdigest(),
                                schemaSha256=hashlib.sha256(json.dumps(schema).encode()).hexdigest())
        db.rollback()
    assert result and sum(table['rows'] for table in result.values()) > 0, 'Zero app rows is not readback'
    return dict(schema='ctox.m7.synthetic-readback.v1', scope='All synthetic e2e app rows including tombstones; no whole-tenant claim',
                tables=result, rows=sum(table['rows'] for table in result.values()))


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--root', required=True, type=Path)
    parser.add_argument('--output', required=True, type=Path)
    parser.add_argument('--expect', type=Path)
    args = parser.parse_args()
    assert os.uname().sysname == 'Linux' and os.environ.get('BUILD_LANE_BIN')
    allowed = (Path(os.environ['BUILD_LANE_BIN'])/'../artifacts').resolve()
    root, output = args.root.resolve(), args.output.resolve()
    assert allowed in root.parents and allowed in output.parents and not output.exists()
    result = readback(root)
    if args.expect:
        before = json.loads(args.expect.read_text())
        assert result['tables'] == before['tables'] and result['rows'] == before['rows'], 'Restored app row/schema fingerprint differs'
        result['matchesBefore'] = True
    output.write_text(json.dumps(result,indent=2)+'\n')
    print(json.dumps(dict(rows=result['rows'], output=str(output), matchesBefore=result.get('matchesBefore'))))

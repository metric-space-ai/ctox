#!/usr/bin/env python3
"""Read one actual kernel command receipt through SQLite's read-only API."""
import argparse, json, os, re, sqlite3, hashlib
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('--root',type=Path,required=True);p.add_argument('--id',required=True);p.add_argument('--output',type=Path,required=True);a=p.parse_args()
assert os.uname().sysname=='Linux' and os.environ.get('BUILD_LANE_BIN')
root=a.root.resolve();allowed=(Path(os.environ['BUILD_LANE_BIN'])/'../artifacts').resolve()
assert allowed in root.parents and allowed in a.output.resolve().parents
m=json.loads((root/'.sync-v4-e2e.json').read_text());assert m['owner']=='01a087a0-169a-7e23-ba3d-71352256cbfb' and m['synthetic'] is True
found=[]
for file in (root/'runtime').rglob('*.sqlite3'):
 if 'backups' in file.parts:continue
 with sqlite3.connect(file.as_uri()+'?mode=ro',uri=True,timeout=2) as db:
  db.execute('PRAGMA query_only=ON');db.execute('BEGIN')
  if not db.execute("SELECT 1 FROM sqlite_master WHERE type='table' AND name='_collections'").fetchone():continue
  for table,version in db.execute("SELECT table_name,schema_ver FROM _collections WHERE name='business_commands' AND state='active'"):
   assert re.fullmatch(r'[A-Za-z0-9_]+',table)
   columns=[r[1] for r in db.execute('PRAGMA table_info("'+table+'")')]
   for col in columns:
    assert re.fullmatch(r'[A-Za-z0-9_]+',col)
    # Inspect only valid JSON containing this exact command ID; no document dump.
    sql='SELECT "'+col+'" FROM "'+table+'" WHERE json_valid("'+col+'") AND json_extract(CASE WHEN json_valid("'+col+'") THEN "'+col+'" ELSE NULL END,\'$.id\')=?'
    for (raw,) in db.execute(sql,[a.id]):
     doc=json.loads(raw)
     if not isinstance(doc,dict) or doc.get('id')!=a.id:continue
     required={'id','type','payload','status','app_id','actor_id','created_at'}
     ok=version==3 and required<=doc.keys() and doc['status']=='done' and doc['type']=='ctox.workjet.device.v1' and doc['app_id']=='ctox-platform'
     found.append(dict(database=str(file),table=table,schemaVersion=version,id=doc['id'],type=doc.get('type'),commandStatus=doc.get('status'),v3Shape=required<=doc.keys(),documentSha256=hashlib.sha256(raw.encode()).hexdigest(),status='pass' if ok else 'fail'))
  db.rollback()
assert len(found)==1,'Exactly one actual native command receipt required'
result=found[0];a.output.write_text(json.dumps(result,indent=2)+'\n');print(json.dumps(result));raise SystemExit(0 if result['status']=='pass' else 2)

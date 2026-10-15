#!/usr/bin/env python3
"""Deterministic size envelopes in a NEW synthetic tenant, never customer tables."""
import argparse, hashlib, json, pathlib, sqlite3, time
p=argparse.ArgumentParser(); p.add_argument('--root',required=True); p.add_argument('--profile',choices=['small','thesen-like'],default='small'); p.add_argument('--operation',choices=['seed','read','behind','forward'],default='seed'); a=p.parse_args()
r=pathlib.Path(a.root).resolve(); marker=json.loads((r/'.sync-v4-e2e.json').read_text())
assert marker['synthetic'] is True and marker['owner']=='01a087a0-169a-7e23-ba3d-71352256cbfb'
assert r.name.startswith('tenant-') and not any(x in str(r).lower() for x in ['thesen','welsch'])
db=r/'runtime/business-os-rxdb.sqlite3'; base=1792022400000
profiles={'small':[('leads',40,2048,0),('history',100,512,80)], 'thesen-like':[
 ('leads',850,88000,0),('sellify',300000,1790,0),('events',242000,586,231000),
 ('commands',8400,16000,4000),('chats',1300,14308,0),('tasks',580,23700,0),
 ('files',1100,100000,0),('messages',2000,37000,0),('threads',3900,11000,0),('runs',4100,1000,0),('other',850,100000,0)]}
q=lambda s:'"'+s.replace('"','""')+'"'
name='e2e_leads'; table='ctox_business_os__'+name+'__v0'
if a.operation=='read':
 c=sqlite3.connect('file:'+str(db)+'?mode=ro',uri=True); c.execute('PRAGMA query_only=ON')
 rows=c.execute("SELECT id,revision,lastWriteTime,deleted,json_extract(data,'$.probe_phase') FROM "+q(table)+' ORDER BY id').fetchall()
 print(json.dumps({'rows':[dict(zip(['id','revision','lwt','deleted','phase'],x)) for x in rows]})); raise SystemExit
if a.operation in ['behind','forward']:
 # Faithful fault injection of store.rs upsert_attached_rxdb_record's ATTACH write.
 # It is not a claim to have called that private Rust function through a CLI.
 c=sqlite3.connect(r/'runtime/e2e-writer.sqlite3'); c.execute('ATTACH DATABASE ? AS projection',(str(db),))
 lwt=base-1000 if a.operation=='behind' else int(time.time()*1000)
 rev=('1' if a.operation=='behind' else '2')+'-e2e-behind'
 doc={'id':'e2e-leads-behind','fixtureOnly':True,'ordinal':999999,'padding':'checkpoint probe','probe_phase':a.operation,'updated_at_ms':lwt,'_rev':rev,'_deleted':False,'_meta':{'lwt':lwt},'_attachments':{}}
 c.execute('INSERT INTO projection.'+q(table)+' (id,revision,deleted,lastWriteTime,data) VALUES(?,?,0,?,?) ON CONFLICT(id) DO UPDATE SET revision=excluded.revision,lastWriteTime=excluded.lastWriteTime,data=excluded.data',(doc['id'],rev,lwt,json.dumps(doc)))
 c.commit(); print(json.dumps({'id':doc['id'],'revision':rev,'lwt':lwt,'operation':a.operation})); raise SystemExit
c=sqlite3.connect(db); populations=[]; digest=hashlib.sha256()
for short,count,size,tombstones in profiles[a.profile]:
 name='e2e_'+short; table='ctox_business_os__'+name+'__v0'
 c.execute('CREATE TABLE '+q(table)+' (id TEXT PRIMARY KEY, revision TEXT NOT NULL, deleted INTEGER NOT NULL DEFAULT 0,lastWriteTime REAL NOT NULL,data TEXT NOT NULL)')
 c.execute('CREATE INDEX '+q(table+'_lwt')+' ON '+q(table)+'(lastWriteTime,id)')
 batch=[]; bytes_=0
 for i in range(count):
  id=f'e2e-{short}-{i:06d}'; lwt=base+i; deleted=i<tombstones; rev='1-e2e-fixed'
  doc={'id':id,'fixtureOnly':True,'ordinal':i,'padding':'','updated_at_ms':lwt,'_rev':rev,'_deleted':deleted,'_meta':{'lwt':lwt},'_attachments':{}}
  # SHAKE avoids compressible repeated filler and is deterministic, no source data.
  n=max(0,size-len(json.dumps(doc,separators=(',',':')).encode()))
  doc['padding']=hashlib.shake_256(id.encode()).hexdigest((n+1)//2)[:n]
  data=json.dumps(doc,separators=(',',':')); digest.update(data.encode()); bytes_+=len(data.encode())
  batch.append((id,rev,int(deleted),lwt,data))
  if len(batch)==250 or i==count-1:
   c.executemany('INSERT INTO '+q(table)+' VALUES(?,?,?,?,?)',batch); c.commit(); batch=[]
 populations.append({'collection':name,'count':count,'live':count-tombstones,'documentBytes':bytes_,'tombstones':tombstones})
print(json.dumps({'profile':a.profile,'seed':'e2e-fixed-v1','sha256':digest.hexdigest(),'populations':populations,'totalDocumentBytes':sum(x['documentBytes'] for x in populations)}))

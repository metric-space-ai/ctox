#!/usr/bin/env python3
"""Lane-only synthetic cutover qualification from supplied inventory metadata.
No customer rows are copied. Exact table names, columns and known row counts
are modelled; four secret-API tables remain metadata-only, never opened.
"""
import argparse, copy, hashlib, importlib.util, json, os, re, shutil, signal, sqlite3, subprocess, time, uuid
from pathlib import Path
import foreground as fg
STAMP=1792152000000
OWNER_ID="00000000-0000-4000-8000-000000000001"
ALIAS="synthetic@example.invalid"
PROTECTED={"password","password_hash","dkim_private_key","access_token","refresh_token","secret_value","api_key","env_value"}
CORE_TABLES={"ticket_items","ticket_events","ticket_cases","ticket_approvals","ticket_verifications","ticket_writebacks","ticket_clarification_requests","ticket_execution_actions","local_ticket_events","iot_assets","iot_attributes","iot_asset_types","iot_alarms","iot_rulesets","iot_agents","iot_agent_status","iot_dashboards","iot_widgets","iot_condition_state","iot_recurrence_block","iot_datapoints","crew_members","crew_member_learnings","continuity_documents","continuity_commits","communication_routing_state","crew_attempts","worker_run_identities","scrape_target","scrape_script_revision","scrape_run","scrape_artifact","scrape_account_state","outbound_native_field_status_witnesses","workjet_supervisor_bindings","stalwart_mail_tracking_tokens","stalwart_mail_tracking_events"}
def synthetic(schema,depth=0):
 if depth>6:return None
 if 'default' in schema:return schema['default']
 if schema.get('enum'):return schema['enum'][0]
 t=schema.get('type','object');t=next((v for v in t if v!='null'),'null') if isinstance(t,list) else t
 if t=='object':return {k:synthetic(v,depth+1) for k,v in schema.get('properties',{}).items() if not k.startswith('_') and k not in PROTECTED}
 if t=='array':return []
 if t in ['integer','number']:return max(1,schema.get('minimum',0))
 if t=='boolean':return False
 if t=='null':return None
 if schema.get('format')=='date-time':return '2026-10-16T00:00:00Z'
 return 'synthetic'[:schema.get('maxLength',64)]
def doc(collection,key,schemas):
 body=synthetic(schemas.get(collection,{'type':'object'}));body=body if isinstance(body,dict) else {}
 body.update(id=key,is_deleted=False)
 for k in ['project_id','computer_id','session_id','record_id','command_id','run_id','thread_id']:
  if k in body:body[k]=key
 if 'owner_user_id' in body:body['owner_user_id']=ALIAS
 if 'user_id' in body:body['user_id']=OWNER_ID
 return body

def main():
 a=argparse.ArgumentParser(description=__doc__)
 for k in ['binary','custody-snapshot','output','v4-root','platform','measurement','coverage']:a.add_argument('--'+k,required=True,type=Path)
 args=a.parse_args();assert os.uname().sysname=='Linux' and os.environ.get('BUILD_LANE_BIN')
 signal.signal(signal.SIGTERM,lambda *_:(_ for _ in ()).throw(InterruptedError('qualification deadline')))
 measurement=json.loads(args.measurement.read_text());coverage=json.loads(args.coverage.read_text());models=measurement['tables'];assert len(models)==495
 by_key={(r['database'],r['table']):r for r in coverage['tables']};assert set(by_key)=={(r['database'],r['table']) for r in models}
 assert not measurement['secret_content_read'];binary=args.binary.resolve(strict=True);repo=Path.cwd()
 out=args.output.resolve();out.mkdir(mode=0o700);original=out/'synthetic-v3';original.mkdir();frozen=out/'frozen';frozen.mkdir()
 result={'schema':'ctox.cutover.coverage495.v1','pass':False,'synthetic':True,'welsch_contacted':False,'customer_rows_copied':False,'secret_content_read':False,'source':subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip(),'binary_sha256':hashlib.sha256(binary.read_bytes()).hexdigest(),'script_sha256':hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),'measurement_metadata_sha256':hashlib.sha256(args.measurement.read_bytes()).hexdigest(),'steps':[]}
 spec=importlib.util.spec_from_file_location('freeze',repo/'scripts/sync-v4/freeze-measure.py');freeze=importlib.util.module_from_spec(spec);spec.loader.exec_module(freeze)
 schemas={}
 core_collections={r['destination'].get('collection') for r in coverage['tables'] if r['database']!='business-os-rxdb.sqlite3' and r['table'] in CORE_TABLES and r['destination']['kind']=='collection'}
 for p in (args.v4_root/'schemas').glob('*.collections.schema.json'):schemas.update(json.loads(p.read_text())['collections'])
 for p in ['manifests','schemas']:shutil.copytree(args.v4_root/p,frozen/'v4'/p)
 (frozen/'v4/tools/manifest/out').mkdir(parents=True);shutil.copyfile(args.v4_root/'tools/manifest/out/ownership.json',frozen/'v4/tools/manifest/out/ownership.json');shutil.copyfile(args.platform,frozen/'platform.json')
 physical={}
 for m in models:
  hit=re.fullmatch(r'ctox_business_os__(.+)__v([0-9]+)',m['table'])
  if hit and not hit[1].strip('_')=='rxdb_internal':physical.setdefault(hit[1],[]).append(int(hit[2]))
 # Deliberate fixture metadata: oldest version is active. This is not an
 # assertion about WELSCH's active versions, nor a MAX(version) fallback.
 active={c:min(vs) for c,vs in physical.items()};internal_docs=[{'context':'collection','data':{'name':c,'version':v,'schema':{'version':v}}} for c,v in sorted(active.items())]
 groups={}
 for m in models:groups.setdefault(m['database'],[]).append(m)
 sources=[];retained=[];inventory={};source_count=0
 for database,entries in sorted(groups.items()):
  inventory[database]=sorted(m['table'] for m in entries)
  if 'secret' in database.lower():
   assert all(m['measurement_status']=='secret_store_api_only' for m in entries)
   retained.extend({'database':database,'table':m['table'],'decision':'secret_store_api_only'} for m in entries);continue
  path=original/database;path.parent.mkdir(parents=True,exist_ok=True);db=sqlite3.connect(path);decisions=[];collectors=[];internal=None
  for m in entries:
   table=m['table'];columns=m['schema_columns'];assert columns and len(set(columns))==len(columns)
   physical_name=re.fullmatch(r'ctox_business_os__(.+)__v([0-9]+)',table)
   is_internal=physical_name and physical_name[1].strip('_')=='rxdb_internal'
   key=next((k for k in ['id','record_id','command_id','user_id','meeting_id','project_id'] if k in columns),columns[0])
   if table=='business_records':key='record_id'
   if table in ['crew_members','business_users']:key='id' if 'id' in columns else 'user_id'
   db.execute('CREATE TABLE '+freeze.identifier(table)+'('+','.join(freeze.identifier(c) for c in columns)+')')
   schema=schemas.get(physical_name[1],{}) if physical_name else {};template=doc(physical_name[1], 'placeholder', schemas) if physical_name else None
   n=m['row_count'];assert n is not None;source_count+=n
   def values(i):
    identity='synthetic-record-'+str(i);data=copy.deepcopy(template) if template is not None else None
    if data is not None:
     data['id']=identity
     for natural in ['project_id','computer_id','session_id','record_id','command_id','run_id','thread_id']:
      if natural in data:data[natural]=identity
    if is_internal:data=internal_docs[i] if i<len(internal_docs) else {'context':'other','fixture':i}
    meeting={'id':identity,'project_id':'synthetic-project','owner_user_id':OWNER_ID,'scheduled_at_ms':STAMP,'prepare_at_ms':STAMP-7200000,'supervisor':{'workjet_thread_id':'synthetic-thread','ctox_thread_key':'synthetic-thread'},'timezone':'Europe/Berlin','state':'planned','revision':0,'deck_revision':0,'previous_goal':None,'todos':None,'error':None,'slides':[],'comments':[],'transcript':[]}
    row={}
    for c in columns:
     if c in PROTECTED:row[c]=None
     elif c==key:row[c]=identity
     elif c=='data' and physical_name:row[c]=json.dumps(data,separators=(',',':'),sort_keys=True)
     elif c in ['payload_json','body_json']:row[c]=json.dumps({'id':identity,'fixture':'synthetic'})
     elif c=='collection':row[c]='ctox_harness_events'
     elif c.endswith('_json'):row[c]='[]' if any(t in c for t in ['task_ids','specialties','items','steps','sources']) else '{}'
     elif c=='conversation_id':row[c]=-1
     elif c in ['crew_assigned_member_id','crew_member_id','parent_commit_id','head_commit_id','preparation_task_id']:row[c]=None
     elif c.endswith('_at_ms') or c.endswith('_ms') or c=='lastWriteTime':row[c]=STAMP
     elif c.endswith('_at') or c in ['observed_at','opened_at','external_created_at']:row[c]='2026-10-16T00:00:00Z'
     elif c in ['active','verified_at_ms','revision','attempt','version','position','sequence','succeeded']:row[c]=1
     elif c in ['deleted','archived','failure_attempt_count']:row[c]=0
     elif c in ['owner_user_id','canonical_user_id']:row[c]=OWNER_ID
     elif c=='alias':row[c]=ALIAS
     elif c=='route_status':row[c]='handled'
     elif c=='shape':row[c]='blob'
     elif c=='color':row[c]='#e97255'
     elif c in ['owner','user_id']:row[c]=OWNER_ID
     else:row[c]='synthetic'
    if table=='business_user_identity_aliases':row.update(alias=ALIAS,canonical_user_id=OWNER_ID,verified_at_ms=STAMP)
    if table=='business_users':row[key]=OWNER_ID if i==0 else str(uuid.uuid5(uuid.NAMESPACE_URL,'synthetic-user-'+str(i)))
    if table=='workjet_jour_fixe_meetings':row.update(meeting_id=identity,project_id='synthetic-project',owner_user_id=OWNER_ID,scheduled_at_ms=STAMP,metadata_json=json.dumps(meeting),preparation_task_id=None)
    return tuple(row[c] for c in columns)
   sql='INSERT INTO '+freeze.identifier(table)+' VALUES('+','.join('?' for _ in columns)+')'
   db.executemany(sql,(values(i) for i in range(n)))
   dest=by_key[(database,table)]['destination'];kind=dest['kind'];collection=dest.get('collection',table);version=0;body=None;entity='core';mode={'kind':'keep_core'};rule='E29-R6'
   if physical_name:
    collection,version=physical_name[1],int(physical_name[2]);key='id';body='data';entity='core' if collection in core_collections else 'app';mode={'kind':'rows'};rule='E29-R1'
    if is_internal:internal=table;mode={'kind':'keep_core'};rule='E29-R4'
   elif kind=='mirror_comparison':
    entity='app';rule='E29-R3';mode={'kind':'rows'}
    if table=='business_records':mode={'kind':'mirror_rows','collection_column':'collection'};collection='';body='payload_json'
   elif kind in ['collection','dataframe']:
    mode={'kind':'archive_only'};key='$rowid';rule='E29-R5'
    if kind=='dataframe':rule='E29.1';collection=dest['table']
    if table in ['stalwart_domains','stalwart_users']:mode={'kind':'keep_core'};rule='E29-R6';collection='retained:'+table
   if mode['kind']=='keep_core' and not is_internal:collection='retained:'+table
   if database=='business-os-rxdb.sqlite3' and not physical_name:mode={'kind':'archive_only'};rule='E29-R2';key='$rowid';collection='frozen_metadata:'+table
   decisions.append({'rule':rule,'table':{'name':table,'collection':collection,'version':version,'id_column':key,'body_column':body,'entity':entity,'mode':mode},'measurement':None})
  db.commit();assert db.execute('PRAGMA integrity_check').fetchone()[0]=='ok';db.close()
  frozen_path=frozen/database;fg.backup(path,frozen_path);frozen_path.chmod(0o444)
  ro=sqlite3.connect(frozen_path.as_uri()+'?mode=ro&immutable=1',uri=True);ro.text_factory=freeze.Text
  for d in decisions:
   measured=freeze.measure(ro,d['table']['name'])
   if measured['measurement_status']=='measured':d['measurement']={k:measured[k] for k in ['row_count','schema_columns','source_table_hash']}
  ro.close()
  known=[d['table']['name'] for d in decisions if d['table']['name'] in CORE_TABLES]
  # Only positively mapped native sources may join the existing native collector.
  if known and any(by_key[(database,t)]['import_path']=='profile_import' for t in known):collectors.append({'kind':'core','tables':known,'original_root':str(original),'copied_root':str(frozen),'owner_user':OWNER_ID})
  if any(d['table']['name']=='workjet_jour_fixe_meetings' for d in decisions):collectors.append({'kind':'jour_fixe'})
  if any(d['table']['name']=='stalwart_runtime_config' and by_key[(database,d['table']['name'])]['import_path']=='profile_import' for d in decisions):collectors.append({'kind':'mail_server'})
  if internal is not None:source_kind='rxdb'
  else:source_kind='core'
  source={'choice':{'database':database,'live_file':str(path),'frozen_file':str(frozen_path),'lsof_receipt_sha256':hashlib.sha256(fg.encoded({'synthetic':True,'selected_file':str(path),'producer_pid':os.getpid()})).hexdigest()},'measurement_sha256':hashlib.sha256(fg.encoded({d['table']['name']:d['measurement'] for d in sorted(decisions,key=lambda d:d['table']['name'])})).hexdigest(),'kind':source_kind,'internal_table':internal,'tables':decisions,'collectors':collectors}
  sources.append(source)
 result.update(synthetic_known_row_count=source_count,table_count=len(models),physical_nonsecret_tables=sum(len(s['tables']) for s in sources),physical_types_and_constraints='not supplied; synthetic table definitions model supplied column names and known counts',protected_tables=sum(m['measurement_status']=='protected_content_not_read' for m in models),secret_api_tables=len(retained),active_versions_source='explicit synthetic RxDB metadata, not WELSCH inference')
 snapshot={'schema':'ctox.v4.cutover_snapshot.v1','frozen':True,'method':'sqlite_backup_api','imported_at_ms':STAMP,'archive_custodian_user':'synthetic-custodian','release':{'v4_root':str(frozen/'v4'),'platform_manifest':str(frozen/'platform.json'),'app_ver':'main@4794af03d+v4','client_limit':100000,'byte_budget':33554432,'rights_ver':1},'databases':sources,'inventory':inventory,'retained':retained}
 fg.save(frozen/'cutover.json',snapshot);(frozen/'cutover.json').chmod(0o444);fg.save(out/'build-result.json',result);print('SYNTHETIC_495_CREATED',flush=True)
 # The accepted DevOps fixture supplies the held owner; the complete synthetic
 # V3 snapshot is separate, explicit input. No customer/default root is opened.
 custody=args.custody_snapshot.resolve(strict=True);meta=json.loads((custody/'snapshot.json').read_text());assert meta['owner']==fg.CUSTODIAN and meta['synthetic'] and meta['complete']
 marker=json.loads((Path(meta['sourceRoot'])/'.sync-v4-e2e.json').read_text());assert marker['owner']==fg.CUSTODIAN and marker['synthetic'] is True
 root=out/'daemon';state=root/'runtime';state.mkdir(parents=True)
 for row in meta['databases']:
  relative=Path(row['relative']);assert not relative.is_absolute() and '..' not in relative.parts
  if 'secret' not in str(relative).lower():fg.backup(custody/'databases'/relative,state/relative)
 with sqlite3.connect(state/'ctox-runtime.sqlite3') as db:
  for key,value in [('CTOX_BUSINESS_OS_WEB_ADDR','127.0.0.1:0'),('CTOX_BUSINESS_OS_WEB_AUTOSTART','0')]:db.execute('INSERT INTO runtime_env_kv(env_key,env_value) VALUES(?,?) ON CONFLICT(env_key) DO UPDATE SET env_value=excluded.env_value',(key,value))
 for name in ['src','contracts','Cargo.toml']:(root/name).symlink_to(repo/name)
 home=root/'home';home.mkdir();env=dict(PATH=os.environ['PATH'],HOME=str(home),TMPDIR=os.environ['TMPDIR'],BUILD_LANE_BIN=os.environ['BUILD_LANE_BIN'],RUST_BACKTRACE='1',CTOX_ROOT=str(root),CTOX_STATE_ROOT=str(state),XDG_CONFIG_HOME=str(home/'.config'),XDG_CACHE_HOME=str(home/'.cache'))
 result['startup_attempts']=[]
 for attempt in range(2):
  logpath=out/('foreground-'+str(attempt)+'.log');log=logpath.open('w')
  daemon=subprocess.Popen([str(binary),'service','--foreground','--root',str(root)],cwd=root,env=env,stdout=log,stderr=log,start_new_session=True)
  until=time.monotonic()+40
  while daemon.poll() is None and time.monotonic()<until:
   if 'ctox v4 store daemon ready:' in logpath.read_text():break
   time.sleep(.1)
  ready=daemon.poll() is None and 'ctox v4 store daemon ready:' in logpath.read_text()
  result['startup_attempts'].append({'attempt':attempt,'pid':daemon.pid,'ready':ready,'exit':daemon.poll(),'log':str(logpath)})
  if ready:break
  fg.stop(daemon);log.close();fg.save(out/'result.json',result)
 result['daemon']={'pid':daemon.pid,'pgid':daemon.pid,'root':str(root),'stop':'finally TERM then KILL'}
 try:
  assert ready,'foreground not ready after bounded recovery; startup failure retained'
  command=[str(binary),'business-os','v4','cutover','import','--snapshot',str(frozen/'cutover.json'),'--dry-run']
  with (out/'dry-run.stdout').open('w') as stdout,(out/'dry-run.stderr').open('w') as stderr:
   child=subprocess.Popen(command,cwd=root,env=env,stdout=stdout,stderr=stderr,start_new_session=True)
   try:code=child.wait(timeout=900)
   except BaseException:
    fg.stop(child);raise
  result['steps'].append({'command':command,'exit':code});assert code==0,(out/'dry-run.stderr').read_text()[-3000:]
  reply=json.loads((out/'dry-run.stdout').read_text());report=reply['report'];assert report['inventory_complete'] and report['dry_run']
  actual={(r['database'],r['table']):r for r in report['tables']};assert set(actual)==set(by_key),'not exactly the complete supplied inventory'
  rows=[]
  for m in models:
   r=actual[(m['database'],m['table'])];assert r['source_rows']==m['row_count'],(m['database'],m['table'],r['source_rows'],m['row_count'])
   rows.append(dict(r,target_rows_kind='planned (dry-run only)',planned_target_rows=r['expected_target_rows'],source_count_kind=m['measurement_status'],production_source_hash_not_relabelled=True))
  # Mutation: removing an actual table declaration must fail, even after the
  # caller updates its inventory and measurement digest to agree with the lie.
  bad=copy.deepcopy(snapshot);selected=next(s for s in bad['databases'] if s['choice']['database']=='ctox.sqlite3')
  omitted=next(d for d in selected['tables'] if d['table']['name']=='ctox_process_events');selected['tables'].remove(omitted);bad['inventory']['ctox.sqlite3'].remove('ctox_process_events')
  selected['measurement_sha256']=hashlib.sha256(fg.encoded({d['table']['name']:d['measurement'] for d in sorted(selected['tables'],key=lambda d:d['table']['name'])})).hexdigest()
  fg.save(frozen/'omitted-table.json',bad)
  bad_command=command.copy();bad_command[bad_command.index('--snapshot')+1]=str(frozen/'omitted-table.json')
  negative=subprocess.Popen(bad_command,cwd=root,env=env,stdout=subprocess.PIPE,stderr=subprocess.PIPE,start_new_session=True)
  try:stdout,stderr=negative.communicate(timeout=90)
  except BaseException:fg.stop(negative);raise
  (out/'omitted-table.stdout').write_bytes(stdout);(out/'omitted-table.stderr').write_bytes(stderr)
  assert negative.returncode!=0,'physical table silently omitted'
  receipts=[]
  for line in stderr.splitlines():
   try:value=json.loads(line)
   except ValueError:continue
   if isinstance(value,dict) and 'report_path' in value:receipts.append(value)
  assert len(receipts)==1
  rejection=json.loads(Path(receipts[0]['report_path']).read_text());assert 'cutover_table_without_decision' in rejection['error'],rejection
  result['input_mutation']={'omit_table':'ctox_process_events','exit':negative.returncode,'error':rejection['error'],'source_rows_preserved':True}
  result.update(pass_=True,zero_silent_skips=True,coverage=rows,snapshot_sha256=hashlib.sha256((frozen/'cutover.json').read_bytes()).hexdigest(),dry_run_writes=False)
  result['pass']=result.pop('pass_');fg.save(out/'result.json',result)
 finally:
  fg.stop(daemon);log.close();result['foreground_exit']=daemon.returncode;result['daemon']['terminal']=True;fg.save(out/'result.json',result)
 print(json.dumps({'pass':result['pass'],'report':str(out/'result.json')}),flush=True)
if __name__=='__main__':main()

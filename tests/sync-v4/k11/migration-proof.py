"""Read only evidence from the actual generation registry, never fabricated DDL."""
import hashlib,json,pathlib,sqlite3,sys
root=pathlib.Path(sys.argv[1]).resolve(); database=pathlib.Path(sys.argv[2]).resolve(); collection=sys.argv[3]
marker=json.loads((root/'.sync-v4-e2e.json').read_text())
assert marker['synthetic'] is True and marker['owner']=='01a087a0-169a-7e23-ba3d-71352256cbfb'
assert database.is_relative_to(root) and collection.startswith('e2e_')
c=sqlite3.connect(database.as_uri()+'?mode=ro',uri=True);c.execute('PRAGMA query_only=ON')
assert c.execute('PRAGMA query_only').fetchone()[0]==1
generations=c.execute('SELECT gen,schema_ver,schema_json,state,table_name,app_ver FROM _collections WHERE name=? ORDER BY gen',(collection,)).fetchall()
assert generations,'Collection is not in the real generation registry'
quote=lambda s:'"'+s.replace('"','""')+'"'
rows=[]
for gen,ver,schema,state,table,app in generations:
 assert table.startswith('e2e_') or collection in table
 cols={x[1] for x in c.execute('PRAGMA table_info('+quote(table)+')')}
 assert 'id' in cols,'No canonical id field'
 ids=[x[0] for x in c.execute('SELECT id FROM '+quote(table)+' ORDER BY id')]
 digest=hashlib.sha256()
 for row in c.execute('SELECT id,data FROM '+quote(table)+' ORDER BY id'):
  digest.update(json.dumps(row,separators=(',',':')).encode());digest.update(b'\n')
 rows.append({'generation':gen,'schemaVersion':ver,'schemaSha256':hashlib.sha256(schema.encode()).hexdigest(),'state':state,'appVersion':app,'table':table,'rows':len(ids),'dataSha256':digest.hexdigest(),'idsSha256':hashlib.sha256(json.dumps(ids,separators=(',',':')).encode()).hexdigest()})
print(json.dumps({'queryOnly':True,'collection':collection,'generations':rows}))

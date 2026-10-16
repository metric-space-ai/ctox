import importlib.util, pathlib, tempfile, sqlite3, json, unittest, subprocess, sys, os
spec=importlib.util.spec_from_file_location('freeze',str(pathlib.Path(__file__).with_name('freeze-measure.py')));m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m)
class Freeze(unittest.TestCase):
    def test_frozen_api_hash_covers_types_duplicates_order_and_cli(self):
        with tempfile.TemporaryDirectory() as tmp:
            root=pathlib.Path(tmp);path=root/'copy.sqlite3'
            with sqlite3.connect(path) as db:
                db.execute('CREATE TABLE values_table(id TEXT, number INTEGER, real REAL, data BLOB, empty TEXT)')
                db.executemany('INSERT INTO values_table VALUES(?,?,?,?,?)',[('a',-2,1.25,b'\x00\xff',None),('a',-2,1.25,b'\x00\xff',None)])
            coverage={'tables':[{'database':'bos.sqlite3','table':'values_table'}]}
            inputs={'schema':'ctox.v4.freeze_inputs.v1','frozen':True,'frozen_root':str(root),'files':[{'database':'bos.sqlite3','live_file':'/not-the-frozen-copy/bos.sqlite3','frozen_file':str(path),'lsof_receipt_sha256':'a'*64}]}
            first=m.collect(coverage,inputs)['tables'][0];self.assertEqual(first['row_count'],2)
            with sqlite3.connect(path) as db: db.execute('DELETE FROM values_table WHERE rowid=1')
            second=m.collect(coverage,inputs)['tables'][0];self.assertNotEqual(first['source_table_hash'],second['source_table_hash'])
            with sqlite3.connect(path) as db: db.execute("UPDATE values_table SET number=-3")
            third=m.collect(coverage,inputs)['tables'][0];self.assertNotEqual(second['source_table_hash'],third['source_table_hash'])
            for name,data in [('coverage',coverage),('inputs',inputs)]: (root/(name+'.json')).write_text(json.dumps(data))
            subprocess.run([sys.executable,str(pathlib.Path(__file__).with_name('freeze-measure.py')),'--coverage',str(root/'coverage.json'),'--inputs',str(root/'inputs.json'),'--output',str(root/'out.json')],check=True)
            self.assertEqual(json.loads((root/'out.json').read_text())['tables'][0],third)
            inputs['frozen']=False
            with self.assertRaises(ValueError): m.collect(coverage,inputs)
            # Cross-language oracle: qualification persists the copy and exact hash.
            if os.environ.get('E34_PROOF_DIR'):
                import shutil
                out=pathlib.Path(os.environ['E34_PROOF_DIR']);out.mkdir(exist_ok=True,parents=True)
                shutil.copyfile(path,out/'freeze.sqlite3');(out/'freeze.json').write_text(json.dumps(third))
    def test_autoincrement_internal_tables_are_excluded_but_unknown_user_tables_rejected(self):
        with tempfile.TemporaryDirectory() as tmp:
            root=pathlib.Path(tmp);path=root/'copy.sqlite3'
            with sqlite3.connect(path) as db:
                db.execute('CREATE TABLE events(id INTEGER PRIMARY KEY AUTOINCREMENT, body TEXT)')
                db.execute("INSERT INTO events(body) VALUES('synthetic event')")
                self.assertIn(('sqlite_sequence',),db.execute("SELECT name FROM sqlite_master").fetchall())
            coverage={'tables':[{'database':'bos.sqlite3','table':'events'}]}
            inputs={'schema':'ctox.v4.freeze_inputs.v1','frozen':True,'frozen_root':str(root),'files':[{'database':'bos.sqlite3','live_file':'/original/bos.sqlite3','frozen_file':str(path),'lsof_receipt_sha256':'b'*64}]}
            measured=m.collect(coverage,inputs)['tables']
            self.assertEqual([(r['table'],r['row_count']) for r in measured],[('events',1)])
            with sqlite3.connect(path) as db: db.execute('CREATE TABLE unlisted_user_table(id TEXT)')
            with self.assertRaisesRegex(ValueError,'unlisted_user_table'): m.collect(coverage,inputs)
    def test_protected_values_not_read_and_unlisted_tables_fail_closed(self):
        with tempfile.TemporaryDirectory() as tmp:
            root=pathlib.Path(tmp);path=root/'copy.sqlite3'
            with sqlite3.connect(path) as db:
                db.execute('CREATE TABLE credentials(password TEXT)');db.execute("INSERT INTO credentials VALUES('synthetic-placeholder')")
                # Deny SQL reads of the protected cell; COUNT and metadata remain legal.
                db.set_authorizer(lambda op,a,b,c,d: sqlite3.SQLITE_DENY if op==sqlite3.SQLITE_READ and a=='credentials' and b=='password' else sqlite3.SQLITE_OK)
                result=m.measure(db,'credentials');self.assertEqual(result['measurement_status'],'protected_content_not_read');self.assertIsNone(result['source_table_hash'])
if __name__=='__main__':unittest.main()

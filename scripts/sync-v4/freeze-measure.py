#!/usr/bin/env python3
"""E29.8: measure explicitly attested, self-contained FROZEN SQLite copies.
No network, discovery, live-file opens, credential reads or database writes.
"""
import argparse, hashlib, json, pathlib, sqlite3, struct
from urllib.parse import quote

SCHEME = "ctox.sqlite.table.v1"
PROTECTED_COLUMNS = {"password", "password_hash", "dkim_private_key", "access_token", "refresh_token", "secret_value", "api_key", "env_value"}
def identifier(name):
    return '"' + name.replace('"', '""') + '"'
def measure(db, table):
    columns = [text(r[1]) for r in db.execute("PRAGMA table_info("+identifier(table)+")")]
    if PROTECTED_COLUMNS.intersection(columns):
        return {"row_count": db.execute("SELECT count(*) FROM " + identifier(table)).fetchone()[0], "source_table_hash": None, "schema_columns": columns, "measurement_status": "protected_content_not_read"}
    cursor = db.execute("SELECT * FROM " + identifier(table))
    hashes = []
    for row in cursor:
        h = hashlib.sha256()
        for cell in row:
            if cell is None: tag, raw = b'n', b''
            elif isinstance(cell, int): tag, raw = b'i', str(cell).encode()
            elif isinstance(cell, float): tag, raw = b'r', struct.pack('>d', cell)
            else:
                # text_factory returns a tagged wrapper, preserving invalid UTF8.
                tag, raw = (b't', cell.raw) if isinstance(cell, Text) else (b'b', cell)
            h.update(tag + len(raw).to_bytes(8, 'big') + raw)
        hashes.append(h.digest())
    hashes.sort()
    h = hashlib.sha256(b'ctox.sqlite.table.v1\0')
    for name in columns:
        raw = name.encode(); h.update(len(raw).to_bytes(8, 'big') + raw)
    h.update(b'\0')
    for digest in hashes: h.update(digest)
    return {"row_count":len(hashes), "source_table_hash":h.hexdigest(), "schema_columns":columns, "measurement_status":"measured"}
class Text:
    def __init__(self, raw): self.raw = raw
    def __str__(self): return self.raw.decode('utf8')
def text(value):
    return str(value) if isinstance(value,Text) else value

def collect(coverage, manifest):
    if manifest.get("schema") != "ctox.v4.freeze_inputs.v1" or manifest.get("frozen") is not True:
        raise ValueError("explicit frozen copy attestation required")
    root = pathlib.Path(manifest["frozen_root"]).resolve(strict=True)
    files = {f["database"]:f for f in manifest["files"]}
    if len(files) != len(manifest["files"]): raise ValueError("duplicate database choice")
    entries = coverage["tables"]
    expected = {}
    for row in entries: expected.setdefault(row["database"],set()).add(row["table"])
    if set(files) != set(expected): raise ValueError("freeze choices must match coverage databases exactly")
    results=[]; selected=[]
    for database in sorted(expected):
        f=files[database]
        if set(f)!={"database","live_file","frozen_file","lsof_receipt_sha256"}: raise ValueError("unknown/missing file choice field")
        receipt=f["lsof_receipt_sha256"]
        if len(receipt)!=64 or any(c not in '0123456789abcdef' for c in receipt): raise ValueError("invalid lsof receipt digest")
        original=pathlib.Path(f["live_file"]); named=pathlib.Path(f["frozen_file"])
        if not original.is_absolute() or not named.is_absolute() or named.is_symlink(): raise ValueError("absolute non-symlink frozen path required")
        path=named.resolve(strict=True)
        if path==original.resolve() or root not in path.parents or (original.exists() and path.samefile(original)): raise ValueError("not a separate frozen copy")
        identity=path.stat()
        selected.append(dict(f, identity={'device':identity.st_dev,'inode':identity.st_ino,'size':identity.st_size,'mtime_ns':identity.st_mtime_ns}))
        if 'secret' in database.lower():
            results.extend({"database":database,"table":t,"row_count":None,"source_table_hash":None,"schema_columns":None,"measurement_status":"secret_store_api_only"} for t in sorted(expected[database]))
            continue
        if any(pathlib.Path(str(path)+suffix).exists() and pathlib.Path(str(path)+suffix).stat().st_size for suffix in ['-wal','-journal']): raise ValueError("frozen copy must be a self-contained SQLite backup, not an active WAL/journal")
        before=path.stat()
        uri='file:'+quote(str(path),safe='/')+'?mode=ro&immutable=1'
        with sqlite3.connect(uri,uri=True) as db:
            db.execute('PRAGMA query_only=ON');db.execute('BEGIN')
            actual={r[0] for r in db.execute("SELECT name FROM sqlite_master WHERE type='table' AND substr(name,1,7)<>'sqlite_'")}
            if actual != expected[database]: raise ValueError("table inventory differs: "+database+" missing="+str(sorted(expected[database]-actual))+" extra="+str(sorted(actual-expected[database])))
            db.text_factory=Text
            for table in sorted(actual):
                result=measure(db,table)
                results.append(dict(database=database,table=table,**result))
            db.rollback()
        db.close()
        after=path.stat()
        if (before.st_ino,before.st_size,before.st_mtime_ns)!=(after.st_ino,after.st_size,after.st_mtime_ns): raise ValueError("frozen file changed during measurement")
    return {"schema":"ctox.v4.freeze_measurement.v1","algorithm":SCHEME,"frozen":True,"secret_content_read":False,"selected_files":selected,"tables":results,"complete":all(r['measurement_status']=='measured' for r in results)}

def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('--coverage',required=True);p.add_argument('--inputs',required=True);p.add_argument('--output',required=True)
    a=p.parse_args();coverage=json.loads(pathlib.Path(a.coverage).read_text());manifest=json.loads(pathlib.Path(a.inputs).read_text())
    result=collect(coverage,manifest)
    with open(a.output,'x') as out: json.dump(result,out,ensure_ascii=False,indent=2);out.write('\n')
if __name__=='__main__': main()

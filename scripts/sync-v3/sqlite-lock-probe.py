#!/usr/bin/env python3
"""Bounded writer-admission probe; only an isolated S0 fixture store is allowed."""
import argparse,json,pathlib,sqlite3,time,threading,tempfile

def probe(database, budget_ms=500):
    start=time.monotonic(); busy=0; error=None; acquired=False
    with sqlite3.connect(f'file:{pathlib.Path(database).resolve()}?mode=rw',uri=True,timeout=0) as db:
        # Fence target to the fixture schema before a writer-admission attempt.
        if not db.execute("SELECT 1 FROM sqlite_master WHERE name='ctox_business_os__sync_v3_scale_leads__v0'").fetchone():
            raise RuntimeError('Not an isolated S0 scale store')
        while True:
            try:
                db.execute('BEGIN IMMEDIATE'); acquired=True; db.execute('ROLLBACK'); break
            except sqlite3.OperationalError as exc:
                code=getattr(exc,'sqlite_errorcode',None)
                # Python3.10 lacks exception codes; accept only exact SQLite lock messages.
                if code is None: code={'database is locked':5,'database table is locked':6}.get(str(exc))
                if code not in (5,6): raise
                busy+=1
                if (time.monotonic()-start)*1000>=budget_ms: error=code; break
                time.sleep(.01)
    return dict(elapsedMs=(time.monotonic()-start)*1000,busyRetries=busy,acquired=acquired,errorCode=error,
        definition='BEGIN IMMEDIATE then immediate ROLLBACK; SQLITE_BUSY/LOCKED retries, 10ms retry sleep, 500ms budget; Python>=3.11 exception code, Python3.10 exact SQLite lock-message classification; no data changed. Not native internal busy-handler telemetry.')

def selftest():
    with tempfile.TemporaryDirectory() as d:
        p=pathlib.Path(d)/'fixture.sqlite'; a=sqlite3.connect(p)
        a.execute('CREATE TABLE ctox_business_os__sync_v3_scale_leads__v0(id TEXT)'); a.commit()
        assert probe(p)['acquired']; a.execute('BEGIN IMMEDIATE')
        blocked=probe(p,30); assert not blocked['acquired'] and blocked['busyRetries']>0
        a.rollback(); assert probe(p)['acquired']; a.close()
        q=pathlib.Path(d)/'foreign.sqlite'; sqlite3.connect(q).close()
        try: probe(q); raise AssertionError('Foreign store accepted')
        except RuntimeError: pass
    print('sqlite_lock_probe_selftest=passed')
if __name__=='__main__':
    parser=argparse.ArgumentParser();parser.add_argument('--database');parser.add_argument('--self-test',action='store_true');args=parser.parse_args()
    if args.self_test:selftest()
    elif args.database:print(json.dumps(probe(args.database)))
    else:parser.error('database or self-test required')

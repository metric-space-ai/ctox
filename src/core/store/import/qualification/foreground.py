#!/usr/bin/env python3
"""Real CLI/foreground cutover witness on a positively identified DevOps fixture.
No live customer database, secret store contents, or installed application is used.
Run only through gpu-build-run.sh. All processes have bounded lifetimes.
"""
import argparse
import copy
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import re
import shutil
import signal
import sqlite3
import subprocess
import time

OWNER = "01a0879f-bdaa"
CUSTODIAN = "01a087a0-169a-7e23-ba3d-71352256cbfb"

def encoded(v):
    return json.dumps(v, ensure_ascii=False, separators=(",", ":")).encode()

def save(p, value):
    p.write_text(json.dumps(value, ensure_ascii=False, indent=2) + "\n")

def backup(source, target):
    target.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
    assert not target.exists()
    src = sqlite3.connect(source.as_uri() + "?mode=ro", uri=True, timeout=5)
    dst = sqlite3.connect(target)
    deadline = time.monotonic() + 30
    def progress(*_):
        if time.monotonic() > deadline:
            raise TimeoutError("SQLite backup deadline")
    try:
        src.backup(dst, pages=128, progress=progress)
    finally:
        dst.close()
        src.close()

def stop(p):
    if p.poll() is None:
        os.killpg(p.pid, signal.SIGTERM)
        try:
            p.wait(timeout=3)
        except subprocess.TimeoutExpired:
            os.killpg(p.pid, signal.SIGKILL)
            p.wait(timeout=2)

def main():
    a = argparse.ArgumentParser(description=__doc__)
    a.add_argument("--binary", required=True, type=Path)
    a.add_argument("--custody-snapshot", required=True, type=Path)
    a.add_argument("--output", required=True, type=Path)
    a.add_argument("--v4-root", required=True, type=Path)
    a.add_argument("--platform", required=True, type=Path)
    args = a.parse_args()
    def interrupted(*_):
        raise SystemExit("acceptance deadline")
    signal.signal(signal.SIGTERM, interrupted)
    assert os.uname().sysname == "Linux" and os.environ.get("BUILD_LANE_BIN")
    binary = args.binary.resolve(strict=True)
    custody = args.custody_snapshot.resolve(strict=True)
    meta = json.loads((custody / "snapshot.json").read_text())
    assert meta["owner"] == CUSTODIAN and meta["synthetic"] and meta["complete"]
    mark = json.loads((Path(meta["sourceRoot"]) / ".sync-v4-e2e.json").read_text())
    assert mark["owner"] == CUSTODIAN and mark["synthetic"] is True
    out = args.output.resolve()
    out.mkdir(mode=0o700)
    root = out / "f"
    root.mkdir(mode=0o700)
    state = root / "runtime"
    state.mkdir(mode=0o700)
    save(root / ".sync-v4-e2e.json", {"owner":OWNER,"synthetic":True,"custody":str(custody)})
    result = {"schema":"ctox.cutover.foreground.acceptance.v1", "owner":OWNER,
        "custody":str(custody), "pass":False, "steps":[], "processes":[],
        "binary_sha256":hashlib.sha256(binary.read_bytes()).hexdigest(), "script_sha256":hashlib.sha256(Path(__file__).read_bytes()).hexdigest(), "secret_content_read":False}
    # Restore by SQLite backup API, not by copying any live SQLite/WAL files.
    for row in meta["databases"]:
        relative = Path(row["relative"])
        assert not relative.is_absolute() and ".." not in relative.parts
        if "secret" in relative.as_posix().lower():
            continue
        source = custody / "databases" / relative
        assert not source.is_symlink()
        backup(source, state / relative)
    # Existing typed setting, only in our disposable restored fixture. Never
    # bind the host's/default tenant's fixed port during isolated acceptance.
    with sqlite3.connect(state / "ctox-runtime.sqlite3") as setup:
        setup.execute("INSERT INTO runtime_env_kv(env_key,env_value) VALUES('CTOX_BUSINESS_OS_WEB_ADDR','127.0.0.1:0') ON CONFLICT(env_key) DO UPDATE SET env_value=excluded.env_value")
        setup.execute("INSERT INTO runtime_env_kv(env_key,env_value) VALUES('CTOX_BUSINESS_OS_WEB_AUTOSTART','0') ON CONFLICT(env_key) DO UPDATE SET env_value=excluded.env_value")
    result["fixture_control_overrides"] = {"CTOX_BUSINESS_OS_WEB_ADDR":"127.0.0.1:0","CTOX_BUSINESS_OS_WEB_AUTOSTART":"0"}
    repo = Path.cwd()
    for name in ["src", "contracts", "Cargo.toml"]:
        (root / name).symlink_to(repo / name)
    home = root / "home"
    home.mkdir()
    env = dict(PATH=os.environ["PATH"], HOME=str(home), TMPDIR=os.environ["TMPDIR"], LANG="C.UTF-8",
        BUILD_LANE_BIN=os.environ["BUILD_LANE_BIN"], RUST_BACKTRACE="1", CTOX_ROOT=str(root), CTOX_STATE_ROOT=str(state),
        XDG_CONFIG_HOME=str(home / ".config"), XDG_CACHE_HOME=str(home / ".cache"))
    def run(name, command, expected=0, timeout=90):
        started = time.monotonic()
        r = subprocess.run([str(x) for x in command], cwd=root, env=env, stdout=subprocess.PIPE,
            stderr=subprocess.PIPE, timeout=timeout)
        (out / (name + ".stdout")).write_bytes(r.stdout)
        (out / (name + ".stderr")).write_bytes(r.stderr)
        result["steps"].append({"name":name,"exit":r.returncode,"seconds":time.monotonic()-started,
            "command":[str(x) for x in command]})
        save(out / "result.json", result)
        if expected is not None:
            assert r.returncode == expected, (name, r.returncode, r.stderr[-3000:].decode(errors="replace"))
        return r
    # Freeze V3 BEFORE V4 initializes its own _seq/_changes/schema tables.
    # Otherwise the input would mix freshly generated V4 bookkeeping with V3.
    frozen = out / "frozen"
    frozen.mkdir(mode=0o700)
    live = state / "business-os-rxdb.sqlite3"
    source_identity = (live.stat().st_dev, live.stat().st_ino)
    backup(live, frozen / "sync.sqlite3")
    (frozen / "sync.sqlite3").chmod(0o444)
    result["snapshot_order"] = "restore V3, SQLite API freeze, start V4 foreground, submit import over IPC"
    log = (out / "foreground.log").open("w")
    daemon = subprocess.Popen([str(binary), "service", "--foreground", "--root", str(root)],
        cwd=root, env=env, stdout=log, stderr=log, start_new_session=True)
    result["processes"].append({"pid":daemon.pid,"pgid":daemon.pid,"purpose":"isolated synthetic foreground",
        "stop_condition":"finally TERM then KILL; outer acceptance job <= 300 seconds"})
    try:
        deadline = time.monotonic() + 35
        while daemon.poll() is None and time.monotonic() < deadline:
            if "ctox v4 store daemon ready:" in (out / "foreground.log").read_text():
                break
            time.sleep(.1)
        assert daemon.poll() is None and "ctox v4 store daemon ready:" in (out / "foreground.log").read_text(), "foreground not ready"
        # A second actual daemon must fail before opening another writer.
        second = run("second-writer", [binary,"service","--foreground","--root",root], expected=None, timeout=15)
        assert second.returncode != 0 and b"AlreadyOwned" in second.stderr, "second owner not rejected with typed fence"
        assert source_identity == (live.stat().st_dev, live.stat().st_ino), "source file incarnation changed"
        descriptors = []
        for fd in (Path("/proc") / str(daemon.pid) / "fd").iterdir():
            try:
                target = os.readlink(fd)
                if target == str(live): descriptors.append({"fd":fd.name,"path":target})
            except OSError:
                pass
        assert descriptors, "foreground source-file custody not proven"
        receipt = {"owner":OWNER,"pid":daemon.pid,"source":str(live),"open_descriptors":descriptors}
        save(out / "source-custody.json", receipt)
        receipt_sha = hashlib.sha256(encoded(receipt)).hexdigest()
        for part in ["manifests", "schemas"]:
            shutil.copytree(args.v4_root / part, frozen / "v4" / part)
        (frozen / "v4/tools/manifest/out").mkdir(parents=True)
        shutil.copyfile(args.v4_root / "tools/manifest/out/ownership.json", frozen / "v4/tools/manifest/out/ownership.json")
        shutil.copyfile(args.platform, frozen / "ctox-platform.manifest.json")
        spec = importlib.util.spec_from_file_location("freeze", repo / "scripts/sync-v4/freeze-measure.py")
        freeze = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(freeze)
        db = sqlite3.connect((frozen / "sync.sqlite3").as_uri()+"?mode=ro&immutable=1", uri=True)
        names = [r[0] for r in db.execute("SELECT name FROM sqlite_master WHERE type='table' AND substr(name,1,7)<>'sqlite_' ORDER BY name")]
        internals = [n for n in names if n.startswith("ctox_business_os__") and n.rsplit("__v",1)[0].removeprefix("ctox_business_os__").strip("_") == "rxdb_internal"]
        assert len(internals) == 1, "explicit unique RxDB internal metadata required"
        internal = internals[0]
        decisions = []
        for name in names:
            fields = list(db.execute("PRAGMA table_info("+freeze.identifier(name)+")"))
            physical = re.fullmatch(r"ctox_business_os__(.+)__v([0-9]+)", name)
            cols = [f[1] for f in fields]
            count = db.execute("SELECT count(*) FROM "+freeze.identifier(name)).fetchone()[0]
            if physical:
                collection, version = physical[1], int(physical[2])
                key, body = "id", "data"
                entity, rule, mode = "app", "E29-R1", {"kind":"rows"}
                if name == internal:
                    rule, mode = "E29-R4", {"kind":"keep_core"}
            else:
                # Technical rows are archived with an explicitly verified key;
                # no synthetic row numbers or MAX(version) selection is used.
                candidates = [f[1] for f in sorted(fields,key=lambda f:f[5]) if f[5]]
                key = next((k for k in candidates if db.execute("SELECT count(DISTINCT "+freeze.identifier(k)+") FROM "+freeze.identifier(name)).fetchone()[0] == count), None)
                if key is None and count == 0:
                    key = cols[0]
                assert key, "no verified single-column source key: "+name
                collection, version, body = "frozen_technical:"+name, 0, None
                entity, rule, mode = "core", "E29-R2", {"kind":"rows"}
            db.text_factory = freeze.Text
            measured = freeze.measure(db, name)
            db.text_factory = str
            assert measured["measurement_status"] == "measured", "protected collection requires explicit retention"
            measurement = {"row_count":measured["row_count"],"schema_columns":measured["schema_columns"],"source_table_hash":measured["source_table_hash"]}
            decisions.append({"rule":rule,"table":{"name":name,"collection":collection,"version":version,
                "id_column":key,"body_column":body,"entity":entity,"mode":mode},"measurement":measurement})
        db.close()
        measurements = {d["table"]["name"]:d["measurement"] for d in decisions}
        snapshot = {"schema":"ctox.v4.cutover_snapshot.v1","frozen":True,"method":"sqlite_backup_api",
            "imported_at_ms":int(time.time()*1000),"archive_custodian_user":"synthetic-custodian",
            "release":{"v4_root":str(frozen/"v4"),"platform_manifest":str(frozen/"ctox-platform.manifest.json"),
                "app_ver":"main@4794af03d+v4","client_limit":100000,"byte_budget":33554432,"rights_ver":1},
            "databases":[{"choice":{"database":"business-os-rxdb.sqlite3","live_file":str(live),"frozen_file":str(frozen/"sync.sqlite3"),"lsof_receipt_sha256":receipt_sha},
                "measurement_sha256":hashlib.sha256(encoded(measurements)).hexdigest(),"kind":"rxdb","internal_table":internal,"tables":decisions,"collectors":[]}],
            "inventory":{"business-os-rxdb.sqlite3":names},"retained":[]}
        path = frozen / "cutover.json"
        save(path, snapshot)
        path.chmod(0o444)
        command = [binary,"business-os","v4","cutover","import","--snapshot",path]
        dry = run("dry-run", command+["--dry-run"])
        dry = json.loads(dry.stdout)
        assert dry["report"]["inventory_complete"] and len(dry["report"]["tables"]) == len(names)
        bad = copy.deepcopy(snapshot)
        omitted = next(d for d in bad["databases"][0]["tables"] if d["table"]["name"] != internal)
        bad["databases"][0]["tables"].remove(omitted)
        bad["inventory"]["business-os-rxdb.sqlite3"].remove(omitted["table"]["name"])
        measures = {d["table"]["name"]:d["measurement"] for d in bad["databases"][0]["tables"]}
        bad["databases"][0]["measurement_sha256"] = hashlib.sha256(encoded(measures)).hexdigest()
        bad_path = frozen / "omitted-table.json"
        save(bad_path, bad)
        rejected = run("omitted-table", [binary,"business-os","v4","cutover","import","--snapshot",bad_path,"--dry-run"], expected=None)
        assert rejected.returncode != 0, "undeclared physical table was silently skipped"
        receipts = []
        for line in rejected.stderr.splitlines():
            try:
                value = json.loads(line)
                if isinstance(value,dict) and "report_path" in value:
                    receipts.append(value)
            except ValueError:
                pass
        assert len(receipts) == 1
        rejection = json.loads(Path(receipts[0]["report_path"]).read_text())
        assert "cutover_table_without_decision" in rejection["error"], rejection
        result["input_mutation"] = {"omit_table":omitted["table"]["name"],"exit":rejected.returncode,"error":rejection["error"]}
        imported = json.loads(run("import",command).stdout)
        assert imported["report"]["inventory_complete"]
        assert all(t["target_rows"] == t["expected_target_rows"] for t in imported["report"]["tables"]), "per-table coverage incomplete"
        def frozen_head(label):
            target = out / (label+".sqlite3")
            backup(live, target)
            with sqlite3.connect(target.as_uri()+"?mode=ro&immutable=1", uri=True) as check:
                return check.execute("SELECT v FROM _seq WHERE singleton=1").fetchone()[0]
        before_replay = frozen_head("after-import")
        replay = json.loads(run("replay",command).stdout)
        after_replay = frozen_head("after-replay")
        assert before_replay == after_replay, "replay changed the canonical sequence"
        result["replay_sequence"] = {"before":before_replay,"after":after_replay}
        assert all(c["inserted"] == 0 for c in replay["report"]["import"]["collections"].values()), "replay reinserted rows"
        result.update(inventory_complete=True, table_coverage=imported["report"]["tables"],
            snapshot_sha256=hashlib.sha256(path.read_bytes()).hexdigest(), selected_scope="all tables of the synthetic sync DB; other core DBs require specialized attachment")
        result["pass"] = True
    finally:
        stop(daemon)
        log.close()
        result["foreground_exit"] = daemon.returncode
        result["processes"][0]["terminal"] = True
        save(out / "result.json", result)
    print(json.dumps({"pass":result["pass"],"evidence":str(out/"result.json")}))
if __name__ == "__main__":
    main()

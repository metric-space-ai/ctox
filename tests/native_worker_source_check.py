#!/usr/bin/env python3
"""Focused Linux Git-source verification; not a daemon or database check."""
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile

root = Path(__file__).resolve().parents[1]
modules = [root / "src/core/business_os" / (name + ".rs")
           for name in ("computer_capabilities", "build_source", "build_delivery")]
before = {str(path.relative_to(root)): hashlib.sha256(path.read_bytes()).hexdigest()
          for path in modules}
staging = Path(os.environ["TMPDIR"])
with tempfile.TemporaryDirectory(prefix="worker-source-check-", dir=staging) as scratch:
    project = Path(scratch)
    (project / "src").mkdir()
    (project / "Cargo.toml").write_text("""[package]
name = "ctox-worker-source-check"
version = "0.0.0"
edition = "2021"
[dependencies]
anyhow = "1"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
sha2 = "0.10"
tempfile = "3"
uuid = { version = "1", features = ["v4"] }
""")
    # These database entry points are outside source reconstruction's contract.
    # They fail closed and are never called by the scoped tests. Types and
    # capability validation come from the actual native production module.
    lib = """pub mod business_os {
        pub mod store {
            pub fn open_store(_: &std::path::Path) -> anyhow::Result<()> {
                anyhow::bail!("database is outside focused source verification")
            }
            pub fn outbound_load_records_by_string_field(
                _: &(), _: &str, _: &str, _: &str
            ) -> anyhow::Result<Vec<serde_json::Value>> {
                anyhow::bail!("database is outside focused source verification")
            }
        }
        pub mod store_workjet_computers {
            pub const COMPUTERS_COLLECTION: &str = "workjet_computers";
        }
"""
    for path in modules:
        lib += '#[path = ' + json.dumps(str(path)) + '] pub mod ' + path.stem + ';\n'
    (project / "src/lib.rs").write_text(lib + "}\n")
    env = dict(os.environ, CARGO_BUILD_JOBS="2", RUST_TEST_THREADS="2")
    subprocess.run(["cargo", "test", "--offline", "--manifest-path", str(project / "Cargo.toml"),
                    "--jobs", "2", "business_os::build_", "--", "--test-threads=2"],
                   env=env, check=True)
after = {str(path.relative_to(root)): hashlib.sha256(path.read_bytes()).hexdigest()
         for path in modules}
assert before == after, "native source changed during focused verification"
print("WORKER_SOURCE_EXACT_MODULES_OK", json.dumps(after, sort_keys=True))

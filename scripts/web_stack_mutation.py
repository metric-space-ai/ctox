#!/usr/bin/env python3
"""Isolated daemon-bound stealth mutation and restored positive control.

Invoked by web_stack_source.py under the caller's shared heavy-job admission.
Neither the operator checkout, root lock nor Cargo Git cache is modified.
"""
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import tempfile
import time

from web_stack_source import BindingError, bounded_env, capture, digest_file, resolve, source_state

MARKER = "(() => {\n  'use strict';"
MUTATION = MARKER + "\n  return; /* e2e-sabotage */"


def copy_revision(checkout, revision, destination, deadline=None):
    # Local shared-object clones retain exact Git revision/build metadata and
    # tracked symlinks. No upstream fetch or live checkout mutation occurs.
    commands = (["git", "clone", "--shared", "--no-checkout", str(checkout), str(destination)],
                ["git", "-C", str(destination), "checkout", "--detach", revision])
    for command in commands:
        remaining = 180 if deadline is None else min(180, deadline - time.monotonic())
        if remaining <= 0:
            raise BindingError("source copy deadline reached")
        result = subprocess.run(command, cwd=checkout, text=True, stdout=subprocess.PIPE,
                                stderr=subprocess.PIPE, timeout=remaining)
        if result.returncode:
            raise BindingError("cannot copy exact source: " + result.stderr)


def mutated_bytes(original):
    text = original.decode("utf-8")
    if text.count(MARKER) != 1:
        raise BindingError("canonical stealth IIFE opening must match exactly once")
    return text.replace(MARKER, MUTATION, 1).encode("utf-8")


def require_probe(stdout, code, negative):
    try:
        probes = json.loads(stdout)["probes"]
        if not isinstance(probes, list) or len(probes) != 1:
            raise BindingError("ambiguous probe evidence")
        count = probes[0]["failed_count"]
        failures = probes[0].get("failed_tests", [])
    except (ValueError, KeyError, IndexError, TypeError):
        raise BindingError("probe did not produce structured failure/pass evidence")
    if (type(count) is not int or count < 0 or not isinstance(failures, list)
            or not all(isinstance(name, str) and name.strip() for name in failures)
            or count != len(failures)):
        raise BindingError("ambiguous or malformed probe evidence")
    if negative:
        if code == 0 or count <= 0:
            raise BindingError("mutated probe must report actual failed tests, not a startup/network error")
    elif code != 0 or count != 0:
        raise BindingError("unmodified/restored positive control did not pass")
    return {"exit": code, "failed_count": count, "failed_tests": failures}


def direct_package(metadata, root):
    packages = {p["id"]: p for p in metadata["packages"]}
    roots = [p for p in packages.values() if Path(p["manifest_path"]).resolve() == (root / "Cargo.toml").resolve()]
    if len(roots) != 1:
        raise BindingError("isolated daemon root missing")
    nodes = [n for n in metadata["resolve"]["nodes"] if n["id"] == roots[0]["id"]]
    if len(nodes) != 1:
        raise BindingError("isolated daemon resolution missing")
    matches = [packages[d["pkg"]] for d in nodes[0]["deps"]
               if d["name"] in ("ctox_web_stack", "ctox-web-stack")]
    if len(matches) != 1 or matches[0]["name"] != "ctox-web-stack":
        raise BindingError("isolated direct web-stack dependency missing/ambiguous")
    return matches[0]


def run_mutation(root, binding):
    if os.name != "posix":
        raise BindingError("isolated mutation requires Linux/macOS process-group cleanup")
    before = source_state(root)
    if capture(["git", "status", "--porcelain"], root):
        raise BindingError("mutation probe requires a committed clean source checkout")
    temporary_base = Path(tempfile.gettempdir()).resolve()
    if sys.platform == "darwin" and not str(temporary_base).startswith("/Volumes/tmp/"):
        raise BindingError("set TMPDIR on /Volumes/tmp before the mutation probe")
    # A retained task-owned evidence directory, not a background watcher.
    directory = Path(tempfile.mkdtemp(prefix="web-stack-mutation-", dir=temporary_base))
    record = {"root_head": before["head"], "binding": binding, "directory": str(directory),
              "steps": [], "passed": False, "limits": {"workers": 2, "seconds": 1800}}
    evidence = directory / "result.json"
    print("mutation evidence: " + str(evidence), flush=True)
    deadline = time.monotonic() + 1800
    env = bounded_env()
    env.update(CTOX_ROOT=str(directory / "ctox"), CTOX_STATE_ROOT=str(directory / "ctox/runtime"),
               CARGO_TARGET_DIR=str(directory / "target"), npm_config_cache=str(directory / "npm-cache"),
               PLAYWRIGHT_BROWSERS_PATH=str(directory / "browser-cache"), XDG_CACHE_HOME=str(directory / "cache"))
    old_handler = signal.getsignal(signal.SIGTERM)
    def interrupted(signum, frame):
        raise KeyboardInterrupt("mutation probe interrupted")
    signal.signal(signal.SIGTERM, interrupted)
    asset = None
    original = None

    def save():
        evidence.write_text(json.dumps(record, indent=2) + "\n")

    def execute(label, command, timeout=600):
        remaining = min(timeout, deadline - time.monotonic())
        if remaining <= 0:
            raise BindingError("mutation probe deadline reached")
        stdout_path = directory / (label + ".stdout")
        stderr_path = directory / (label + ".stderr")
        with stdout_path.open("w") as stdout, stderr_path.open("w") as stderr:
            child = subprocess.Popen(command, cwd=directory / "ctox", env=env,
                                     stdout=stdout, stderr=stderr, start_new_session=True)
            row = {"stage": label, "pid": child.pid, "pgid": child.pid,
                   "stdout": str(stdout_path), "stderr": str(stderr_path)}
            record["steps"].append(row); save()
            print("stage=%s pid=%s pgid=%s" % (label, child.pid, child.pid), flush=True)
            try:
                code = child.wait(timeout=remaining)
                row["exit"] = code
            except BaseException as error:
                row["error"] = str(error)
                raise
            finally:
                try:
                    os.killpg(child.pid, signal.SIGTERM)
                    child.wait(timeout=5)
                except ProcessLookupError:
                    pass
                except subprocess.TimeoutExpired:
                    os.killpg(child.pid, signal.SIGKILL); child.wait()
                # An exited leader can leave descendants behind. Escalate on
                # the owned group even when wait() has already reaped it.
                try:
                    os.killpg(child.pid, 0)
                    os.killpg(child.pid, signal.SIGKILL)
                    row["cleanup"] = "remaining owned group killed"
                except ProcessLookupError:
                    row["cleanup"] = "owned group absent"
                save()
        return code, stdout_path.read_text()

    try:
        checkout = Path(capture(["git", "rev-parse", "--show-toplevel"], Path(binding["manifest"]).parent).strip())
        relative = Path(binding["manifest"]).relative_to(checkout)
        copy_revision(root, before["head"], directory / "ctox", deadline)
        copy_revision(checkout, binding["revision"], directory / "workjet", deadline)
        manifest = directory / "workjet" / relative
        asset = manifest.parent / "assets/stealth_init.js"
        original = asset.read_bytes()
        record["original_asset_sha256"] = digest_file(asset)
        if digest_file(asset) != digest_file(Path(binding["manifest"]).parent / "assets/stealth_init.js"):
            raise BindingError("isolated canonical asset differs from the resolved source")
        mutant = mutated_bytes(original)
        source_url = binding["source"].split("#", 1)[0]
        config = "patch.%s.ctox-web-stack.path=%s" % (json.dumps(source_url), json.dumps(str(manifest.parent)))
        cargo = ["--manifest-path", str(directory / "ctox/Cargo.toml"), "--config", config]
        # Only the isolated root lock can change to encode the intentional path
        # substitution. Every subsequent build is locked to that resolved graph.
        code, output = execute("resolve-isolated", ["cargo", "metadata", "--format-version", "1"] + cargo, 180)
        if code:
            raise BindingError("isolated path resolution failed")
        package = direct_package(json.loads(output), directory / "ctox")
        if package.get("source") is not None or Path(package["manifest_path"]).resolve() != manifest.resolve():
            raise BindingError("daemon does not actually compile the isolated canonical copy")
        record["isolated_manifest"] = str(manifest)
        record["isolated_lock_sha256"] = digest_file(directory / "ctox/Cargo.lock")
        record["original_asset_sha256"] = digest_file(asset)

        def build(label):
            code, output = execute(label, ["cargo", "build", "--locked", "--bin", "ctox",
                                           "--message-format=json-render-diagnostics"] + cargo)
            if code:
                raise BindingError(label + " failed")
            messages = [json.loads(line) for line in output.splitlines() if line.startswith("{")]
            artifacts = [m for m in messages if m.get("reason") == "compiler-artifact"]
            if not any(m.get("package_id") == package["id"] for m in artifacts):
                raise BindingError("build did not report the isolated dependency artifact")
            executables = [m["executable"] for m in artifacts if m.get("target", {}).get("name") == "ctox" and m.get("executable")]
            if len(executables) != 1 or digest_file(directory / "ctox/Cargo.lock") != record["isolated_lock_sha256"]:
                raise BindingError("ambiguous executable or changed isolated dependency lock")
            record[label] = {"executable": executables[0], "sha256": digest_file(executables[0]),
                             "asset_sha256": digest_file(asset)}
            save()
            return executables[0]

        executable = build("build-initial")
        code, _ = execute("browser-prepare", [executable, "web", "browser-prepare", "--install-reference", "--install-browser"], 180)
        if code:
            raise BindingError("isolated browser preparation failed")
        def probe(label, negative):
            code, output = execute(label, [executable, "web", "unlock", "baseline", "incolumitas", "--record"], 180)
            record[label] = require_probe(output, code, negative); save()
        probe("initial-positive", False)
        negative_error = None
        try:
            asset.write_bytes(mutant)
            executable = build("build-mutated")
            if record["build-mutated"]["sha256"] == record["build-initial"]["sha256"]:
                raise BindingError("mutation did not change the executable")
            probe("mutated-negative", True)
        except Exception as error:
            negative_error = error
            record["negative_error"] = str(error)
        finally:
            asset.write_bytes(original)
            record["asset_restored"] = digest_file(asset) == record["original_asset_sha256"]
            save()
        executable = build("build-restored")
        probe("restored-positive", False)
        if negative_error is not None:
            raise negative_error
        record["passed"] = True
    except BaseException as error:
        record["error"] = str(error)
        raise
    finally:
        if asset is not None and original is not None:
            asset.write_bytes(original)
            record["asset_restored"] = digest_file(asset) == record.get("original_asset_sha256")
        try:
            record["original_root_unchanged"] = source_state(root) == before
            record["original_binding_unchanged"] = resolve(root) == binding
        except Exception as error:
            record["final_validation_error"] = str(error)
            record["original_root_unchanged"] = False
            record["original_binding_unchanged"] = False
        if not record["original_root_unchanged"] or not record["original_binding_unchanged"]:
            record["passed"] = False
        save()
        signal.signal(signal.SIGTERM, old_handler)
    return 0 if record["passed"] else 1

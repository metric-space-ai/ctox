#!/usr/bin/env python3
"""Isolated daemon-bound stealth mutation and restored positive control.

Invoked by web_stack_source.py under the caller's shared heavy-job admission.
Neither the operator checkout, root lock nor Cargo Git cache is modified.
"""
import json
import os
from pathlib import Path
import re
import shutil
import signal
import subprocess
import sys
import tempfile
import time

from web_stack_source import BindingError, bounded_env, capture, digest_file, remaining_budget, resolve, source_state

MARKER = "(() => {\n  'use strict';"
# Concatenated IIFEs may start after a semicolon rather than a new line.
OPENING = re.compile(r'''\(\(\) => \{\r?\n[ \t]+(?P<quote>['"])use strict(?P=quote);[ \t]*(?=\r?\n)''')


BUILD_TIMEOUT_SECONDS = 1200


class CleanupError(BindingError):
    """Owned processes could not be proven stopped; never continue probing."""


def signal_owned_group(child, signum, row):
    try:
        os.killpg(child.pid, signum)
        return True
    except ProcessLookupError:
        return False
    except PermissionError as error:
        # A denied signal proves neither liveness nor absence. Query numeric
        # group membership, without exposing unrelated command lines.
        try:
            result = subprocess.run(["ps", "-axo", "pid=,pgid="], check=True,
                                    stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                                    text=True, timeout=5)
            members = []
            for line in result.stdout.splitlines():
                if not line.strip():
                    continue
                pid, pgid = map(int, line.split())
                if pgid == child.pid:
                    members.append(pid)
        except Exception as inspection_error:
            raise CleanupError("owned group inspection failed after denied signal") from inspection_error
        row["denied_signal"] = {"signal": signum, "members": members}
        if members:
            raise CleanupError("signal denied; owned group still present: %s" % members) from error
        row["cleanup"] = "owned group absent (ps verified after denied signal)"
        return False


def stop_owned_group(child, row):
    if signal_owned_group(child, signal.SIGTERM, row):
        try:
            child.wait(timeout=5)
        except subprocess.TimeoutExpired:
            signal_owned_group(child, signal.SIGKILL, row)
            child.wait(timeout=5)
    else:
        child.wait(timeout=5)
    # An exited leader may leave descendants; keep group escalation.
    if signal_owned_group(child, 0, row):
        if signal_owned_group(child, signal.SIGKILL, row):
            row["cleanup"] = "remaining owned group killed"
    else:
        row.setdefault("cleanup", "owned group absent")


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
    openings = list(OPENING.finditer(text))
    if len(openings) != 1:
        raise BindingError("canonical stealth IIFE opening must match exactly once")
    opening = openings[0]
    newline = "\r\n" if "\r\n" in opening.group() else "\n"
    return (text[:opening.end()] + newline + "  return; /* e2e-sabotage */"
            + text[opening.end():]).encode("utf-8")


def require_browser_preparation(stdout, reference_dir):
    """A successful CLI report is not proof that its browser smoke ran."""
    try:
        payload = json.loads(stdout)
    except (ValueError, TypeError) as error:
        raise BindingError("browser preparation did not produce a JSON report") from error
    if (not isinstance(payload, dict) or payload.get("ok") is not True
            or payload.get("tool") != "ctox_browser_prepare"):
        raise BindingError("browser preparation did not report its actual CLI result")
    doctor = payload.get("doctor")
    if (not isinstance(doctor, dict) or doctor.get("automation_ready") is not True
            or doctor.get("runner_dependency_installed") is not True
            or doctor.get("runner_browser_installed") is not True):
        raise BindingError("browser preparation prerequisites are incomplete")
    reported_reference = doctor.get("reference_dir")
    if (not isinstance(reported_reference, str) or not reported_reference
            or Path(reported_reference).resolve() != Path(reference_dir).resolve()):
        raise BindingError("browser preparation inspected a different reference directory")
    smoke = doctor.get("smoke")
    if (not isinstance(smoke, dict) or smoke.get("ran") is not True
            or smoke.get("ok") is not True):
        raise BindingError("browser preparation requires an actually passing browser smoke")
    return {"reference_dir": reported_reference, "automation_ready": True,
            "smoke": {"ran": True, "ok": True}}


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


def git_pdf_source(binding):
    repository, revision = binding["source"].rsplit("#", 1)
    if revision != binding["revision"]:
        raise BindingError("canonical PDF source revision differs from web-stack")
    return repository, "git+%s?rev=%s#%s" % (repository, revision, revision)


def retain_pdf_git_dependency(manifest, binding):
    # A Git package's sibling path dependency inherits its Git identity. Moving
    # only web-stack to a path must not turn PDF into a second local package.
    if manifest.resolve() == Path(binding["manifest"]).resolve():
        raise BindingError("cannot rewrite the canonical Cargo checkout")
    repository, source = git_pdf_source(binding)
    old = 'ctox-pdf-parse = { path = "../pdf-parse", optional = true }'
    original = manifest.read_text()
    if original.count(old) != 1:
        raise BindingError("canonical optional PDF sibling selector must match exactly once")
    new = 'ctox-pdf-parse = { git = %s, rev = %s, optional = true }' % (
        json.dumps(repository), json.dumps(binding["revision"]))
    manifest.write_text(original.replace(old, new, 1))
    return source


def require_git_pdf(metadata, package, binding):
    nodes = [n for n in metadata["resolve"]["nodes"] if n["id"] == package["id"]]
    deps = [d for n in nodes for d in n["deps"] if d["name"] in ("ctox_pdf_parse", "ctox-pdf-parse")]
    packages = {p["id"]: p for p in metadata["packages"]}
    if len(deps) != 1 or deps[0]["pkg"] not in packages:
        raise BindingError("isolated canonical PDF dependency missing/ambiguous")
    pdf = packages[deps[0]["pkg"]]
    sibling = Path(binding["manifest"]).parent.parent / "pdf-parse/Cargo.toml"
    if (pdf["name"] != "ctox-pdf-parse" or pdf.get("source") != git_pdf_source(binding)[1]
            or Path(pdf["manifest_path"]).resolve() != sibling.resolve()):
        raise BindingError("isolated PDF dependency lost its original canonical Git identity")
    return {"id": pdf["id"], "source": pdf["source"], "manifest": pdf["manifest_path"]}


def mutation_target_directory(binding):
    # Source isolation does not require discarding this same project's Cargo
    # dependency cache. Source/package/executable receipts still prove each build.
    target = Path(binding["target_directory"])
    if not target.is_absolute():
        raise BindingError("mutation target directory must be absolute")
    target = target.resolve()
    if sys.platform == "darwin" and not str(target).startswith("/Volumes/tmp/"):
        raise BindingError("mutation target directory must be on /Volumes/tmp")
    return target


def reuse_sidecar_bundle(root, isolated, receipt_path, deadline):
    """Copy only the generated bundle from a proven same-input official build."""
    receipt = json.loads(Path(receipt_path).read_text())
    proof = receipt["sidecar_build"]
    if proof.get("official_build_script") is not True or proof.get("scripts_ignored_on_install") is not True:
        raise BindingError("sidecar receipt lacks the official locked build")
    stages = {stage["stage"]: stage for stage in receipt["stages"]}
    for name, suffix in (("00a-locked-sidecar-install", ["ci", "--ignore-scripts", "--no-audit", "--no-fund"]),
                         ("00b-real-sidecar-bundle", ["run", "build"])):
        stage = stages.get(name, {})
        command = stage.get("command", [])
        if stage.get("exit") != 0 or command != ["npm", "--prefix", str(root / "src/core/coding_agents/pi-sidecar"), *suffix]:
            raise BindingError("sidecar receipt has no successful official stage: " + name)
    relative = "src/core/coding_agents/pi-sidecar"
    for selector, field in ((relative, "sidecar_source_tree"),
                            (relative + "/package-lock.json", "package_lock_blob")):
        current = capture(["git", "rev-parse", "HEAD:" + selector], root,
                          timeout=remaining_budget(30, deadline)).strip()
        copied = capture(["git", "rev-parse", "HEAD:" + selector], isolated,
                         timeout=remaining_budget(30, deadline)).strip()
        if current != proof[field] or copied != current:
            raise BindingError("sidecar inputs differ from the recorded build")
    for command, field in ((["node", "--version"], "node_version"), (["npm", "--version"], "npm_version")):
        if capture(command, root, timeout=remaining_budget(30, deadline)).strip() != proof[field]:
            raise BindingError("sidecar build tool version differs from receipt")
    bundle = root / relative / "dist/ctox-pi-sidecar.mjs"
    if not bundle.is_file() or bundle.stat().st_size != proof["bytes"] or digest_file(bundle) != proof["sha256"]:
        raise BindingError("sidecar bundle differs from the recorded official build")
    destination = isolated / relative / "dist/ctox-pi-sidecar.mjs"
    destination.parent.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(bundle, destination)
    if digest_file(destination) != proof["sha256"]:
        raise BindingError("copied sidecar bundle digest differs")
    return {"receipt": str(Path(receipt_path).resolve()), **proof}


def run_mutation(root, binding, sidecar_build_receipt=None):
    if os.name != "posix":
        raise BindingError("isolated mutation requires Linux/macOS process-group cleanup")
    deadline = time.monotonic() + 1800
    before = source_state(root, deadline=deadline)
    if capture(["git", "status", "--porcelain"], root, timeout=remaining_budget(120, deadline)):
        raise BindingError("mutation probe requires a committed clean source checkout")
    temporary_base = Path(tempfile.gettempdir()).resolve()
    if sys.platform == "darwin" and not str(temporary_base).startswith("/Volumes/tmp/"):
        raise BindingError("set TMPDIR on /Volumes/tmp before the mutation probe")
    # A retained task-owned evidence directory, not a background watcher.
    directory = Path(tempfile.mkdtemp(prefix="web-stack-mutation-", dir=temporary_base))
    record = {"root_head": before["head"], "binding": binding, "directory": str(directory),
              "steps": [], "passed": False,
              "limits": {"workers": 2, "seconds": 1800, "build_seconds": BUILD_TIMEOUT_SECONDS}}
    evidence = directory / "result.json"
    print("mutation evidence: " + str(evidence), flush=True)
    env = bounded_env()
    target = mutation_target_directory(binding)
    record["target_directory"] = str(target)
    env.update(CTOX_ROOT=str(directory / "ctox"), CTOX_STATE_ROOT=str(directory / "ctox/runtime"),
               CARGO_TARGET_DIR=str(target), npm_config_cache=str(directory / "npm-cache"),
               PLAYWRIGHT_BROWSERS_PATH=str(directory / "browser-cache"), XDG_CACHE_HOME=str(directory / "cache"))
    old_handler = signal.getsignal(signal.SIGTERM)
    def interrupted(signum, frame):
        raise KeyboardInterrupt("mutation probe interrupted")
    signal.signal(signal.SIGTERM, interrupted)
    asset = None
    original = None
    manifest = None
    original_manifest = None

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
                    stop_owned_group(child, row)
                except Exception as cleanup_error:
                    row["cleanup_error"] = str(cleanup_error)
                    record["cleanup_failed"] = True
                    # Preserve timeout/cancellation as the primary failure.
                    if "error" not in row:
                        raise CleanupError(str(cleanup_error)) from cleanup_error
                finally:
                    save()
        return code, stdout_path.read_text()

    try:
        checkout = Path(capture(["git", "rev-parse", "--show-toplevel"], Path(binding["manifest"]).parent,
                                timeout=remaining_budget(120, deadline)).strip())
        relative = Path(binding["manifest"]).relative_to(checkout)
        copy_revision(root, before["head"], directory / "ctox", deadline)
        copy_revision(checkout, binding["revision"], directory / "workjet", deadline)
        if sidecar_build_receipt is not None:
            record["sidecar_build"] = reuse_sidecar_bundle(root, directory / "ctox",
                                                          sidecar_build_receipt, deadline)
        else:
            sidecar = directory / "ctox/src/core/coding_agents/pi-sidecar"
            for label, arguments, timeout in (
                    ("sidecar-install", ["ci", "--ignore-scripts", "--no-audit", "--no-fund"], 300),
                    ("sidecar-build", ["run", "build"], 120)):
                code, _ = execute(label, ["npm", "--prefix", str(sidecar), *arguments], timeout)
                if code:
                    raise BindingError(label + " failed")
            bundle = sidecar / "dist/ctox-pi-sidecar.mjs"
            record["sidecar_build"] = {"sha256": digest_file(bundle), "bytes": bundle.stat().st_size,
                                       "official_build_script": True, "scripts_ignored_on_install": True}
        save()
        manifest = directory / "workjet" / relative
        original_manifest = manifest.read_bytes()
        if original_manifest != Path(binding["manifest"]).read_bytes():
            raise BindingError("isolated canonical manifest differs from resolved source")
        record["original_manifest_sha256"] = digest_file(manifest)
        record["expected_pdf_git_source"] = retain_pdf_git_dependency(manifest, binding)
        record["isolated_manifest_sha256"] = digest_file(manifest)
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
        metadata = json.loads(output)
        package = direct_package(metadata, directory / "ctox")
        if package.get("source") is not None or Path(package["manifest_path"]).resolve() != manifest.resolve():
            raise BindingError("daemon does not actually compile the isolated canonical copy")
        record["isolated_manifest"] = str(manifest)
        record["canonical_pdf_dependency"] = require_git_pdf(metadata, package, binding)
        record["isolated_lock_sha256"] = digest_file(directory / "ctox/Cargo.lock")
        record["original_asset_sha256"] = digest_file(asset)

        # wha-proto resolves CARGO_MANIFEST_DIR when Cargo invokes its build
        # script. A relocated clone can reuse the checked executable; Cargo
        # still tracks and rebuilds changed package inputs normally.
        record["protobuf_cache_strategy"] = "invocation-time source root; no package clean"
        save()

        def build(label):
            code, output = execute(label, ["cargo", "build", "--locked", "--bin", "ctox",
                                           "--message-format=json-render-diagnostics"] + cargo,
                                   BUILD_TIMEOUT_SECONDS)
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
        code, prepared = execute("browser-prepare", [executable, "web", "browser-prepare", "--install-reference", "--install-browser"], 180)
        if code:
            raise BindingError("isolated browser preparation failed")
        record["browser_preparation"] = require_browser_preparation(
            prepared, directory / "ctox/runtime/browser/interactive-reference")
        save()
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
            if record.get("cleanup_failed"):
                raise
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
        if manifest is not None and original_manifest is not None:
            manifest.write_bytes(original_manifest)
            record["manifest_restored"] = digest_file(manifest) == record.get("original_manifest_sha256")
        if asset is not None and original is not None:
            asset.write_bytes(original)
            record["asset_restored"] = digest_file(asset) == record.get("original_asset_sha256")
        try:
            record["original_root_unchanged"] = source_state(root, deadline=deadline) == before
            record["original_binding_unchanged"] = resolve(root, deadline=deadline) == binding
        except Exception as error:
            record["final_validation_error"] = str(error)
            record["original_root_unchanged"] = False
            record["original_binding_unchanged"] = False
        if not record["original_root_unchanged"] or not record["original_binding_unchanged"]:
            record["passed"] = False
        save()
        signal.signal(signal.SIGTERM, old_handler)
    return 0 if record["passed"] else 1

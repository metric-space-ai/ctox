#!/usr/bin/env python3
"""Bind focused web-stack checks and E2E binaries to Cargo's daemon dependency.

This is a development tool, not a runtime configuration or admission gate.
Run compilation through the host's shared admission gate. Mirror diagnostics
remain available via their manifest, but do not validate the daemon dependency.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import sys
from urllib.parse import urlsplit, urlunsplit


class BindingError(RuntimeError):
    pass


def capture(command, root):
    result = subprocess.run(command, cwd=root, text=True, stdout=subprocess.PIPE,
                            stderr=subprocess.PIPE, timeout=120)
    if result.returncode:
        # Cargo diagnostics go to stderr; never convert resolution failure into
        # a local-manifest fallback.
        sys.stderr.write(result.stderr)
        raise BindingError("command failed (exit %s): %s" %
                           (result.returncode, command[0]))
    return result.stdout


def resolve(root):
    metadata = json.loads(capture([
        "cargo", "metadata", "--locked", "--format-version", "1",
        "--manifest-path", str(root / "Cargo.toml")], root))
    binding = select_dependency(metadata, root)
    checkout = Path(binding["manifest"]).parent
    head = capture(["git", "rev-parse", "HEAD"], checkout).strip()
    dirty = capture(["git", "status", "--porcelain", "--untracked-files=no"], checkout)
    if head != binding["revision"] or dirty:
        raise BindingError("effective Git checkout is modified or differs from its resolved commit")
    return binding


def select_dependency(metadata, root):
    root_manifest = (root / "Cargo.toml").resolve()
    packages = {p["id"]: p for p in metadata["packages"]}
    roots = [p for p in packages.values()
             if Path(p["manifest_path"]).resolve() == root_manifest]
    if len(roots) != 1:
        raise BindingError("expected exactly one daemon root package")
    nodes = [n for n in metadata["resolve"]["nodes"] if n["id"] == roots[0]["id"]]
    if len(nodes) != 1:
        raise BindingError("missing daemon dependency resolution")
    matches = [packages[d["pkg"]] for d in nodes[0]["deps"]
               if d["name"] in ("ctox_web_stack", "ctox-web-stack")
               and packages[d["pkg"]]["name"] == "ctox-web-stack"]
    if len(matches) != 1:
        raise BindingError("expected exactly one direct daemon web-stack dependency")
    package = matches[0]
    source = package.get("source")
    if not source or not source.startswith("git+"):
        raise BindingError("daemon web-stack must resolve to immutable Git source; "
                           "path/registry override is not canonical proof")
    url = urlsplit(source[4:])
    if not re.fullmatch(r"[0-9a-f]{40}", url.fragment):
        raise BindingError("Cargo did not resolve an immutable web-stack commit")
    # Never log embedded userinfo/query credentials from a source URI.
    safe_source = urlunsplit((url.scheme, url.hostname or "", url.path, "", url.fragment))
    manifest = Path(package["manifest_path"]).resolve()
    if manifest == (root / "src/tools/web-stack/Cargo.toml").resolve():
        raise BindingError("local mirror cannot be the canonical Git checkout")
    return {"source": safe_source, "revision": url.fragment,
            "manifest": str(manifest), "target_directory": metadata["target_directory"],
            "scope": "daemon-bound web-stack source; focused checks are not full daemon acceptance"}


def digest_file(path):
    digest = hashlib.sha256()
    with open(path, "rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def source_state(root):
    # Include tracked edits in addition to HEAD: a reused binary after a local
    # source/lock change must not pass. Runtime/untracked fixtures are excluded.
    head = capture(["git", "rev-parse", "HEAD"], root).strip()
    diff = capture(["git", "diff", "HEAD", "--binary", "--", "."], root)
    untracked = capture(["git", "ls-files", "--others", "--exclude-standard", "-z"], root)
    extras = [(name, digest_file(root / name)) for name in untracked.split("\0")
              if name and (root / name).is_file()]
    return {"head": head, "tracked_diff_sha256": hashlib.sha256(diff.encode()).hexdigest(),
            "untracked_sha256": hashlib.sha256(json.dumps(extras).encode()).hexdigest()}


def receipt_path(binding):
    return Path(binding["target_directory"]) / "web-stack-daemon-source.json"


def verified_binary(root, binding):
    path = receipt_path(binding)
    if not path.is_file():
        raise BindingError("no daemon build receipt; run web_stack_source.py daemon-build under admission")
    receipt = json.loads(path.read_text())
    if receipt["binding"] != binding or receipt["root"] != str(root) or receipt["state"] != source_state(root):
        raise BindingError("daemon build receipt is stale for this source/checkout")
    executable = Path(receipt["executable"])
    if not executable.is_file() or digest_file(executable) != receipt["sha256"]:
        raise BindingError("daemon executable differs from its build receipt")
    return executable


def check_mutation_asset(root, binding):
    local = (root / "src/tools/web-stack/assets/stealth_init.js").resolve()
    effective = (Path(binding["manifest"]).parent / "assets/stealth_init.js").resolve()
    if effective != local:
        raise BindingError("stage2 refused: local stealth mutation is not compiled into the daemon. "
                           "Use an isolated canonical-source mutation build and positive control; "
                           "never mutate Cargo's shared Git cache.")


def bounded_env():
    env = os.environ.copy()
    for key in ("CARGO_BUILD_JOBS", "RUST_TEST_THREADS"):
        value = env.get(key, "2")
        if value not in ("1", "2"):
            raise BindingError("%s must be 1 or 2" % key)
        env[key] = value
    return env


def focused_command(binding, action, arguments):
    # Selector overrides could silently redirect the invocation back to a
    # mirror. Keep all test filters/features after the source selection.
    selectors = ("--manifest-path", "--package", "-p", "--workspace", "--exclude",
                 "--jobs", "-j", "--config", "--lockfile-path")
    before_separator = arguments[:arguments.index("--")] if "--" in arguments else arguments
    if any(arg == flag or arg.startswith(flag + "=") or
           (flag in ("-p", "-j") and arg.startswith(flag))
           for arg in before_separator for flag in selectors):
        raise BindingError("source/job selector overrides are not allowed")
    return ["cargo", action, "--locked", "--manifest-path", binding["manifest"]] + arguments


def build_daemon(root, binding):
    path = receipt_path(binding)
    path.parent.mkdir(parents=True, exist_ok=True)
    path.unlink(missing_ok=True)
    before = source_state(root)
    command = ["cargo", "build", "--locked", "--manifest-path", str(root / "Cargo.toml"),
               "--bin", "ctox", "--message-format=json-render-diagnostics"]
    executable = None
    process = subprocess.Popen(command, cwd=root, env=bounded_env(), text=True,
                               stdout=subprocess.PIPE)
    for line in process.stdout:
        try:
            message = json.loads(line)
        except ValueError:
            sys.stderr.write(line)
            continue
        if message.get("reason") == "compiler-message":
            sys.stderr.write(message["message"].get("rendered", ""))
        if (message.get("reason") == "compiler-artifact" and
                message.get("target", {}).get("name") == "ctox" and message.get("executable")):
            executable = message["executable"]
    code = process.wait()
    if code:
        return code
    if not executable or before != source_state(root) or binding != resolve(root):
        raise BindingError("daemon build produced no executable or source changed during build")
    receipt = {"root": str(root), "binding": binding, "state": before,
               "executable": executable, "sha256": digest_file(executable)}
    path.write_text(json.dumps(receipt, indent=2) + "\n")
    return 0


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("action", choices=["describe", "build", "test", "daemon-build",
                                           "daemon-path", "mutation-check", "mutation-probe"])
    parser.add_argument("arguments", nargs=argparse.REMAINDER)
    args = parser.parse_args(argv)
    root = Path(__file__).resolve().parent.parent
    try:
        binding = resolve(root)
        print(json.dumps(binding, sort_keys=True), file=sys.stderr, flush=True)
        arguments = args.arguments[1:] if args.arguments[:1] == ["--"] else args.arguments
        if args.action not in ("build", "test") and arguments:
            raise BindingError("this action accepts no extra arguments")
        if args.action in ("build", "test"):
            code = subprocess.call(focused_command(binding, args.action, arguments),
                                   cwd=root, env=bounded_env())
            if code == 0 and resolve(root) != binding:
                raise BindingError("source binding changed during focused verification")
            return code
        if args.action == "daemon-build":
            return build_daemon(root, binding)
        if args.action == "daemon-path":
            print(verified_binary(root, binding))
        elif args.action == "mutation-probe":
            from web_stack_mutation import run_mutation
            return run_mutation(root, binding)
        elif args.action == "mutation-check":
            check_mutation_asset(root, binding)
        else:
            print(json.dumps(binding, indent=2))
        return 0
    except (BindingError, ValueError, KeyError, OSError, subprocess.TimeoutExpired) as error:
        print("web-stack source binding failed: %s" % error, file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())

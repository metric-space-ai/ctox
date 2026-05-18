#!/usr/bin/env python3
"""CTOX Google bootstrap profile probe.

Launches a headed Chrome with a cloned user profile, drives it with the
sibling `browser_profile_probe.mjs` Playwright script to reach
google.com/search, and emits a debug envelope on stdout that the Rust
caller parses into a GoogleBootstrapProfile.

Chrome/profile inputs are explicit CLI arguments supplied by the Rust caller
from CTOX runtime config. This probe intentionally does not read global CTOX_*
process environment variables.
"""

import argparse
import json
import pathlib
import shutil
import subprocess
import sys
import tempfile
import time
import urllib.request


DEFAULT_GOOGLE_SEARCH_URL = (
    "https://www.google.com/search?"
    "q=RFC+9110+HTTP+Semantics&hl=en-US&lr=lang_en&cr=countryUS&ie=utf8&oe=utf8&start=0"
)


def find_chrome_executable(override: str | None) -> pathlib.Path:
    if not override:
        raise SystemExit(
            "--chrome-bin is required. Set the CTOX runtime config key CTOX_WEB_CHROME_BIN "
            "before running google-bootstrap-refresh."
        )
    path = pathlib.Path(override)
    if path.exists():
        return path
    raise SystemExit(
        f"--chrome-bin {override} does not exist"
    )


def find_chrome_user_data_dir(override: str | None) -> pathlib.Path:
    if not override:
        raise SystemExit(
            "--chrome-user-data-dir is required. Set the CTOX runtime config key "
            "CTOX_WEB_CHROME_USER_DATA_DIR before running google-bootstrap-refresh."
        )
    path = pathlib.Path(override)
    if path.exists():
        return path
    raise SystemExit(
        f"--chrome-user-data-dir {override} does not exist"
    )


def find_reference_dir(override: str | None) -> pathlib.Path:
    if override:
        path = pathlib.Path(override)
        if (path / "node_modules" / "playwright").exists():
            return path
        raise SystemExit(
            f"{path}/node_modules/playwright is missing. "
            "Run `ctox web browser-prepare --install-reference --install-browser`."
        )
    raise SystemExit(
        "--reference-dir is required. The Rust caller sets this to the Playwright workspace dir."
    )


def wait_for_devtools_url(port: int, timeout_s: float) -> dict:
    deadline = time.time() + timeout_s
    last_error: Exception | None = None
    while time.time() < deadline:
        try:
            with urllib.request.urlopen(
                f"http://127.0.0.1:{port}/json/version", timeout=1
            ) as response:
                return json.load(response)
        except Exception as exc:  # pragma: no cover - dev utility
            last_error = exc
            time.sleep(0.5)
    raise RuntimeError(f"DevTools not reachable on {port}: {last_error}")


def wait_for_chrome_shutdown(chrome_bin: pathlib.Path, timeout_s: float) -> None:
    deadline = time.time() + timeout_s
    binary_name = chrome_bin.name
    while time.time() < deadline:
        proc = subprocess.run(
            ["pgrep", "-f", binary_name], capture_output=True, text=True
        )
        if proc.returncode != 0 or not proc.stdout.strip():
            return
        time.sleep(0.5)
    # Non-fatal: the caller can retry; do not crash the probe because another
    # Chrome is lingering — the profile clone runs against a copy either way.
    print(
        f"Warning: {binary_name} did not shut down within {timeout_s}s",
        file=sys.stderr,
    )


def quit_running_chrome(chrome_bin: pathlib.Path) -> None:
    """Ask the source Chrome to close cleanly before cloning its profile.

    The user's running Chrome holds a SingletonLock and has in-flight SQLite
    writes to Cookies/Local State. Cloning while it runs can catch partial
    state. On macOS we politely ask via AppleScript; on Linux we skip
    because there is no portable, non-destructive way to ask Chrome to
    close without clobbering the user's session — use --leave-chrome-running
    or close the browser manually.
    """
    if sys.platform == "darwin":
        subprocess.run(
            ["osascript", "-e", 'tell application "Google Chrome" to quit'],
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )
        wait_for_chrome_shutdown(chrome_bin, 60)
    else:
        print(
            "Note: skipping auto-quit of running Chrome on this platform. "
            "Close Chrome manually for best results, or pass --leave-chrome-running.",
            file=sys.stderr,
        )


def restart_chrome_app(chrome_bin: pathlib.Path) -> None:
    if sys.platform != "darwin":
        return
    subprocess.run(
        ["open", "-a", "Google Chrome"],
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )


def clone_data_dir(source: pathlib.Path, full_clone: bool) -> pathlib.Path:
    target = pathlib.Path(tempfile.mkdtemp(prefix="ctox-chrome-data-"))
    if full_clone:
        excludes = [
            "*/Cache",
            "*/Code Cache",
            "*/GPUCache",
            "*/DawnCache",
            "*/GrShaderCache",
            "*/GraphiteDawnCache",
            "*/ShaderCache",
            "*/Service Worker",
            "*/File System",
            "*/blob_storage",
            "*/IndexedDB",
            "*/Local Storage/leveldb",
            "*/Session Storage",
            "*/WebStorage",
            "*/shared_proto_db",
            "*/WebRTC Logs",
            "*/optimization_guide_model_store",
            "*/optimization_guide_hint_cache_store",
            "*/optimization_guide_model_metadata_store",
            "Crashpad",
            "Crowd Deny",
            "OptimizationHints",
            "Subresource Filter",
            "WidevineCdm",
            "ClientSidePhishing",
            "PKIMetadata",
            "ActorSafetyLists",
            "AmountExtractionHeuristicRegexes",
            "FirstPartySetsPreloaded",
            "TrustTokenKeyCommitments",
            "download_cache",
            "component_crx_cache",
            "extensions_crx_cache",
            "ShaderCache",
            "GrShaderCache",
            "GraphiteDawnCache",
        ]
        cmd = ["rsync", "-a", "--delete"]
        for pattern in excludes:
            cmd.extend(["--exclude", pattern])
        cmd.extend([f"{source}/", f"{target}/"])
        subprocess.run(cmd, check=True)
        return target

    for rel in [
        "Local State",
        "Default/Preferences",
        "Default/Cookies",
        "Default/Network/Cookies",
        "Default/Login Data",
    ]:
        src = source / rel
        dst = target / rel
        dst.parent.mkdir(parents=True, exist_ok=True)
        if src.exists():
            shutil.copy2(src, dst)
    return target


def run_probe(
    reference_dir: pathlib.Path,
    devtools_url: str,
    target_url: str,
    output_dir: pathlib.Path,
    interactive_unlock: bool,
    wait_timeout_secs: int,
) -> subprocess.CompletedProcess[str]:
    script = pathlib.Path(__file__).resolve().with_name("browser_profile_probe.mjs")
    if not script.exists():
        raise SystemExit(f"probe driver missing: {script}")
    # Probe must complete within wait_timeout_secs + navigation + close overhead.
    # Previously hardcoded to 120s, which silently killed interactive unlocks
    # that asked for longer challenge windows.
    subprocess_timeout = max(120, wait_timeout_secs + 60)
    return subprocess.run(
        [
            "node",
            str(script),
            devtools_url,
            target_url,
            str(output_dir),
            "1" if interactive_unlock else "0",
            str(wait_timeout_secs),
        ],
        cwd=reference_dir,
        capture_output=True,
        text=True,
        timeout=subprocess_timeout,
    )


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--full-clone", action="store_true")
    parser.add_argument("--quit-running-chrome", action="store_true")
    parser.add_argument(
        "--leave-chrome-running",
        action="store_true",
        help="Do not ask the user's Chrome to quit before cloning the profile.",
    )
    parser.add_argument("--emit-fetch-json", action="store_true")
    parser.add_argument("--interactive-unlock", action="store_true")
    parser.add_argument("--wait-timeout-secs", type=int, default=300)
    parser.add_argument("--port", type=int, default=9222)
    parser.add_argument("--url", default=DEFAULT_GOOGLE_SEARCH_URL)
    parser.add_argument("--chrome-bin")
    parser.add_argument("--chrome-user-data-dir")
    parser.add_argument("--reference-dir")
    args = parser.parse_args()

    chrome_bin = find_chrome_executable(args.chrome_bin)
    source_data_dir = find_chrome_user_data_dir(args.chrome_user_data_dir)
    reference_dir = find_reference_dir(args.reference_dir)

    if args.quit_running_chrome and not args.leave_chrome_running:
        quit_running_chrome(chrome_bin)

    data_dir = clone_data_dir(source_data_dir, args.full_clone)
    capture_dir = data_dir / "capture"
    capture_dir.mkdir(parents=True, exist_ok=True)
    chrome_out = data_dir / "chrome.out"
    chrome_err = data_dir / "chrome.err"
    # Intentionally omit --log-net-log and --net-log-capture-mode: they trigger
    # Chrome's "unsupported command-line flag" warning banner on every launch,
    # and no downstream consumer reads the net-log produced by this probe (the
    # capture dir is wiped with the tempdir on exit).
    chrome_args = [
        f"--user-data-dir={data_dir}",
        "--profile-directory=Default",
        "--remote-debugging-address=127.0.0.1",
        f"--remote-debugging-port={args.port}",
        "--restore-last-session",
        "--no-first-run",
        "--no-default-browser-check",
        "about:blank",
    ]
    if sys.platform == "darwin" and chrome_bin.suffix == "":
        # Launch the configured browser binary directly so the probe-owned process
        # does not inherit the user's running session state.
        launch_cmd = [str(chrome_bin), *chrome_args]
    else:
        launch_cmd = [str(chrome_bin), *chrome_args]

    proc = subprocess.Popen(
        launch_cmd,
        stdout=chrome_out.open("wb"),
        stderr=chrome_err.open("wb"),
    )

    exit_code = 0
    try:
        meta = wait_for_devtools_url(args.port, 45)
        result = run_probe(
            reference_dir,
            meta["webSocketDebuggerUrl"],
            args.url,
            capture_dir,
            args.interactive_unlock,
            args.wait_timeout_secs,
        )
        fetch_payload = None
        if args.emit_fetch_json and result.returncode == 0:
            probe_payload = json.loads(result.stdout)
            html_path = capture_dir / "page.html"
            fetch_payload = {
                "final_url": probe_payload["finalUrl"],
                "body": html_path.read_text(errors="replace"),
            }
        debug_payload = {
            "data_dir": str(data_dir),
            "capture_dir": str(capture_dir),
            "browser": meta.get("Browser"),
            "webSocketDebuggerUrl": meta.get("webSocketDebuggerUrl"),
            "probe_returncode": result.returncode,
            "probe_stdout": result.stdout,
            "probe_stderr": result.stderr,
            "chrome_err_tail": chrome_err.read_text(errors="replace")[-2000:],
        }
        if fetch_payload is not None:
            print(json.dumps(fetch_payload, indent=2))
        elif args.emit_fetch_json:
            print(json.dumps(debug_payload, indent=2), file=sys.stderr)
            exit_code = result.returncode or 1
        else:
            print(json.dumps(debug_payload, indent=2))
    finally:
        proc.terminate()
        try:
            proc.wait(timeout=10)
        except Exception:
            proc.kill()
            proc.wait(timeout=5)
        if (
            args.quit_running_chrome
            and not args.leave_chrome_running
            and sys.platform == "darwin"
        ):
            restart_chrome_app(chrome_bin)

    raise SystemExit(exit_code)


if __name__ == "__main__":
    main()

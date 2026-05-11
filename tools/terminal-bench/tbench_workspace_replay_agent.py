from __future__ import annotations

import os
import textwrap
from pathlib import Path

from terminal_bench.agents.base_agent import AgentResult, BaseAgent
from terminal_bench.agents.failure_mode import FailureMode
from terminal_bench.terminal.tmux_session import TmuxSession


class WorkspaceReplayAgent(BaseAgent):
    """Replay an already-produced workspace into a Terminal-Bench container.

    This is a validation-only agent. It does not solve the task; it copies the
    CTOX-produced workspace into `/app` and then lets Terminal-Bench run the
    official task tests.
    """

    def __init__(self, workspace_root: str | None = None, **kwargs):
        super().__init__(**kwargs)
        raw_workspace = workspace_root or os.environ.get("TBENCH_REPLAY_WORKSPACE", "")
        self.workspace_root = Path(raw_workspace).expanduser().resolve()
        if not self.workspace_root.is_dir():
            raise RuntimeError(f"workspace_root is not a directory: {self.workspace_root}")

    @staticmethod
    def name() -> str:
        return "workspace-replay"

    def perform_task(
        self,
        instruction: str,
        session: TmuxSession,
        logging_dir: Path | None = None,
    ) -> AgentResult:
        paths = list(sorted(self.workspace_root.iterdir()))
        session.send_keys(
            ["rm -rf /tmp/replay-src && mkdir -p /tmp/replay-src", "Enter"],
            max_timeout_sec=float("inf"),
            block=True,
        )
        session.copy_to_container(
            paths,
            container_dir="/tmp/replay-src",
        )
        overlay_script = r"""
import json
import os
import shutil
import sys
import traceback
from pathlib import Path

src = Path("/tmp/replay-src")
dst = Path("/app")
audit_path = Path("/tmp/ctox_replay_overlay_audit.json")
skip_names = {
    ".git",
    ".hg",
    ".svn",
    ".ctox",
    "__pycache__",
    ".pytest_cache",
    ".mypy_cache",
    ".ruff_cache",
    "target",
    "target.nosync",
}
audit = {
    "copied": [],
    "merged_dirs": [],
    "removed_conflicts": [],
    "skipped_conflicts": [],
    "errors": [],
}


def rel(path: Path) -> str:
    return str(path.relative_to(src))


def remove_existing(path: Path) -> None:
    if path.is_dir() and not path.is_symlink():
        shutil.rmtree(path)
    else:
        path.unlink()


def copy_overlay(source: Path, target: Path) -> None:
    if source.name in skip_names:
        return
    if source.is_dir() and not source.is_symlink():
        if target.exists() and not target.is_dir():
            remove_existing(target)
            audit["removed_conflicts"].append(str(target))
        target.mkdir(parents=True, exist_ok=True)
        audit["merged_dirs"].append(str(target))
        for child in sorted(source.iterdir(), key=lambda item: item.name):
            copy_overlay(child, target / child.name)
        return

    if target.exists() and target.is_dir():
        audit["skipped_conflicts"].append(
            {
                "source": rel(source),
                "target": str(target),
                "reason": "source_file_target_directory",
            }
        )
        return

    target.parent.mkdir(parents=True, exist_ok=True)
    if target.exists() or target.is_symlink():
        remove_existing(target)
    if source.is_symlink():
        os.symlink(os.readlink(source), target)
    else:
        shutil.copy2(source, target)
    audit["copied"].append(str(target))


try:
    if not src.is_dir():
        raise RuntimeError(f"missing replay source: {src}")
    if not dst.is_dir():
        raise RuntimeError(f"missing app target: {dst}")
    for child in sorted(src.iterdir(), key=lambda item: item.name):
        copy_overlay(child, dst / child.name)
except Exception as exc:
    audit["errors"].append(f"{type(exc).__name__}: {exc}")
    audit["traceback"] = traceback.format_exc()
    audit_path.write_text(json.dumps(audit, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    print("CTOX_REPLAY_OVERLAY_FAILED", file=sys.stderr)
    print(audit["traceback"], file=sys.stderr)
    sys.exit(66)

audit_path.write_text(json.dumps(audit, indent=2, sort_keys=True) + "\n", encoding="utf-8")
shutil.copy2(audit_path, dst / ".ctox_replay_overlay_audit.json")
print("CTOX_REPLAY_OVERLAY_OK")
print(json.dumps({key: len(value) for key, value in audit.items() if isinstance(value, list)}, sort_keys=True))
"""
        session.send_keys(
            [
                "\n".join(
                    [
                        "test -d /tmp/replay-src",
                        "python3 - <<'PY'",
                        textwrap.dedent(overlay_script).strip(),
                        "PY",
                        "overlay_status=$?",
                        "cat /tmp/ctox_replay_overlay_audit.json 2>/dev/null || true",
                        "test \"$overlay_status\" -eq 0",
                        "find /app -maxdepth 2 -type f | sort | sed -n '1,120p' > /tmp/replay-files.txt",
                    ]
                ),
                "Enter",
            ],
            max_timeout_sec=float("inf"),
            block=True,
        )
        if logging_dir is not None:
            logging_dir.mkdir(parents=True, exist_ok=True)
            (logging_dir / "replayed_workspace.txt").write_text(
                str(self.workspace_root) + "\n",
                encoding="utf-8",
            )
        return AgentResult(
            total_input_tokens=0,
            total_output_tokens=0,
            failure_mode=FailureMode.NONE,
        )

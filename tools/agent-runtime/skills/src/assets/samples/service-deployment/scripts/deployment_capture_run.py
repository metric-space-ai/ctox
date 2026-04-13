#!/usr/bin/env python3
import argparse
import json
import sys
from datetime import datetime, timezone
from pathlib import Path

import deployment_collect


def now_iso() -> str:
    return datetime.now(timezone.utc).isoformat()


def main() -> int:
    parser = argparse.ArgumentParser(description="Run a bounded deployment preflight sweep.")
    parser.add_argument("--target", default="local")
    parser.add_argument("--service")
    parser.add_argument("--repo-root")
    args = parser.parse_args()
    collectors = []
    for name in sorted(deployment_collect.COLLECTORS):
        collectors.append(
            {
                "collector": name,
                "captured_at": now_iso(),
                "captures": [deployment_collect.run_argv(argv) for argv in deployment_collect.COLLECTORS[name]],
            }
        )
    payload = {
        "skill_key": "service_deployment",
        "captured_at": now_iso(),
        "target": args.target,
        "service": args.service,
        "repo_root": args.repo_root,
        "collectors": collectors,
    }
    print(json.dumps(payload, indent=2))
    return 0


if __name__ == "__main__":
    sys.exit(main())

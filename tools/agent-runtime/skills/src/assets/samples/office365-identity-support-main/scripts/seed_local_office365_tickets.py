#!/usr/bin/env python3
from __future__ import annotations

import argparse
import json
import subprocess
from pathlib import Path
from typing import Any


TICKETS = [
    {
        "title": "Office365 Konto gesperrt: Lena Weber",
        "body": "Hallo Support, das Office365-Konto von lena.weber@contoso.test ist gesperrt. Anmeldung nicht möglich. Bitte entsperren.",
        "priority": "high",
    },
    {
        "title": "Office365 Lizenz fehlt: Mario Schulz",
        "body": "Mario Schulz kann sein Postfach nicht nutzen. Für mario.schulz@contoso.test scheint keine M365 Lizenz vorhanden zu sein.",
        "priority": "normal",
    },
    {
        "title": "Office365 Passwort zurücksetzen: Sara König",
        "body": "Bitte Passwort für sara.koenig@contoso.test zurücksetzen. Benutzer hat das Kennwort vergessen.",
        "priority": "normal",
    },
]


def run_json(cmd: list[str]) -> dict[str, Any]:
    completed = subprocess.run(cmd, check=True, capture_output=True, text=True)
    return json.loads(completed.stdout)


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--ctox-bin", default="./target/debug/ctox")
    parser.add_argument("--sync", action="store_true")
    args = parser.parse_args()

    created: list[dict[str, Any]] = []
    run_json([args.ctox_bin, "ticket", "local", "init"])
    for spec in TICKETS:
        result = run_json(
            [
                args.ctox_bin,
                "ticket",
                "local",
                "create",
                "--title",
                spec["title"],
                "--body",
                spec["body"],
                "--priority",
                spec["priority"],
            ]
        )
        created.append(result["ticket"])
    sync_result = None
    if args.sync:
        sync_result = run_json([args.ctox_bin, "ticket", "sync", "--system", "local"])
    print(json.dumps({"ok": True, "created": created, "sync": sync_result}, ensure_ascii=False, indent=2))


if __name__ == "__main__":
    main()

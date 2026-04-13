#!/usr/bin/env python3
import argparse
import json
from pathlib import Path


def load_json(path: str) -> dict:
    return json.loads(Path(path).read_text(encoding="utf-8"))


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--execution-result", required=True)
    parser.add_argument("--resolution-result")
    args = parser.parse_args()

    result = load_json(args.execution_result)
    decision = result.get("decision")
    target_identity = result.get("target_identity", "unbekannt")
    summary = result.get("execution_summary", "")
    verification = result.get("verification_result", "failed")

    if decision == "executed":
        body = f"{summary} Verifikation: {verification}. Zielidentität: {target_identity}."
    elif decision == "no_change":
        body = f"Keine Änderung durchgeführt. {summary} Verifikation: {verification}. Zielidentität: {target_identity}."
    else:
        body = f"Bearbeitung blockiert. Grund: {result.get('reason', 'unbekannt')}. Zielidentität: {target_identity}."

    print(
        json.dumps(
            {
                "decision": "suggestion",
                "ticket_update": body,
                "verification_result": verification,
            },
            ensure_ascii=False,
            indent=2,
        )
    )


if __name__ == "__main__":
    main()

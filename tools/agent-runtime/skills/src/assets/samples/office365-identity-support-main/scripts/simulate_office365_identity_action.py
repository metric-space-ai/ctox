#!/usr/bin/env python3
import argparse
import json
from pathlib import Path


def load_json(path: str) -> dict:
    return json.loads(Path(path).read_text(encoding="utf-8"))


def write_json(path: str, payload: dict) -> None:
    Path(path).write_text(json.dumps(payload, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")


def load_item(items_path: str, item_id: str) -> dict:
    for line in Path(items_path).read_text(encoding="utf-8").splitlines():
        if not line.strip():
            continue
        item = json.loads(line)
        if item["item_id"] == item_id:
            return item
    raise SystemExit(f"item not found: {item_id}")


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--state", required=True)
    parser.add_argument("--items", required=True)
    parser.add_argument("--item-id", required=True)
    parser.add_argument("--target-identity", required=True)
    args = parser.parse_args()

    state = load_json(args.state)
    item = load_item(args.items, args.item_id)
    target_identity = args.target_identity.lower().strip()
    user = next((entry for entry in state.get("users", []) if entry.get("upn", "").lower() == target_identity), None)
    if not user:
        print(json.dumps({"decision": "blocked", "reason": "user_not_found", "target_identity": target_identity}, ensure_ascii=False, indent=2))
        return

    label = item["label"]
    summary = ""
    verification_result = "failed"

    if label == "O365-ACC-01":
        if user.get("locked"):
            user["locked"] = False
            summary = f"Konto {target_identity} wurde entsperrt."
            verification_result = "passed"
        else:
            summary = f"Konto {target_identity} war nicht gesperrt."
    elif label == "O365-LIC-01":
        licenses = user.setdefault("licenses", [])
        if "M365_E3" not in licenses:
            licenses.append("M365_E3")
            user["mailbox"] = True
            summary = f"Lizenz M365_E3 wurde für {target_identity} zugewiesen."
            verification_result = "passed"
        else:
            summary = f"Lizenz M365_E3 ist für {target_identity} bereits vorhanden."
    elif label == "O365-PWD-01":
        user["temporary_password"] = state.get("temporary_password")
        user["must_change_password"] = True
        summary = f"Temporäres Passwort für {target_identity} wurde gesetzt."
        verification_result = "passed"
    else:
        print(json.dumps({"decision": "blocked", "reason": "unsupported_label", "label": label}, ensure_ascii=False, indent=2))
        return

    write_json(args.state, state)
    print(
        json.dumps(
            {
                "decision": "executed" if verification_result == "passed" else "no_change",
                "label": label,
                "target_identity": target_identity,
                "execution_summary": summary,
                "verification_result": verification_result,
                "user_state": user,
            },
            ensure_ascii=False,
            indent=2,
        )
    )


if __name__ == "__main__":
    main()

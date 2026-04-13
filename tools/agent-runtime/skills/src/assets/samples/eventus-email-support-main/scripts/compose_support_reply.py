#!/usr/bin/env python3
import argparse
import json
import re
from pathlib import Path


ENGLISH_HINTS = {
    "hello",
    "hi",
    "please",
    "password",
    "registration",
    "login",
    "support",
    "thanks",
    "thank",
    "supplier",
}


def read_text(path: str | None, raw: str | None) -> str:
    if raw:
        return raw
    if path:
        return Path(path).read_text(encoding="utf-8")
    return ""


def load_json(path: str) -> dict:
    return json.loads(Path(path).read_text(encoding="utf-8"))


def load_item(items_path: str, item_id: str) -> dict:
    for line in Path(items_path).read_text(encoding="utf-8").splitlines():
        if not line.strip():
            continue
        item = json.loads(line)
        if item["item_id"] == item_id:
            return item
    raise SystemExit(f"item not found: {item_id}")


def detect_language(text: str) -> str:
    lowered = text.lower()
    english_hits = sum(1 for word in ENGLISH_HINTS if word in lowered)
    return "en" if english_hits >= 2 else "de"


def build_manual_reference(item: dict) -> str:
    pages = ", ".join(item.get("pages", []))
    return f"Manual reference: {pages}" if pages else ""


def compose_body(language: str, item: dict) -> str:
    guidance = item.get("expected_guidance", "").strip()
    manual_reference = build_manual_reference(item)
    if language == "en":
        return "\n\n".join(
            [
                "Hello,",
                guidance,
                manual_reference,
            ]
        ).strip()
    return "\n\n".join(
        [
            "Hallo,",
            guidance,
            manual_reference,
        ]
    ).strip()


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--main-skill", required=True)
    parser.add_argument("--skillbook", required=True)
    parser.add_argument("--items", required=True)
    parser.add_argument("--item-id", required=True)
    parser.add_argument("--email-file")
    parser.add_argument("--email-text")
    parser.add_argument("--subject", default="Support request")
    parser.add_argument(
        "--send-policy",
        choices=["suggestion", "draft", "send"],
        default="suggestion",
    )
    args = parser.parse_args()

    main_skill = load_json(args.main_skill)
    skillbook = load_json(args.skillbook)
    item = load_item(args.items, args.item_id)
    inbound_text = read_text(args.email_file, args.email_text)
    language = detect_language(inbound_text)
    body = compose_body(language, item)
    print(
        json.dumps(
            {
                "decision": args.send_policy,
                "primary_channel": main_skill["primary_channel"],
                "matched_label": item["label"],
                "item_id": item["item_id"],
                "reply_subject": f"Re: {args.subject}",
                "reply_body": body,
                "manual_reference": build_manual_reference(item),
                "writeback_policy": item.get("writeback_policy", {}),
            },
            ensure_ascii=False,
            indent=2,
        )
    )


if __name__ == "__main__":
    main()

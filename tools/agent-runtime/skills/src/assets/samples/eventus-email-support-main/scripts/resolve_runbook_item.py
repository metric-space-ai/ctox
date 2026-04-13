#!/usr/bin/env python3
import argparse
import json
import re
from pathlib import Path


TOKEN_RE = re.compile(r"[a-z0-9]+")
LABEL_RE = re.compile(r"\b([A-Z]+-\d+)\b")


def read_email(args) -> str:
    if args.email_text:
        return args.email_text
    return Path(args.email_file).read_text(encoding="utf-8")


def normalize(text: str) -> str:
    return text.lower().replace("`", " ")


def tokens(text: str) -> set[str]:
    return set(TOKEN_RE.findall(normalize(text)))


def score_item(query_text: str, query_tokens: set[str], item: dict) -> tuple[float, list[str]]:
    reasons = []
    score = 0.0

    label_match = LABEL_RE.search(query_text)
    if label_match and label_match.group(1) == item["label"]:
        score += 10.0
        reasons.append(f"exact label match {item['label']}")

    title_tokens = tokens(item["title"])
    trigger_tokens = tokens(" ".join(item.get("trigger_phrases", [])))
    blocker_tokens = tokens(item.get("earliest_blocker", ""))
    chunk_tokens = tokens(item.get("chunk_text", ""))

    for field_name, field_tokens, weight in (
        ("title", title_tokens, 4.0),
        ("triggers", trigger_tokens, 3.0),
        ("blocker", blocker_tokens, 2.0),
        ("chunk", chunk_tokens, 1.0),
    ):
        overlap = sorted(query_tokens & field_tokens)
        if overlap:
            score += min(len(overlap), 8) * weight
            reasons.append(f"{field_name} overlap: {', '.join(overlap[:6])}")

    return score, reasons


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--items", required=True)
    parser.add_argument("--email-file")
    parser.add_argument("--email-text")
    parser.add_argument("--top-k", type=int, default=5)
    args = parser.parse_args()

    if not args.email_file and not args.email_text:
        raise SystemExit("provide --email-file or --email-text")

    query_text = read_email(args)
    query_tokens = tokens(query_text)
    items = [
        json.loads(line)
        for line in Path(args.items).read_text(encoding="utf-8").splitlines()
        if line.strip()
    ]

    ranked = []
    for item in items:
        score, reasons = score_item(query_text, query_tokens, item)
        if score > 0:
            ranked.append(
                {
                    "item_id": item["item_id"],
                    "label": item["label"],
                    "title": item["title"],
                    "problem_class": item["problem_class"],
                    "score": score,
                    "reasons": reasons,
                }
            )

    ranked.sort(key=lambda row: (-row["score"], row["label"]))
    top = ranked[: args.top_k]
    best = top[0] if top else None
    top_score = best["score"] if best else 0.0
    second_score = top[1]["score"] if len(top) > 1 else 0.0
    confident = bool(best and top_score >= 6.0 and (top_score - second_score >= 2.0 or top_score >= 10.0))

    print(
        json.dumps(
            {
                "query": query_text,
                "decision": "matched" if confident else "needs_review",
                "best_match": best,
                "candidates": top,
            },
            ensure_ascii=False,
            indent=2,
        )
    )


if __name__ == "__main__":
    main()

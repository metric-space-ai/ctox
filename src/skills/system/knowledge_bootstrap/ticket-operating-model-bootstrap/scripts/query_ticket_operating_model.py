#!/usr/bin/env python3
from __future__ import annotations

import argparse
import json
import re
from pathlib import Path


def tokenize(text: str) -> set[str]:
    return {token.lower() for token in re.findall(r"[A-Za-zÄÖÜäöüß0-9._/-]{3,}", text)}


def overlap_score(query: str, text: str) -> float:
    left = tokenize(query)
    right = tokenize(text)
    if not left or not right:
        return 0.0
    return len(left & right) / len(left | right)


def load_cards(path: Path) -> list[dict]:
    return [json.loads(line) for line in path.read_text(encoding="utf-8").splitlines() if line.strip()]


def load_json(path: Path) -> list[dict]:
    return json.loads(path.read_text(encoding="utf-8"))


def vector_score(query: str, cards: list[dict], model_dir: Path, provider: str | None, model_name: str | None) -> list[float] | None:
    vectors_path = model_dir / "retrieval_vectors.npy"
    if not vectors_path.exists() or not provider or not model_name:
        return None
    if provider != "sentence-transformers":
        raise ValueError(f"Unsupported embedding provider: {provider}")
    import numpy as np
    from sentence_transformers import SentenceTransformer

    vectors = np.load(vectors_path)
    if len(vectors) != len(cards):
        return None
    model = SentenceTransformer(model_name)
    query_vector = model.encode([query], normalize_embeddings=True, show_progress_bar=False)[0]
    return (vectors @ query_vector).tolist()


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Query a built ticket operating model.")
    parser.add_argument("--model-dir", required=True)
    parser.add_argument("--query", required=True)
    parser.add_argument("--family")
    parser.add_argument("--request-type")
    parser.add_argument("--category")
    parser.add_argument("--top-k", type=int, default=8)
    parser.add_argument("--embedding-provider")
    parser.add_argument("--embedding-model")
    parser.add_argument("--hybrid-alpha", type=float, default=0.65)
    return parser.parse_args()


def resolve_model_dir(path: Path) -> Path:
    if (path / "retrieval_index.jsonl").exists() and (path / "family_playbooks.json").exists():
        return path
    generated = path / "references" / "generated"
    if (generated / "retrieval_index.jsonl").exists() and (generated / "family_playbooks.json").exists():
        return generated
    raise SystemExit(f"could not find operating-model artifacts under {path}")


def main() -> None:
    args = parse_args()
    model_dir = resolve_model_dir(Path(args.model_dir))
    cards = load_cards(model_dir / "retrieval_index.jsonl")
    playbooks = load_json(model_dir / "family_playbooks.json")
    playbook_by_family = {playbook["family_key"]: playbook for playbook in playbooks}
    filtered = []
    for card in cards:
        if args.family and card.get("family_key") != args.family:
            continue
        if args.request_type and card.get("request_type") != args.request_type:
            continue
        if args.category and card.get("category") != args.category:
            continue
        filtered.append(card)
    semantic_scores = vector_score(
        args.query,
        filtered,
        model_dir,
        args.embedding_provider,
        args.embedding_model,
    )
    ranked_input = []
    for idx, card in enumerate(filtered):
        lexical = overlap_score(args.query, card["text"])
        semantic = semantic_scores[idx] if semantic_scores is not None else lexical
        score = args.hybrid_alpha * semantic + (1.0 - args.hybrid_alpha) * lexical
        ranked_input.append(
            {
                "score": score,
                "lexical_score": lexical,
                "semantic_score": semantic,
                **card,
            }
        )
    ranked_cards = sorted(
        ranked_input,
        key=lambda item: item["score"],
        reverse=True,
    )[: args.top_k]
    grouped: dict[str, dict] = {}
    for card in ranked_cards:
        family_key = card["family_key"]
        family = grouped.setdefault(
            family_key,
            {
                "family_key": family_key,
                "score": card["score"],
                "playbook": playbook_by_family.get(family_key),
                "matching_cards": [],
            },
        )
        family["score"] = max(family["score"], card["score"])
        family["matching_cards"].append(
            {
                "card_type": card["card_type"],
                "ticket_id": card.get("ticket_id"),
                "title": card.get("title"),
                "score": card["score"],
                "lexical_score": card["lexical_score"],
                "semantic_score": card["semantic_score"],
            }
        )
    ranked_families = sorted(grouped.values(), key=lambda item: item["score"], reverse=True)
    results = []
    for family in ranked_families[: args.top_k]:
        playbook = family.get("playbook") or {}
        decision_support = playbook.get("decision_support") or {}
        examples = (playbook.get("historical_examples") or {}).get("canonical") or []
        results.append(
            {
                "family_key": family["family_key"],
                "score": family["score"],
                "why_it_matches": {
                    "top_cards": family["matching_cards"][:4],
                    "token_signals": (playbook.get("signals") or {}).get("token_signals", [])[:6],
                    "common_phrases": (playbook.get("signals") or {}).get("common_phrases", [])[:6],
                },
                "decision_support": {
                    "mode": decision_support.get("mode"),
                    "operator_summary": decision_support.get("operator_summary"),
                    "triage_focus": decision_support.get("triage_focus", [])[:5],
                    "handling_steps": decision_support.get("handling_steps", [])[:6],
                    "close_when": decision_support.get("close_when"),
                    "caution_signals": decision_support.get("caution_signals", [])[:4],
                    "note_guidance": decision_support.get("note_guidance"),
                },
                "desk_norms": {
                    "channels": (playbook.get("usual_handling") or {}).get("dominant_channels", [])[:3],
                    "states": (playbook.get("usual_handling") or {}).get("dominant_states", [])[:5],
                    "actions_seen": (playbook.get("usual_handling") or {}).get("actions_seen", [])[:5],
                    "closure_tendency": (playbook.get("usual_handling") or {}).get("closure_tendency"),
                },
                "historical_examples": {
                    "canonical": examples[:3],
                    "matching_cards": family["matching_cards"][:4],
                },
            }
        )
    print(json.dumps({"query": args.query, "families": results}, ensure_ascii=False, indent=2))


if __name__ == "__main__":
    main()

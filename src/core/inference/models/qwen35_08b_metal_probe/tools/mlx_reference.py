#!/usr/bin/env python3
"""MLX reference runner for Qwen3.5-0.8B raw-token greedy decode."""

from __future__ import annotations

import argparse
import json
import time

import mlx.core as mx
import numpy as np
from mlx_lm import load
from mlx_lm.generate import generate_step


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--model",
        default="/Users/michaelwelsch/.cache/huggingface/local/Qwen3.5-0.8B",
        help="Local Qwen3.5-0.8B model directory.",
    )
    parser.add_argument("--prompt-token", type=int, default=107)
    parser.add_argument("--max-tokens", type=int, default=4)
    parser.add_argument("--runs", type=int, default=3)
    parser.add_argument(
        "--top-k",
        type=int,
        default=0,
        help="Also report top-k logits for the first raw-token model call.",
    )
    parser.add_argument(
        "--fresh-single-token-chain",
        action="store_true",
        help="Generate each next token from a fresh one-token model call with no cache.",
    )
    return parser.parse_args()


def cached_decode(model, prompt_token: int, max_tokens: int) -> list[int]:
    prompt = mx.array([prompt_token], dtype=mx.int32)
    tokens: list[int] = []
    for y, _ in generate_step(prompt, model, max_tokens=max_tokens):
        tokens.append(int(y))
    return tokens


def fresh_single_token_chain(model, prompt_token: int, max_tokens: int) -> list[int]:
    token = prompt_token
    tokens: list[int] = []
    for _ in range(max_tokens):
        logits = model(mx.array([[token]], dtype=mx.int32))[:, -1, :]
        token = int(mx.argmax(logits, axis=-1).item())
        tokens.append(token)
    return tokens


def first_token_topk(model, prompt_token: int, k: int) -> list[dict[str, float | int]]:
    logits = model(mx.array([[prompt_token]], dtype=mx.int32))[:, -1, :]
    scores = np.array(logits[0].astype(mx.float32))
    top = np.argpartition(scores, -k)[-k:]
    top = top[np.argsort(scores[top])[::-1]]
    return [{"token": int(idx), "score": float(scores[idx])} for idx in top]


def main() -> None:
    args = parse_args()
    model, tokenizer = load(args.model)
    del tokenizer

    runs = []
    for _ in range(args.runs):
        start = time.perf_counter()
        if args.fresh_single_token_chain:
            tokens = fresh_single_token_chain(model, args.prompt_token, args.max_tokens)
        else:
            tokens = cached_decode(model, args.prompt_token, args.max_tokens)
        elapsed = time.perf_counter() - start
        runs.append(
            {
                "tokens": tokens,
                "elapsed_s": elapsed,
                "tok_s": len(tokens) / elapsed if elapsed > 0 else None,
            }
        )

    result = {
        "model": args.model,
        "prompt_token": args.prompt_token,
        "max_tokens": args.max_tokens,
        "mode": "fresh-single-token-chain"
        if args.fresh_single_token_chain
        else "cached-decode",
        "runs": runs,
    }
    if args.top_k > 0:
        result["first_token_topk"] = first_token_topk(model, args.prompt_token, args.top_k)

    print(
        json.dumps(
            result,
            indent=2,
        )
    )


if __name__ == "__main__":
    main()

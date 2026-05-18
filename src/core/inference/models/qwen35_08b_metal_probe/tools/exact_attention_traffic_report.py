#!/usr/bin/env python3
"""Model exact prefill attention K/V traffic for Qwen3.5-0.8B.

This is a decision tool, not a benchmark. It makes explicit how much exact
traffic could be saved by query blocking before register pressure and occupancy
decide whether the schedule is actually faster.
"""

from __future__ import annotations

import argparse
from dataclasses import dataclass


Q_HEADS = 8
KV_HEADS = 2
HEAD_DIM = 256
KV_BYTES_PER_KEY_PER_KV_HEAD = HEAD_DIM * 2 * 2  # K FP16 + V FP16
LLAMA_TOK_S = {4096: 2852.70, 16384: 2065.71, 32768: 1325.20}


@dataclass(frozen=True)
class MeasuredCandidate:
    name: str
    tokens: int
    seconds: float
    status: str

    @property
    def tok_s(self) -> float:
        return self.tokens / self.seconds


MEASURED: list[MeasuredCandidate] = [
    MeasuredCandidate("qh4_qblk1_vec8_exact", 4096, 0.127131375, "accepted attention core"),
    MeasuredCandidate("qh4_qblk1_vec8_exact", 8192, 0.450240125, "accepted attention core"),
    MeasuredCandidate("qh4_qblk1_vec8_exact", 16384, 1.588621708, "accepted previous run"),
    MeasuredCandidate("qh4_qblk1_vec8_exact", 32768, 6.319225, "near modeled byte floor"),
    MeasuredCandidate("qh4_qblk2_vec8_exact", 8192, 0.505392417, "rejected: register pressure"),
    MeasuredCandidate("qblk4_batch_exact", 4096, 0.433838, "rejected: slower than qh4"),
    MeasuredCandidate("qblk8_batch_exact", 4096, 0.482253, "rejected: slower than qh4"),
    MeasuredCandidate("qh4_splitk512_exact", 4096, 0.142699708, "rejected: scratch traffic"),
    MeasuredCandidate("qh4_splitk512_exact", 8192, 0.525104833, "rejected: scratch traffic"),
    MeasuredCandidate("qh4_splitk512_exact", 16384, 1.939569000, "rejected: scratch traffic"),
]


def qh4_query_block_bytes(tokens: int, query_block: int) -> int:
    total = 0
    for query_start in range(0, tokens, query_block):
        last_query = min(query_start + query_block - 1, tokens - 1)
        total += (last_query + 1) * KV_HEADS * KV_BYTES_PER_KEY_PER_KV_HEAD
    return total


def dense_per_q_head_bytes(tokens: int) -> int:
    return tokens * (tokens + 1) // 2 * Q_HEADS * KV_BYTES_PER_KEY_PER_KV_HEAD


def gib(value: int | float) -> float:
    return float(value) / (1024.0**3)


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--tokens", default="4096,8192,16384,32768")
    parser.add_argument("--sustained-gb-s", type=float, default=174.0)
    args = parser.parse_args()

    token_counts = [int(part) for part in args.tokens.split(",") if part.strip()]
    print("exact_attention_traffic_report")
    print("model: Qwen3.5-0.8B GQA attention, FP16 K/V")
    print(f"sustained_gb_s: {args.sustained_gb_s:.3f}")
    print("contract: query-block byte wins are hypothetical until measured")
    print()

    print("| tokens | layout | qblk | kv_gib | vs_dense | vs_qh4_qblk1 | byte_floor_ms |")
    print("|---:|---|---:|---:|---:|---:|---:|")
    for tokens in token_counts:
        dense = dense_per_q_head_bytes(tokens)
        qh4_base = qh4_query_block_bytes(tokens, 1)
        for qblk in [1, 2, 4, 8, 16]:
            bytes_ = qh4_query_block_bytes(tokens, qblk)
            floor_ms = bytes_ / (args.sustained_gb_s * 1.0e9) * 1000.0
            print(
                f"| {tokens} | qh4_exact | {qblk} | {gib(bytes_):.2f} | "
                f"{bytes_ / dense:.3f}x | {bytes_ / qh4_base:.3f}x | {floor_ms:.3f} |"
            )
    print()

    print("| candidate | tokens | seconds | tok_s | llama_tok_s | status |")
    print("|---|---:|---:|---:|---:|---|")
    for item in MEASURED:
        llama = LLAMA_TOK_S.get(item.tokens)
        llama_text = f"{llama:.2f}" if llama is not None else "n/a"
        print(
            f"| {item.name} | {item.tokens} | {item.seconds:.9f} | "
            f"{item.tok_s:.2f} | {llama_text} | {item.status} |"
        )
    print()

    print("decision_hint:")
    print(
        "  Exact qh4/qblk1 is near its modeled K/V byte floor at p32768. "
        "A new exact kernel must either make qblk>1 faster despite register "
        "pressure, or reduce K/V precision/storage without scalar dequant."
    )


if __name__ == "__main__":
    main()

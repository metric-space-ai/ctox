#!/usr/bin/env python3
"""Plan scratch and work counts for a tiled exact QK-softmax-V prototype."""

from __future__ import annotations

import argparse


Q_HEADS = 8
KV_HEADS = 2
HEAD_DIM = 256
BYTES_FP16 = 2
BYTES_FP32 = 4


def mib(value: int | float) -> float:
    return float(value) / (1024.0**2)


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--tokens", type=int, default=16384)
    parser.add_argument("--q-tiles", default="64,128,256,512")
    parser.add_argument("--k-tiles", default="256,512,1024,2048")
    parser.add_argument("--score-dtype", choices=["fp16", "fp32"], default="fp16")
    parser.add_argument("--max-score-mib", type=float, default=256.0)
    args = parser.parse_args()

    score_bytes = BYTES_FP16 if args.score_dtype == "fp16" else BYTES_FP32
    q_tiles = [int(part) for part in args.q_tiles.split(",") if part.strip()]
    k_tiles = [int(part) for part in args.k_tiles.split(",") if part.strip()]

    print("tiled_attention_plan")
    print(f"tokens: {args.tokens}")
    print(f"head_dim: {HEAD_DIM}")
    print(f"q_heads: {Q_HEADS}")
    print(f"kv_heads: {KV_HEADS}")
    print(f"score_dtype: {args.score_dtype}")
    print(f"max_score_mib: {args.max_score_mib:.3f}")
    print("contract: planning only; tiled exact kernel not implemented by this tool")
    print()
    print(
        "| q_tile | k_tile | score_mib_per_qhead | qk_calls_per_qhead | "
        "causal_tile_pairs | q_tile_mib | kv_tile_mib | viable_scratch |"
    )
    print("|---:|---:|---:|---:|---:|---:|---:|---|")
    for q_tile in q_tiles:
        q_blocks = (args.tokens + q_tile - 1) // q_tile
        q_tile_bytes = q_tile * HEAD_DIM * BYTES_FP16
        for k_tile in k_tiles:
            k_blocks = (args.tokens + k_tile - 1) // k_tile
            causal_pairs = 0
            for qb in range(q_blocks):
                q_last = min((qb + 1) * q_tile, args.tokens) - 1
                causal_pairs += min(k_blocks, q_last // k_tile + 1)
            score_tile_bytes = q_tile * k_tile * score_bytes
            kv_tile_bytes = k_tile * HEAD_DIM * BYTES_FP16 * 2
            viable = "yes" if mib(score_tile_bytes) <= args.max_score_mib else "no"
            print(
                f"| {q_tile} | {k_tile} | {mib(score_tile_bytes):.3f} | "
                f"{q_blocks * k_blocks} | {causal_pairs} | {mib(q_tile_bytes):.3f} | "
                f"{mib(kv_tile_bytes):.3f} | {viable} |"
            )
    print()
    print(
        "decision_hint: Start with q_tile in 128..256 and k_tile in 512..1024. "
        "Those keep score tiles small while exposing QK shapes large enough for "
        "matrix hardware. The prototype must fuse or stream softmax/V so dense "
        "T x T scores are never materialized for the full context."
    )


if __name__ == "__main__":
    main()

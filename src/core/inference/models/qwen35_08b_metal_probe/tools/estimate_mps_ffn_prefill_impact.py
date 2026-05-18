#!/usr/bin/env python3
"""Estimate p4096 prefill impact from replacing MSL FFN matmul phases with MPS."""

from __future__ import annotations

import argparse


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--tokens", type=int, default=4096)
    parser.add_argument("--full-s", type=float, default=3.364)
    parser.add_argument("--llama-tok-s", type=float, default=2852.70)
    parser.add_argument("--msl-gate-up-s", type=float, default=0.019215459)
    parser.add_argument("--msl-down-s", type=float, default=0.038315541)
    parser.add_argument("--mps-ffn-s", type=float, default=0.008587583)
    parser.add_argument("--ffn-layers", type=int, default=24)
    args = parser.parse_args()

    msl_ffn = args.msl_gate_up_s + args.msl_down_s
    per_layer_saved = msl_ffn - args.mps_ffn_s
    total_saved = per_layer_saved * args.ffn_layers
    projected_s = max(args.full_s - total_saved, 1e-9)
    projected_tok_s = args.tokens / projected_s
    reference_s = args.tokens / args.llama_tok_s

    print("mps_ffn_prefill_impact_estimate")
    print(f"tokens: {args.tokens}")
    print(f"baseline_full_s: {args.full_s:.9f}")
    print(f"baseline_tok_s: {args.tokens / args.full_s:.2f}")
    print(f"llama_tok_s: {args.llama_tok_s:.2f}")
    print(f"llama_equiv_s: {reference_s:.9f}")
    print(f"msl_ffn_per_layer_s: {msl_ffn:.9f}")
    print(f"mps_ffn_per_layer_s: {args.mps_ffn_s:.9f}")
    print(f"per_layer_saved_s: {per_layer_saved:.9f}")
    print(f"ffn_layers: {args.ffn_layers}")
    print(f"total_saved_s: {total_saved:.9f}")
    print(f"projected_full_s: {projected_s:.9f}")
    print(f"projected_tok_s: {projected_tok_s:.2f}")
    print(f"projected_vs_llama_gap_x: {args.llama_tok_s / projected_tok_s:.3f}")
    print(f"remaining_seconds_to_llama: {projected_s - reference_s:.9f}")
    if projected_tok_s < args.llama_tok_s:
        print("decision: mps_ffn_helps_but_not_enough")
    else:
        print("decision: mps_ffn_estimate_beats_llama")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

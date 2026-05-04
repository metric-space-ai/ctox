#!/usr/bin/env python3
"""Report current Qwen3.5 prefill projections against llama.cpp references.

The report intentionally separates exact/conservative paths from approximate
precision or sparse-window paths. Approximate wins are not accepted-profile wins.
"""

from __future__ import annotations

from dataclasses import dataclass


@dataclass(frozen=True)
class Projection:
    name: str
    tokens: int
    seconds: float
    llama_tok_s: float
    semantics: str
    source: str

    @property
    def tok_s(self) -> float:
        return self.tokens / self.seconds

    @property
    def speedup_vs_llama(self) -> float:
        return self.tok_s / self.llama_tok_s


PROJECTIONS: list[Projection] = [
    Projection(
        name="exact_mps_deltaout",
        tokens=4096,
        seconds=1.316,
        llama_tok_s=2852.70,
        semantics="exact-ish sidecar drift; not accepted profile",
        source="RESEARCH_LOG 2026-05-01 long prefill sweep",
    ),
    Projection(
        name="exact_mps_deltaout",
        tokens=16384,
        seconds=11.733,
        llama_tok_s=2065.71,
        semantics="exact-ish sidecar drift; not accepted profile",
        source="RESEARCH_LOG 2026-05-01 long prefill sweep",
    ),
    Projection(
        name="exact_mps_deltaout",
        tokens=32768,
        seconds=41.642,
        llama_tok_s=1325.20,
        semantics="exact-ish sidecar drift; not accepted profile",
        source="RESEARCH_LOG 2026-05-01 long prefill sweep",
    ),
    Projection(
        name="exact_mps_tiled_forensics",
        tokens=4096,
        seconds=0.853,
        llama_tok_s=2852.70,
        semantics="full-prefill forensics: MPS FFN/Delta/Attention-O sidecars + exact MPS tiled attention",
        source="memory_forensics 2026-05-01 after MPS tiled attention promotion",
    ),
    Projection(
        name="exact_mps_tiled_forensics",
        tokens=16384,
        seconds=4.000,
        llama_tok_s=2065.71,
        semantics="full-prefill forensics: MPS FFN/Delta/Attention-O sidecars + exact MPS tiled attention",
        source="memory_forensics 2026-05-01 after MPS tiled attention promotion",
    ),
    Projection(
        name="exact_mps_tiled_forensics",
        tokens=32768,
        seconds=9.684,
        llama_tok_s=1325.20,
        semantics="full-prefill forensics: MPS FFN/Delta/Attention-O sidecars + exact MPS tiled attention",
        source="memory_forensics 2026-05-01 after MPS tiled attention promotion",
    ),
    Projection(
        name="approx_lanes4_sharedqk_forensics",
        tokens=4096,
        seconds=0.772,
        llama_tok_s=2852.70,
        semantics="approximate SIMD32 Delta scan with shared Q/K threadgroup reuse",
        source="memory_forensics 2026-05-01 CTOX_QWEN35_FORENSICS_DELTA_SCAN_LANES4_SHAREDQK=1",
    ),
    Projection(
        name="approx_lanes4_sharedqk_forensics",
        tokens=16384,
        seconds=3.727,
        llama_tok_s=2065.71,
        semantics="approximate SIMD32 Delta scan with shared Q/K threadgroup reuse",
        source="memory_forensics 2026-05-01 CTOX_QWEN35_FORENSICS_DELTA_SCAN_LANES4_SHAREDQK=1",
    ),
    Projection(
        name="approx_lanes4_sharedqk_forensics",
        tokens=32768,
        seconds=9.117,
        llama_tok_s=1325.20,
        semantics="approximate SIMD32 Delta scan with shared Q/K threadgroup reuse",
        source="memory_forensics 2026-05-01 CTOX_QWEN35_FORENSICS_DELTA_SCAN_LANES4_SHAREDQK=1",
    ),
    Projection(
        name="halfdot_full_context",
        tokens=16384,
        seconds=7.792280752,
        llama_tok_s=2065.71,
        semantics="approximate precision, full context",
        source="RESEARCH_LOG 2026-05-01 HALFDOT",
    ),
    Projection(
        name="halfdot_full_context",
        tokens=32768,
        seconds=29.639952502,
        llama_tok_s=1325.20,
        semantics="approximate precision, full context",
        source="RESEARCH_LOG 2026-05-01 HALFDOT",
    ),
    Projection(
        name="window_halfdot_4096",
        tokens=16384,
        seconds=16384 / 2982.59,
        llama_tok_s=2065.71,
        semantics="approximate precision + local window",
        source="RESEARCH_LOG 2026-05-01 WINDOW_HALFDOT",
    ),
    Projection(
        name="window_halfdot_8192",
        tokens=16384,
        seconds=16384 / 2154.04,
        llama_tok_s=2065.71,
        semantics="approximate precision + local window",
        source="RESEARCH_LOG 2026-05-01 WINDOW_HALFDOT",
    ),
    Projection(
        name="window_halfdot_4096",
        tokens=32768,
        seconds=32768 / 2778.54,
        llama_tok_s=1325.20,
        semantics="approximate precision + local window",
        source="RESEARCH_LOG 2026-05-01 WINDOW_HALFDOT",
    ),
    Projection(
        name="window_halfdot_8192",
        tokens=32768,
        seconds=32768 / 1921.66,
        llama_tok_s=1325.20,
        semantics="approximate precision + local window",
        source="RESEARCH_LOG 2026-05-01 WINDOW_HALFDOT",
    ),
    Projection(
        name="window_halfdot_16384",
        tokens=32768,
        seconds=32768 / 1301.65,
        llama_tok_s=1325.20,
        semantics="approximate precision + local window",
        source="RESEARCH_LOG 2026-05-01 WINDOW_HALFDOT",
    ),
]


def main() -> None:
    print("prefill_reference_report")
    print("reference: llama.cpp BF16/Metal llama-bench")
    print("contract: approximate rows are not accepted-profile wins")
    print()
    print(
        "| tokens | candidate | tok_s | llama_tok_s | vs_llama | status | semantics |"
    )
    print("|---:|---|---:|---:|---:|---|---|")
    for item in PROJECTIONS:
        status = "beats" if item.speedup_vs_llama >= 1.0 else "misses"
        print(
            f"| {item.tokens} | {item.name} | {item.tok_s:.2f} | "
            f"{item.llama_tok_s:.2f} | {item.speedup_vs_llama:.3f}x | "
            f"{status} | {item.semantics} |"
        )

    print()
    exact = [item for item in PROJECTIONS if item.name == "exact_mps_deltaout"]
    misses = [item for item in exact if item.speedup_vs_llama < 1.0]
    if misses:
        worst = min(misses, key=lambda item: item.speedup_vs_llama)
        print(
            "legacy_exact_deltaout_gap: "
            f"worst_tokens={worst.tokens} "
            f"tok_s={worst.tok_s:.2f} "
            f"llama_tok_s={worst.llama_tok_s:.2f} "
            f"vs_llama={worst.speedup_vs_llama:.3f}x"
        )
    projected_exact = [
        item
        for item in PROJECTIONS
        if item.name == "exact_mps_tiled_forensics"
    ]
    projected_wins = [item for item in projected_exact if item.speedup_vs_llama >= 1.0]
    print(f"exact_mps_tiled_forensics_wins: {len(projected_wins)}")
    approx_wins = [
        item
        for item in PROJECTIONS
        if not item.name.startswith("exact_") and item.speedup_vs_llama >= 1.0
    ]
    print(f"approximate_wins: {len(approx_wins)}")
    print("next_exact_target: optimize Delta18+FFN scan/gated-norm/out bottleneck")


if __name__ == "__main__":
    main()

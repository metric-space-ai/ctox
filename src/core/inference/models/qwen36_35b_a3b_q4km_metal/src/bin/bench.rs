// Origin: CTOX
// License: Apache-2.0

//! Stage-1 bench harness entry point. Real prefill/decode benchmarks
//! land in stage 2 once the GGUF loader and the MSL kernel forward
//! path are wired up. For now this binary exits non-zero with a clear
//! "not yet" message so it cannot be mistaken for a working baseline.

use std::process::ExitCode;

fn main() -> ExitCode {
    eprintln!(
        "qwen36-35b-a3b-q4km-metal-bench is a stage-1 skeleton.\n\
         Stage 2 wires this binary against the (yet to land) native\n\
         engine path and reports speedup = t_baseline / t_ours per\n\
         phase. The baseline is the existing qwen36_35b_a3b_ggml\n\
         shim — same M5, same Q4_K_M GGUF, same Responses-IPC\n\
         contract — used purely as a measuring stick, not as a\n\
         runtime dependency. See docs/kernel-dev/BENCHMARK_PROTOCOL.md."
    );
    ExitCode::from(2)
}

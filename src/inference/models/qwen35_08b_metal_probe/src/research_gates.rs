//! Research gates for the Qwen3.5-0.8B Metal probe.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GateStatus {
    Pending,
    InProgress,
    Passed,
    Blocked,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExperimentGate {
    pub name: &'static str,
    pub status: GateStatus,
    pub done_when: &'static str,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResearchPlan {
    pub gates: Vec<ExperimentGate>,
}

impl Default for ResearchPlan {
    fn default() -> Self {
        Self {
            gates: vec![
                ExperimentGate {
                    name: "shape-contract",
                    status: GateStatus::Passed,
                    done_when: "Qwen3.5-0.8B constants and layer pattern are fixed in code",
                },
                ExperimentGate {
                    name: "metal-device-and-bandwidth",
                    status: GateStatus::Passed,
                    done_when:
                        "stream read/write benchmarks report median, p95, and effective GB/s",
                },
                ExperimentGate {
                    name: "hf-artifact-inspection-and-pack-plan",
                    status: GateStatus::Passed,
                    done_when:
                        "local Qwen3.5-0.8B config and safetensors headers validate against the fixed shape and produce a Metal pack plan",
                },
                ExperimentGate {
                    name: "metalpack-writer",
                    status: GateStatus::Passed,
                    done_when:
                        "safetensors FP16/BF16 tensors are written into deterministic FP16-tiled weights.bin plus manifest.json",
                },
                ExperimentGate {
                    name: "fp16-matvec-1024",
                    status: GateStatus::Passed,
                    done_when: "synthetic matvec benchmark covers Qwen3.5-0.8B projection shapes",
                },
                ExperimentGate {
                    name: "gpu-local-lm-head-argmax",
                    status: GateStatus::Passed,
                    done_when: "LM head computes argmax on GPU and returns only next_token",
                },
                ExperimentGate {
                    name: "tiled-lm-head-argmax",
                    status: GateStatus::Passed,
                    done_when:
                        "full-vocab LM head consumes the same row-tiled layout emitted by the metalpack writer",
                },
                ExperimentGate {
                    name: "packed-matvec-1024",
                    status: GateStatus::Passed,
                    done_when:
                        "1024-wide projection matvec and RMS+matvec consume row-tiled metalpack layout",
                },
                ExperimentGate {
                    name: "deltanet-step-kernel",
                    status: GateStatus::Passed,
                    done_when:
                        "single-token DeltaNet recurrent update matches reference within tolerance",
                },
                ExperimentGate {
                    name: "deltanet-recurrent-multistep-stability",
                    status: GateStatus::Passed,
                    done_when:
                        "repeated DeltaNet recurrent updates remain close to reference under a realistic normalized Q/K/V decode trace",
                },
                ExperimentGate {
                    name: "deltanet-decay-activation-kernel",
                    status: GateStatus::Passed,
                    done_when:
                        "Metal computes beta sigmoid and Qwen DeltaNet decay from A_log, a, and dt_bias with CPU-reference agreement",
                },
                ExperimentGate {
                    name: "full-decode-greedy-parity",
                    status: GateStatus::Passed,
                    done_when: "greedy decode matches captured reference tokens",
                },
                ExperimentGate {
                    name: "real-first-token-greedy-parity",
                    status: GateStatus::Passed,
                    done_when:
                        "real Qwen3.5-0.8B Metal decode matches the MLX greedy next token for a raw-token single-step prompt",
                },
                ExperimentGate {
                    name: "one-cpu-sync-per-token",
                    status: GateStatus::Passed,
                    done_when:
                        "the layered decode loop reads only the compact next_token output for each step",
                },
                ExperimentGate {
                    name: "metalpack-decode-skeleton",
                    status: GateStatus::Passed,
                    done_when:
                        "token -> tiled embedding gather -> tiled LM-head argmax runs from a metalpack tensor",
                },
                ExperimentGate {
                    name: "metalpack-decode-plus-projection",
                    status: GateStatus::Passed,
                    done_when:
                        "token -> tiled embedding -> packed RMS projection -> tiled LM-head argmax runs from metalpack tensors",
                },
                ExperimentGate {
                    name: "metalpack-decode-plus-ffn",
                    status: GateStatus::Passed,
                    done_when:
                        "token -> tiled embedding -> packed RMS gate/up -> SwiGLU -> packed down -> tiled LM-head argmax runs from metalpack tensors",
                },
                ExperimentGate {
                    name: "metalpack-decode-plus-attention",
                    status: GateStatus::Passed,
                    done_when:
                        "token -> tiled embedding -> packed RMS Q/K/V projections -> single-token attention combine -> packed O projection -> tiled LM-head argmax runs from metalpack tensors",
                },
                ExperimentGate {
                    name: "metalpack-decode-plus-deltanet",
                    status: GateStatus::Passed,
                    done_when:
                        "token -> tiled embedding -> packed DeltaNet qkv/z/b/a projections -> recurrent state update -> packed out projection -> tiled LM-head argmax runs from metalpack tensors",
                },
                ExperimentGate {
                    name: "metalpack-repeated-ffn-stack",
                    status: GateStatus::Passed,
                    done_when:
                        "multiple packed FFN slices execute back-to-back in one command buffer before LM-head argmax",
                },
                ExperimentGate {
                    name: "metalpack-ddda-superblock",
                    status: GateStatus::Passed,
                    done_when:
                        "a packed D+FFN D+FFN D+FFN A+FFN superblock executes in one command buffer before LM-head argmax",
                },
                ExperimentGate {
                    name: "metalpack-full-pattern-24layer-scheduler",
                    status: GateStatus::Passed,
                    done_when:
                        "six packed D/D/D/A superblocks execute as the full 24-layer Qwen pattern in one command buffer before LM-head argmax",
                },
                ExperimentGate {
                    name: "metalpack-layer-specific-24layer-binding",
                    status: GateStatus::Passed,
                    done_when:
                        "the 24-layer scheduler accepts distinct DeltaNet, attention, and FFN layer arrays instead of one reused superblock slice",
                },
                ExperimentGate {
                    name: "metalpack-auto-layer-resolver",
                    status: GateStatus::Passed,
                    done_when:
                        "a full metalpack manifest is resolved by layer id and tensor class into 18 DeltaNet, 6 attention, and 24 FFN slots",
                },
                ExperimentGate {
                    name: "metalpack-shape-audit",
                    status: GateStatus::Passed,
                    done_when:
                        "shape audit reports Qwen3.5 expected tensor shapes, current kernel-supported shapes, and actual metalpack mismatches",
                },
                ExperimentGate {
                    name: "deltanet-state-param-audit",
                    status: GateStatus::Passed,
                    done_when:
                        "shape audit exposes A_log, dt_bias, and causal Conv1D state parameters as explicit DeltaNet kernel placeholders",
                },
                ExperimentGate {
                    name: "synthetic-true-shape-metalpack",
                    status: GateStatus::Passed,
                    done_when:
                        "a compact alias metalpack with full 24-layer metadata uses real Qwen3.5 DeltaNet, GQA attention, FFN, embedding, and LM-head shapes",
                },
                ExperimentGate {
                    name: "metalpack-qwen-gqa-attention-shapes",
                    status: GateStatus::Passed,
                    done_when:
                        "Metal attention slice accepts q=2048/2056, k/v=512, and o-projection K=2048 in the full 24-layer scheduler",
                },
                ExperimentGate {
                    name: "attention-kv-cache-online-softmax-surface",
                    status: GateStatus::Passed,
                    done_when:
                        "GQA attention dispatch writes K/V into GPU cache buffers and computes output via an online-softmax cache loop",
                },
                ExperimentGate {
                    name: "attention-multistep-kv-cache-smoke",
                    status: GateStatus::Passed,
                    done_when:
                        "a multi-token attention benchmark reuses one GPU KV cache across positions 0..N and reads only next_token between steps",
                },
                ExperimentGate {
                    name: "attention-rope-kv-reference",
                    status: GateStatus::Passed,
                    done_when:
                        "the Metal GQA RoPE/KV-cache attention kernel matches a CPU reference over a multi-step synthetic trace",
                },
                ExperimentGate {
                    name: "layered-24layer-multistep-state-cache-smoke",
                    status: GateStatus::Passed,
                    done_when:
                        "the full 24-layer layered scheduler reuses DeltaNet recurrent state and all attention KV caches across a multi-token sequence",
                },
                ExperimentGate {
                    name: "layered-deltanet-decay-param-binding",
                    status: GateStatus::Passed,
                    done_when:
                        "the full 24-layer layered scheduler reads A_log/dt_bias from the metalpack and feeds them into the DeltaNet decay kernel",
                },
                ExperimentGate {
                    name: "layered-deltanet-conv1d-state-binding",
                    status: GateStatus::Passed,
                    done_when:
                        "the full 24-layer layered scheduler applies packed DeltaNet causal Conv1D weights with persistent per-layer conv state before Q/K/V split",
                },
                ExperimentGate {
                    name: "layered-deltanet-qk-l2norm-binding",
                    status: GateStatus::Passed,
                    done_when:
                        "the full 24-layer layered scheduler applies Qwen DeltaNet Q/K L2 normalization and query scaling before the recurrent update",
                },
                ExperimentGate {
                    name: "layered-deltanet-gated-rmsnorm-binding",
                    status: GateStatus::Passed,
                    done_when:
                        "the full 24-layer layered scheduler applies packed DeltaNet gated RMSNorm weight and SiLU(z) before out projection",
                },
                ExperimentGate {
                    name: "layered-decoder-residual-add-binding",
                    status: GateStatus::Passed,
                    done_when:
                        "the full 24-layer layered scheduler writes token-mixer and FFN outputs as residual + projection on GPU",
                },
                ExperimentGate {
                    name: "layered-decoder-layernorm-binding",
                    status: GateStatus::Passed,
                    done_when:
                        "the full 24-layer layered scheduler uses per-layer input/post RMSNorm weights from the metalpack",
                },
                ExperimentGate {
                    name: "gpu-final-rmsnorm-before-lm-head",
                    status: GateStatus::Passed,
                    done_when:
                        "the full 24-layer layered scheduler applies final RMSNorm on GPU before LM-head argmax",
                },
                ExperimentGate {
                    name: "ffn-gate-up-swiglu-dispatch-fusion",
                    status: GateStatus::Passed,
                    done_when:
                        "the real layered scheduler fuses FFN RMSNorm, gate/up projections, and SwiGLU into one Metal dispatch",
                },
                ExperimentGate {
                    name: "deltanet-qkv-z-b-a-dispatch-fusion",
                    status: GateStatus::Passed,
                    done_when:
                        "the real layered scheduler fuses DeltaNet qkv/z/b/a RMS projections into one Metal dispatch",
                },
                ExperimentGate {
                    name: "deltanet-split-qk-norm-dispatch-fusion",
                    status: GateStatus::Passed,
                    done_when:
                        "the real layered scheduler fuses DeltaNet qkv split and q/k normalization into one Metal dispatch",
                },
                ExperimentGate {
                    name: "attention-q-k-v-dispatch-fusion",
                    status: GateStatus::Passed,
                    done_when:
                        "the real layered scheduler fuses attention q/k/v RMS projections into one Metal dispatch",
                },
                ExperimentGate {
                    name: "attention-qk-norm-rope-cache-fusion",
                    status: GateStatus::Passed,
                    done_when:
                        "the real layered scheduler fuses attention q/k RMSNorm into the RoPE/KV-cache attention dispatch",
                },
                ExperimentGate {
                    name: "projection-residual-writeback-fusion",
                    status: GateStatus::Passed,
                    done_when:
                        "the real layered scheduler writes token-mixer and FFN projection outputs as residual-add in the projection dispatch",
                },
                ExperimentGate {
                    name: "lm-head-rowtile-argmax-fusion",
                    status: GateStatus::Passed,
                    done_when:
                        "the real layered scheduler emits one LM-head argmax candidate per vocab row tile before global reduction",
                },
                ExperimentGate {
                    name: "synthetic-single-dispatch-megakernel",
                    status: GateStatus::Passed,
                    done_when: "one Metal dispatch owns token->embedding->24 synthetic layers->LM-head argmax",
                },
                ExperimentGate {
                    name: "qwen-pattern-single-dispatch-megakernel",
                    status: GateStatus::Passed,
                    done_when: "one Metal dispatch follows [D,D,D,A]x6 with stateful D slices and A slices",
                },
                ExperimentGate {
                    name: "mlx-baseline-beaten",
                    status: GateStatus::Passed,
                    done_when: "warm decode throughput beats MLX on the same Mac and prompt",
                },
                ExperimentGate {
                    name: "ane-coreml-baseline",
                    status: GateStatus::Passed,
                    done_when:
                        "Core ML all/cpuAndGPU/cpuAndNeuralEngine runs are measured or ruled out",
                },
            ],
        }
    }
}

impl ResearchPlan {
    pub fn pending_count(&self) -> usize {
        self.gates
            .iter()
            .filter(|gate| gate.status == GateStatus::Pending)
            .count()
    }

    pub fn passed_count(&self) -> usize {
        self.gates
            .iter()
            .filter(|gate| gate.status == GateStatus::Passed)
            .count()
    }
}

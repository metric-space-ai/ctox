<!-- BEGIN GENERATED 128K RESOURCE CONTRACTS -->

# Local 128k Resource Contracts

This document carries the detailed planner-side `128k` resource contracts derived from the checked-in runtime manifests.

The planner now treats `128k` feasibility as explicit math over model contracts, not as a host-profile guess.

For a `device_layers` candidate with `n_gpu` selected GPUs, CTOX requires:

```text
sum(usable_mb[gpu_i])
  >= required_effective_total_budget_mb

and the anchor/distributed placement must still absorb activation_128k_mb on the assigned GPUs
```

For an `nccl` candidate with `n_gpu` selected GPUs, CTOX requires:

```text
min(usable_mb[gpu_i]) * n_gpu
  >= required_effective_total_budget_mb

and the anchor/distributed placement must still absorb activation_128k_mb on the assigned GPUs
```

Where:

- `required_effective_base_mb = weights_mb + backend_runtime_overhead_mb + load_peak_mb + ceil(kv_128k_mb / pa_memory_fraction)`
- `required_effective_total_budget_mb = required_effective_base_mb + base_overhead(n_gpu) + per_gpu_headroom_mb*n_gpu`
- `activation_128k_mb` is the explicit anchor/prefill/decode term at `128k`
- `usable_mb` is live free VRAM after GPU0 desktop reserve and auxiliary reservations
- `contract_source=explicit` means the manifest carries explicit measurement components; `legacy` means CTOX is still falling back to older coarse sizing fields
- `Backend` in the table is the preferred manifest path for that preset; the planner may still fall back to another technically feasible backend when the platform contract disallows the preferred one
- `legacy` rows are still planner-visible, but they are the next candidates that need explicit per-component measurement contracts instead of fallback sizing

| Model | Preset | 128k | Backend | Quant | Cache | Weights MB | KV@128k MB | Activation@128k MB | Backend Runtime MB | Load Peak MB | PA Fraction | Base Overhead | Safety Headroom | Required Effective Base MB | Required Effective Formula | Required Total Formula | Contract |
| --- | --- | --- | --- | --- | --- | ---: | ---: | ---: | ---: | ---: | --- | --- | --- | ---: | --- | --- | --- |
| google/gemma-4-26B-A4B-it | quality | yes | device_layers | q6k | turboquant3 | 22816 | 9856 | 0 | 0 | 2976 | 800/1000 | 900 + 180*n_gpu | 768*n_gpu | 38112 | 38112 + 900 + 180*n_gpu + 768*n_gpu | 38112 + 900 + 180*n_gpu + 768*n_gpu + 0 | legacy |
| google/gemma-4-26B-A4B-it | performance | yes | device_layers | q4k | turboquant3 | 18400 | 9856 | 0 | 0 | 2400 | 800/1000 | 900 + 180*n_gpu | 256*n_gpu | 33120 | 33120 + 900 + 180*n_gpu + 256*n_gpu | 33120 + 900 + 180*n_gpu + 256*n_gpu + 0 | legacy |
| google/gemma-4-31B-it | quality | yes | device_layers | q6k | turboquant3 | 26660 | 17536 | 0 | 0 | 1736 | 800/1000 | 900 + 180*n_gpu | 768*n_gpu | 50316 | 50316 + 900 + 180*n_gpu + 768*n_gpu | 50316 + 900 + 180*n_gpu + 768*n_gpu + 0 | legacy |
| google/gemma-4-31B-it | performance | yes | device_layers | q4k | turboquant3 | 21500 | 17536 | 0 | 0 | 1400 | 800/1000 | 900 + 180*n_gpu | 256*n_gpu | 44820 | 44820 + 900 + 180*n_gpu + 256*n_gpu | 44820 + 900 + 180*n_gpu + 256*n_gpu + 0 | legacy |
| zai-org/GLM-4.7-Flash | quality | no | - | - | turboquant3 | - | - | - | - | - | - | - | - | - | - | - | legacy |
| zai-org/GLM-4.7-Flash | performance | no | - | - | turboquant3 | - | - | - | - | - | - | - | - | - | - | - | legacy |
| openai/gpt-oss-20b | quality | yes | device_layers | native_mxfp4 | turboquant3 | 15784 | 15488 | 0 | 256 | 512 | 800/1000 | 900 + 180*n_gpu | 0*n_gpu | 35912 | 35912 + 900 + 180*n_gpu + 0*n_gpu | 35912 + 900 + 180*n_gpu + 0*n_gpu + 0 | explicit |
| openai/gpt-oss-20b | performance | yes | nccl | native_mxfp4 | turboquant3 | 15784 | 15488 | 0 | 768 | 1280 | 800/1000 | 1400*n_gpu | 0*n_gpu | 37192 | 37192 + 1400*n_gpu + 0*n_gpu | 37192 + 1400*n_gpu + 0*n_gpu + 0 | explicit |
| nvidia/Nemotron-Cascade-2-30B-A3B | quality | yes | device_layers | q6k | turboquant3 | 28619 | 9856 | 0 | 0 | 1984 | 450/1000 | 900 + 180*n_gpu | 768*n_gpu | 52506 | 52506 + 900 + 180*n_gpu + 768*n_gpu | 52506 + 900 + 180*n_gpu + 768*n_gpu + 0 | legacy |
| nvidia/Nemotron-Cascade-2-30B-A3B | performance | no | - | - | turboquant3 | - | - | - | - | - | - | - | - | - | - | - | legacy |
| Qwen/Qwen3.5-27B | quality | yes | device_layers | q6k | turboquant3 | 22692 | 17408 | 0 | 0 | 952 | 800/1000 | 900 + 180*n_gpu | 768*n_gpu | 45404 | 45404 + 900 + 180*n_gpu + 768*n_gpu | 45404 + 900 + 180*n_gpu + 768*n_gpu + 0 | legacy |
| Qwen/Qwen3.5-27B | performance | yes | device_layers | q4k | turboquant3 | 18300 | 17408 | 0 | 0 | 768 | 800/1000 | 900 + 180*n_gpu | 768*n_gpu | 40828 | 40828 + 900 + 180*n_gpu + 768*n_gpu | 40828 + 900 + 180*n_gpu + 768*n_gpu + 0 | legacy |
| Qwen/Qwen3.5-35B-A3B | quality | yes | device_layers | q6k | turboquant3 | 26660 | 18944 | 0 | 0 | 2728 | 800/1000 | 900 + 180*n_gpu | 768*n_gpu | 53068 | 53068 + 900 + 180*n_gpu + 768*n_gpu | 53068 + 900 + 180*n_gpu + 768*n_gpu + 0 | legacy |
| Qwen/Qwen3.5-35B-A3B | performance | yes | device_layers | q4k | turboquant3 | 21500 | 18944 | 0 | 0 | 2200 | 800/1000 | 900 + 180*n_gpu | 512*n_gpu | 47380 | 47380 + 900 + 180*n_gpu + 512*n_gpu | 47380 + 900 + 180*n_gpu + 512*n_gpu + 0 | legacy |
| Qwen/Qwen3.5-4B | quality | yes | device_layers | q6k | turboquant3 | 4464 | 5504 | 10368 | 0 | 476 | 800/1000 | 900 + 180*n_gpu | 512*n_gpu | 11820 | 11820 + 900 + 180*n_gpu + 512*n_gpu | 11820 + 900 + 180*n_gpu + 512*n_gpu + 10368 | explicit |
| Qwen/Qwen3.5-4B | performance | yes | nccl | q4k | turboquant3 | 3600 | 5504 | 10368 | 0 | 384 | 800/1000 | 1400*n_gpu | 512*n_gpu | 10864 | 10864 + 1400*n_gpu + 512*n_gpu | 10864 + 1400*n_gpu + 512*n_gpu + 10368 | explicit |
| Qwen/Qwen3.5-9B | quality | yes | device_layers | q6k | turboquant3 | 8308 | 7808 | 0 | 0 | 634 | 800/1000 | 900 + 180*n_gpu | 768*n_gpu | 18702 | 18702 + 900 + 180*n_gpu + 768*n_gpu | 18702 + 900 + 180*n_gpu + 768*n_gpu + 0 | legacy |
| Qwen/Qwen3.5-9B | performance | yes | device_layers | q4k | turboquant3 | 6700 | 7808 | 0 | 0 | 512 | 800/1000 | 900 + 180*n_gpu | 768*n_gpu | 16972 | 16972 + 900 + 180*n_gpu + 768*n_gpu | 16972 + 900 + 180*n_gpu + 768*n_gpu + 0 | legacy |

<!-- END GENERATED 128K RESOURCE CONTRACTS -->

// Vendored from llama.cpp ggml-cuda/norm.cu verbatim.
// The ONLY modification: strip `static` from `rms_norm_f32` and
// `l2_norm_f32` template definitions so their mangled PTX symbols
// become `.visible` for cudarc's module loader. All other code is
// byte-identical to upstream at the commit pinned in
// ../../vendor/llama-cpp.version.
#include "../../vendor/ggml-cuda/common.cuh"
#include "../../vendor/ggml-cuda/norm.cuh"
#include <cstdint>

// Copy-paste the body of upstream ggml-cuda/norm.cu here would
// duplicate the static helpers (norm_f32, group_norm_f32,
// rms_norm_back_f32) that we don't need. Instead: include the
// upstream translation unit directly. Its `static __global__`
// decls stay internal to THIS TU. We expose only what we need
// via the two externally-linked wrappers below — themselves
// literal copies of upstream kernel body with template params
// specialized to our use case (block_size=1024, unfused).
#include "../../vendor/ggml-cuda/norm.cu"


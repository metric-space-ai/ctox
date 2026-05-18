// Shim TU: pulls in vendored ssm-conv.cu and forces explicit template
// instantiations for the (apply_silu, 128, d_conv) combos we need at
// runtime. Upstream declares the templates non-static but never
// instantiates them in the cu source — so nvcc doesn't emit PTX for
// them. We force emission by taking function pointers into an externally-
// visible table. Unlike host-stub launches, function-pointer take cannot
// be DCE'd even at -O3 because the external symbol observes the address.

#include "../../../vendor/ggml-cuda/ssm-conv.cu"

// Signature types. Must match the templated kernel declarations exactly
// (see ssm-conv.cu:10-12 and :55-58).
typedef void (*ssm_conv_short_fn)(const float *, const float *,
                                  int, int, int, int,
                                  float *, int, int, int, const int64_t);
typedef void (*ssm_conv_long_fn)(const float *, const float *,
                                 int, int, int, int,
                                 float *, int, int, int, const int64_t);

// Externally-visible function-pointer tables. nvcc cannot DCE these
// because their addresses escape the TU via extern "C" symbols.
extern "C" __attribute__((visibility("default")))
ssm_conv_short_fn ctox_ssm_conv_short_tbl[8] = {
    &ssm_conv_f32<false, 128, 3>,
    &ssm_conv_f32<false, 128, 4>,
    &ssm_conv_f32<false, 128, 5>,
    &ssm_conv_f32<false, 128, 9>,
    &ssm_conv_f32<true,  128, 3>,
    &ssm_conv_f32<true,  128, 4>,
    &ssm_conv_f32<true,  128, 5>,
    &ssm_conv_f32<true,  128, 9>,
};

extern "C" __attribute__((visibility("default")))
ssm_conv_long_fn ctox_ssm_conv_long_tbl[8] = {
    &ssm_conv_long_token_f32<false, 128, 3, 32>,
    &ssm_conv_long_token_f32<false, 128, 4, 32>,
    &ssm_conv_long_token_f32<false, 128, 5, 32>,
    &ssm_conv_long_token_f32<false, 128, 9, 32>,
    &ssm_conv_long_token_f32<true,  128, 3, 32>,
    &ssm_conv_long_token_f32<true,  128, 4, 32>,
    &ssm_conv_long_token_f32<true,  128, 5, 32>,
    &ssm_conv_long_token_f32<true,  128, 9, 32>,
};

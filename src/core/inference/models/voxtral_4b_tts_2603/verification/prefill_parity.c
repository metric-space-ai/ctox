/* Actual-weight verification of host dispatch; not a production runtime switch.
 * ref: vendor/voxtral-tts.c/voxtral_tts_llm.c:277-392 (pinned CPU baseline).
 * Compile against the crate's CUDA libvoxtral_native.a and original headers. */
#include "voxtral_tts.h"
#include "voxtral_tts_kernels.h"
#include "voxtral_tts_cuda.h"
#include <math.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>
extern void ctox_tts_llm_prefill_cpu(tts_ctx_t *, const float *, int);
extern int ctox_voxtral_cuda_status(void);
static double now_ms(void) {
    struct timespec t;
    clock_gettime(CLOCK_MONOTONIC, &t);
    return 1000.0 * t.tv_sec + t.tv_nsec / 1000000.0;
}
static double relative_l2(const float *reference, const float *actual, size_t n) {
    double error = 0, norm = 0;
    for (size_t i = 0; i < n; i++) {
        if (!isfinite(reference[i]) || !isfinite(actual[i])) return INFINITY;
        double diff = (double)actual[i] - reference[i];
        error += diff * diff;
        norm += (double)reference[i] * reference[i];
    }
    return sqrt(error / fmax(norm, 1e-30));
}
int main(int argc, char **argv) {
    if (argc != 2) return 64;
    tts_ctx_t *ctx = tts_load(argv[1]);
    if (!ctx || !tts_cuda_available()) { if (ctx) tts_free(ctx); return 1; }
    enum { N = 3, D = TTS_DEC_DIM, K = TTS_DEC_KV_HEADS * TTS_DEC_HEAD_DIM };
    const int tokens[N] = { TTS_TOK_BOS, TTS_TOK_BEGIN_AUDIO, TTS_TOK_AUDIO };
    float *embeds = malloc((size_t)N * D * sizeof(float));
    size_t count = (size_t)TTS_DEC_LAYERS * ctx->kv_cache_max * K;
    float *reference_k = malloc(count * sizeof(float));
    float *reference_v = malloc(count * sizeof(float));
    float *actual_k = malloc(count * sizeof(float));
    float *actual_v = malloc(count * sizeof(float));
    if (!embeds || !reference_k || !reference_v || !actual_k || !actual_v) return 1;
    for (int i = 0; i < N; i++)
        tts_embed_token_bf16(embeds + i * D, ctx->decoder.tok_embeddings_bf16, tokens[i], D);
    ctx->kv_cache_len = 0;
    double started = now_ms();
    ctox_tts_llm_prefill_cpu(ctx, embeds, N);
    double cpu_ms = now_ms() - started;
    memcpy(reference_k, ctx->kv_cache_k, count * sizeof(float));
    memcpy(reference_v, ctx->kv_cache_v, count * sizeof(float));
    float reference_hidden[D], actual_hidden[D];
    tts_llm_forward(ctx, embeds + (N - 1) * D, reference_hidden);
    ctx->kv_cache_len = 0;
    started = now_ms();
    tts_llm_prefill(ctx, embeds, N);
    double cuda_ms = now_ms() - started;
    tts_cuda_to_host(actual_k, g_cuda.kv_cache_k_gpu, count * sizeof(float));
    tts_cuda_to_host(actual_v, g_cuda.kv_cache_v_gpu, count * sizeof(float));
    double k_error = 0, v_error = 0;
    for (int layer = 0; layer < TTS_DEC_LAYERS; layer++) {
        size_t offset = (size_t)layer * ctx->kv_cache_max * K;
        k_error = fmax(k_error, relative_l2(reference_k + offset, actual_k + offset, N * K));
        v_error = fmax(v_error, relative_l2(reference_v + offset, actual_v + offset, N * K));
    }
    tts_llm_forward(ctx, embeds + (N - 1) * D, actual_hidden);
    double hidden_error = relative_l2(reference_hidden, actual_hidden, D);
    /* CPU uses f32 activations; the vendored CUDA cuBLAS path rounds to bf16.
     * Require <=2% relative L2 at each layer and on the continuation hidden state. */
    int pass = ctox_voxtral_cuda_status() == 0 && ctx->kv_cache_len == N + 1 &&
        k_error <= 0.02 && v_error <= 0.02 && hidden_error <= 0.02;
    printf("{\"pass\":%s,\"tokens\":%d,\"cpu_prefill_ms\":%.3f,\"cuda_prefill_ms\":%.3f,\"max_layer_k_relative_l2\":%.9f,\"max_layer_v_relative_l2\":%.9f,\"continuation_relative_l2\":%.9f}\n",
        pass ? "true" : "false", N, cpu_ms, cuda_ms, k_error, v_error, hidden_error);
    free(embeds); free(reference_k); free(reference_v); free(actual_k); free(actual_v);
    tts_free(ctx);
    return pass ? 0 : 1;
}

/* CTOX host boundary; upstream graph and kernels remain model-local. */
#include "voxtral_tts.h"
#include <stdlib.h>
#ifdef USE_CUDA
extern int ctox_voxtral_cuda_status(void);
#endif
void *ctox_voxtral_load(const char *dir) { return tts_load(dir); }
void ctox_voxtral_free(void *ctx) { tts_free(ctx); }
int ctox_voxtral_generate(void *ctx, const char *text, const char *voice,
                         float **samples, int *count) {
    int status = tts_generate(ctx, text, voice, samples, count);
#ifdef USE_CUDA
    if (ctox_voxtral_cuda_status() != 0) status = -1;
#endif
    return status;
}
void ctox_voxtral_samples_free(float *samples) { free(samples); }

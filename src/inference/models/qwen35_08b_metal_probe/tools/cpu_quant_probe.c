#include <arm_neon.h>
#include <mach/mach_time.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static uint64_t now_ns(void) {
    static mach_timebase_info_data_t info;
    if (info.denom == 0) {
        mach_timebase_info(&info);
    }
    uint64_t t = mach_absolute_time();
    return t * info.numer / info.denom;
}

static int parse_arg(int argc, char **argv, int idx, int fallback) {
    if (argc <= idx) {
        return fallback;
    }
    char *end = NULL;
    long value = strtol(argv[idx], &end, 10);
    if (end == argv[idx] || value <= 0) {
        return fallback;
    }
    return (int)value;
}

static void fill_i8(int8_t *ptr, size_t len, uint32_t seed) {
    uint32_t x = seed;
    for (size_t i = 0; i < len; ++i) {
        x = 1664525u * x + 1013904223u;
        ptr[i] = (int8_t)((int)((x >> 24) & 0x7f) - 63);
    }
}

static void pack_q4(uint8_t *dst, const int8_t *src, size_t len) {
    for (size_t i = 0; i < len; i += 2) {
        int lo = src[i] / 8;
        int hi = (i + 1 < len) ? src[i + 1] / 8 : 0;
        if (lo < -8) lo = -8;
        if (lo > 7) lo = 7;
        if (hi < -8) hi = -8;
        if (hi > 7) hi = 7;
        dst[i / 2] = (uint8_t)(lo & 0x0f) | (uint8_t)((hi & 0x0f) << 4);
    }
}

static int8x16_t unpack_q4x16(const uint8_t *src) {
    uint8x8_t packed = vld1_u8(src);
    uint8x8_t lo = vand_u8(packed, vdup_n_u8(0x0f));
    uint8x8_t hi = vshr_n_u8(packed, 4);
    uint8x16_t joined = vcombine_u8(lo, hi);
    int8x16_t signed_nibbles = vreinterpretq_s8_u8(joined);
    uint8x16_t sign = vcgtq_u8(joined, vdupq_n_u8(7));
    signed_nibbles = vsubq_s8(signed_nibbles, vreinterpretq_s8_u8(vandq_u8(sign, vdupq_n_u8(16))));
    return vshlq_n_s8(signed_nibbles, 3);
}

static int32_t dot_i8_dotprod(const int8_t *x, const int8_t *w, int k) {
    int32x4_t acc = vdupq_n_s32(0);
    for (int c = 0; c < k; c += 16) {
        int8x16_t xv = vld1q_s8(x + c);
        int8x16_t wv = vld1q_s8(w + c);
#if defined(__ARM_FEATURE_DOTPROD)
        acc = vdotq_s32(acc, xv, wv);
#else
        int16x8_t lo = vmull_s8(vget_low_s8(xv), vget_low_s8(wv));
        int16x8_t hi = vmull_s8(vget_high_s8(xv), vget_high_s8(wv));
        acc = vaddq_s32(acc, vpaddlq_s16(lo));
        acc = vaddq_s32(acc, vpaddlq_s16(hi));
#endif
    }
    int32x2_t sum2 = vadd_s32(vget_low_s32(acc), vget_high_s32(acc));
    return vget_lane_s32(vpadd_s32(sum2, sum2), 0);
}

static int32_t dot_q4_unpack_dotprod(const int8_t *x, const uint8_t *wq4, int k) {
    int32x4_t acc = vdupq_n_s32(0);
    for (int c = 0; c < k; c += 16) {
        int8x16_t xv = vld1q_s8(x + c);
        int8x16_t wv = unpack_q4x16(wq4 + c / 2);
#if defined(__ARM_FEATURE_DOTPROD)
        acc = vdotq_s32(acc, xv, wv);
#else
        int16x8_t lo = vmull_s8(vget_low_s8(xv), vget_low_s8(wv));
        int16x8_t hi = vmull_s8(vget_high_s8(xv), vget_high_s8(wv));
        acc = vaddq_s32(acc, vpaddlq_s16(lo));
        acc = vaddq_s32(acc, vpaddlq_s16(hi));
#endif
    }
    int32x2_t sum2 = vadd_s32(vget_low_s32(acc), vget_high_s32(acc));
    return vget_lane_s32(vpadd_s32(sum2, sum2), 0);
}

static double run_i8(const int8_t *x, const int8_t *w, int32_t *y, int tokens, int rows, int k) {
    uint64_t start = now_ns();
    for (int t = 0; t < tokens; ++t) {
        const int8_t *xt = x + (size_t)t * k;
        for (int r = 0; r < rows; ++r) {
            __builtin_prefetch(w + (size_t)(r + 1) * k, 0, 3);
            y[(size_t)t * rows + r] = dot_i8_dotprod(xt, w + (size_t)r * k, k);
        }
    }
    return (double)(now_ns() - start) / 1.0e9;
}

static double run_q4(const int8_t *x, const uint8_t *wq4, int32_t *y, int tokens, int rows, int k) {
    uint64_t start = now_ns();
    for (int t = 0; t < tokens; ++t) {
        const int8_t *xt = x + (size_t)t * k;
        for (int r = 0; r < rows; ++r) {
            __builtin_prefetch(wq4 + (size_t)(r + 1) * (k / 2), 0, 3);
            y[(size_t)t * rows + r] = dot_q4_unpack_dotprod(xt, wq4 + (size_t)r * (k / 2), k);
        }
    }
    return (double)(now_ns() - start) / 1.0e9;
}

static int cmp_double(const void *a, const void *b) {
    double da = *(const double *)a;
    double db = *(const double *)b;
    return (da > db) - (da < db);
}

int main(int argc, char **argv) {
    int tokens = parse_arg(argc, argv, 1, 128);
    int rows = parse_arg(argc, argv, 2, 3584);
    int k = parse_arg(argc, argv, 3, 1024);
    int iterations = parse_arg(argc, argv, 4, 5);
    int warmup = parse_arg(argc, argv, 5, 2);
    if ((k % 16) != 0) {
        fprintf(stderr, "k must be divisible by 16\n");
        return 2;
    }

    int8_t *x = NULL;
    int8_t *w = NULL;
    uint8_t *wq4 = NULL;
    int32_t *y = NULL;
    posix_memalign((void **)&x, 128, (size_t)tokens * k);
    posix_memalign((void **)&w, 128, (size_t)rows * k);
    posix_memalign((void **)&wq4, 128, (size_t)rows * (k / 2));
    posix_memalign((void **)&y, 128, (size_t)tokens * rows * sizeof(int32_t));
    if (!x || !w || !wq4 || !y) {
        fprintf(stderr, "allocation failed\n");
        return 1;
    }

    fill_i8(x, (size_t)tokens * k, 0x12345678u);
    fill_i8(w, (size_t)rows * k, 0x9abcdef0u);
    pack_q4(wq4, w, (size_t)rows * k);
    memset(y, 0, (size_t)tokens * rows * sizeof(int32_t));

    for (int i = 0; i < warmup; ++i) {
        (void)run_i8(x, w, y, tokens, rows, k);
        (void)run_q4(x, wq4, y, tokens, rows, k);
    }

    double *i8_samples = calloc((size_t)iterations, sizeof(double));
    double *q4_samples = calloc((size_t)iterations, sizeof(double));
    for (int i = 0; i < iterations; ++i) {
        i8_samples[i] = run_i8(x, w, y, tokens, rows, k);
        q4_samples[i] = run_q4(x, wq4, y, tokens, rows, k);
    }
    qsort(i8_samples, (size_t)iterations, sizeof(double), cmp_double);
    qsort(q4_samples, (size_t)iterations, sizeof(double), cmp_double);

    double i8_median = i8_samples[iterations / 2];
    double q4_median = q4_samples[iterations / 2];
    double ops = 2.0 * (double)tokens * (double)rows * (double)k;
    double i8_bytes = (double)tokens * k + (double)rows * k + (double)tokens * rows * sizeof(int32_t);
    double q4_bytes = (double)tokens * k + (double)rows * (k / 2) + (double)tokens * rows * sizeof(int32_t);
    long long checksum = 0;
    for (int i = 0; i < rows && i < 16; ++i) {
        checksum += y[i];
    }

    printf("cpu_quant_probe\n");
    printf("shape: tokens=%d rows=%d k=%d\n", tokens, rows, k);
#if defined(__ARM_FEATURE_DOTPROD)
    printf("neon_dotprod_compile_feature: 1\n");
#else
    printf("neon_dotprod_compile_feature: 0\n");
#endif
#if defined(__ARM_FEATURE_MATMUL_INT8)
    printf("i8mm_compile_feature: 1\n");
#else
    printf("i8mm_compile_feature: 0\n");
#endif
#if defined(__ARM_FEATURE_BF16)
    printf("bf16_compile_feature: 1\n");
#else
    printf("bf16_compile_feature: 0\n");
#endif
#if defined(__ARM_FEATURE_SME)
    printf("sme_compile_feature: 1\n");
#else
    printf("sme_compile_feature: 0\n");
#endif
#if defined(__ARM_FEATURE_SME2)
    printf("sme2_compile_feature: 1\n");
#else
    printf("sme2_compile_feature: 0\n");
#endif
    printf("sme2_usage_status: not_used_by_this_probe\n");
    printf("iterations: %d\n", iterations);
    printf("warmup: %d\n", warmup);
    printf("i8_median_s: %.9f\n", i8_median);
    printf("i8_effective_tops: %.3f\n", ops / i8_median / 1.0e12);
    printf("i8_visible_gb_s: %.3f\n", i8_bytes / i8_median / 1.0e9);
    printf("q4_unpack_median_s: %.9f\n", q4_median);
    printf("q4_unpack_effective_tops: %.3f\n", ops / q4_median / 1.0e12);
    printf("q4_unpack_visible_gb_s: %.3f\n", q4_bytes / q4_median / 1.0e9);
    printf("checksum16: %lld\n", checksum);

    free(i8_samples);
    free(q4_samples);
    free(x);
    free(w);
    free(wq4);
    free(y);
    return 0;
}

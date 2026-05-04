#include <mach/mach_time.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>

#if defined(__ARM_FEATURE_SME)
#include <arm_sme.h>
#include <arm_sve.h>
#endif

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

#if defined(__ARM_FEATURE_SME)
__arm_locally_streaming uint64_t sme_streaming_vector_bytes(void) {
    return (uint64_t)svcntb();
}

__arm_locally_streaming uint64_t sme_streaming_vector_words(void) {
    return (uint64_t)svcntsw();
}

__arm_new("za") __arm_locally_streaming int64_t
sme_i8_mopa_repeated(const int8_t *a, const int8_t *b, int repeats) {
    svbool_t pg8 = svptrue_b8();
    svbool_t pg32 = svptrue_b32();
    svzero_za();

    for (int i = 0; i < repeats; ++i) {
        svint8_t av = svld1_s8(pg8, a);
        svint8_t bv = svld1_s8(pg8, b);
        svmopa_za32_s8_m(0, pg8, pg8, av, bv);
    }

    int64_t checksum = 0;
    uint64_t rows = svcntsw();
    for (uint64_t row = 0; row < rows; ++row) {
        svint32_t z = svread_hor_za32_s32_m(svdup_s32(0), pg32, 0, (uint32_t)row);
        checksum += (int64_t)svaddv_s32(pg32, z);
    }
    return checksum;
}
#endif

int main(int argc, char **argv) {
    int repeats = parse_arg(argc, argv, 1, 100000);
    int iterations = parse_arg(argc, argv, 2, 5);
    int warmup = parse_arg(argc, argv, 3, 1);

    printf("sme2_mopa_probe\n");
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

#if !defined(__ARM_FEATURE_SME)
    printf("status: unavailable\n");
    return 0;
#else
    uint64_t bytes = sme_streaming_vector_bytes();
    int8_t *a = NULL;
    int8_t *b = NULL;
    if (posix_memalign((void **)&a, 128, bytes) != 0 ||
        posix_memalign((void **)&b, 128, bytes) != 0) {
        fprintf(stderr, "allocation failed\n");
        return 1;
    }
    fill_i8(a, bytes, 0x12345678u);
    fill_i8(b, bytes, 0x9abcdef0u);

    int64_t checksum = 0;
    for (int i = 0; i < warmup; ++i) {
        checksum += sme_i8_mopa_repeated(a, b, repeats);
    }

    double best_s = 1.0e30;
    double total_s = 0.0;
    for (int i = 0; i < iterations; ++i) {
        uint64_t start = now_ns();
        checksum += sme_i8_mopa_repeated(a, b, repeats) * (int64_t)(i + 1);
        double seconds = (double)(now_ns() - start) / 1.0e9;
        total_s += seconds;
        if (seconds < best_s) {
            best_s = seconds;
        }
    }

    printf("status: ok\n");
    printf("streaming_vector_bytes: %llu\n", (unsigned long long)bytes);
    printf("za_rows_s32: %llu\n", (unsigned long long)sme_streaming_vector_words());
    printf("repeats: %d\n", repeats);
    printf("iterations: %d\n", iterations);
    printf("warmup: %d\n", warmup);
    printf("best_s: %.9f\n", best_s);
    printf("mean_s: %.9f\n", total_s / (double)iterations);
    printf("mopa_per_s_best: %.3f\n", (double)repeats / best_s);
    printf("checksum: %lld\n", (long long)checksum);
    printf("hotpath_status: microkernel_probe_not_model_path\n");

    free(a);
    free(b);
    return 0;
#endif
}

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

static int ceil_div_int(int a, int b) {
    return (a + b - 1) / b;
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
sme_i8_tile_stream(const int8_t *a,
                   const int8_t *b,
                   int32_t *out,
                   int m_tiles,
                   int n_tiles,
                   int k_blocks) {
    svbool_t pg8 = svptrue_b8();
    svbool_t pg32 = svptrue_b32();
    uint64_t bytes = svcntb();
    uint64_t za_rows = svcntsw();
    int64_t checksum = 0;
    size_t out_tile_elems = (size_t)za_rows * (size_t)za_rows;

    for (int mt = 0; mt < m_tiles; ++mt) {
        for (int nt = 0; nt < n_tiles; ++nt) {
            svzero_za();

            for (int kb = 0; kb < k_blocks; ++kb) {
                const int8_t *ap = a + ((size_t)mt * (size_t)k_blocks + (size_t)kb) * bytes;
                const int8_t *bp = b + ((size_t)nt * (size_t)k_blocks + (size_t)kb) * bytes;
                svint8_t av = svld1_s8(pg8, ap);
                svint8_t bv = svld1_s8(pg8, bp);
                svmopa_za32_s8_m(0, pg8, pg8, av, bv);
            }

            int32_t *tile_out = out + ((size_t)mt * (size_t)n_tiles + (size_t)nt) * out_tile_elems;
            for (uint64_t row = 0; row < za_rows; ++row) {
                svint32_t z = svread_hor_za32_s32_m(svdup_s32(0), pg32, 0, (uint32_t)row);
                checksum += (int64_t)svaddv_s32(pg32, z);
                svst1_hor_za32(0, (uint32_t)row, pg32, tile_out + row * za_rows);
            }
        }
    }

    return checksum;
}
#endif

int main(int argc, char **argv) {
    int tokens = parse_arg(argc, argv, 1, 512);
    int rows = parse_arg(argc, argv, 2, 3584);
    int k = parse_arg(argc, argv, 3, 1024);
    int iterations = parse_arg(argc, argv, 4, 5);
    int warmup = parse_arg(argc, argv, 5, 1);

    printf("sme2_i8_tile_probe\n");
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
    uint64_t za_rows = sme_streaming_vector_words();
    int m_tiles = ceil_div_int(tokens, (int)za_rows);
    int n_tiles = ceil_div_int(rows, (int)za_rows);
    int k_blocks = ceil_div_int(k, (int)bytes);

    size_t a_bytes = (size_t)m_tiles * (size_t)k_blocks * (size_t)bytes;
    size_t b_bytes = (size_t)n_tiles * (size_t)k_blocks * (size_t)bytes;
    size_t out_elems =
        (size_t)m_tiles * (size_t)n_tiles * (size_t)za_rows * (size_t)za_rows;
    size_t out_bytes = out_elems * sizeof(int32_t);

    int8_t *a = NULL;
    int8_t *b = NULL;
    int32_t *out = NULL;
    if (posix_memalign((void **)&a, 128, a_bytes) != 0 ||
        posix_memalign((void **)&b, 128, b_bytes) != 0 ||
        posix_memalign((void **)&out, 128, out_bytes) != 0) {
        fprintf(stderr, "allocation failed\n");
        free(a);
        free(b);
        free(out);
        return 1;
    }

    fill_i8(a, a_bytes, 0x13579bdfu);
    fill_i8(b, b_bytes, 0x2468ace0u);

    int64_t checksum = 0;
    for (int i = 0; i < warmup; ++i) {
        checksum += sme_i8_tile_stream(a, b, out, m_tiles, n_tiles, k_blocks);
    }

    double best_s = 1.0e30;
    double total_s = 0.0;
    for (int i = 0; i < iterations; ++i) {
        uint64_t start = now_ns();
        checksum += sme_i8_tile_stream(a, b, out, m_tiles, n_tiles, k_blocks) * (int64_t)(i + 1);
        double seconds = (double)(now_ns() - start) / 1.0e9;
        total_s += seconds;
        if (seconds < best_s) {
            best_s = seconds;
        }
    }

    size_t mopa_per_run = (size_t)m_tiles * (size_t)n_tiles * (size_t)k_blocks;
    size_t stream_read_bytes_per_run =
        (size_t)m_tiles * (size_t)n_tiles * (size_t)k_blocks * (size_t)bytes * 2u;
    size_t stream_write_bytes_per_run =
        (size_t)m_tiles * (size_t)n_tiles * (size_t)za_rows * (size_t)za_rows *
        sizeof(int32_t);
    double stream_gb_best =
        ((double)stream_read_bytes_per_run + (double)stream_write_bytes_per_run) /
        best_s / 1.0e9;

    printf("status: ok\n");
    printf("tokens: %d\n", tokens);
    printf("rows: %d\n", rows);
    printf("k: %d\n", k);
    printf("streaming_vector_bytes: %llu\n", (unsigned long long)bytes);
    printf("za_rows_s32: %llu\n", (unsigned long long)za_rows);
    printf("m_tiles: %d\n", m_tiles);
    printf("n_tiles: %d\n", n_tiles);
    printf("k_blocks: %d\n", k_blocks);
    printf("iterations: %d\n", iterations);
    printf("warmup: %d\n", warmup);
    printf("working_set_a_bytes: %zu\n", a_bytes);
    printf("working_set_b_bytes: %zu\n", b_bytes);
    printf("working_set_out_bytes: %zu\n", out_bytes);
    printf("stream_read_bytes_per_run: %zu\n", stream_read_bytes_per_run);
    printf("stream_write_bytes_per_run: %zu\n", stream_write_bytes_per_run);
    printf("mopa_per_run: %zu\n", mopa_per_run);
    printf("best_s: %.9f\n", best_s);
    printf("mean_s: %.9f\n", total_s / (double)iterations);
    printf("mopa_per_s_best: %.3f\n", (double)mopa_per_run / best_s);
    printf("stream_gb_s_best: %.3f\n", stream_gb_best);
    printf("checksum: %lld\n", (long long)checksum);
    printf("hotpath_status: tile_probe_not_model_path\n");
    printf("interpretation: qwen_shape_streaming_probe_not_layout_correct_matmul\n");

    free(a);
    free(b);
    free(out);
    return 0;
#endif
}

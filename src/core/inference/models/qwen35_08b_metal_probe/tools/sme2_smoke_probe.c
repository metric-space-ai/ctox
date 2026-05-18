#include <stdint.h>
#include <stdio.h>

#if defined(__ARM_FEATURE_SME)
#include <arm_sme.h>
#include <arm_sve.h>
#endif

#if defined(__ARM_FEATURE_SME)
__arm_locally_streaming int sme_streaming_vector_words(void) {
    return (int)svcntsw();
}

__arm_new("za") __arm_locally_streaming void sme_zero_za_smoke(void) {
    svzero_za();
}
#endif

int main(void) {
    printf("sme2_smoke_probe\n");
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
#if defined(__ARM_FEATURE_LOCALLY_STREAMING)
    printf("locally_streaming_compile_feature: 1\n");
#else
    printf("locally_streaming_compile_feature: 0\n");
#endif
#if defined(__ARM_FEATURE_MATMUL_INT8)
    printf("i8mm_compile_feature: 1\n");
#else
    printf("i8mm_compile_feature: 0\n");
#endif

#if defined(__ARM_FEATURE_SME)
    int words = sme_streaming_vector_words();
    sme_zero_za_smoke();
    printf("sme_streaming_call_status: ok\n");
    printf("sme_streaming_vector_words: %d\n", words);
    printf("sme_streaming_vector_bytes: %d\n", words * 4);
    printf("sme_za_zero_status: ok\n");
    printf("sme2_hotpath_status: smoke_only_not_model_path\n");
#else
    printf("sme_streaming_call_status: unavailable\n");
    printf("sme_za_zero_status: unavailable\n");
    printf("sme2_hotpath_status: unavailable\n");
#endif

    return 0;
}

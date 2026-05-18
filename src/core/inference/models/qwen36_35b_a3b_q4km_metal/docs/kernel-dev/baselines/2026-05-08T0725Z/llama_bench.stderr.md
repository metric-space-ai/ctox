load_backend: loaded BLAS backend from /opt/homebrew/Cellar/ggml/0.11.0/libexec/libggml-blas.so
ggml_metal_device_init: testing tensor API for f16 support
ggml_metal_library_compile_pipeline: compiling pipeline: base = 'dummy_kernel', name = 'dummy_kernel'
ggml_metal_library_compile_pipeline: loaded dummy_kernel                                  0x101cf61d0 | th_max = 1024 | th_width =   32
ggml_metal_device_init: testing tensor API for bfloat support
ggml_metal_library_compile_pipeline: compiling pipeline: base = 'dummy_kernel', name = 'dummy_kernel'
ggml_metal_library_compile_pipeline: loaded dummy_kernel                                  0x101cc9e80 | th_max = 1024 | th_width =   32
ggml_metal_library_init: using embedded metal library
ggml_metal_library_init: loaded in 6.035 sec
ggml_metal_rsets_init: creating a residency set collection (keep_alive = 180 s)
ggml_metal_device_init: GPU name:   MTL0 (Apple M5)
ggml_metal_device_init: GPU family: MTLGPUFamilyApple10  (1010)
ggml_metal_device_init: GPU family: MTLGPUFamilyCommon3 (3003)
ggml_metal_device_init: GPU family: MTLGPUFamilyMetal4  (5002)
ggml_metal_device_init: simdgroup reduction   = true
ggml_metal_device_init: simdgroup matrix mul. = true
ggml_metal_device_init: has unified memory    = true
ggml_metal_device_init: has bfloat            = true
ggml_metal_device_init: has tensor            = true
ggml_metal_device_init: use residency sets    = true
ggml_metal_device_init: use shared buffers    = true
ggml_metal_device_init: recommendedMaxWorkingSetSize  = 26800.60 MB
load_backend: loaded MTL backend from /opt/homebrew/Cellar/ggml/0.11.0/libexec/libggml-metal.so
load_backend: loaded CPU backend from /opt/homebrew/Cellar/ggml/0.11.0/libexec/libggml-cpu-apple_m4.so
| model                          |       size |     params | backend    | threads |            test |                  t/s |
| ------------------------------ | ---------: | ---------: | ---------- | ------: | --------------: | -------------------: |
| qwen35moe 35B.A3B Q4_K - Medium |  19.91 GiB |    34.66 B | BLAS,MTL   |       8 |           pp512 |       758.08 ± 19.71 |
| qwen35moe 35B.A3B Q4_K - Medium |  19.91 GiB |    34.66 B | BLAS,MTL   |       8 |          pp4096 |       710.69 ± 16.62 |
| qwen35moe 35B.A3B Q4_K - Medium |  19.91 GiB |    34.66 B | BLAS,MTL   |       8 |         pp16384 |       544.69 ± 17.60 |
| qwen35moe 35B.A3B Q4_K - Medium |  19.91 GiB |    34.66 B | BLAS,MTL   |       8 |           tg128 |         33.62 ± 0.94 |
| qwen35moe 35B.A3B Q4_K - Medium |  19.91 GiB |    34.66 B | BLAS,MTL   |       8 |           tg512 |         33.76 ± 0.71 |

build: ad0922465 (9060)

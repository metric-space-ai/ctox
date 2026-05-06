use crate::inference::engine;

pub const DEFAULT_LOCAL_CHAT_FAMILY: engine::LocalModelFamily =
    engine::LocalModelFamily::Qwen35Vision;
pub const SUPPORTED_LOCAL_CHAT_MODELS: &[&str] = &["Qwen/Qwen3.5-27B", "Qwen/Qwen3.6-35B-A3B"];

pub const SUPPORTED_OPENAI_API_CHAT_MODELS: &[&str] =
    &["gpt-5.5", "gpt-5.4", "gpt-5.4-mini", "gpt-5.4-nano"];
pub const SUPPORTED_ANTHROPIC_API_CHAT_MODELS: &[&str] = &[
    "claude-opus-4-7",
    "claude-opus-4-6",
    "claude-sonnet-4-7",
    "claude-sonnet-4-6",
];
// MiniMax Direct-API (platform.minimax.io). These are the cloud-hosted
// variants; the `minimax/minimax-m2.7` entry lower down is the OpenRouter-
// routed variant with the same weights.
pub const SUPPORTED_MINIMAX_API_CHAT_MODELS: &[&str] = &["MiniMax-M2.7", "MiniMax-M2.7-highspeed"];
pub const SUPPORTED_OPENROUTER_API_CHAT_MODELS: &[&str] = &[
    "openai/gpt-oss-120b",
    "anthropic/claude-opus-4.7",
    "anthropic/claude-opus-4.6",
    "anthropic/claude-sonnet-4.7",
    "z-ai/glm-5.1",
    "qwen/qwen3.5-9b",
    "qwen/qwen3.5-27b",
    "qwen/qwen3.5-35b-a3b",
    "qwen/qwen3.5-122b-a10b",
    "qwen/qwen3.5-397b-a17b",
    "qwen/qwen3.5-plus",
    "google/gemma-4-26b-a4b-it",
    "google/gemma-4-26b-a4b-it:free",
    "google/gemma-4-31b-it",
    "google/gemma-4-31b-it:free",
    "anthropic/claude-sonnet-4.6",
    "moonshotai/kimi-k2.5",
    "moonshotai/kimi-k2.6",
    "deepseek/deepseek-v4-flash",
    "tencent/hy3-preview:free",
    "minimax/minimax-m2.7",
    "mistralai/mistral-small-2603",
    "x-ai/grok-4.20",
    "z-ai/glm-4.7-flash",
];

pub const SUPPORTED_CHAT_MODELS: &[&str] = &[
    "gpt-5.5",
    "gpt-5.4",
    "gpt-5.4-mini",
    "gpt-5.4-nano",
    "claude-opus-4-7",
    "claude-opus-4-6",
    "claude-sonnet-4-7",
    "claude-sonnet-4-6",
    "MiniMax-M2.7",
    "MiniMax-M2.7-highspeed",
    "openai/gpt-oss-120b",
    "Qwen/Qwen3.5-2B",
    "Qwen/Qwen3.5-4B",
    "Qwen/Qwen3.5-9B",
    "Qwen/Qwen3.5-27B",
    "Qwen/Qwen3.5-35B-A3B",
    "Qwen/Qwen3.6-35B-A3B",
    "google/gemma-4-E2B-it",
    "google/gemma-4-E4B-it",
    "google/gemma-4-26B-A4B-it",
    "google/gemma-4-31B-it",
    "nvidia/Nemotron-Cascade-2-30B-A3B",
    "zai-org/GLM-4.7-Flash",
    "z-ai/glm-5.1",
    "qwen/qwen3.5-plus",
    "google/gemma-4-26b-a4b-it:free",
    "google/gemma-4-31b-it:free",
    "x-ai/grok-4.20",
    "minimax/minimax-m2.7",
    "mistralai/mistral-small-2603",
    "qwen/qwen3.5-122b-a10b",
    "anthropic/claude-opus-4.7",
    "anthropic/claude-opus-4.6",
    "anthropic/claude-sonnet-4.7",
    "anthropic/claude-sonnet-4.6",
    "qwen/qwen3.5-397b-a17b",
    "moonshotai/kimi-k2.5",
    "moonshotai/kimi-k2.6",
    "deepseek/deepseek-v4-flash",
    "tencent/hy3-preview:free",
];

pub const SUPPORTED_LOCAL_CHAT_FAMILIES: &[engine::ChatModelFamily] =
    &[engine::ChatModelFamily::Qwen35];

pub const SUPPORTED_EMBEDDING_MODELS: &[&str] = &[
    "Qwen/Qwen3-Embedding-0.6B [GPU]",
    "Qwen/Qwen3-Embedding-0.6B [CPU]",
];

pub const SUPPORTED_STT_MODELS: &[&str] = &["engineai/Voxtral-Mini-4B-Realtime-2602 [GPU]"];

pub const SUPPORTED_TTS_MODELS: &[&str] = &[
    "engineai/Voxtral-4B-TTS-2603 [GPU]",
    "Qwen/Qwen3-TTS-12Hz-0.6B-Base [GPU]",
    "Qwen/Qwen3-TTS-12Hz-0.6B-CustomVoice [GPU]",
    "speaches-ai/piper-de_DE-thorsten-high [CPU DE]",
    "speaches-ai/piper-fr_FR-siwis-medium [CPU FR]",
    "speaches-ai/piper-en_US-lessac-medium [CPU EN]",
];

/// Auxiliary vision models. Used by the vision preprocessor to describe
/// images for primary LLMs that cannot natively accept image input.
/// Qwen3-VL-2B-Instruct is the current default — small enough for a single
/// GPU (~1.5 GB weights Q4K), supported by the ctox-engine Candle vision
/// loader (Qwen3VLForConditionalGeneration).
pub const SUPPORTED_VISION_MODELS: &[&str] = &["Qwen/Qwen3-VL-2B-Instruct [GPU]"];

/// API-hosted models that natively accept image input. Used by
/// `model_supports_vision()` so the vision preprocessor can recognise
/// vision-capable remote providers and skip the aux-describe round trip.
/// Conservative list — only models where vision support is documented by
/// the provider. `gpt-5.4-nano` is deliberately excluded (cheapest variant,
/// vision not guaranteed).
pub const VISION_API_MODELS: &[&str] = &[
    // OpenAI (gpt-4o lineage)
    "gpt-5.5",
    "gpt-5.4",
    "gpt-5.4-mini",
    // Anthropic (all Claude 3+ have vision)
    "claude-opus-4-7",
    "claude-opus-4-6",
    "claude-sonnet-4-7",
    "claude-sonnet-4-6",
    "anthropic/claude-opus-4.7",
    "anthropic/claude-opus-4.6",
    "anthropic/claude-sonnet-4.7",
    "anthropic/claude-sonnet-4.6",
    // MiniMax VL
    "MiniMax-M2.7",
    "MiniMax-M2.7-highspeed",
    "minimax/minimax-m2.7",
    // Gemma 4 via OpenRouter (vision-enabled tiers)
    "google/gemma-4-26b-a4b-it",
    "google/gemma-4-26b-a4b-it:free",
    "google/gemma-4-31b-it",
    "google/gemma-4-31b-it:free",
    // Qwen 3.5-VL family via OpenRouter
    "qwen/qwen3.5-9b",
    "qwen/qwen3.5-27b",
    "qwen/qwen3.5-35b-a3b",
    "qwen/qwen3.5-122b-a10b",
    "qwen/qwen3.5-397b-a17b",
    // Mistral Pixtral
    "mistralai/mistral-small-2603",
    // xAI Grok (vision-enabled)
    "x-ai/grok-4.20",
];

#[derive(Debug, Clone, Copy)]
pub struct ChatFamilyCatalogEntry {
    pub family: engine::ChatModelFamily,
    pub label: &'static str,
    pub selector: &'static str,
    pub parse_aliases: &'static [&'static str],
    pub variants: &'static [&'static str],
    pub planning_variants: &'static [&'static str],
    /// True when the family's primary-generation models can natively accept
    /// image content blocks in the request. When false, the vision
    /// preprocessor describes images via the Vision aux before the primary
    /// model ever sees the request.
    pub supports_vision: bool,
}

#[derive(Debug, Clone, Copy)]
struct StaticRuntimeConfig {
    port: u16,
    max_seq_len: Option<u32>,
    max_seqs: u32,
    max_batch_size: u32,
}

#[derive(Debug, Clone, Copy)]
struct StaticFamilyProfile {
    launcher_mode: &'static str,
    arch: Option<&'static str>,
    paged_attn: &'static str,
    pa_cache_type: Option<&'static str>,
    pa_memory_fraction: Option<&'static str>,
    pa_context_len: Option<u32>,
    max_seq_len: u32,
    max_batch_size: u32,
    max_seqs: u32,
    isq: Option<&'static str>,
    tensor_parallel_backend: Option<&'static str>,
    disable_nccl: bool,
    target_world_size: Option<u32>,
    preferred_gpu_count: Option<u32>,
}

#[derive(Debug, Clone, Copy)]
struct LocalModelCatalogEntry {
    canonical_model: &'static str,
    aliases: &'static [&'static str],
    chat_family: Option<engine::ChatModelFamily>,
    runtime_manifest_slug: Option<&'static str>,
    auxiliary_manifest_slug: Option<&'static str>,
    runtime: StaticRuntimeConfig,
    profile: StaticFamilyProfile,
    family: engine::LocalModelFamily,
}

#[derive(Debug, Clone, Copy)]
struct LocalFamilyCatalogEntry {
    family: engine::LocalModelFamily,
    parse_aliases: &'static [&'static str],
    default_model: &'static str,
    bridge_mode: &'static str,
    default_runtime: StaticRuntimeConfig,
    default_profile: StaticFamilyProfile,
}

#[derive(Debug, Clone, Copy)]
struct AuxiliarySelectionEntry {
    role: engine::AuxiliaryRole,
    choice: &'static str,
    request_model: &'static str,
    aliases: &'static [&'static str],
    backend_kind: engine::AuxiliaryBackendKind,
    compute_target: engine::ComputeTarget,
    default_port: u16,
    default_for_role: bool,
}

#[derive(Debug, Clone, Copy)]
struct ModelOpsMetadataEntry {
    canonical_model: &'static str,
    process_aliases: &'static [&'static str],
    startup_wait_secs: u64,
    default_tokens_per_second: Option<f64>,
    estimated_chat_base_memory_mb: Option<u64>,
    gpu_short_label: Option<&'static str>,
}

const CHAT_FAMILY_REGISTRY: &[ChatFamilyCatalogEntry] = &[
    ChatFamilyCatalogEntry {
        family: engine::ChatModelFamily::GptOss,
        label: "GPT-OSS",
        selector: "gpt_oss",
        parse_aliases: &["gpt_oss", "gpt-oss", "gpt oss"],
        variants: &["openai/gpt-oss-120b"],
        planning_variants: &["openai/gpt-oss-120b"],
        supports_vision: false,
    },
    ChatFamilyCatalogEntry {
        family: engine::ChatModelFamily::Qwen35,
        label: "Qwen 3.5",
        selector: "qwen3_5",
        parse_aliases: &["qwen3_5", "qwen3.5", "qwen 3.5", "qwen"],
        variants: &["Qwen/Qwen3.5-27B"],
        planning_variants: &["Qwen/Qwen3.5-27B"],
        // Qwen3.5 family is vision-capable via the Qwen35Vision local
        // family registered in LOCAL_FAMILY_REGISTRY.
        supports_vision: true,
    },
    ChatFamilyCatalogEntry {
        family: engine::ChatModelFamily::Gemma4,
        label: "Gemma 4",
        selector: "gemma4",
        parse_aliases: &["gemma4", "gemma_4", "gemma-4", "gemma 4", "gemma"],
        variants: &[
            "google/gemma-4-E2B-it",
            "google/gemma-4-E4B-it",
            "google/gemma-4-26B-A4B-it",
            "google/gemma-4-31B-it",
        ],
        planning_variants: &[
            "google/gemma-4-31B-it",
            "google/gemma-4-26B-A4B-it",
            "google/gemma-4-E4B-it",
            "google/gemma-4-E2B-it",
        ],
        // Gemma 4 family is vision-capable via the Gemma4Vision local
        // family registered in LOCAL_FAMILY_REGISTRY.
        supports_vision: true,
    },
    ChatFamilyCatalogEntry {
        family: engine::ChatModelFamily::NemotronCascade2,
        label: "Nemotron Cascade 2",
        selector: "nemotron_cascade_2",
        parse_aliases: &[
            "nemotron",
            "nemotron_cascade",
            "nemotron_cascade_2",
            "nemotron-cascade",
            "nemotron-cascade-2",
            "nemotron cascade",
            "nemotron cascade 2",
        ],
        variants: &["nvidia/Nemotron-Cascade-2-30B-A3B"],
        planning_variants: &["nvidia/Nemotron-Cascade-2-30B-A3B"],
        supports_vision: false,
    },
    ChatFamilyCatalogEntry {
        family: engine::ChatModelFamily::Glm47Flash,
        label: "GLM 4.7 Flash",
        selector: "glm47_flash",
        parse_aliases: &[
            "glm47_flash",
            "glm47",
            "glm-4.7-flash",
            "glm 4.7 flash",
            "glm",
        ],
        variants: &["zai-org/GLM-4.7-Flash"],
        planning_variants: &["zai-org/GLM-4.7-Flash"],
        supports_vision: false,
    },
    ChatFamilyCatalogEntry {
        family: engine::ChatModelFamily::Anthropic,
        label: "Claude",
        selector: "anthropic",
        parse_aliases: &["anthropic", "claude"],
        variants: &[
            "claude-opus-4-7",
            "claude-opus-4-6",
            "claude-sonnet-4-7",
            "claude-sonnet-4-6",
        ],
        planning_variants: &[
            "claude-opus-4-7",
            "claude-opus-4-6",
            "claude-sonnet-4-7",
            "claude-sonnet-4-6",
        ],
        supports_vision: true,
    },
    ChatFamilyCatalogEntry {
        family: engine::ChatModelFamily::Kimi,
        label: "Kimi",
        selector: "kimi",
        parse_aliases: &["kimi", "kimi-k2", "kimi k2", "moonshot"],
        variants: &["moonshotai/kimi-k2.6", "moonshotai/kimi-k2.5"],
        planning_variants: &["moonshotai/kimi-k2.6", "moonshotai/kimi-k2.5"],
        supports_vision: true,
    },
    ChatFamilyCatalogEntry {
        family: engine::ChatModelFamily::DeepSeek,
        label: "DeepSeek",
        selector: "deepseek",
        parse_aliases: &["deepseek", "deepseek-v4", "deepseek v4"],
        variants: &["deepseek/deepseek-v4-flash"],
        planning_variants: &["deepseek/deepseek-v4-flash"],
        supports_vision: false,
    },
    ChatFamilyCatalogEntry {
        family: engine::ChatModelFamily::Hy3,
        label: "HY3",
        selector: "hy3",
        parse_aliases: &["hy3", "tencent", "tencent-hy3", "hy3-preview"],
        variants: &["tencent/hy3-preview:free"],
        planning_variants: &["tencent/hy3-preview:free"],
        supports_vision: false,
    },
];

const LOCAL_FAMILY_REGISTRY: &[LocalFamilyCatalogEntry] = &[
    LocalFamilyCatalogEntry {
        family: engine::LocalModelFamily::Qwen35Vision,
        parse_aliases: &["qwen3_5", "qwen3.5", "qwen3-5", "qwen35", "qwen3_5_vision"],
        default_model: "Qwen/Qwen3.5-27B",
        bridge_mode: "qwen_custom_execution",
        default_runtime: StaticRuntimeConfig {
            port: 1235,
            max_seq_len: Some(131_072),
            max_seqs: 1,
            max_batch_size: 1,
        },
        default_profile: StaticFamilyProfile {
            launcher_mode: "vision",
            arch: None,
            paged_attn: "auto",
            pa_cache_type: Some("turboquant3"),
            pa_memory_fraction: Some("0.80"),
            pa_context_len: None,
            max_seq_len: 131_072,
            max_batch_size: 1,
            max_seqs: 1,
            isq: Some("Q4K"),
            tensor_parallel_backend: None,
            disable_nccl: true,
            target_world_size: None,
            preferred_gpu_count: Some(1),
        },
    },
    LocalFamilyCatalogEntry {
        family: engine::LocalModelFamily::Gemma4Vision,
        parse_aliases: &[
            "gemma4",
            "gemma_4",
            "gemma-4",
            "gemma4_vision",
            "gemma-4-vision",
        ],
        default_model: "google/gemma-4-31B-it",
        bridge_mode: "gemma_custom_execution",
        default_runtime: StaticRuntimeConfig {
            port: 1235,
            max_seq_len: Some(131_072),
            max_seqs: 1,
            max_batch_size: 1,
        },
        default_profile: StaticFamilyProfile {
            launcher_mode: "vision",
            arch: None,
            paged_attn: "auto",
            pa_cache_type: Some("turboquant3"),
            pa_memory_fraction: Some("0.80"),
            pa_context_len: None,
            max_seq_len: 131_072,
            max_batch_size: 1,
            max_seqs: 1,
            isq: Some("Q4K"),
            tensor_parallel_backend: None,
            disable_nccl: true,
            target_world_size: None,
            preferred_gpu_count: Some(3),
        },
    },
    LocalFamilyCatalogEntry {
        family: engine::LocalModelFamily::NemotronCascade2,
        parse_aliases: &[
            "nemotron",
            "nemotron_cascade",
            "nemotron-cascade",
            "nemotron_cascade_2",
            "nemotron-cascade-2",
            "nemotron-cascade-2-30b-a3b",
            "nemotroncascade230ba3b",
        ],
        default_model: "nvidia/Nemotron-Cascade-2-30B-A3B",
        bridge_mode: "chatml_custom_execution",
        default_runtime: StaticRuntimeConfig {
            port: 1236,
            max_seq_len: Some(8_192),
            max_seqs: 1,
            max_batch_size: 1,
        },
        default_profile: StaticFamilyProfile {
            launcher_mode: "text",
            arch: None,
            paged_attn: "auto",
            pa_cache_type: Some("turboquant3"),
            pa_memory_fraction: Some("0.45"),
            pa_context_len: None,
            max_seq_len: 8_192,
            max_batch_size: 1,
            max_seqs: 1,
            isq: Some("Q4K"),
            tensor_parallel_backend: None,
            disable_nccl: true,
            target_world_size: None,
            preferred_gpu_count: Some(2),
        },
    },
    LocalFamilyCatalogEntry {
        family: engine::LocalModelFamily::Glm47Flash,
        parse_aliases: &[
            "glm4moelite",
            "glm4_flash",
            "glm4.7flash",
            "glm-4.7-flash",
            "gln-4.7-flash",
            "gln4.7flash",
        ],
        default_model: "zai-org/GLM-4.7-Flash",
        bridge_mode: "codex_responses_runtime",
        default_runtime: StaticRuntimeConfig {
            port: 1236,
            max_seq_len: Some(65_536),
            max_seqs: 1,
            max_batch_size: 1,
        },
        default_profile: StaticFamilyProfile {
            launcher_mode: "text",
            arch: Some("glm4moelite"),
            paged_attn: "auto",
            pa_cache_type: Some("turboquant3"),
            pa_memory_fraction: Some("0.65"),
            pa_context_len: None,
            max_seq_len: 4_096,
            max_batch_size: 1,
            max_seqs: 1,
            isq: Some("Q4K"),
            tensor_parallel_backend: None,
            disable_nccl: true,
            target_world_size: None,
            preferred_gpu_count: Some(3),
        },
    },
    LocalFamilyCatalogEntry {
        family: engine::LocalModelFamily::Qwen3Embedding,
        parse_aliases: &["qwen3_embedding", "qwen3-embedding", "qwen3embedding"],
        default_model: "Qwen/Qwen3-Embedding-0.6B",
        bridge_mode: "embedding_server",
        default_runtime: StaticRuntimeConfig {
            port: 1237,
            max_seq_len: Some(32_768),
            max_seqs: 8,
            max_batch_size: 8,
        },
        default_profile: StaticFamilyProfile {
            launcher_mode: "embedding",
            arch: None,
            paged_attn: "auto",
            pa_cache_type: Some("f8e4m3"),
            pa_memory_fraction: Some("0.30"),
            pa_context_len: None,
            max_seq_len: 32_768,
            max_batch_size: 8,
            max_seqs: 8,
            isq: Some("Q4K"),
            tensor_parallel_backend: None,
            disable_nccl: true,
            target_world_size: None,
            preferred_gpu_count: Some(1),
        },
    },
    LocalFamilyCatalogEntry {
        family: engine::LocalModelFamily::VoxtralTranscription,
        parse_aliases: &[
            "voxtral",
            "voxtral_realtime",
            "voxtral-transcription",
            "stt",
        ],
        default_model: "engineai/Voxtral-Mini-4B-Realtime-2602",
        bridge_mode: "transcription_server",
        default_runtime: StaticRuntimeConfig {
            port: 1238,
            max_seq_len: Some(32_768),
            max_seqs: 2,
            max_batch_size: 2,
        },
        default_profile: StaticFamilyProfile {
            launcher_mode: "vision",
            arch: None,
            paged_attn: "auto",
            pa_cache_type: Some("f8e4m3"),
            pa_memory_fraction: Some("0.55"),
            pa_context_len: None,
            max_seq_len: 32_768,
            max_batch_size: 2,
            max_seqs: 2,
            isq: Some("Q4K"),
            tensor_parallel_backend: None,
            disable_nccl: true,
            target_world_size: None,
            preferred_gpu_count: Some(1),
        },
    },
    LocalFamilyCatalogEntry {
        family: engine::LocalModelFamily::PiperSpeech,
        parse_aliases: &["piper", "piper_tts", "piper-tts", "piper_speech"],
        default_model: "speaches-ai/piper-en_US-lessac-medium",
        bridge_mode: "speech_server",
        default_runtime: StaticRuntimeConfig {
            port: 1239,
            max_seq_len: None,
            max_seqs: 1,
            max_batch_size: 1,
        },
        default_profile: StaticFamilyProfile {
            launcher_mode: "speech",
            arch: None,
            paged_attn: "off",
            pa_cache_type: None,
            pa_memory_fraction: None,
            pa_context_len: None,
            max_seq_len: 4_096,
            max_batch_size: 1,
            max_seqs: 1,
            isq: None,
            tensor_parallel_backend: None,
            disable_nccl: true,
            target_world_size: None,
            preferred_gpu_count: None,
        },
    },
    LocalFamilyCatalogEntry {
        family: engine::LocalModelFamily::Qwen3Speech,
        parse_aliases: &["qwen3_tts", "qwen3-tts", "qwen3speech", "tts"],
        default_model: "Qwen/Qwen3-TTS-12Hz-0.6B-Base",
        bridge_mode: "speech_server",
        default_runtime: StaticRuntimeConfig {
            port: 1239,
            max_seq_len: None,
            max_seqs: 1,
            max_batch_size: 1,
        },
        default_profile: StaticFamilyProfile {
            launcher_mode: "speech",
            arch: None,
            paged_attn: "off",
            pa_cache_type: None,
            pa_memory_fraction: None,
            pa_context_len: None,
            max_seq_len: 4_096,
            max_batch_size: 1,
            max_seqs: 1,
            isq: Some("Q4K"),
            tensor_parallel_backend: None,
            disable_nccl: true,
            target_world_size: None,
            preferred_gpu_count: Some(1),
        },
    },
    LocalFamilyCatalogEntry {
        family: engine::LocalModelFamily::VoxtralSpeech,
        parse_aliases: &[
            "voxtral_tts",
            "voxtral-tts",
            "voxtral4btts",
            "voxtral_speech",
        ],
        default_model: "engineai/Voxtral-4B-TTS-2603",
        bridge_mode: "speech_server",
        default_runtime: StaticRuntimeConfig {
            port: 1239,
            max_seq_len: Some(8_192),
            max_seqs: 1,
            max_batch_size: 1,
        },
        default_profile: StaticFamilyProfile {
            launcher_mode: "speech",
            arch: Some("voxtral_tts"),
            paged_attn: "off",
            pa_cache_type: None,
            pa_memory_fraction: None,
            pa_context_len: None,
            max_seq_len: 8_192,
            max_batch_size: 1,
            max_seqs: 1,
            isq: Some("Q4K"),
            tensor_parallel_backend: None,
            disable_nccl: true,
            target_world_size: None,
            preferred_gpu_count: Some(1),
        },
    },
];

const LOCAL_MODEL_REGISTRY: &[LocalModelCatalogEntry] = &[
    LocalModelCatalogEntry {
        canonical_model: "Qwen/Qwen3.5-2B",
        aliases: &[],
        chat_family: Some(engine::ChatModelFamily::Qwen35),
        runtime_manifest_slug: Some("qwen3_5_2b"),
        auxiliary_manifest_slug: None,
        runtime: StaticRuntimeConfig {
            port: 1235,
            max_seq_len: Some(262_144),
            max_seqs: 1,
            max_batch_size: 1,
        },
        profile: StaticFamilyProfile {
            launcher_mode: "vision",
            arch: None,
            paged_attn: "auto",
            pa_cache_type: Some("turboquant3"),
            pa_memory_fraction: Some("0.80"),
            pa_context_len: None,
            max_seq_len: 262_144,
            max_batch_size: 1,
            max_seqs: 1,
            isq: Some("Q4K"),
            tensor_parallel_backend: None,
            disable_nccl: true,
            target_world_size: None,
            preferred_gpu_count: Some(1),
        },
        family: engine::LocalModelFamily::Qwen35Vision,
    },
    LocalModelCatalogEntry {
        canonical_model: "Qwen/Qwen3.5-4B",
        aliases: &[],
        chat_family: Some(engine::ChatModelFamily::Qwen35),
        runtime_manifest_slug: Some("qwen3_5_4b"),
        auxiliary_manifest_slug: None,
        runtime: StaticRuntimeConfig {
            port: 1235,
            max_seq_len: Some(262_144),
            max_seqs: 1,
            max_batch_size: 1,
        },
        profile: StaticFamilyProfile {
            launcher_mode: "vision",
            arch: None,
            paged_attn: "auto",
            pa_cache_type: Some("turboquant3"),
            pa_memory_fraction: Some("0.80"),
            pa_context_len: None,
            max_seq_len: 262_144,
            max_batch_size: 1,
            max_seqs: 1,
            isq: Some("Q4K"),
            tensor_parallel_backend: None,
            disable_nccl: true,
            target_world_size: None,
            preferred_gpu_count: Some(1),
        },
        family: engine::LocalModelFamily::Qwen35Vision,
    },
    LocalModelCatalogEntry {
        canonical_model: "Qwen/Qwen3.5-9B",
        aliases: &[],
        chat_family: Some(engine::ChatModelFamily::Qwen35),
        runtime_manifest_slug: Some("qwen3_5_9b"),
        auxiliary_manifest_slug: None,
        runtime: StaticRuntimeConfig {
            port: 1235,
            max_seq_len: Some(262_144),
            max_seqs: 1,
            max_batch_size: 1,
        },
        profile: StaticFamilyProfile {
            launcher_mode: "vision",
            arch: None,
            paged_attn: "auto",
            pa_cache_type: Some("turboquant3"),
            pa_memory_fraction: Some("0.80"),
            pa_context_len: None,
            max_seq_len: 262_144,
            max_batch_size: 1,
            max_seqs: 1,
            isq: Some("Q4K"),
            tensor_parallel_backend: None,
            disable_nccl: true,
            target_world_size: None,
            preferred_gpu_count: Some(1),
        },
        family: engine::LocalModelFamily::Qwen35Vision,
    },
    // Qwen3.5-27B Q4_K_M + DFlash draft — served via the in-tree
    // per-model socket server `qwen35-27b-q4km-dflash-server`
    // (see src/execution/models/local_model.rs and
    // src/inference/models/qwen35_27b_q4km_dflash/). The runtime
    // manifest below is cosmetic for the Candle-era `launcher_mode`
    // path; the supervisor routes the spawn to the per-model server
    // binary before those fields are consumed.
    LocalModelCatalogEntry {
        canonical_model: "Qwen/Qwen3.5-27B",
        aliases: &["Qwen3.5-27B", "qwen35-27b-q4km-dflash"],
        chat_family: Some(engine::ChatModelFamily::Qwen35),
        runtime_manifest_slug: Some("qwen3_5_27b"),
        auxiliary_manifest_slug: None,
        runtime: StaticRuntimeConfig {
            port: 1235,
            max_seq_len: Some(131_072),
            max_seqs: 1,
            max_batch_size: 1,
        },
        profile: StaticFamilyProfile {
            launcher_mode: "chat",
            arch: None,
            paged_attn: "auto",
            pa_cache_type: None,
            pa_memory_fraction: Some("0.85"),
            pa_context_len: None,
            max_seq_len: 131_072,
            max_batch_size: 1,
            max_seqs: 1,
            isq: Some("Q4K"),
            tensor_parallel_backend: None,
            disable_nccl: true,
            target_world_size: None,
            preferred_gpu_count: Some(1),
        },
        family: engine::LocalModelFamily::Qwen35Vision,
    },
    LocalModelCatalogEntry {
        canonical_model: "Qwen/Qwen3.5-35B-A3B",
        aliases: &["Qwen3.5-35B-A3B"],
        chat_family: Some(engine::ChatModelFamily::Qwen35),
        runtime_manifest_slug: Some("qwen3_5_35b_a3b"),
        auxiliary_manifest_slug: None,
        runtime: StaticRuntimeConfig {
            port: 1235,
            max_seq_len: Some(262_144),
            max_seqs: 1,
            max_batch_size: 1,
        },
        profile: StaticFamilyProfile {
            launcher_mode: "vision",
            arch: None,
            paged_attn: "auto",
            pa_cache_type: Some("turboquant3"),
            pa_memory_fraction: Some("0.80"),
            pa_context_len: None,
            max_seq_len: 262_144,
            max_batch_size: 1,
            max_seqs: 1,
            isq: Some("Q4K"),
            tensor_parallel_backend: None,
            disable_nccl: true,
            target_world_size: None,
            preferred_gpu_count: Some(3),
        },
        family: engine::LocalModelFamily::Qwen35Vision,
    },
    LocalModelCatalogEntry {
        canonical_model: "Qwen/Qwen3.6-35B-A3B",
        aliases: &["Qwen3.6-35B-A3B", "qwen36-35b-a3b", "qwen36-35b-a3b-ggml"],
        chat_family: Some(engine::ChatModelFamily::Qwen35),
        runtime_manifest_slug: Some("qwen3_6_35b_a3b"),
        auxiliary_manifest_slug: None,
        runtime: StaticRuntimeConfig {
            port: 1235,
            max_seq_len: Some(262_144),
            max_seqs: 1,
            max_batch_size: 1,
        },
        profile: StaticFamilyProfile {
            launcher_mode: "chat",
            arch: None,
            paged_attn: "auto",
            pa_cache_type: None,
            pa_memory_fraction: Some("0.80"),
            pa_context_len: None,
            max_seq_len: 262_144,
            max_batch_size: 1,
            max_seqs: 1,
            isq: Some("Q4K"),
            tensor_parallel_backend: None,
            disable_nccl: true,
            target_world_size: None,
            preferred_gpu_count: Some(4),
        },
        family: engine::LocalModelFamily::Qwen35Vision,
    },
    LocalModelCatalogEntry {
        canonical_model: "google/gemma-4-E2B-it",
        aliases: &[
            "google/gemma-4-e2b-it",
            "gemma-4-e2b-it",
            "gemma 4 e2b",
            "gemma4 e2b",
        ],
        chat_family: Some(engine::ChatModelFamily::Gemma4),
        runtime_manifest_slug: Some("gemma_4_e2b_it"),
        auxiliary_manifest_slug: None,
        runtime: StaticRuntimeConfig {
            port: 1235,
            max_seq_len: Some(131_072),
            max_seqs: 1,
            max_batch_size: 1,
        },
        profile: StaticFamilyProfile {
            launcher_mode: "vision",
            arch: None,
            paged_attn: "off",
            pa_cache_type: None,
            pa_memory_fraction: None,
            pa_context_len: None,
            max_seq_len: 131_072,
            max_batch_size: 1,
            max_seqs: 1,
            isq: Some("Q4K"),
            tensor_parallel_backend: None,
            disable_nccl: true,
            target_world_size: None,
            preferred_gpu_count: Some(1),
        },
        family: engine::LocalModelFamily::Gemma4Vision,
    },
    LocalModelCatalogEntry {
        canonical_model: "google/gemma-4-E4B-it",
        aliases: &[
            "google/gemma-4-e4b-it",
            "gemma-4-e4b-it",
            "gemma 4 e4b",
            "gemma4 e4b",
        ],
        chat_family: Some(engine::ChatModelFamily::Gemma4),
        runtime_manifest_slug: Some("gemma_4_e4b_it"),
        auxiliary_manifest_slug: None,
        runtime: StaticRuntimeConfig {
            port: 1235,
            max_seq_len: Some(131_072),
            max_seqs: 1,
            max_batch_size: 1,
        },
        profile: StaticFamilyProfile {
            launcher_mode: "vision",
            arch: None,
            paged_attn: "off",
            pa_cache_type: None,
            pa_memory_fraction: None,
            pa_context_len: None,
            max_seq_len: 131_072,
            max_batch_size: 1,
            max_seqs: 1,
            isq: Some("Q4K"),
            tensor_parallel_backend: None,
            disable_nccl: true,
            target_world_size: None,
            preferred_gpu_count: Some(1),
        },
        family: engine::LocalModelFamily::Gemma4Vision,
    },
    LocalModelCatalogEntry {
        canonical_model: "google/gemma-4-26B-A4B-it",
        aliases: &[
            "google/gemma-4-26b-a4b-it",
            "gemma-4-26b-a4b-it",
            "gemma 4 26b a4b it",
            "gemma4 26b a4b it",
        ],
        chat_family: Some(engine::ChatModelFamily::Gemma4),
        runtime_manifest_slug: Some("gemma_4_26b_a4b_it"),
        auxiliary_manifest_slug: None,
        runtime: StaticRuntimeConfig {
            port: 1235,
            max_seq_len: Some(131_072),
            max_seqs: 1,
            max_batch_size: 1,
        },
        profile: StaticFamilyProfile {
            launcher_mode: "vision",
            arch: None,
            paged_attn: "auto",
            pa_cache_type: Some("f8e4m3"),
            pa_memory_fraction: Some("0.80"),
            pa_context_len: None,
            max_seq_len: 131_072,
            max_batch_size: 1,
            max_seqs: 1,
            isq: Some("Q4K"),
            tensor_parallel_backend: None,
            disable_nccl: true,
            target_world_size: None,
            preferred_gpu_count: Some(3),
        },
        family: engine::LocalModelFamily::Gemma4Vision,
    },
    LocalModelCatalogEntry {
        canonical_model: "google/gemma-4-31B-it",
        aliases: &[
            "google/gemma-4-31b-it",
            "gemma-4-31b-it",
            "gemma 4 31b it",
            "gemma4 31b it",
        ],
        chat_family: Some(engine::ChatModelFamily::Gemma4),
        runtime_manifest_slug: Some("gemma_4_31b_it"),
        auxiliary_manifest_slug: None,
        runtime: StaticRuntimeConfig {
            port: 1235,
            max_seq_len: Some(131_072),
            max_seqs: 1,
            max_batch_size: 1,
        },
        profile: StaticFamilyProfile {
            launcher_mode: "vision",
            arch: None,
            paged_attn: "auto",
            pa_cache_type: Some("f8e4m3"),
            pa_memory_fraction: Some("0.80"),
            pa_context_len: None,
            max_seq_len: 131_072,
            max_batch_size: 1,
            max_seqs: 1,
            isq: Some("Q4K"),
            tensor_parallel_backend: None,
            disable_nccl: true,
            target_world_size: None,
            preferred_gpu_count: Some(3),
        },
        family: engine::LocalModelFamily::Gemma4Vision,
    },
    LocalModelCatalogEntry {
        canonical_model: "nvidia/Nemotron-Cascade-2-30B-A3B",
        aliases: &[
            "nvidia/nemotron-cascade-2-30b-a3b",
            "nemotron-cascade-2-30b-a3b",
            "nemotron cascade 2 30b a3b",
            "nemotron cascade 2",
        ],
        chat_family: Some(engine::ChatModelFamily::NemotronCascade2),
        runtime_manifest_slug: Some("nemotron_cascade_2_30b_a3b"),
        auxiliary_manifest_slug: None,
        runtime: StaticRuntimeConfig {
            port: 1236,
            max_seq_len: Some(8_192),
            max_seqs: 1,
            max_batch_size: 1,
        },
        profile: StaticFamilyProfile {
            launcher_mode: "text",
            arch: None,
            paged_attn: "auto",
            pa_cache_type: Some("turboquant3"),
            pa_memory_fraction: Some("0.45"),
            pa_context_len: None,
            max_seq_len: 8_192,
            max_batch_size: 1,
            max_seqs: 1,
            isq: Some("Q4K"),
            tensor_parallel_backend: None,
            disable_nccl: true,
            target_world_size: None,
            preferred_gpu_count: Some(2),
        },
        family: engine::LocalModelFamily::NemotronCascade2,
    },
    LocalModelCatalogEntry {
        canonical_model: "zai-org/GLM-4.7-Flash",
        aliases: &[
            "glm-4.7-flash",
            "glm 4.7 flash",
            "gln-4.7-flash",
            "gln 4.7 flash",
            "zai/glm-4.7b-flash",
            "zai-org/glm-4.7b-flash",
            "zai/glm-4.7-flash",
            "zai-org/glm-4.7-flash",
        ],
        chat_family: Some(engine::ChatModelFamily::Glm47Flash),
        runtime_manifest_slug: Some("glm_4_7_flash"),
        auxiliary_manifest_slug: None,
        runtime: StaticRuntimeConfig {
            port: 1236,
            max_seq_len: Some(2_048),
            max_seqs: 1,
            max_batch_size: 1,
        },
        profile: StaticFamilyProfile {
            launcher_mode: "text",
            arch: Some("glm4moelite"),
            paged_attn: "auto",
            pa_cache_type: Some("turboquant3"),
            pa_memory_fraction: Some("0.45"),
            pa_context_len: None,
            max_seq_len: 2_048,
            max_batch_size: 1,
            max_seqs: 1,
            isq: Some("Q4K"),
            tensor_parallel_backend: None,
            disable_nccl: true,
            target_world_size: None,
            preferred_gpu_count: Some(3),
        },
        family: engine::LocalModelFamily::Glm47Flash,
    },
    LocalModelCatalogEntry {
        canonical_model: "Qwen/Qwen3-Embedding-0.6B",
        aliases: &[
            "qwen/qwen3-embedding-0.6b",
            "qwen3-embedding-0.6b",
            "qwen3 embedding 0.6b",
        ],
        chat_family: None,
        runtime_manifest_slug: None,
        auxiliary_manifest_slug: Some("qwen3_embedding_0_6b"),
        runtime: StaticRuntimeConfig {
            port: 1237,
            max_seq_len: Some(32_768),
            max_seqs: 8,
            max_batch_size: 8,
        },
        profile: StaticFamilyProfile {
            launcher_mode: "embedding",
            arch: None,
            paged_attn: "auto",
            pa_cache_type: Some("f8e4m3"),
            pa_memory_fraction: Some("0.30"),
            pa_context_len: None,
            max_seq_len: 32_768,
            max_batch_size: 8,
            max_seqs: 8,
            isq: None,
            tensor_parallel_backend: None,
            disable_nccl: true,
            target_world_size: None,
            preferred_gpu_count: Some(1),
        },
        family: engine::LocalModelFamily::Qwen3Embedding,
    },
    LocalModelCatalogEntry {
        canonical_model: "engineai/Voxtral-Mini-4B-Realtime-2602",
        aliases: &[
            "engineai/voxtral-mini-4b-realtime-2602",
            "voxtral-mini-4b-realtime-2602",
        ],
        chat_family: None,
        runtime_manifest_slug: None,
        auxiliary_manifest_slug: Some("voxtral_mini_4b_realtime_2602"),
        runtime: StaticRuntimeConfig {
            port: 1238,
            max_seq_len: Some(32_768),
            max_seqs: 2,
            max_batch_size: 2,
        },
        profile: StaticFamilyProfile {
            launcher_mode: "vision",
            arch: None,
            paged_attn: "auto",
            pa_cache_type: Some("f8e4m3"),
            pa_memory_fraction: Some("0.55"),
            pa_context_len: None,
            max_seq_len: 32_768,
            max_batch_size: 2,
            max_seqs: 2,
            isq: Some("Q4K"),
            tensor_parallel_backend: None,
            disable_nccl: true,
            target_world_size: None,
            preferred_gpu_count: Some(1),
        },
        family: engine::LocalModelFamily::VoxtralTranscription,
    },
    LocalModelCatalogEntry {
        canonical_model: "Qwen/Qwen3-VL-2B-Instruct",
        aliases: &[
            "qwen/qwen3-vl-2b-instruct [gpu]",
            "qwen/qwen3-vl-2b-instruct (gpu)",
            "qwen/qwen3-vl-2b-instruct",
            "qwen3-vl-2b-instruct",
            "qwen3-vl-2b",
            "qwen3vl-2b",
        ],
        chat_family: None,
        runtime_manifest_slug: None,
        auxiliary_manifest_slug: Some("qwen3_vl_2b_instruct"),
        runtime: StaticRuntimeConfig {
            port: 1240,
            max_seq_len: Some(32_768),
            max_seqs: 1,
            max_batch_size: 1,
        },
        profile: StaticFamilyProfile {
            // launcher_mode "vision" maps to the ctox-engine vision loader
            // kind — the same input-modality bucket the engine uses for
            // any non-text input pipeline (Qwen3VLForConditionalGeneration
            // here; the engine auto-detects the arch from config.json).
            launcher_mode: "vision",
            arch: None,
            paged_attn: "auto",
            pa_cache_type: Some("f8e4m3"),
            pa_memory_fraction: Some("0.55"),
            pa_context_len: None,
            max_seq_len: 32_768,
            max_batch_size: 1,
            max_seqs: 1,
            isq: Some("Q4K"),
            tensor_parallel_backend: None,
            disable_nccl: true,
            target_world_size: None,
            preferred_gpu_count: Some(1),
        },
        family: engine::LocalModelFamily::Qwen3VisionAuxiliary,
    },
    LocalModelCatalogEntry {
        canonical_model: "speaches-ai/piper-de_DE-thorsten-high",
        aliases: &[
            "speaches-ai/piper-de_de-thorsten-high [cpu de]",
            "speaches-ai/piper-de_de-thorsten-high (cpu de)",
            "speaches-ai/piper-de_de-thorsten-high",
        ],
        chat_family: None,
        runtime_manifest_slug: None,
        auxiliary_manifest_slug: None,
        runtime: StaticRuntimeConfig {
            port: 1239,
            max_seq_len: None,
            max_seqs: 1,
            max_batch_size: 1,
        },
        profile: StaticFamilyProfile {
            launcher_mode: "speech",
            arch: None,
            paged_attn: "off",
            pa_cache_type: None,
            pa_memory_fraction: None,
            pa_context_len: None,
            max_seq_len: 4_096,
            max_batch_size: 1,
            max_seqs: 1,
            isq: None,
            tensor_parallel_backend: None,
            disable_nccl: true,
            target_world_size: None,
            preferred_gpu_count: None,
        },
        family: engine::LocalModelFamily::PiperSpeech,
    },
    LocalModelCatalogEntry {
        canonical_model: "speaches-ai/piper-fr_FR-siwis-medium",
        aliases: &[
            "speaches-ai/piper-fr_fr-siwis-medium [cpu fr]",
            "speaches-ai/piper-fr_fr-siwis-medium (cpu fr)",
            "speaches-ai/piper-fr_fr-siwis-medium",
        ],
        chat_family: None,
        runtime_manifest_slug: None,
        auxiliary_manifest_slug: None,
        runtime: StaticRuntimeConfig {
            port: 1239,
            max_seq_len: None,
            max_seqs: 1,
            max_batch_size: 1,
        },
        profile: StaticFamilyProfile {
            launcher_mode: "speech",
            arch: None,
            paged_attn: "off",
            pa_cache_type: None,
            pa_memory_fraction: None,
            pa_context_len: None,
            max_seq_len: 4_096,
            max_batch_size: 1,
            max_seqs: 1,
            isq: None,
            tensor_parallel_backend: None,
            disable_nccl: true,
            target_world_size: None,
            preferred_gpu_count: None,
        },
        family: engine::LocalModelFamily::PiperSpeech,
    },
    LocalModelCatalogEntry {
        canonical_model: "speaches-ai/piper-en_US-lessac-medium",
        aliases: &[
            "speaches-ai/piper-en_us-lessac-medium [cpu en]",
            "speaches-ai/piper-en_us-lessac-medium (cpu en)",
            "speaches-ai/piper-en_us-lessac-medium",
        ],
        chat_family: None,
        runtime_manifest_slug: None,
        auxiliary_manifest_slug: None,
        runtime: StaticRuntimeConfig {
            port: 1239,
            max_seq_len: None,
            max_seqs: 1,
            max_batch_size: 1,
        },
        profile: StaticFamilyProfile {
            launcher_mode: "speech",
            arch: None,
            paged_attn: "off",
            pa_cache_type: None,
            pa_memory_fraction: None,
            pa_context_len: None,
            max_seq_len: 4_096,
            max_batch_size: 1,
            max_seqs: 1,
            isq: None,
            tensor_parallel_backend: None,
            disable_nccl: true,
            target_world_size: None,
            preferred_gpu_count: None,
        },
        family: engine::LocalModelFamily::PiperSpeech,
    },
    LocalModelCatalogEntry {
        canonical_model: "Qwen/Qwen3-TTS-12Hz-0.6B-Base",
        aliases: &[
            "qwen/qwen3-tts-12hz-0.6b-base",
            "qwen3-tts-12hz-0.6b-base",
            "qwen3 tts 0.6b base",
            "qwen3-tts 0.6b base",
        ],
        chat_family: None,
        runtime_manifest_slug: None,
        auxiliary_manifest_slug: Some("qwen3_tts_12hz_0_6b_base"),
        runtime: StaticRuntimeConfig {
            port: 1239,
            max_seq_len: None,
            max_seqs: 1,
            max_batch_size: 1,
        },
        profile: StaticFamilyProfile {
            launcher_mode: "speech",
            arch: None,
            paged_attn: "off",
            pa_cache_type: None,
            pa_memory_fraction: None,
            pa_context_len: None,
            max_seq_len: 4_096,
            max_batch_size: 1,
            max_seqs: 1,
            isq: Some("Q4K"),
            tensor_parallel_backend: None,
            disable_nccl: true,
            target_world_size: None,
            preferred_gpu_count: Some(1),
        },
        family: engine::LocalModelFamily::Qwen3Speech,
    },
    LocalModelCatalogEntry {
        canonical_model: "Qwen/Qwen3-TTS-12Hz-0.6B-CustomVoice",
        aliases: &[
            "qwen/qwen3-tts-12hz-0.6b-customvoice",
            "qwen3-tts-12hz-0.6b-customvoice",
            "qwen3 tts 0.6b customvoice",
            "qwen3-tts 0.6b customvoice",
        ],
        chat_family: None,
        runtime_manifest_slug: None,
        auxiliary_manifest_slug: Some("qwen3_tts_12hz_0_6b_customvoice"),
        runtime: StaticRuntimeConfig {
            port: 1239,
            max_seq_len: None,
            max_seqs: 1,
            max_batch_size: 1,
        },
        profile: StaticFamilyProfile {
            launcher_mode: "speech",
            arch: None,
            paged_attn: "off",
            pa_cache_type: None,
            pa_memory_fraction: None,
            pa_context_len: None,
            max_seq_len: 4_096,
            max_batch_size: 1,
            max_seqs: 1,
            isq: Some("Q4K"),
            tensor_parallel_backend: None,
            disable_nccl: true,
            target_world_size: None,
            preferred_gpu_count: Some(1),
        },
        family: engine::LocalModelFamily::Qwen3Speech,
    },
    LocalModelCatalogEntry {
        canonical_model: "engineai/Voxtral-4B-TTS-2603",
        aliases: &[
            "engineai/voxtral-4b-tts-2603",
            "voxtral-4b-tts-2603",
            "voxtral 4b tts 2603",
        ],
        chat_family: None,
        runtime_manifest_slug: None,
        auxiliary_manifest_slug: Some("voxtral_4b_tts_2603"),
        runtime: StaticRuntimeConfig {
            port: 1239,
            max_seq_len: Some(8_192),
            max_seqs: 1,
            max_batch_size: 1,
        },
        profile: StaticFamilyProfile {
            launcher_mode: "speech",
            arch: Some("voxtral_tts"),
            paged_attn: "off",
            pa_cache_type: None,
            pa_memory_fraction: None,
            pa_context_len: None,
            max_seq_len: 8_192,
            max_batch_size: 1,
            max_seqs: 1,
            isq: Some("Q4K"),
            tensor_parallel_backend: None,
            disable_nccl: true,
            target_world_size: None,
            preferred_gpu_count: Some(1),
        },
        family: engine::LocalModelFamily::VoxtralSpeech,
    },
];

/// Chat family mappings for models that are only available via remote API
/// (e.g. OpenRouter) and have no local Candle runtime. The adapter selected
/// here handles Responses→ChatCompletions translation identically to the
/// local path — the same `rewrite_request` / `rewrite_success_response`
/// functions run regardless of whether the upstream is local or remote.
struct RemoteChatFamilyEntry {
    model: &'static str,
    chat_family: engine::ChatModelFamily,
}

const REMOTE_CHAT_FAMILY_REGISTRY: &[RemoteChatFamilyEntry] = &[
    // GPT-OSS (Harmony format)
    RemoteChatFamilyEntry {
        model: "openai/gpt-oss-120b",
        chat_family: engine::ChatModelFamily::GptOss,
    },
    // Qwen family
    RemoteChatFamilyEntry {
        model: "qwen/qwen3.5-122b-a10b",
        chat_family: engine::ChatModelFamily::Qwen35,
    },
    RemoteChatFamilyEntry {
        model: "qwen/qwen3.5-397b-a17b",
        chat_family: engine::ChatModelFamily::Qwen35,
    },
    RemoteChatFamilyEntry {
        model: "qwen/qwen3.5-plus",
        chat_family: engine::ChatModelFamily::Qwen35,
    },
    // GLM family
    RemoteChatFamilyEntry {
        model: "z-ai/glm-5.1",
        chat_family: engine::ChatModelFamily::Glm47Flash,
    },
    // MiniMax family
    RemoteChatFamilyEntry {
        // OpenRouter-routed alias (minimax/* prefix is OpenRouter's
        // namespace). Goes via openrouter.ai/api/v1.
        model: "minimax/minimax-m2.7",
        chat_family: engine::ChatModelFamily::MiniMax,
    },
    RemoteChatFamilyEntry {
        // Direct API alias (capitalised name as published on
        // platform.minimax.io). Goes via api.minimax.io/v1 with
        // MINIMAX_API_KEY. Both aliases share the same MiniMax adapter
        // which translates the agent runtime's /v1/responses requests into
        // MiniMax's /v1/chat/completions surface.
        model: "MiniMax-M2.7",
        chat_family: engine::ChatModelFamily::MiniMax,
    },
    RemoteChatFamilyEntry {
        model: "MiniMax-M2.7-highspeed",
        chat_family: engine::ChatModelFamily::MiniMax,
    },
    // Mistral family
    RemoteChatFamilyEntry {
        model: "mistralai/mistral-small-2603",
        chat_family: engine::ChatModelFamily::Mistral,
    },
    // Kimi family
    RemoteChatFamilyEntry {
        model: "moonshotai/kimi-k2.5",
        chat_family: engine::ChatModelFamily::Kimi,
    },
    RemoteChatFamilyEntry {
        model: "moonshotai/kimi-k2.6",
        chat_family: engine::ChatModelFamily::Kimi,
    },
    // DeepSeek family
    RemoteChatFamilyEntry {
        model: "deepseek/deepseek-v4-flash",
        chat_family: engine::ChatModelFamily::DeepSeek,
    },
    // Tencent HY3 family
    RemoteChatFamilyEntry {
        model: "tencent/hy3-preview:free",
        chat_family: engine::ChatModelFamily::Hy3,
    },
    // Gemma family (free tier aliases)
    RemoteChatFamilyEntry {
        model: "google/gemma-4-26b-a4b-it:free",
        chat_family: engine::ChatModelFamily::Gemma4,
    },
    RemoteChatFamilyEntry {
        model: "google/gemma-4-31b-it:free",
        chat_family: engine::ChatModelFamily::Gemma4,
    },
    // Anthropic direct API and OpenRouter-routed Claude aliases.
    RemoteChatFamilyEntry {
        model: "claude-opus-4-7",
        chat_family: engine::ChatModelFamily::Anthropic,
    },
    RemoteChatFamilyEntry {
        model: "claude-opus-4-6",
        chat_family: engine::ChatModelFamily::Anthropic,
    },
    RemoteChatFamilyEntry {
        model: "claude-sonnet-4-7",
        chat_family: engine::ChatModelFamily::Anthropic,
    },
    RemoteChatFamilyEntry {
        model: "claude-sonnet-4-6",
        chat_family: engine::ChatModelFamily::Anthropic,
    },
    RemoteChatFamilyEntry {
        model: "anthropic/claude-opus-4.7",
        chat_family: engine::ChatModelFamily::Qwen35,
    },
    RemoteChatFamilyEntry {
        model: "anthropic/claude-opus-4.6",
        chat_family: engine::ChatModelFamily::Qwen35,
    },
    RemoteChatFamilyEntry {
        model: "anthropic/claude-sonnet-4.7",
        chat_family: engine::ChatModelFamily::Qwen35,
    },
    RemoteChatFamilyEntry {
        model: "anthropic/claude-sonnet-4.6",
        chat_family: engine::ChatModelFamily::Qwen35,
    },
    // Grok
    RemoteChatFamilyEntry {
        model: "x-ai/grok-4.20",
        chat_family: engine::ChatModelFamily::Qwen35,
    },
];

const AUXILIARY_SELECTION_REGISTRY: &[AuxiliarySelectionEntry] = &[
    AuxiliarySelectionEntry {
        role: engine::AuxiliaryRole::Embedding,
        choice: "Qwen/Qwen3-Embedding-0.6B [GPU]",
        request_model: "Qwen/Qwen3-Embedding-0.6B",
        aliases: &[],
        backend_kind: engine::AuxiliaryBackendKind::NativeCtox,
        compute_target: engine::ComputeTarget::Gpu,
        default_port: 1237,
        default_for_role: true,
    },
    AuxiliarySelectionEntry {
        role: engine::AuxiliaryRole::Embedding,
        choice: "Qwen/Qwen3-Embedding-0.6B [CPU]",
        request_model: "Qwen/Qwen3-Embedding-0.6B",
        aliases: &[
            "qwen/qwen3-embedding-0.6b [cpu]",
            "qwen/qwen3-embedding-0.6b (cpu)",
            "qwen3-embedding-0.6b [cpu]",
            "qwen3-embedding-0.6b (cpu)",
        ],
        backend_kind: engine::AuxiliaryBackendKind::NativeCtox,
        compute_target: engine::ComputeTarget::Cpu,
        default_port: 1237,
        default_for_role: false,
    },
    AuxiliarySelectionEntry {
        role: engine::AuxiliaryRole::Stt,
        choice: "engineai/Voxtral-Mini-4B-Realtime-2602 [GPU]",
        request_model: "engineai/Voxtral-Mini-4B-Realtime-2602",
        aliases: &[],
        backend_kind: engine::AuxiliaryBackendKind::NativeCtox,
        compute_target: engine::ComputeTarget::Gpu,
        default_port: 1238,
        default_for_role: true,
    },
    AuxiliarySelectionEntry {
        role: engine::AuxiliaryRole::Tts,
        choice: "engineai/Voxtral-4B-TTS-2603 [GPU]",
        request_model: "engineai/Voxtral-4B-TTS-2603",
        aliases: &[
            "engineai/voxtral-4b-tts-2603 [gpu]",
            "engineai/voxtral-4b-tts-2603 (gpu)",
            "engineai/voxtral-4b-tts-2603",
            "voxtral-4b-tts-2603",
            "voxtral 4b tts 2603",
        ],
        backend_kind: engine::AuxiliaryBackendKind::NativeCtox,
        compute_target: engine::ComputeTarget::Gpu,
        default_port: 1239,
        default_for_role: true,
    },
    AuxiliarySelectionEntry {
        role: engine::AuxiliaryRole::Tts,
        choice: "Qwen/Qwen3-TTS-12Hz-0.6B-Base [GPU]",
        request_model: "Qwen/Qwen3-TTS-12Hz-0.6B-Base",
        aliases: &[
            "qwen/qwen3-tts-12hz-0.6b-base [gpu]",
            "qwen/qwen3-tts-12hz-0.6b-base (gpu)",
            "qwen/qwen3-tts-12hz-0.6b-base",
        ],
        backend_kind: engine::AuxiliaryBackendKind::MistralRs,
        compute_target: engine::ComputeTarget::Gpu,
        default_port: 1239,
        default_for_role: false,
    },
    AuxiliarySelectionEntry {
        role: engine::AuxiliaryRole::Tts,
        choice: "Qwen/Qwen3-TTS-12Hz-0.6B-CustomVoice [GPU]",
        request_model: "Qwen/Qwen3-TTS-12Hz-0.6B-CustomVoice",
        aliases: &[
            "qwen/qwen3-tts-12hz-0.6b-customvoice [gpu]",
            "qwen/qwen3-tts-12hz-0.6b-customvoice (gpu)",
            "qwen/qwen3-tts-12hz-0.6b-customvoice",
        ],
        backend_kind: engine::AuxiliaryBackendKind::MistralRs,
        compute_target: engine::ComputeTarget::Gpu,
        default_port: 1239,
        default_for_role: false,
    },
    AuxiliarySelectionEntry {
        role: engine::AuxiliaryRole::Tts,
        choice: "speaches-ai/piper-de_DE-thorsten-high [CPU DE]",
        request_model: "speaches-ai/piper-de_DE-thorsten-high",
        aliases: &[
            "speaches-ai/piper-de_de-thorsten-high [cpu de]",
            "speaches-ai/piper-de_de-thorsten-high (cpu de)",
            "speaches-ai/piper-de_de-thorsten-high",
        ],
        backend_kind: engine::AuxiliaryBackendKind::MistralRs,
        compute_target: engine::ComputeTarget::Cpu,
        default_port: 1239,
        default_for_role: false,
    },
    AuxiliarySelectionEntry {
        role: engine::AuxiliaryRole::Tts,
        choice: "speaches-ai/piper-fr_FR-siwis-medium [CPU FR]",
        request_model: "speaches-ai/piper-fr_FR-siwis-medium",
        aliases: &[
            "speaches-ai/piper-fr_fr-siwis-medium [cpu fr]",
            "speaches-ai/piper-fr_fr-siwis-medium (cpu fr)",
            "speaches-ai/piper-fr_fr-siwis-medium",
        ],
        backend_kind: engine::AuxiliaryBackendKind::MistralRs,
        compute_target: engine::ComputeTarget::Cpu,
        default_port: 1239,
        default_for_role: false,
    },
    AuxiliarySelectionEntry {
        role: engine::AuxiliaryRole::Tts,
        choice: "speaches-ai/piper-en_US-lessac-medium [CPU EN]",
        request_model: "speaches-ai/piper-en_US-lessac-medium",
        aliases: &[
            "speaches-ai/piper-en_us-lessac-medium [cpu en]",
            "speaches-ai/piper-en_us-lessac-medium (cpu en)",
            "speaches-ai/piper-en_us-lessac-medium",
        ],
        backend_kind: engine::AuxiliaryBackendKind::MistralRs,
        compute_target: engine::ComputeTarget::Cpu,
        default_port: 1239,
        default_for_role: false,
    },
    // Vision auxiliary — default describer model for tools and messages
    // that carry image content when the primary LLM cannot natively accept
    // images. Served via the ctox-engine vision loader.
    AuxiliarySelectionEntry {
        role: engine::AuxiliaryRole::Vision,
        choice: "Qwen/Qwen3-VL-2B-Instruct [GPU]",
        request_model: "Qwen/Qwen3-VL-2B-Instruct",
        aliases: &[
            "qwen/qwen3-vl-2b-instruct [gpu]",
            "qwen/qwen3-vl-2b-instruct (gpu)",
            "qwen/qwen3-vl-2b-instruct",
            "qwen3-vl-2b-instruct",
            "qwen3-vl-2b",
        ],
        backend_kind: engine::AuxiliaryBackendKind::MistralRs,
        compute_target: engine::ComputeTarget::Gpu,
        default_port: 1240,
        default_for_role: true,
    },
];

const MODEL_OPS_METADATA_REGISTRY: &[ModelOpsMetadataEntry] = &[
    ModelOpsMetadataEntry {
        canonical_model: "openai/gpt-oss-120b",
        process_aliases: &["openai/gpt-oss-120b", "gpt-oss-120b"],
        startup_wait_secs: 240,
        default_tokens_per_second: Some(90.0),
        estimated_chat_base_memory_mb: Some(18_500),
        gpu_short_label: None,
    },
    ModelOpsMetadataEntry {
        canonical_model: "Qwen/Qwen3.5-2B",
        process_aliases: &["Qwen/Qwen3.5-2B", "Qwen3.5-2B"],
        startup_wait_secs: 180,
        default_tokens_per_second: Some(185.0),
        estimated_chat_base_memory_mb: Some(2_500),
        gpu_short_label: None,
    },
    ModelOpsMetadataEntry {
        canonical_model: "Qwen/Qwen3.5-4B",
        process_aliases: &["Qwen/Qwen3.5-4B", "Qwen3.5-4B"],
        startup_wait_secs: 240,
        default_tokens_per_second: Some(140.0),
        estimated_chat_base_memory_mb: Some(4_000),
        gpu_short_label: None,
    },
    ModelOpsMetadataEntry {
        canonical_model: "Qwen/Qwen3.5-9B",
        process_aliases: &["Qwen/Qwen3.5-9B", "Qwen3.5-9B"],
        startup_wait_secs: 240,
        default_tokens_per_second: Some(95.0),
        estimated_chat_base_memory_mb: Some(7_000),
        gpu_short_label: None,
    },
    ModelOpsMetadataEntry {
        // Qwen3.5-27B Q4_K_M + DFlash. Bench harness reaches 156.69
        // decode tok/s on repetitive prompt128/gen128. Natural chat
        // through the Responses adapter is acceptance-bound; measured
        // 29-54 decode tok/s on A6000 depending on prompt.
        canonical_model: "Qwen/Qwen3.5-27B",
        process_aliases: &["Qwen/Qwen3.5-27B", "Qwen3.5-27B", "qwen35-27b-q4km-dflash"],
        startup_wait_secs: 240,
        default_tokens_per_second: Some(54.0),
        // Q4_K_M weights ~16 GB + draft ~3.5 GB + KV cache + activations.
        estimated_chat_base_memory_mb: Some(22_000),
        gpu_short_label: None,
    },
    ModelOpsMetadataEntry {
        canonical_model: "Qwen/Qwen3.5-35B-A3B",
        process_aliases: &["Qwen/Qwen3.5-35B-A3B", "Qwen3.5-35B-A3B"],
        startup_wait_secs: 1_500,
        default_tokens_per_second: Some(38.0),
        estimated_chat_base_memory_mb: Some(20_500),
        gpu_short_label: None,
    },
    ModelOpsMetadataEntry {
        canonical_model: "Qwen/Qwen3.6-35B-A3B",
        process_aliases: &[
            "Qwen/Qwen3.6-35B-A3B",
            "Qwen3.6-35B-A3B",
            "qwen36-35b-a3b-ggml",
        ],
        startup_wait_secs: 1_500,
        default_tokens_per_second: Some(121.0),
        estimated_chat_base_memory_mb: Some(21_000),
        gpu_short_label: None,
    },
    ModelOpsMetadataEntry {
        canonical_model: "google/gemma-4-E2B-it",
        process_aliases: &["google/gemma-4-E2B-it", "gemma-4-E2B-it"],
        startup_wait_secs: 600,
        default_tokens_per_second: Some(155.0),
        estimated_chat_base_memory_mb: Some(2_800),
        gpu_short_label: None,
    },
    ModelOpsMetadataEntry {
        canonical_model: "google/gemma-4-E4B-it",
        process_aliases: &["google/gemma-4-E4B-it", "gemma-4-E4B-it"],
        startup_wait_secs: 900,
        default_tokens_per_second: Some(120.0),
        estimated_chat_base_memory_mb: Some(4_200),
        gpu_short_label: None,
    },
    ModelOpsMetadataEntry {
        canonical_model: "google/gemma-4-26B-A4B-it",
        process_aliases: &["google/gemma-4-26B-A4B-it"],
        startup_wait_secs: 1_500,
        default_tokens_per_second: None,
        estimated_chat_base_memory_mb: None,
        gpu_short_label: None,
    },
    ModelOpsMetadataEntry {
        canonical_model: "google/gemma-4-31B-it",
        process_aliases: &["google/gemma-4-31B-it"],
        startup_wait_secs: 1_800,
        default_tokens_per_second: None,
        estimated_chat_base_memory_mb: None,
        gpu_short_label: None,
    },
    ModelOpsMetadataEntry {
        canonical_model: "nvidia/Nemotron-Cascade-2-30B-A3B",
        process_aliases: &["nvidia/Nemotron-Cascade-2-30B-A3B", "Nemotron-Cascade"],
        startup_wait_secs: 1_800,
        default_tokens_per_second: Some(42.0),
        estimated_chat_base_memory_mb: Some(19_500),
        gpu_short_label: None,
    },
    ModelOpsMetadataEntry {
        canonical_model: "zai-org/GLM-4.7-Flash",
        process_aliases: &["zai-org/GLM-4.7-Flash", "GLM-4.7-Flash"],
        startup_wait_secs: 2_400,
        default_tokens_per_second: Some(48.0),
        estimated_chat_base_memory_mb: Some(21_000),
        gpu_short_label: None,
    },
    ModelOpsMetadataEntry {
        canonical_model: "Qwen/Qwen3-Embedding-0.6B",
        process_aliases: &[
            "Qwen/Qwen3-Embedding-0.6B",
            "Qwen3-Embedding-0.6B",
            "Embedding-0.6B",
        ],
        startup_wait_secs: 120,
        default_tokens_per_second: None,
        estimated_chat_base_memory_mb: None,
        gpu_short_label: Some("embed"),
    },
    ModelOpsMetadataEntry {
        canonical_model: "engineai/Voxtral-Mini-4B-Realtime-2602",
        process_aliases: &[
            "engineai/Voxtral-Mini-4B-Realtime-2602",
            "Voxtral-Mini-4B-Realtime-2602",
            "Voxtral-Mini-4B-Realtime",
            "Voxtral-Mini",
        ],
        startup_wait_secs: 120,
        default_tokens_per_second: None,
        estimated_chat_base_memory_mb: None,
        gpu_short_label: Some("stt"),
    },
    ModelOpsMetadataEntry {
        canonical_model: "speaches-ai/piper-en_US-lessac-medium",
        process_aliases: &[
            "speaches-ai/piper-en_US-lessac-medium",
            "piper-en_US-lessac-medium",
            "piper-en-us-lessac-medium",
        ],
        startup_wait_secs: 120,
        default_tokens_per_second: None,
        estimated_chat_base_memory_mb: None,
        gpu_short_label: Some("tts"),
    },
    ModelOpsMetadataEntry {
        canonical_model: "speaches-ai/piper-fr_FR-siwis-medium",
        process_aliases: &[
            "speaches-ai/piper-fr_FR-siwis-medium",
            "piper-fr_FR-siwis-medium",
            "piper-fr-fr-siwis-medium",
        ],
        startup_wait_secs: 120,
        default_tokens_per_second: None,
        estimated_chat_base_memory_mb: None,
        gpu_short_label: Some("tts"),
    },
    ModelOpsMetadataEntry {
        canonical_model: "speaches-ai/piper-de_DE-thorsten-high",
        process_aliases: &[
            "speaches-ai/piper-de_DE-thorsten-high",
            "piper-de_DE-thorsten-high",
            "piper-de-de-thorsten-high",
        ],
        startup_wait_secs: 120,
        default_tokens_per_second: None,
        estimated_chat_base_memory_mb: None,
        gpu_short_label: Some("tts"),
    },
    ModelOpsMetadataEntry {
        canonical_model: "engineai/Voxtral-4B-TTS-2603",
        process_aliases: &[
            "engineai/Voxtral-4B-TTS-2603",
            "Voxtral-4B-TTS-2603",
            "Voxtral-4B-TTS",
            "Voxtral-TTS",
        ],
        startup_wait_secs: 120,
        default_tokens_per_second: None,
        estimated_chat_base_memory_mb: None,
        gpu_short_label: Some("tts"),
    },
    ModelOpsMetadataEntry {
        canonical_model: "Qwen/Qwen3-TTS-12Hz-0.6B-Base",
        process_aliases: &[
            "Qwen/Qwen3-TTS-12Hz-0.6B-Base",
            "Qwen3-TTS-12Hz-0.6B-Base",
            "Qwen3-TTS",
        ],
        startup_wait_secs: 120,
        default_tokens_per_second: None,
        estimated_chat_base_memory_mb: None,
        gpu_short_label: Some("tts"),
    },
    ModelOpsMetadataEntry {
        canonical_model: "Qwen/Qwen3-TTS-12Hz-0.6B-CustomVoice",
        process_aliases: &["Qwen/Qwen3-TTS-12Hz-0.6B-CustomVoice", "CustomVoice"],
        startup_wait_secs: 120,
        default_tokens_per_second: None,
        estimated_chat_base_memory_mb: None,
        gpu_short_label: Some("tts"),
    },
];

fn matches_candidate(query: &str, candidate: &str) -> bool {
    query.eq_ignore_ascii_case(candidate)
}

fn local_model_entry(model: &str) -> Option<&'static LocalModelCatalogEntry> {
    let trimmed = model.trim();
    LOCAL_MODEL_REGISTRY.iter().find(|entry| {
        matches_candidate(trimmed, entry.canonical_model)
            || entry
                .aliases
                .iter()
                .any(|candidate| matches_candidate(trimmed, candidate))
    })
}

fn local_chat_model_is_supported(entry: &LocalModelCatalogEntry) -> bool {
    entry.chat_family.is_none()
        || SUPPORTED_LOCAL_CHAT_MODELS
            .iter()
            .any(|model| matches_candidate(entry.canonical_model, model))
}

pub fn is_local_chat_model(model: &str) -> bool {
    local_model_entry(model).is_some_and(|entry| entry.chat_family.is_some())
}

pub fn is_supported_local_chat_model(model: &str) -> bool {
    local_model_entry(model)
        .filter(|entry| entry.chat_family.is_some())
        .is_some_and(local_chat_model_is_supported)
}

fn model_ops_metadata_entry(model: &str) -> Option<&'static ModelOpsMetadataEntry> {
    let canonical = canonical_model_id(model).unwrap_or_else(|| model.trim());
    MODEL_OPS_METADATA_REGISTRY
        .iter()
        .find(|entry| matches_candidate(canonical, entry.canonical_model))
}

fn local_family_entry(
    family: engine::LocalModelFamily,
) -> Option<&'static LocalFamilyCatalogEntry> {
    LOCAL_FAMILY_REGISTRY
        .iter()
        .find(|entry| entry.family == family)
}

fn materialize_runtime_config(
    family: engine::LocalModelFamily,
    model: &str,
    runtime: StaticRuntimeConfig,
) -> engine::EngineRuntimeConfig {
    engine::EngineRuntimeConfig {
        family,
        model: model.to_string(),
        port: runtime.port,
        max_seq_len: runtime.max_seq_len,
        max_seqs: runtime.max_seqs,
        max_batch_size: runtime.max_batch_size,
    }
}

fn materialize_family_profile(
    family: engine::LocalModelFamily,
    profile: StaticFamilyProfile,
) -> engine::EngineFamilyProfile {
    engine::EngineFamilyProfile {
        family,
        launcher_mode: profile.launcher_mode.to_string(),
        arch: profile.arch.map(str::to_string),
        paged_attn: profile.paged_attn.to_string(),
        pa_cache_type: profile.pa_cache_type.map(str::to_string),
        pa_memory_fraction: profile.pa_memory_fraction.map(str::to_string),
        pa_context_len: profile.pa_context_len,
        max_seq_len: profile.max_seq_len,
        max_batch_size: profile.max_batch_size,
        max_seqs: profile.max_seqs,
        isq: profile.isq.map(str::to_string),
        tensor_parallel_backend: profile.tensor_parallel_backend.map(str::to_string),
        disable_nccl: profile.disable_nccl,
        target_world_size: profile.target_world_size,
        preferred_gpu_count: profile.preferred_gpu_count,
    }
}

pub fn chat_family_catalog_entry(
    family: engine::ChatModelFamily,
) -> Option<&'static ChatFamilyCatalogEntry> {
    CHAT_FAMILY_REGISTRY
        .iter()
        .find(|entry| entry.family == family)
}

pub fn parse_chat_model_family(value: &str) -> Option<engine::ChatModelFamily> {
    let trimmed = value.trim();
    CHAT_FAMILY_REGISTRY.iter().find_map(|entry| {
        entry
            .parse_aliases
            .iter()
            .any(|candidate| matches_candidate(trimmed, candidate))
            .then_some(entry.family)
    })
}

pub fn parse_local_model_family(value: &str) -> Option<engine::LocalModelFamily> {
    let trimmed = value.trim();
    LOCAL_FAMILY_REGISTRY.iter().find_map(|entry| {
        entry
            .parse_aliases
            .iter()
            .any(|candidate| matches_candidate(trimmed, candidate))
            .then_some(entry.family)
    })
}

pub fn default_local_chat_model() -> &'static str {
    local_family_entry(DEFAULT_LOCAL_CHAT_FAMILY)
        .map(|entry| entry.default_model)
        .expect("default local chat family must exist in local family registry")
}

pub fn default_local_chat_family() -> engine::ChatModelFamily {
    chat_model_family_for_model(default_local_chat_model())
        .expect("default local chat model must resolve to a chat family")
}

pub fn default_local_chat_family_selector() -> &'static str {
    chat_family_catalog_entry(default_local_chat_family())
        .map(|entry| entry.selector)
        .expect("default local chat family selector must exist")
}

pub fn default_local_chat_family_label() -> &'static str {
    chat_family_catalog_entry(default_local_chat_family())
        .map(|entry| entry.label)
        .expect("default local chat family label must exist")
}

pub fn supported_clean_room_family_selectors() -> Vec<&'static str> {
    SUPPORTED_LOCAL_CHAT_FAMILIES
        .iter()
        .filter_map(|family| chat_family_catalog_entry(*family).map(|entry| entry.selector))
        .collect()
}

pub fn chat_model_family_for_model(model: &str) -> Option<engine::ChatModelFamily> {
    if let Some(family) = local_model_entry(model).and_then(|entry| entry.chat_family) {
        return Some(family);
    }
    let trimmed = model.trim();
    REMOTE_CHAT_FAMILY_REGISTRY
        .iter()
        .find(|entry| matches_candidate(trimmed, entry.model))
        .map(|entry| entry.chat_family)
}

/// True when the model can natively accept image content blocks
/// (`input_image` / `image_url`). The vision preprocessor consults this
/// before deciding whether to describe images via the Vision aux.
///
/// Resolution order:
/// 1. Local model catalog → `ChatFamilyCatalogEntry::supports_vision` for
///    the model's chat family (covers Qwen3.5, Gemma 4, Mistral local).
/// 2. Local model family-based marker — `Qwen35Vision`, `Gemma4Vision`,
///    `Qwen3VisionAuxiliary` are vision-capable regardless of the chat
///    family mapping (covers the Qwen3-VL-2B aux itself).
/// 3. Explicit `VISION_API_MODELS` allowlist for remote/API providers.
pub fn model_supports_vision(model: &str) -> bool {
    let trimmed = model.trim();
    if trimmed.is_empty() {
        return false;
    }
    // (1) chat family-based lookup
    if let Some(family) = chat_model_family_for_model(trimmed) {
        if let Some(entry) = CHAT_FAMILY_REGISTRY
            .iter()
            .find(|candidate| candidate.family == family)
        {
            if entry.supports_vision {
                return true;
            }
        }
    }
    // (2) local model family — catches Qwen35Vision/Gemma4Vision/Qwen3VisionAuxiliary
    //     even if the chat family hasn't been enriched yet.
    if let Some(entry) = local_model_entry(trimmed) {
        if matches!(
            entry.family,
            engine::LocalModelFamily::Qwen35Vision
                | engine::LocalModelFamily::Gemma4Vision
                | engine::LocalModelFamily::Qwen3VisionAuxiliary
        ) {
            return true;
        }
    }
    // (3) API / remote allowlist
    VISION_API_MODELS
        .iter()
        .any(|candidate| candidate.eq_ignore_ascii_case(trimmed))
}

pub fn canonical_model_id(model: &str) -> Option<&'static str> {
    local_model_entry(model).map(|entry| entry.canonical_model)
}

pub fn runtime_manifest_slug(model: &str) -> Option<&'static str> {
    local_model_entry(model).and_then(|entry| entry.runtime_manifest_slug)
}

pub fn auxiliary_manifest_slug(model: &str) -> Option<&'static str> {
    local_model_entry(model).and_then(|entry| entry.auxiliary_manifest_slug)
}

pub fn supported_local_model_profiles() -> Vec<engine::LocalModelProfile> {
    LOCAL_MODEL_REGISTRY
        .iter()
        .filter(|entry| local_chat_model_is_supported(entry))
        .map(|entry| engine::LocalModelProfile {
            runtime: materialize_runtime_config(entry.family, entry.canonical_model, entry.runtime),
            family_profile: materialize_family_profile(entry.family, entry.profile),
        })
        .collect()
}

pub fn model_profile_for_model(model: &str) -> Option<engine::LocalModelProfile> {
    let entry = local_model_entry(model)?;
    Some(engine::LocalModelProfile {
        runtime: materialize_runtime_config(entry.family, entry.canonical_model, entry.runtime),
        family_profile: materialize_family_profile(entry.family, entry.profile),
    })
}

pub fn default_runtime_config(
    family: engine::LocalModelFamily,
) -> Option<engine::EngineRuntimeConfig> {
    let entry = local_family_entry(family)?;
    Some(materialize_runtime_config(
        entry.family,
        entry.default_model,
        entry.default_runtime,
    ))
}

pub fn default_family_profile(
    family: engine::LocalModelFamily,
) -> Option<engine::EngineFamilyProfile> {
    let entry = local_family_entry(family)?;
    Some(materialize_family_profile(
        entry.family,
        entry.default_profile,
    ))
}

pub fn bridge_mode_for_family(family: engine::LocalModelFamily) -> Option<&'static str> {
    local_family_entry(family).map(|entry| entry.bridge_mode)
}

pub fn backend_startup_wait_secs(model: &str) -> Option<u64> {
    model_ops_metadata_entry(model).map(|entry| entry.startup_wait_secs)
}

pub fn process_command_model_name(command: &str) -> Option<&'static str> {
    MODEL_OPS_METADATA_REGISTRY.iter().find_map(|entry| {
        entry
            .process_aliases
            .iter()
            .any(|alias| command.contains(alias))
            .then_some(entry.canonical_model)
    })
}

pub fn estimated_tokens_per_second(model: &str) -> Option<f64> {
    model_ops_metadata_entry(model).and_then(|entry| entry.default_tokens_per_second)
}

pub fn estimated_chat_base_memory_mb(model: &str) -> Option<u64> {
    model_ops_metadata_entry(model).and_then(|entry| entry.estimated_chat_base_memory_mb)
}

pub fn gpu_short_label(model: &str) -> Option<&'static str> {
    model_ops_metadata_entry(model).and_then(|entry| entry.gpu_short_label)
}

pub fn default_auxiliary_model(role: engine::AuxiliaryRole) -> Option<&'static str> {
    AUXILIARY_SELECTION_REGISTRY
        .iter()
        .find(|entry| entry.role == role && entry.default_for_role)
        .map(|entry| entry.request_model)
}

pub fn auxiliary_model_selection(
    role: engine::AuxiliaryRole,
    configured_model: Option<&str>,
) -> engine::AuxiliaryModelSelection {
    let trimmed = configured_model.map(str::trim).unwrap_or("");
    let selected = AUXILIARY_SELECTION_REGISTRY
        .iter()
        .find(|entry| {
            entry.role == role
                && entry
                    .aliases
                    .iter()
                    .any(|candidate| matches_candidate(trimmed, candidate))
        })
        .or_else(|| {
            AUXILIARY_SELECTION_REGISTRY
                .iter()
                .find(|entry| entry.role == role && entry.default_for_role)
        })
        .expect("auxiliary registry must define one default variant per role");
    engine::AuxiliaryModelSelection {
        role: selected.role,
        choice: selected.choice,
        request_model: selected.request_model,
        backend_kind: selected.backend_kind,
        compute_target: selected.compute_target,
        default_port: selected.default_port,
    }
}

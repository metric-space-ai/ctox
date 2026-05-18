//! Voxtral Realtime 4B constants.

// Audio preprocessing
pub const VOX_SAMPLE_RATE: usize = 16_000;
pub const VOX_MEL_BINS: usize = 128;
pub const VOX_HOP_LENGTH: usize = 160;
pub const VOX_WINDOW_SIZE: usize = 400;
pub const VOX_FRAME_RATE: f32 = 12.5;
pub const VOX_LOG_MEL_MAX: f32 = 1.5;

// Audio encoder
pub const VOX_ENC_DIM: usize = 1280;
pub const VOX_ENC_LAYERS: usize = 32;
pub const VOX_ENC_HEADS: usize = 32;
pub const VOX_ENC_KV_HEADS: usize = 32;
pub const VOX_ENC_HEAD_DIM: usize = 64;
pub const VOX_ENC_HIDDEN: usize = 5120;
pub const VOX_ENC_WINDOW: usize = 750;
pub const VOX_ENC_NORM_EPS: f32 = 1e-5;

// Downsampling
pub const VOX_DOWNSAMPLE: usize = 4;

// LLM decoder
pub const VOX_DEC_DIM: usize = 3072;
pub const VOX_DEC_LAYERS: usize = 26;
pub const VOX_DEC_HEADS: usize = 32;
pub const VOX_DEC_KV_HEADS: usize = 8;
pub const VOX_DEC_HEAD_DIM: usize = 128;
pub const VOX_DEC_HIDDEN: usize = 9216;
pub const VOX_DEC_WINDOW: usize = 8192;
pub const VOX_DEC_NORM_EPS: f32 = 1e-5;
pub const VOX_VOCAB_SIZE: usize = 131_072;
pub const VOX_ADA_NORM_DIM: usize = 32;
pub const VOX_ROPE_THETA: f32 = 1_000_000.0;

// Token IDs
pub const TOKEN_BOS: u32 = 1;
pub const TOKEN_EOS: u32 = 2;
pub const TOKEN_STREAMING_PAD: u32 = 32;
pub const TOKEN_TEXT_MIN: u32 = 1000;
pub const TOKEN_AUDIO: u32 = 24;
pub const TOKEN_BEGIN_AUDIO: u32 = 25;

pub const DEFAULT_DELAY_TOKENS: usize = 6; // 480 ms
pub const LEFT_PAD_TOKENS: usize = 32;
pub const OFFLINE_STREAMING_BUFFER_TOKENS: usize = 10;
pub const RIGHT_PAD_TOKENS_OFFLINE: usize =
    (DEFAULT_DELAY_TOKENS + 1) + OFFLINE_STREAMING_BUFFER_TOKENS;

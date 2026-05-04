pub const VOXTRAL_MINI_4B_REALTIME_2602_CANONICAL_MODEL: &str =
    "engineai/Voxtral-Mini-4B-Realtime-2602";

pub const VOX_ENC_DIM: usize = 1280;
pub const VOX_ENC_LAYERS: usize = 32;
pub const VOX_ENC_HEADS: usize = 32;
pub const VOX_ENC_HEAD_DIM: usize = 64;
pub const VOX_ENC_HIDDEN: usize = 5120;
pub const VOX_ENC_KV_HEADS: usize = 32;
pub const VOX_ENC_WINDOW: usize = 750;
pub const VOX_ENC_NORM_EPS: f32 = 1e-5;
pub const VOX_ENC_ROPE_THETA: f32 = 1_000_000.0;

pub const VOX_DEC_DIM: usize = 3072;
pub const VOX_DEC_LAYERS: usize = 26;
pub const VOX_DEC_HEADS: usize = 32;
pub const VOX_DEC_HEAD_DIM: usize = 128;
pub const VOX_DEC_HIDDEN: usize = 9216;
pub const VOX_DEC_KV_HEADS: usize = 8;
pub const VOX_DEC_WINDOW: usize = 8192;
pub const VOX_DEC_NORM_EPS: f32 = 1e-5;
pub const VOX_DEC_ROPE_THETA: f32 = 1_000_000.0;
pub const VOX_VOCAB_SIZE: usize = 131_072;

pub const VOX_SAMPLE_RATE: usize = 16_000;
pub const VOX_FRAME_RATE: f32 = 12.5;
pub const VOX_NUM_MEL_BINS: usize = 128;
pub const VOX_HOP_LENGTH: usize = 160;
pub const VOX_WINDOW_SIZE: usize = 400;
pub const VOX_GLOBAL_LOG_MEL_MAX: f32 = 1.5;
pub const VOX_DOWNSAMPLE_FACTOR: usize = 4;

pub const VOX_ADA_NORM_DIM: usize = 32;

pub const VOX_N_LEFT_PAD_TOKENS: usize = 32;
pub const VOX_TRANSCRIPTION_DELAY_MS: usize = 480;
pub const VOX_N_DELAY_TOKENS: usize = 6;
pub const VOX_N_RIGHT_PAD_TOKENS: usize = 17;
pub const VOX_RAW_AUDIO_LENGTH_PER_TOK: usize = 1280;

pub const VOX_TOKEN_BOS: i32 = 1;
pub const VOX_TOKEN_EOS: i32 = 2;
pub const VOX_TOKEN_STREAMING_PAD: i32 = 32;
pub const VOX_TOKEN_BEGIN_AUDIO: i32 = 25;
pub const VOX_TOKEN_AUDIO: i32 = 24;

//! Wire constants — see `whatsmeow/socket/constants.go`. The dictionary
//! version comes from the binary token table.

pub const ORIGIN: &str = "https://web.whatsapp.com";
pub const URL: &str = "wss://web.whatsapp.com/ws/chat";

pub const NOISE_START_PATTERN: &[u8] = b"Noise_XX_25519_AESGCM_SHA256\x00\x00\x00\x00";
pub const WA_MAGIC_VALUE: u8 = 6;

/// Length-prefix of every frame on the websocket. `< 2^24` per Whatsmeow.
pub const FRAME_LENGTH_SIZE: usize = 3;
pub const FRAME_MAX_SIZE: usize = 1 << 24;

/// Connection header sent on the first frame: `WA<magic><dictVer>`.
pub fn wa_conn_header() -> [u8; 4] {
    [b'W', b'A', WA_MAGIC_VALUE, 3 /* dict version */]
}

/// Module-level constant convenience copy.
pub static WA_CONN_HEADER: [u8; 4] = [b'W', b'A', WA_MAGIC_VALUE, 3];

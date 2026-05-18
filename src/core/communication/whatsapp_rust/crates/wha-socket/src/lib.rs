//! Frame-socket + Noise-XX layer used to talk to `wss://web.whatsapp.com/ws/chat`.
//!
//! The transport is straightforward: every websocket binary message contains
//! one or more length-prefixed frames (`[len_high][len_mid][len_low][bytes…]`),
//! and after the Noise handshake every frame is AEAD-encrypted with AES-GCM.
//! This crate exposes:
//!
//! * [`FrameSocket`] — async websocket wrapper that emits raw frames as a
//!   stream and lets you push frames out.
//! * [`NoiseHandshake`] — state machine for the XX handshake, mirroring
//!   `whatsmeow/socket/noisehandshake.go` byte for byte.
//! * [`NoiseSocket`] — wraps a `FrameSocket` after `NoiseHandshake::finish()`
//!   so callers can send/receive plaintext frames.

pub mod constants;
pub mod error;
pub mod frame;
pub mod noise;

pub use constants::{
    FRAME_LENGTH_SIZE, FRAME_MAX_SIZE, NOISE_START_PATTERN, ORIGIN, URL, WA_CONN_HEADER,
    WA_MAGIC_VALUE,
};
pub use error::SocketError;
pub use frame::FrameSocket;
pub use noise::{NoiseHandshake, NoiseSocket};

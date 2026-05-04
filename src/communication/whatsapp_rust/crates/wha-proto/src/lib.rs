//! Generated protobuf bindings for a curated subset of the WhatsApp proto tree.
//!
//! The bindings are produced from the `.proto` files in
//! `_upstream/whatsmeow/proto/` by `build.rs` at build time. Re-export each
//! generated module under a friendly Rust name so callers can write
//! `wha_proto::wa6::HandshakeMessage` instead of the protobuf-cased original.

// prost-build generates one Rust file per `.proto` package, and each file
// addresses its dependencies via `super::<sibling-module>`. So every package
// MUST live as a direct sibling under the same parent module — and the
// module names MUST match the prost-generated names. We rename them at the
// top level for ergonomic use.
#[allow(clippy::all)]
#[allow(non_snake_case)]
#[allow(non_camel_case_types)]
pub mod inner {
    pub mod wa_cert {
        include!(concat!(env!("OUT_DIR"), "/wa_cert.rs"));
    }
    pub mod wa_web_protobufs_wa6 {
        include!(concat!(env!("OUT_DIR"), "/wa_web_protobufs_wa6.rs"));
    }
    pub mod wa_common {
        include!(concat!(env!("OUT_DIR"), "/wa_common.rs"));
    }
    pub mod wa_companion_reg {
        include!(concat!(env!("OUT_DIR"), "/wa_companion_reg.rs"));
    }
    pub mod wa_adv {
        include!(concat!(env!("OUT_DIR"), "/wa_adv.rs"));
    }
    pub mod wa_mms_retry {
        include!(concat!(env!("OUT_DIR"), "/wa_mms_retry.rs"));
    }
    pub mod wa_status_attributions {
        include!(concat!(env!("OUT_DIR"), "/wa_status_attributions.rs"));
    }
    pub mod waai_common_deprecated {
        include!(concat!(env!("OUT_DIR"), "/waai_common_deprecated.rs"));
    }
    pub mod wa_web_protobufs_ai_common {
        include!(concat!(env!("OUT_DIR"), "/wa_web_protobufs_ai_common.rs"));
    }
    pub mod wa_web_protobufs_e2e {
        include!(concat!(env!("OUT_DIR"), "/wa_web_protobufs_e2e.rs"));
    }
    pub mod wa_web_protobufs_chat_lock_settings {
        include!(concat!(env!("OUT_DIR"), "/wa_web_protobufs_chat_lock_settings.rs"));
    }
    pub mod wa_web_protobufs_device_capabilities {
        include!(concat!(env!("OUT_DIR"), "/wa_web_protobufs_device_capabilities.rs"));
    }
    pub mod wa_web_protobufs_user_password {
        include!(concat!(env!("OUT_DIR"), "/wa_web_protobufs_user_password.rs"));
    }
    pub mod wa_web_protobuf_sync_action {
        include!(concat!(env!("OUT_DIR"), "/wa_web_protobuf_sync_action.rs"));
    }
    pub mod wa_web_protobufs_web {
        include!(concat!(env!("OUT_DIR"), "/wa_web_protobufs_web.rs"));
    }
    pub mod wa_web_protobufs_history_sync {
        include!(concat!(env!("OUT_DIR"), "/wa_web_protobufs_history_sync.rs"));
    }
    pub mod wa_server_sync {
        include!(concat!(env!("OUT_DIR"), "/wa_server_sync.rs"));
    }
    pub mod wa_msg_application {
        include!(concat!(env!("OUT_DIR"), "/wa_msg_application.rs"));
    }
    pub mod wa_msg_transport {
        include!(concat!(env!("OUT_DIR"), "/wa_msg_transport.rs"));
    }
    pub mod wa_consumer_application {
        include!(concat!(env!("OUT_DIR"), "/wa_consumer_application.rs"));
    }
}

pub mod cert {
    pub use super::inner::wa_cert::*;
}
pub mod wa6 {
    pub use super::inner::wa_web_protobufs_wa6::*;
}
pub mod common {
    pub use super::inner::wa_common::*;
}
pub mod companion_reg {
    pub use super::inner::wa_companion_reg::*;
}
pub mod adv {
    pub use super::inner::wa_adv::*;
}
pub mod mms_retry {
    pub use super::inner::wa_mms_retry::*;
}
pub mod status_attributions {
    pub use super::inner::wa_status_attributions::*;
}
pub mod ai_common_deprecated {
    pub use super::inner::waai_common_deprecated::*;
}
pub mod ai_common {
    pub use super::inner::wa_web_protobufs_ai_common::*;
}
pub mod e2e {
    pub use super::inner::wa_web_protobufs_e2e::*;
}
pub mod chat_lock_settings {
    pub use super::inner::wa_web_protobufs_chat_lock_settings::*;
}
pub mod device_capabilities {
    pub use super::inner::wa_web_protobufs_device_capabilities::*;
}
pub mod user_password {
    pub use super::inner::wa_web_protobufs_user_password::*;
}
pub mod sync_action {
    pub use super::inner::wa_web_protobuf_sync_action::*;
}
pub mod web {
    pub use super::inner::wa_web_protobufs_web::*;
}
pub mod history_sync {
    pub use super::inner::wa_web_protobufs_history_sync::*;
}
pub mod server_sync {
    pub use super::inner::wa_server_sync::*;
}
/// Messenger / Facebook E2EE wrapper protos. The user-facing
/// `EncryptedMessage` alias maps to `MessageApplication` upstream — the
/// outer envelope used by `prepareFBMessage` / `SendFBMessage` in
/// `_upstream/whatsmeow/sendfb.go`.
pub mod messenger {
    pub use super::inner::wa_msg_application::*;
    pub use super::inner::wa_msg_transport as transport;

    /// User-facing alias matching the type referenced by the
    /// `send_fb_message` API. Maps to upstream's `MessageApplication`
    /// envelope (`waMsgApplication.MessageApplication`).
    ///
    /// Only available when the upstream `_upstream/whatsmeow/proto`
    /// tree is present. When proto sources are missing, the build
    /// emits empty stubs and sets the `wha_proto_stubbed` cfg flag —
    /// in that mode the alias is omitted because the underlying
    /// `MessageApplication` type does not exist.
    #[cfg(not(wha_proto_stubbed))]
    pub type EncryptedMessage = MessageApplication;
}

/// Consumer-application sub-protocol carried inside a Messenger /
/// FB-side `MessageApplication.payload.subProtocol.consumerMessage`.
/// Mirrors `waConsumerApplication.ConsumerApplication` upstream.
pub mod consumer_application {
    pub use super::inner::wa_consumer_application::*;
}

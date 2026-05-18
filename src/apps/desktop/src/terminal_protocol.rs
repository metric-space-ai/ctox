use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum RemoteSessionKind {
    Tui,
    Command,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum HubMessage {
    Init {
        #[serde(rename = "yourPeerId")]
        your_peer_id: String,
    },
    Join {
        room: String,
    },
    Joined {
        #[serde(rename = "otherPeerIds")]
        other_peer_ids: Vec<String>,
    },
    Signal {
        #[serde(rename = "senderPeerId")]
        sender_peer_id: String,
        #[serde(rename = "receiverPeerId")]
        receiver_peer_id: String,
        signal: PeerSignal,
    },
    Ping {
        #[serde(default)]
        t: Option<u64>,
    },
    Pong {
        #[serde(default)]
        t: Option<u64>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum PeerSignal {
    Offer {
        sdp: String,
    },
    Answer {
        sdp: String,
    },
    Candidate {
        candidate: String,
        sdp_mid: Option<String>,
        sdp_mline_index: Option<u16>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum TerminalWireMessage {
    Start {
        session_kind: RemoteSessionKind,
        args: Vec<String>,
        rows: u16,
        cols: u16,
        pixel_width: u16,
        pixel_height: u16,
    },
    Input {
        data_base64: String,
    },
    Resize {
        rows: u16,
        cols: u16,
        pixel_width: u16,
        pixel_height: u16,
    },
    Close,
    Output {
        data_base64: String,
    },
    Exit {
        code: i32,
    },
    Status {
        message: String,
    },
}

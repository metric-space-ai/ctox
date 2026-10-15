//! V4 reliable lanes. Real-time media is deliberately not a reliable lane.
use serde::{Deserialize, Serialize};

pub const FRAME_BYTES: usize = 16 * 1024;
pub const ASSOCIATION_BUDGET: usize = 128 * 1024;
pub const BULK_WEIGHTS: [usize; 3] = [8, 3, 1];

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[repr(u8)]
pub enum Lane {
    Control = 0,
    Commands = 1,
    Interactive = 2,
    Live = 3,
    InitialSync = 4,
    Transfer = 5,
}
impl Lane {
    pub const ALL: [Self; 6] = [
        Self::Control,
        Self::Commands,
        Self::Interactive,
        Self::Live,
        Self::InitialSync,
        Self::Transfer,
    ];
    pub fn index(self) -> usize {
        self as usize
    }
    pub fn from_index(index: usize) -> Option<Self> {
        Self::ALL.get(index).copied()
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Control => "ctox-v4-control",
            Self::Commands => "ctox-v4-commands",
            Self::Interactive => "ctox-v4-interactive",
            Self::Live => "ctox-v4-live",
            Self::InitialSync => "ctox-v4-initial-sync",
            Self::Transfer => "ctox-v4-transfer",
        }
    }
}
pub const REALTIME_LABEL: &str = "ctox-v4-realtime";

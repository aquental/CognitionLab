//! Closed sets of domain values shared across modules (PLAN §2.3: enums, not strings).

use serde::{Deserialize, Serialize};

/// Modality of an indexed item.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Modality {
    /// Extracted text.
    Text,
    /// An image asset.
    Image,
}

impl Modality {
    /// Canonical lowercase name used in identifiers and records.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::Image => "image",
        }
    }
}

/// Where a process runs, used for remote-permission checks and cost reporting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Execution {
    /// Runs inside this process or on this machine.
    Local,
    /// Sends data to a remote provider.
    Remote,
}

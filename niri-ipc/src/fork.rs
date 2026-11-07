//! Types the fork adds to the IPC.

use serde::{Deserialize, Serialize};

/// A feature this compositor has beyond upstream niri, with the version of its contract.
///
/// Returned by [`Request::Features`](crate::Request::Features). Upstream niri can't parse that
/// request and answers with an error, so an error means none of these exist.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct Feature {
    /// The feature's name, such as `blur-shapes` or `desktop-obscured`.
    pub name: String,
    /// The version of the feature's contract. It goes up when the feature changes in a way a
    /// client can tell.
    pub version: u32,
}


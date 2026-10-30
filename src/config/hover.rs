//! The hover options an editor supplies in `initializationOptions`.

use serde::Deserialize;

/// Hover settings supplied by the editor rather than by
/// `.phpantom.toml`:
///
/// ```json
/// { "hover": { "declarations": true } }
/// ```
///
/// Read only during `initialize`, never from
/// `workspace/didChangeConfiguration`.
#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct ClientHoverOptions {
    /// Whether hover on declarations is on.
    pub declarations: bool,
}

impl ClientHoverOptions {
    /// Read the `hover` block out of a client-supplied settings
    /// blob, in the shapes [`ClientIndexingOptions::from_client_settings`]
    /// accepts: at the top level, or nested under a `phpantom` key.
    ///
    /// Returns `None` when the blob carries no readable `hover`
    /// block, meaning "this blob is not about hover" rather than
    /// "hover is off": no `hover` key, a non-object block, or a
    /// non-boolean `declarations` value. A present block whose
    /// `declarations` key is missing reads as `false` instead. Only
    /// `true` turns the feature on, so neither outcome can lower a
    /// switch the command line raised.
    ///
    /// [`ClientIndexingOptions::from_client_settings`]: super::ClientIndexingOptions::from_client_settings
    pub fn from_client_settings(settings: &serde_json::Value) -> Option<Self> {
        let scoped = settings.get("phpantom").unwrap_or(settings);
        serde_json::from_value(scoped.get("hover")?.clone()).ok()
    }
}

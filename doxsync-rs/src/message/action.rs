use crate::Value;

/// A doxsync action.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Action {
    /// Replaces the current document with the given JSON value.
    Snapshot {
        /// The snapshot value to replace the current document with.
        value: Value,
    }
}

use serde::Serialize;

/// A field's exact range, with nested children partitioning compound fields.
#[derive(Debug, Serialize)]
pub struct Field {
    pub label: String,
    pub offset: usize,
    pub length: usize,
    pub hex: String,
    pub explanation: String,
    pub children: Vec<Field>,
    /// Short comment for an annotated hex listing.
    pub annotation: String,
    /// Number of leading child fields grouped on this node's listing line.
    pub prefix: usize,
}

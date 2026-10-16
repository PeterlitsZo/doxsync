//! Decoded semantics, independent of the diagnostic byte tree and JSON transport.
use serde::{Deserialize, Serialize, Serializer};
use std::collections::BTreeMap;

/// A document value. Integers and floats serialize as strings to preserve their
/// full range, including non-finite floats, across the JavaScript boundary.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", content = "value", rename_all = "camelCase")]
pub enum Value {
    Null(()),
    Boolean(bool),
    #[serde(rename = "bigint")]
    Integer(#[serde(serialize_with = "as_string")] i128),
    Number(#[serde(serialize_with = "as_string")] f64),
    Decimal(#[serde(serialize_with = "as_string")] Decimal),
    String(String),
    Bytes(Vec<u8>),
    Array(Vec<Value>),
    Map(BTreeMap<String, Value>),
}

fn as_string<T: std::fmt::Display, S: Serializer>(
    value: &T,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    serializer.collect_str(value)
}

impl Value {
    pub(crate) fn decimal(coefficient: i128, scale: u8) -> Self {
        Self::Decimal(Decimal { coefficient, scale })
    }

    /// JSON-shaped scalar text for wire comments, without reparsing diagnostic prose.
    pub(crate) fn display_scalar(&self) -> String {
        match self {
            Self::Null(()) => "null".into(),
            Self::Boolean(value) => value.to_string(),
            Self::Integer(value) => format!("\"{value}\""),
            Self::Number(value) => format!("\"{value}\""),
            Self::Decimal(value) => format!("\"{value}\""),
            Self::String(value) => serde_json::json!(value).to_string(),
            Self::Bytes(value) => serde_json::json!(value).to_string(),
            Self::Array(_) | Self::Map(_) => unreachable!("containers have their own annotations"),
        }
    }
}

/// String keys and numeric indexes are distinct; an empty path addresses the root.
pub type Path = Vec<Segment>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Segment {
    Key(String),
    Index(u64),
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Action {
    Snapshot {
        value: Value,
    },
    Add {
        path: Path,
        value: Value,
    },
    Delete {
        path: Path,
    },
    Copy {
        path: Path,
        from: Path,
    },
    Replace {
        path: Path,
        value: Value,
    },
    StringAppend {
        path: Path,
        #[serde(serialize_with = "string_value")]
        value: String,
    },
    StringPrepend {
        path: Path,
        #[serde(serialize_with = "string_value")]
        value: String,
    },
    ArrayPatch {
        path: Path,
        edits: Vec<ArrayEdit>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArrayEdit {
    pub start: u64,
    pub delete_count: u64,
    pub values: Vec<Value>,
}

/// Exact base-10 value: coefficient × 10^-scale.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Decimal {
    pub coefficient: i128,
    pub scale: u8,
}

impl std::fmt::Display for Decimal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}e-{}", self.coefficient, self.scale)
    }
}

fn string_value<S: Serializer>(value: &str, serializer: S) -> Result<S::Ok, S::Error> {
    #[derive(Serialize)]
    struct TaggedString<'a> {
        kind: &'static str,
        value: &'a str,
    }
    TaggedString {
        kind: "string",
        value,
    }
    .serialize(serializer)
}

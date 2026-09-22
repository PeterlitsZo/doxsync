use std::{fmt::Debug, sync::Arc};

#[cfg(test)]
use crate::Result;
use crate::Value;

/// A doxsync action.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Action {
    /// Replaces the current document with the given JSON value.
    Snapshot {
        /// The snapshot value to replace the current document with.
        value: Value,
    },

    /// Adds a value to the given path.
    Add {
        /// The path to add the value to.
        path: Path,
        /// The value to add.
        value: Value,
    },

    /// Replaces an existing value without inserting or shifting array elements.
    Replace {
        /// The path of the existing value; an empty path replaces the root.
        path: Path,
        /// The replacement value.
        value: Value,
    },

    /// Deletes the value at the given path.
    Delete {
        /// The path to delete the value from.
        path: Path,
    },

    /// Copies the value at the given path to another path.
    Copy {
        /// The path to copy the value to.
        path: Path,
        /// The path to copy the value from.
        from: Path,
    },
}

#[cfg(test)]
impl Action {
    pub(crate) fn snapshot(value: Value) -> Self {
        Self::Snapshot { value }
    }

    pub(crate) fn add(path: Path, value: Value) -> Self {
        Self::Add { path, value }
    }

    pub(crate) fn delete(path: Path) -> Self {
        Self::Delete { path }
    }

    pub(crate) fn replace(path: Path, value: Value) -> Self {
        Self::Replace { path, value }
    }

    pub(crate) fn copy(path: Path, from: Path) -> Self {
        Self::Copy { path, from }
    }
}

/// A path in the document.
#[derive(Clone, Default, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) struct Path {
    inner: Vec<PathSegment>,
}

impl Debug for Path {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Path(")?;
        for (i, segment) in self.inner.iter().enumerate() {
            if i > 0 {
                write!(f, ".")?;
            }
            match segment {
                PathSegment::Key(k) => write!(f, "{}", k)?,
                PathSegment::Index(i) => write!(f, "{}", i)?,
            }
        }
        write!(f, ")")
    }
}

impl Path {
    pub(crate) fn empty() -> Self {
        Self::new(vec![])
    }

    pub(crate) fn new(inner: Vec<PathSegment>) -> Self {
        Self { inner }
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    #[cfg(test)]
    pub(crate) fn parse(path: &str) -> Result<Self> {
        use crate::{Error, ErrorKind};

        let mut segments = vec![];
        let chars = path.chars().collect::<Vec<_>>();
        let mut i = 0;
        loop {
            if i >= chars.len() {
                break;
            }
            match chars[i] {
                '0'..='9' => {
                    let mut num = 0;
                    while i < chars.len() {
                        match chars[i] {
                            '0'..='9' => {
                                num = num * 10 + (chars[i] as usize - '0' as usize);
                                i += 1;
                            }
                            '.' => {
                                i += 1;
                                break;
                            }
                            _ => {
                                return Err(Error::new(
                                    crate::ErrorKind::InvalidData,
                                    "expected digit",
                                )
                                .with_metadata("index", i));
                            }
                        }
                    }
                    segments.push(PathSegment::Index(num));
                }
                '.' => {
                    return Err(Error::new(
                        crate::ErrorKind::InvalidData,
                        "unexpected character '.'",
                    )
                    .with_metadata("index", i));
                }
                '\'' => {
                    let mut key = String::new();
                    while i < chars.len() {
                        match chars[i] {
                            '\'' => {
                                i += 1;
                                break;
                            }
                            '\\' => {
                                i += 1;
                                key.push(chars[i]);
                                i += 1;
                            }
                            _ => {
                                key.push(chars[i]);
                                i += 1;
                            }
                        }
                    }
                    segments.push(PathSegment::Key(Arc::new(key)));
                }
                _ => {
                    let mut key = String::new();
                    while i < chars.len() {
                        match chars[i] {
                            '.' => {
                                i += 1;
                                break;
                            }
                            '\'' | '\\' => {
                                return Err(Error::new(
                                    ErrorKind::InvalidData,
                                    "invalid character in key",
                                ));
                            }
                            _ => {
                                key.push(chars[i]);
                                i += 1;
                            }
                        }
                    }
                    segments.push(PathSegment::Key(Arc::new(key)));
                }
            }
        }
        Ok(Self::new(segments))
    }

    pub(crate) fn segments(&self) -> &[PathSegment] {
        &self.inner
    }

    pub(crate) fn push_segment(&mut self, segment: PathSegment) {
        self.inner.push(segment);
    }

    pub(crate) fn pop_segment(&mut self) -> Option<PathSegment> {
        self.inner.pop()
    }
}

/// A segment in a path.
#[derive(Clone, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) enum PathSegment {
    Key(Arc<String>),
    Index(usize),
}

impl Debug for PathSegment {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "PathSegment(")?;
        match self {
            PathSegment::Key(k) => write!(f, "{}", k)?,
            PathSegment::Index(i) => write!(f, "{}", i)?,
        }
        write!(f, ")")
    }
}

impl PathSegment {
    pub(crate) fn key<T>(k: T) -> Self
    where
        T: Into<String>,
    {
        Self::Key(Arc::new(k.into()))
    }

    pub(crate) fn key_arc(k: Arc<String>) -> Self {
        Self::Key(k)
    }

    pub(crate) fn index(i: usize) -> Self {
        Self::Index(i)
    }
}

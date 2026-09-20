use std::{fmt::Debug, sync::Arc};

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

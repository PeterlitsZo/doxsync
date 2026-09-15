use std::{collections::BTreeMap, fmt::{Debug, Display}};

pub struct Error {
    kind: ErrorKind,
    message: &'static str,
    context: Vec<&'static str>,
    metadata: BTreeMap<&'static str, String>,
}

impl Error {
    pub fn new(kind: ErrorKind, message: &'static str) -> Self {
        Self { kind, message, context: vec![], metadata: BTreeMap::new() }
    }

    pub fn kind(&self) -> ErrorKind {
        self.kind
    }

    pub fn with_context(mut self, context: &'static str) -> Self {
        self.context.push(context);
        self
    }

    pub fn with_metadata<T>(mut self, key: &'static str, value: T) -> Self
    where
        T: Display,
    {
        self.metadata.insert(key, value.to_string());
        self
    }
}

impl Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl Debug for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{:?}] ", self.kind)?;
        for context in self.context.iter().rev() {
            write!(f, "{}: ", context)?;
        }
        write!(f, "{}", self.message)?;
        let mut is_first = true;
        for (key, value) in self.metadata.iter() {
            if is_first {
                write!(f, " (")?;
                is_first = false;
            } else {
                write!(f, ", ")?;
            }
            write!(f, "{}={}", key, value)?;
        }
        if !is_first {
            write!(f, ")")?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorKind {
    /// An internal error occurred.
    Internal,
    /// The data is invalid.
    InvalidData,
}

pub type Result<T> = std::result::Result<T, Error>;

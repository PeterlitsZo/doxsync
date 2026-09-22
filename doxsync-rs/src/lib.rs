mod consumer;
mod document;
mod error;
mod message;
mod patch;
mod producer;
mod protocol;
mod state;
mod value;

#[doc(hidden)]
pub mod __private;

pub use consumer::Consumer;
pub use document::Document;
pub use error::{Error, ErrorKind, Result};
pub use message::{Message, PackedMessage};
pub use producer::Producer;
pub use value::{Value, ValueKind};

pub(crate) use state::{ConsumerState, ProducerState};
pub(crate) use value::ValueInner;

#[cfg(test)]
mod tests;

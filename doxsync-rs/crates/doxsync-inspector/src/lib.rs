//! Independent, stateful wire disassembly. No dependency on the sync engine.
//!
//! Feed one complete ordered stream starting at its protocol declaration. State
//! commits only after a complete successful parse. This diagnoses encoding, not
//! whether actions can be applied to a particular document.
mod error;
mod field;
mod model;
mod parser;
mod state;
#[cfg(target_arch = "wasm32")]
mod wasm;

pub use error::{Error, Result};
pub use field::Field;
pub use model::{Action, ArrayEdit, Decimal, Path, Segment, Value};
use serde::Serialize;
pub use state::State;

/// Decoded semantics and byte-level evidence for one complete message.
#[derive(Debug, Serialize)]
pub struct Inspection {
    pub protocol: u32,
    pub actions: Vec<Action>,
    pub wire: Vec<Field>,
    #[serde(rename = "metadataCount")]
    pub metadata_count: usize,
}

/// Stateful inspector for one ordered stream; failed messages never commit state.
#[derive(Default)]
pub struct Inspector {
    state: State,
}
impl Inspector {
    /// Resume from a previously captured stream checkpoint.
    pub fn new(state: State) -> Self {
        Self { state }
    }
    /// Current protocol and pool checkpoint.
    pub fn state(&self) -> &State {
        &self.state
    }
    /// Decode and annotate a complete message, committing state only on success.
    pub fn inspect(&mut self, bytes: &[u8]) -> Result<Inspection> {
        let mut next = self.state.clone();
        let result = parser::parse(bytes, &mut next)?;
        self.state = next;
        Ok(result)
    }
}

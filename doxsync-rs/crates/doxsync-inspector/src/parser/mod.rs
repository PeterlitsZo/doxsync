mod actions;
mod aoo;
mod cursor;
mod limits;
mod metadata;
mod primitives;
mod references;
mod trace;
mod value;
use trace::annotate;

use crate::{Error, Inspection, Result, State};
use cursor::Cursor;

struct Parser<'a> {
    cursor: Cursor<'a>,
    state: &'a mut State,
    fields: usize,
}

pub(crate) fn parse(bytes: &[u8], state: &mut State) -> Result<Inspection> {
    if bytes.len() > limits::MESSAGE_BYTES {
        return Err(Error::new(0, "Inspector message limit: 16 MiB"));
    }
    let mut parser = Parser {
        cursor: Cursor::new(bytes),
        state,
        fields: 0,
    };
    parser.message()
}

impl<'a> Parser<'a> {
    fn message(&mut self) -> Result<Inspection> {
        let mut wire = vec![];
        let metadata_count = self.metadata(&mut wire)?;
        let protocol = self
            .state
            .protocol
            .ok_or_else(|| self.error("Missing protocol declaration"))?;
        let actions = self.actions(&mut wire)?;
        if self.cursor.pos != self.cursor.bytes.len() {
            return self.fail("Unexpected trailing bytes");
        }
        Ok(Inspection {
            protocol,
            actions,
            wire,
            metadata_count,
        })
    }
    fn error(&self, message: impl Into<String>) -> Error {
        Error::new(self.cursor.pos, message)
    }
    pub(super) fn fail<T>(&self, message: &str) -> Result<T> {
        Err(self.error(message))
    }
    pub(super) fn require_protocol(&self, minimum: u32) -> Result<()> {
        if self.state.protocol.unwrap_or(0) < minimum {
            self.fail(&format!("Requires protocol {minimum}"))
        } else {
            Ok(())
        }
    }
}

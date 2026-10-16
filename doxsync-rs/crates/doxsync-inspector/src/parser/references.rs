use super::limits;
use super::{Parser, Result, annotate};
use crate::Field;
use crate::{Path, Segment};
/// Carries key provenance explicitly; presentation never needs to parse prose.
pub(super) struct DecodedKey {
    pub(super) value: String,
    pub(super) source: KeySource,
}

pub(super) enum KeySource {
    Pool(u64),
    Inline { descriptor: u64, byte_length: u64 },
}

impl DecodedKey {
    pub(super) fn annotation(&self) -> String {
        match self.source {
            KeySource::Pool(slot) => format!("READ strings[{slot}] = {:?}", self.value),
            KeySource::Inline {
                descriptor,
                byte_length,
            } => format!("{descriptor} >> 2 = {byte_length} bytes"),
        }
    }
}

impl Parser<'_> {
    pub(super) fn string(&self, slot: u64) -> Result<String> {
        self.state
            .strings
            .get(&slot)
            .cloned()
            .ok_or_else(|| self.error(format!("Undefined strings[{slot}]")))
    }
    pub(super) fn key(&mut self, out: &mut Vec<Field>) -> Result<DecodedKey> {
        let code = self.var(out, "Map / column key descriptor", 64, false)? as u64;
        let payload = code >> 2;
        match code & 3 {
            0 => {
                let key = self.string(payload)?;
                out.last_mut().unwrap().explanation.push_str(&format!("; low bits 00 = string pool reference; {code} >> 2 = {payload}; READ strings[{payload}] = {key:?}"));
                Ok(DecodedKey {
                    value: key,
                    source: KeySource::Pool(payload),
                })
            }
            1 => {
                out.last_mut().unwrap().explanation.push_str(&format!(
                    "; low bits 01 = inline UTF-8; {code} >> 2 = {payload} bytes"
                ));
                let value = self.text(
                    out,
                    "Inline key bytes",
                    usize::try_from(payload).map_err(|_| self.error("Key length overflow"))?,
                )?;
                Ok(DecodedKey {
                    value,
                    source: KeySource::Inline {
                        descriptor: code,
                        byte_length: payload,
                    },
                })
            }
            _ => self.fail("Invalid map key descriptor"),
        }
    }
    pub(super) fn path(&mut self, out: &mut Vec<Field>) -> Result<Path> {
        let n = self.count(out, "Path segment count", limits::PATH_SEGMENTS)?;
        let mut path = vec![];
        let mut key_bytes = 0;
        for i in 0..n {
            let code = self.var(out, &format!("Segment {i} descriptor"), 64, false)? as u64;
            let payload = code >> 2;
            let (value, meaning) = match code & 3 {
                0 => (
                    Segment::Key(self.text(
                        out,
                        "Literal path key",
                        usize::try_from(payload).map_err(|_| self.error("Path length overflow"))?,
                    )?),
                    format!("low bits 00: inline key, {payload} UTF-8 bytes"),
                ),
                1 => (
                    Segment::Index(payload),
                    format!("low bits 01: array index {payload}"),
                ),
                2 => {
                    let key = self.string(payload)?;
                    (
                        Segment::Key(key.clone()),
                        format!("low bits 10: READ strings[{payload}] = {key:?}"),
                    )
                }
                _ => return self.fail("Invalid path descriptor"),
            };
            // Descriptor may be followed by a literal key leaf.
            let index = out.len() - if code & 3 == 0 { 2 } else { 1 };
            out[index]
                .explanation
                .push_str(&format!("; descriptor >> 2 = {payload}; {meaning}"));
            if let Segment::Key(key) = &value {
                key_bytes += key.len();
            }
            if key_bytes > limits::PATH_KEY_BYTES {
                return self.fail("Path key budget exceeded");
            }
            path.push(value);
        }
        Ok(path)
    }
    pub(super) fn path_ref(&mut self, out: &mut Vec<Field>, label: &str) -> Result<Path> {
        let slot = self.var(out, label, 64, false)? as u64;
        let value = self
            .state
            .paths
            .get(&slot)
            .cloned()
            .ok_or_else(|| self.error(format!("Undefined paths[{slot}]")))?;
        let display = serde_json::json!(value);
        out.last_mut()
            .unwrap()
            .explanation
            .push_str(&format!("; READ paths[{slot}] = {display}"));
        annotate(out, format!("{label}: paths[{slot}] = {display}"), 0);
        Ok(value)
    }
}

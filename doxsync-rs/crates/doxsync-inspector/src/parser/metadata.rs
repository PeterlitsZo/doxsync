use super::limits;
use super::{Parser, Result, annotate};
use crate::Field;
use serde_json::json;
use std::collections::BTreeSet;
impl Parser<'_> {
    pub(super) fn metadata(&mut self, wire: &mut Vec<Field>) -> Result<usize> {
        let metadata_count = self.count(wire, "Metadata count", limits::METADATA)?;
        annotate(wire, format!("{metadata_count} metadatas"), 0);
        let mut seen = BTreeSet::new();
        for i in 0..metadata_count {
            let start = self.cursor.pos;
            let mut fields = vec![];
            let opcode = self.var(&mut fields, "Metadata opcode", 64, false)? as u64;
            if !seen.insert(opcode) {
                return self.fail("Duplicate metadata instruction");
            }
            if self.state.protocol.is_none() && opcode != 2 {
                return self.fail("Protocol must be declared first");
            }
            let name = match opcode {
                0 => "String pool patch",
                1 => "Path pool patch",
                2 => "Declare protocol",
                3 => "Binary pool patch",
                _ => return self.fail("Unknown metadata opcode"),
            };
            fields[0]
                .explanation
                .push_str(&format!("; {opcode} = {name}"));
            if opcode == 2 {
                if self.state.protocol.is_some() {
                    return self.fail("Protocol already declared");
                }
                let version = self.var(&mut fields, "Protocol version", 64, false)?;
                if !(1..=4).contains(&version) {
                    return self.fail("Unsupported protocol");
                }
                self.state.protocol = Some(version as u32);
            } else {
                self.pool_patch(&mut fields, opcode)?;
            }
            let annotation = if opcode == 2 {
                format!("Use protocol {}", self.state.protocol.unwrap())
            } else {
                format!(
                    "Apply {} pool patch, {} entries",
                    match opcode {
                        0 => "string",
                        1 => "path",
                        _ => "binary",
                    },
                    fields.len() - 2
                )
            };
            self.group(
                wire,
                start,
                format!("Metadata {} · {name}", i + 1),
                "Applied before actions".into(),
                fields,
            )?;
            annotate(wire, annotation, 2);
        }
        Ok(metadata_count)
    }
    fn pool_patch(&mut self, fields: &mut Vec<Field>, opcode: u64) -> Result<()> {
        if opcode == 3 {
            self.require_protocol(2)?;
        }
        let limit = if opcode == 3 {
            limits::BINARY_SLOTS
        } else {
            limits::STRING_PATH_SLOTS
        };
        let count = self.count(fields, "Pool entry count", limit)?;
        let mut slots = BTreeSet::new();
        let patch_start = self.cursor.pos;
        let mut path_bytes = 0;
        for n in 0..count {
            let entry_start = self.cursor.pos;
            let mut entry = vec![];
            let slot = self.var(&mut entry, "Destination slot", 64, false)? as u64;
            if slot >= limit as u64 || !slots.insert(slot) {
                return self.fail("Invalid or duplicate pool slot");
            }
            let (pool, value, previous) = match opcode {
                0 => {
                    let len =
                        self.count(&mut entry, "UTF-8 byte length", self.cursor.bytes.len())?;
                    let value = self.text(&mut entry, "String bytes", len)?;
                    let old = self.state.strings.insert(slot, value.clone());
                    ("strings", json!(value), json!(old))
                }
                1 => {
                    let path_start = self.cursor.pos;
                    let value = self.path(&mut entry)?;
                    path_bytes += self.cursor.pos - path_start;
                    if path_bytes > limits::PATCH_BYTES {
                        return self.fail("Path patch budget exceeded");
                    }
                    let old = self.state.paths.insert(slot, value.clone());
                    ("paths", json!(value), json!(old))
                }
                _ => {
                    let len = self.count(&mut entry, "Binary byte length", limits::BINARY_BYTES)?;
                    let start = self.cursor.pos;
                    if self.cursor.pos - patch_start + len > limits::PATCH_BYTES {
                        return self.fail("Binary patch budget exceeded");
                    }
                    let value = self.cursor.take(len)?.to_vec();
                    self.leaf(
                        &mut entry,
                        start,
                        "Binary bytes",
                        format!("{len} literal bytes"),
                    )?;
                    let old = self.state.bytes.insert(slot, value.clone());
                    ("bytes", json!(value), json!(old))
                }
            };
            let prefix = entry.len();
            self.group(
                fields,
                entry_start,
                format!("Entry {n} · WRITE {pool}[{slot}]"),
                format!("Previous: {previous}; new: {value}"),
                entry,
            )?;
            annotate(
                fields,
                format!(
                    "{pool}[{slot}] = {value}{}",
                    if previous.is_null() {
                        String::new()
                    } else {
                        format!(" (was {previous})")
                    }
                ),
                prefix,
            );
        }
        Ok(())
    }
}

use super::limits;
use super::{Parser, Result, annotate};
use crate::Field;
use crate::Value;
use std::collections::BTreeMap;
impl<'a> Parser<'a> {
    pub(super) fn value(
        &mut self,
        out: &mut Vec<Field>,
        label: &str,
        depth: usize,
    ) -> Result<Value> {
        if depth > limits::NESTING {
            return self.fail("Inspector nesting limit: 128");
        }
        let start = self.cursor.pos;
        let mut f = vec![];
        let header = self.byte(&mut f, "Value header")?;
        let tag = header >> 4;
        let low = header & 15;
        f[0].explanation = format!(
            "0x{header:02x}: high nibble 0x{tag:x} selects type; low nibble 0x{low:x} is its payload"
        );
        let mut reference = None;
        let (kind, value) = match tag {
            0 | 1 => {
                let n = self.payload(&mut f, low, "integer payload")?;
                let value = if tag == 0 { n as i128 } else { -1 - n as i128 };
                f[0].explanation.push_str(if tag == 0 {
                    "; positive integer"
                } else {
                    "; negative integer = -1 - payload"
                });
                ("Integer", Value::Integer(value))
            }
            2 | 3 => {
                let n = self.payload(&mut f, low, "byte length")?;
                let n = usize::try_from(n).map_err(|_| self.error("Byte length overflow"))?;
                if tag == 3 {
                    (
                        "UTF-8 string",
                        Value::String(self.text(&mut f, "UTF-8 bytes", n)?),
                    )
                } else {
                    let start = self.cursor.pos;
                    let bytes = self.cursor.take(n)?.to_vec();
                    self.leaf(&mut f, start, "Binary bytes", format!("{n} literal bytes"))?;
                    ("Binary", Value::Bytes(bytes))
                }
            }
            4 | 5 => {
                let n = self.payload(
                    &mut f,
                    low,
                    if tag == 4 {
                        "element count"
                    } else {
                        "entry count"
                    },
                )?;
                if n > (self.cursor.bytes.len() - self.cursor.pos) as u64 {
                    return self.fail("Truncated container");
                }
                if tag == 4 {
                    let mut values = vec![];
                    for i in 0..n {
                        values.push(self.value(&mut f, &format!("Element {i}"), depth + 1)?);
                    }
                    ("Array", Value::Array(values))
                } else {
                    let mut entries = BTreeMap::new();
                    for i in 0..n {
                        let s = self.cursor.pos;
                        let mut ef = vec![];
                        let decoded_key = self.key(&mut ef)?;
                        let key_comment = decoded_key.annotation();
                        let key = decoded_key.value;
                        let v = self.value(&mut ef, "Value", depth + 1)?;
                        if entries.insert(key.clone(), v).is_some() {
                            return self.fail("Duplicate map key");
                        }
                        let prefix = ef.len() - 1;
                        self.group(
                            &mut f,
                            s,
                            format!("Entry {i} · {key}"),
                            "Key descriptor followed by value".into(),
                            ef,
                        )?;
                        annotate(&mut f, format!("Key {key:?}; {key_comment}"), prefix);
                    }
                    ("Map", Value::Map(entries))
                }
            }
            7 => {
                let value = match low {
                    4 => Value::Boolean(false),
                    5 => Value::Boolean(true),
                    6 => Value::Null(()),
                    9..=11 => {
                        if low < 11 {
                            self.require_protocol(2)?;
                        }
                        let width = 1usize << (low - 8);
                        let s = self.cursor.pos;
                        let bytes = self.cursor.take(width)?;
                        let value = match width {
                            2 => {
                                half::f16::from_bits(u16::from_le_bytes(bytes.try_into().unwrap()))
                                    .to_f64()
                            }
                            4 => f32::from_le_bytes(bytes.try_into().unwrap()) as f64,
                            _ => f64::from_le_bytes(bytes.try_into().unwrap()),
                        };
                        self.leaf(
                            &mut f,
                            s,
                            "IEEE 754 payload",
                            format!("Little-endian binary{} = {value}", width * 8),
                        )?;
                        Value::Number(value)
                    }
                    _ => return self.fail("Unknown simple/float payload"),
                };
                f[0].explanation.push_str(&format!(
                    "; simple/float code {low}: {}",
                    value.display_scalar()
                ));
                ("Simple / float", value)
            }
            8 | 11 => {
                if tag == 11 {
                    self.require_protocol(2)?;
                    if low > 12 {
                        return self.fail("Invalid binary reference width");
                    }
                }
                let slot = self.payload(&mut f, low, "pool slot")?;
                let (name, value) = if tag == 8 {
                    ("strings", Value::String(self.string(slot)?))
                } else {
                    (
                        "bytes",
                        Value::Bytes(
                            self.state
                                .bytes
                                .get(&slot)
                                .ok_or_else(|| self.error("Undefined binary slot"))?
                                .clone(),
                        ),
                    )
                };
                f.last_mut().unwrap().explanation.push_str(&format!(
                    "; READ {name}[{slot}] = {}",
                    value.display_scalar()
                ));
                reference = Some(format!("{name}[{slot}] -> {}", value.display_scalar()));
                ("Pool reference", value)
            }
            9 | 10 => {
                self.require_protocol(2)?;
                let scale = if low == 15 {
                    let s = self.byte(&mut f, "Extended decimal scale")?;
                    if !(15..=28).contains(&s) {
                        return self.fail("Invalid decimal scale");
                    }
                    s
                } else {
                    low
                };
                let coefficient = self.var(&mut f, "Unsigned decimal coefficient", 96, true)?;
                if tag == 10 && coefficient == 0 {
                    return self.fail("Negative decimal zero");
                }
                let signed = if tag == 10 {
                    -(coefficient as i128)
                } else {
                    coefficient as i128
                };
                f[0].explanation.push_str(&format!(
                    "; {} decimal, scale {scale}; value = {signed} × 10^-{scale}",
                    if tag == 10 { "negative" } else { "positive" }
                ));
                ("Decimal", Value::decimal(signed, scale))
            }
            12 => {
                self.require_protocol(4)?;
                if low != 0 {
                    return self.fail("Reserved AOO header bits");
                }
                f[0].explanation = "0xc0: high nibble 1100 = array-of-objects (AOO); low nibble 0000 is reserved. Columns follow instead of separate row maps.".into();
                ("AOO · array-of-objects", self.aoo(&mut f, depth + 1)?)
            }
            _ => return self.fail("Unknown value tag"),
        };
        let (comment, prefix) = match (&value, tag) {
            (Value::Array(items), 4) => (
                format!("Array, {} items", items.len()),
                if low <= 11 { 1 } else { 2 },
            ),
            (Value::Map(entries), 5) => (
                format!("Map, {} entries", entries.len()),
                if low <= 11 { 1 } else { 2 },
            ),
            (Value::Array(items), 12) => (
                format!(
                    "AOO (c0), {} rows ({}), {} columns ({})",
                    items.len(),
                    f[1].hex,
                    f.len() - 3,
                    f[2].hex
                ),
                3,
            ),
            (Value::Integer(number), _) => (
                format!(
                    "{number}n{}",
                    if tag == 1 {
                        " (negative integer: -1 - payload)"
                    } else {
                        ""
                    }
                ),
                f.len(),
            ),
            (_, 8 | 11) => (reference.expect("pool reference annotation"), f.len()),
            (Value::Decimal(_), _) => (
                format!(
                    "Decimal({}); scale {}, unsigned LEB128 coefficient",
                    value.display_scalar(),
                    if low == 15 {
                        self.cursor.bytes[start + 1]
                    } else {
                        low
                    }
                ),
                f.len(),
            ),
            (Value::Number(_), _) => (
                format!(
                    "{} (IEEE 754 binary{}, little-endian)",
                    value.display_scalar(),
                    1usize << (low - 5)
                ),
                f.len(),
            ),
            _ => (format!("{kind}: {}", value.display_scalar()), f.len()),
        };
        self.group(out, start, format!("{label} · {kind}"), if tag == 12 {"Column-oriented encoding: header, row count, column count, then each column's key, mode, and data".into()} else {format!("Decoded type: {kind}")}, f)?;
        annotate(out, comment, prefix);
        Ok(value)
    }
}

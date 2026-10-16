use super::limits;
use super::{Parser, Result, annotate};
use crate::Field;
use crate::Value;
use std::collections::BTreeMap;
impl<'a> Parser<'a> {
    pub(super) fn aoo(&mut self, out: &mut Vec<Field>, depth: usize) -> Result<Value> {
        let rows = self.count(out, "AOO row count", limits::AOO_ROWS)?;
        let columns = self.count(out, "AOO column count", limits::AOO_COLUMNS)?;
        if rows == 0
            || rows * columns > limits::AOO_CELLS
            || (columns == 0 && rows > limits::AOO_EMPTY_ROWS)
        {
            return self.fail("Invalid AOO dimensions");
        }
        let mut values = vec![BTreeMap::new(); rows];
        let mut previous_key: Option<String> = None;
        let mut entries = 0;
        for c in 0..columns {
            let start = self.cursor.pos;
            let mut f = vec![];
            let decoded_key = self.key(&mut f)?;
            let key_comment = decoded_key.annotation();
            let key = decoded_key.value;
            if previous_key.as_ref().is_some_and(|prev| prev >= &key) {
                return self.fail("AOO keys must be strictly increasing");
            }
            previous_key = Some(key.clone());
            let mode = self.byte(&mut f, "Column mode")?;
            let (mode_name, description) = match mode {
                0 => ("DENSE_VALUES", "one tagged value per row"),
                1 => (
                    "SPARSE_VALUES",
                    "presence bitmap, then tagged values for present rows",
                ),
                2 => (
                    "DENSE_INT_DELTA",
                    "first tagged integer, then ZigZag LEB128 deltas",
                ),
                3 => (
                    "DENSE_DECIMAL_DELTA",
                    "shared scale, signed coefficient, then coefficient deltas",
                ),
                _ => return self.fail("Unknown AOO mode"),
            };
            let name = format!("{mode_name}: {description}");
            f.last_mut().unwrap().explanation = format!("{mode:02x} = {name}");
            let prefix = f.len();
            entries += if mode <= 1 {
                self.value_column(&mut f, &key, &mut values, mode == 1, depth)?
            } else {
                self.delta_column(&mut f, &key, &mut values, mode == 3, depth)?
            };
            if entries > limits::AOO_ENTRIES {
                return self.fail("AOO entry limit exceeded");
            }
            self.group(out, start, format!("Column {c} · {key}"), name, f)?;
            annotate(
                out,
                format!("Column {key:?}; {key_comment}; {}", mode_name),
                prefix,
            );
        }
        Ok(Value::Array(values.into_iter().map(Value::Map).collect()))
    }

    fn value_column(
        &mut self,
        f: &mut Vec<Field>,
        key: &str,
        values: &mut [BTreeMap<String, Value>],
        sparse: bool,
        depth: usize,
    ) -> Result<usize> {
        let rows = values.len();
        let bitmap = if sparse {
            let start = self.cursor.pos;
            let bitmap = self.cursor.take(rows.div_ceil(8))?.to_vec();
            if !rows.is_multiple_of(8) && bitmap.last().unwrap() >> (rows % 8) != 0 {
                return self.fail("Nonzero unused bitmap bits");
            }
            let presence = (0..rows)
                .map(|i| {
                    format!(
                        "row {i}: {}",
                        if bitmap[i / 8] & (1 << (i % 8)) != 0 {
                            "present"
                        } else {
                            "absent (not null)"
                        }
                    )
                })
                .collect::<Vec<_>>()
                .join("; ");
            self.leaf(
                f,
                start,
                "Presence bitmap",
                format!("LSB first: bit (row % 8) in byte (row / 8); {presence}"),
            )?;
            Some(bitmap)
        } else {
            None
        };
        let mut count = 0;
        for (i, row) in values.iter_mut().enumerate() {
            if bitmap
                .as_ref()
                .is_none_or(|b| b[i / 8] & (1 << (i % 8)) != 0)
            {
                row.insert(
                    key.to_owned(),
                    self.value(f, &format!("Row {i} · {key}"), depth)?,
                );
                count += 1;
            }
        }
        if count == 0 {
            return self.fail("Empty AOO column");
        }
        Ok(count)
    }

    fn delta_column(
        &mut self,
        f: &mut Vec<Field>,
        key: &str,
        values: &mut [BTreeMap<String, Value>],
        decimal: bool,
        depth: usize,
    ) -> Result<usize> {
        let scale = if decimal {
            let scale = self.byte(f, "Shared decimal scale")?;
            if scale > 28 {
                return self.fail("Invalid decimal scale");
            }
            Some(scale)
        } else {
            None
        };
        let mut prev = if !decimal {
            let value = self.value(f, "Row 0 · first integer", depth)?;
            match value {
                Value::Integer(value) => value,
                _ => return self.fail("Invalid first integer"),
            }
        } else {
            self.signed(f, "Row 0 · signed coefficient", 97)?
        };
        for (i, row) in values.iter_mut().enumerate() {
            if i > 0 {
                let delta = self.signed(
                    f,
                    &format!("Row {i} · delta"),
                    if !decimal { 64 } else { 98 },
                )?;
                let next = prev
                    .checked_add(delta)
                    .ok_or_else(|| self.error("Delta overflow"))?;
                f.last_mut()
                    .unwrap()
                    .explanation
                    .push_str(&format!("; previous {prev} + delta {delta} = {next}"));
                annotate(
                    f,
                    format!("Row {i}: delta {delta} (ZigZag); {prev} + {delta} = {next}"),
                    0,
                );
                prev = next;
            }
            let value = if let Some(scale) = scale {
                if prev.unsigned_abs() >= 1u128 << 96 {
                    return self.fail("Decimal coefficient overflow");
                }
                f.last_mut()
                    .unwrap()
                    .explanation
                    .push_str(&format!("; row value = {prev} × 10^-{scale}"));
                Value::decimal(prev, scale)
            } else {
                if !(-(1i128 << 64)..(1i128 << 64)).contains(&prev) {
                    return self.fail("Integer overflow");
                }
                Value::Integer(prev)
            };
            row.insert(key.to_owned(), value);
        }
        Ok(values.len())
    }
}

use super::{Parser, Result, annotate};
use crate::Field;
impl<'a> Parser<'a> {
    pub(super) fn byte(&mut self, out: &mut Vec<Field>, label: &str) -> Result<u8> {
        let start = self.cursor.pos;
        let value = self.cursor.take(1)?[0];
        self.leaf(out, start, label, format!("Unsigned byte = {value}"))?;
        annotate(out, format!("{label} = {value}"), 0);
        Ok(value)
    }
    pub(super) fn var(
        &mut self,
        out: &mut Vec<Field>,
        label: &str,
        bits: usize,
        minimal: bool,
    ) -> Result<u128> {
        let start = self.cursor.pos;
        let mut value = 0u128;
        let mut parts = vec![];
        // General u64 varuints permit the core decoder's overlong terminal zero.
        let groups = if bits == 64 && !minimal {
            11
        } else {
            bits.div_ceil(7)
        };
        for i in 0..groups {
            let byte = self.cursor.take(1)?[0];
            let low = byte & 127;
            if (i * 7 >= bits && low != 0)
                || (i * 7 < bits && i * 7 + 7 > bits && low as u128 >= (1u128 << (bits - i * 7)))
            {
                return self.fail("LEB128 overflow");
            }
            value |= (low as u128) << (i * 7);
            parts.push(format!(
                "{byte:02x}: payload {low} × 2^{}, {}",
                i * 7,
                if byte & 128 != 0 { "continue" } else { "stop" }
            ));
            if byte & 128 == 0 {
                if minimal && i > 0 && low == 0 {
                    return self.fail("Non-minimal LEB128");
                }
                self.leaf(
                    out,
                    start,
                    label,
                    format!("Unsigned LEB128 = {value}; {}", parts.join("; ")),
                )?;
                annotate(out, format!("{label} = {value} (LEB128)"), 0);
                return Ok(value);
            }
        }
        self.fail("LEB128 overflow")
    }
    pub(super) fn count(
        &mut self,
        out: &mut Vec<Field>,
        label: &str,
        limit: usize,
    ) -> Result<usize> {
        let n = self.var(out, label, 64, false)?;
        if n > limit as u128 {
            return self.fail(&format!("{label} exceeds {limit}"));
        }
        Ok(n as usize)
    }
    pub(super) fn text(&mut self, out: &mut Vec<Field>, label: &str, len: usize) -> Result<String> {
        let start = self.cursor.pos;
        let value = std::str::from_utf8(self.cursor.take(len)?)
            .map_err(|_| crate::Error::new(start, "Invalid UTF-8"))?
            .to_owned();
        self.leaf(
            out,
            start,
            label,
            format!("UTF-8 = {value:?} ({len} bytes)"),
        )?;
        Ok(value)
    }
    pub(super) fn payload(&mut self, out: &mut Vec<Field>, nibble: u8, label: &str) -> Result<u64> {
        if nibble <= 11 {
            out.last_mut()
                .unwrap()
                .explanation
                .push_str(&format!("; inline {label} = {nibble}"));
            return Ok(nibble as u64);
        }
        let width = 1usize << (nibble - 12);
        out.last_mut().unwrap().explanation.push_str(&format!(
            "; low nibble {nibble} selects a {width}-byte little-endian {label}"
        ));
        let start = self.cursor.pos;
        let bytes = self.cursor.take(width)?;
        let mut buffer = [0u8; 8];
        buffer[..width].copy_from_slice(bytes);
        let value = u64::from_le_bytes(buffer);
        self.leaf(
            out,
            start,
            label,
            format!("Little-endian u{} = {value}", width * 8),
        )?;
        Ok(value)
    }
    pub(super) fn signed(
        &mut self,
        out: &mut Vec<Field>,
        label: &str,
        bits: usize,
    ) -> Result<i128> {
        let n = self.var(out, label, bits, true)?;
        let value = (n >> 1) as i128 ^ -((n & 1) as i128);
        out.last_mut()
            .unwrap()
            .explanation
            .push_str(&format!("; ZigZag ({n} >> 1) XOR -({n} & 1) = {value}"));
        annotate(out, format!("{label}: ZigZag {n} -> {value}"), 0);
        Ok(value)
    }
}

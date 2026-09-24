//! Protocol 2 decimals: sign in the tag, scale in the header, unsigned LEB128 coefficient.

use super::consts::{PAYLOAD_MASK, TAG_NEGDECIMAL, TAG_POSDECIMAL, TAG_WIDTH};
use crate::{Decimal, Error, ErrorKind, Result};

const EXTENDED_SCALE: u8 = 15;
const MAX_COEFFICIENT_BYTES: usize = 14;

fn coefficient_len(coefficient: u128) -> usize {
    ((128 - coefficient.leading_zeros()) as usize)
        .max(1)
        .div_ceil(7)
}

pub(crate) fn encoded_len(value: Decimal) -> usize {
    1 + usize::from(value.scale() >= u32::from(EXTENDED_SCALE))
        + coefficient_len(value.mantissa().unsigned_abs())
}

pub(crate) fn encode(bytes: &mut Vec<u8>, value: Decimal) {
    let tag = if value.is_sign_negative() {
        TAG_NEGDECIMAL
    } else {
        TAG_POSDECIMAL
    };
    let scale = value.scale() as u8;
    bytes.push((tag << TAG_WIDTH) | scale.min(EXTENDED_SCALE));
    if scale >= EXTENDED_SCALE {
        bytes.push(scale);
    }
    let mut coefficient = value.mantissa().unsigned_abs();
    let len = coefficient_len(coefficient);
    for index in 0..len {
        let byte = (coefficient & 0x7f) as u8;
        bytes.push(byte | if index + 1 < len { 0x80 } else { 0 });
        coefficient >>= 7;
    }
}

pub(crate) fn decode(bytes: &mut &[u8]) -> Result<Decimal> {
    let header = take_byte(bytes)?;
    let negative = match header >> TAG_WIDTH {
        TAG_POSDECIMAL => false,
        TAG_NEGDECIMAL => true,
        _ => return Err(invalid("invalid decimal tag")),
    };
    let mut scale = header & PAYLOAD_MASK;
    if scale == EXTENDED_SCALE {
        scale = take_byte(bytes)?;
        if !(u32::from(EXTENDED_SCALE)..=Decimal::MAX_SCALE).contains(&u32::from(scale)) {
            return Err(invalid("invalid extended decimal scale"));
        }
    }

    let coefficient = decode_coefficient(bytes)?;
    if negative && coefficient == 0 {
        return Err(invalid("negative decimal zero"));
    }
    Ok(Decimal::from_parts(
        coefficient as u32,
        (coefficient >> 32) as u32,
        (coefficient >> 64) as u32,
        negative,
        u32::from(scale),
    ))
}

fn decode_coefficient(bytes: &mut &[u8]) -> Result<u128> {
    let mut coefficient = 0u128;
    for index in 0..MAX_COEFFICIENT_BYTES {
        let byte = take_byte(bytes)?;
        let payload = byte & 0x7f;
        // Only five bits remain in the last group of a 96-bit coefficient.
        if index == MAX_COEFFICIENT_BYTES - 1 && byte > 0x1f {
            return Err(invalid("decimal coefficient overflow"));
        }
        coefficient |= u128::from(payload) << (index * 7);
        if byte & 0x80 == 0 {
            if index > 0 && payload == 0 {
                return Err(invalid("non-minimal decimal coefficient"));
            }
            return Ok(coefficient);
        }
    }
    Err(invalid("decimal coefficient overflow"))
}

fn take_byte(bytes: &mut &[u8]) -> Result<u8> {
    let (&byte, rest) = bytes
        .split_first()
        .ok_or_else(|| invalid("unexpected end of decimal"))?;
    *bytes = rest;
    Ok(byte)
}

fn invalid(message: &'static str) -> Error {
    Error::new(ErrorKind::InvalidData, message)
}

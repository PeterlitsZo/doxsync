pub(super) const DEFAULT_ACTIONS_LIMIT: usize = 65535;

pub(super) const TAG_POSINT: u8 = 0b0000;
pub(super) const TAG_NEGINT: u8 = 0b0001;
pub(super) const TAG_BSTR: u8 = 0b0010;
pub(super) const TAG_TSTR: u8 = 0b0011;
pub(super) const TAG_ARRAY: u8 = 0b0100;
pub(super) const TAG_MAP: u8 = 0b0101;
pub(super) const TAG_FLOAT: u8 = 0b0111;

pub(super) const TAG_WIDTH: usize = 4;
pub(super) const PAYLOAD_MASK: u8 = 0x0F;

pub(super) const ACTION_SNAPSHOT: u8 = 0;
pub(super) const ACTION_ADD: u8 = 1;
pub(super) const ACTION_DELETE: u8 = 2;
pub(super) const ACTION_COPY: u8 = 3;

pub(super) mod posint {
    pub(in crate::message::packed) const INLINE: u8 = 11;
    pub(in crate::message::packed) const BITS_8: u8 = 12;
    pub(in crate::message::packed) const BITS_16: u8 = 13;
    pub(in crate::message::packed) const BITS_32: u8 = 14;
    pub(in crate::message::packed) const BITS_64: u8 = 15;
}

pub(super) mod negint {
    pub(in crate::message::packed) const INLINE: u8 = 11;
    pub(in crate::message::packed) const BITS_8: u8 = 12;
    pub(in crate::message::packed) const BITS_16: u8 = 13;
    pub(in crate::message::packed) const BITS_32: u8 = 14;
    pub(in crate::message::packed) const BITS_64: u8 = 15;
}

pub(super) mod bstr {
    pub(in crate::message::packed) const INLINE: u8 = 11;
    pub(in crate::message::packed) const BITS_8: u8 = 12;
    pub(in crate::message::packed) const BITS_16: u8 = 13;
    pub(in crate::message::packed) const BITS_32: u8 = 14;
    pub(in crate::message::packed) const BITS_64: u8 = 15;
}

pub(super) mod tstr {
    pub(in crate::message::packed) const INLINE: u8 = 11;
    pub(in crate::message::packed) const BITS_8: u8 = 12;
    pub(in crate::message::packed) const BITS_16: u8 = 13;
    pub(in crate::message::packed) const BITS_32: u8 = 14;
    pub(in crate::message::packed) const BITS_64: u8 = 15;
}

pub(super) mod array {
    pub(in crate::message::packed) const INLINE: u8 = 11;
    pub(in crate::message::packed) const BITS_8: u8 = 12;
    pub(in crate::message::packed) const BITS_16: u8 = 13;
    pub(in crate::message::packed) const BITS_32: u8 = 14;
    pub(in crate::message::packed) const BITS_64: u8 = 15;
}

pub(super) mod float {
    pub(in crate::message::packed) const FALSE: u8 = 4;
    pub(in crate::message::packed) const TRUE: u8 = 5;
    pub(in crate::message::packed) const NULL: u8 = 6;
    pub(in crate::message::packed) const BITS_64: u8 = 11;
}

pub(super) mod map {
    pub(in crate::message::packed) const INLINE: u8 = 11;
    pub(in crate::message::packed) const BITS_8: u8 = 12;
    pub(in crate::message::packed) const BITS_16: u8 = 13;
    pub(in crate::message::packed) const BITS_32: u8 = 14;
    pub(in crate::message::packed) const BITS_64: u8 = 15;
}

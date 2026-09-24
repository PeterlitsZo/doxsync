pub(crate) const DEFAULT_ACTIONS_LIMIT: usize = 65535;

pub(crate) const TAG_POSINT: u8 = 0b0000;
pub(crate) const TAG_NEGINT: u8 = 0b0001;
pub(crate) const TAG_BSTR: u8 = 0b0010;
pub(crate) const TAG_TSTR: u8 = 0b0011;
pub(crate) const TAG_ARRAY: u8 = 0b0100;
pub(crate) const TAG_MAP: u8 = 0b0101;
pub(crate) const TAG_FLOAT: u8 = 0b0111;
pub(crate) const TAG_TSTR_REF: u8 = 0b1000;
pub(crate) const TAG_POSDECIMAL: u8 = 0b1001;
pub(crate) const TAG_NEGDECIMAL: u8 = 0b1010;

pub(crate) const TAG_WIDTH: usize = 4;
pub(crate) const PAYLOAD_MASK: u8 = 0x0F;

pub(crate) const ACTION_SNAPSHOT: u8 = 0;
pub(crate) const ACTION_ADD: u8 = 1;
pub(crate) const ACTION_DELETE: u8 = 2;
pub(crate) const ACTION_COPY: u8 = 3;
pub(crate) const ACTION_REPLACE: u8 = 4;

pub(crate) mod posint {
    pub(crate) const INLINE: u8 = 11;
    pub(crate) const BITS_8: u8 = 12;
    pub(crate) const BITS_16: u8 = 13;
    pub(crate) const BITS_32: u8 = 14;
    pub(crate) const BITS_64: u8 = 15;
}

pub(crate) mod negint {
    pub(crate) const INLINE: u8 = 11;
    pub(crate) const BITS_8: u8 = 12;
    pub(crate) const BITS_16: u8 = 13;
    pub(crate) const BITS_32: u8 = 14;
    pub(crate) const BITS_64: u8 = 15;
}

pub(crate) mod bstr {
    pub(crate) const INLINE: u8 = 11;
    pub(crate) const BITS_8: u8 = 12;
    pub(crate) const BITS_16: u8 = 13;
    pub(crate) const BITS_32: u8 = 14;
    pub(crate) const BITS_64: u8 = 15;
}

pub(crate) mod tstr {
    pub(crate) const INLINE: u8 = 11;
    pub(crate) const BITS_8: u8 = 12;
    pub(crate) const BITS_16: u8 = 13;
    pub(crate) const BITS_32: u8 = 14;
    pub(crate) const BITS_64: u8 = 15;
}

pub(crate) mod array {
    pub(crate) const INLINE: u8 = 11;
    pub(crate) const BITS_8: u8 = 12;
    pub(crate) const BITS_16: u8 = 13;
    pub(crate) const BITS_32: u8 = 14;
    pub(crate) const BITS_64: u8 = 15;
}

pub(crate) mod float {
    pub(crate) const FALSE: u8 = 4;
    pub(crate) const TRUE: u8 = 5;
    pub(crate) const NULL: u8 = 6;
    pub(crate) const BITS_64: u8 = 11;
}

pub(crate) mod map {
    pub(crate) const INLINE: u8 = 11;
    pub(crate) const BITS_8: u8 = 12;
    pub(crate) const BITS_16: u8 = 13;
    pub(crate) const BITS_32: u8 = 14;
    pub(crate) const BITS_64: u8 = 15;
}

/// Protocol versions implemented by this build.
pub(crate) const SUPPORTED_PROTOCOLS: &[u32] = &[1, 2];

pub(crate) const METADATA_PROTOCOL: u64 = 2;
pub(crate) const METADATA_STRINGS: u64 = 0;
pub(crate) const METADATA_PATHS: u64 = 1;

//! Implementation details used by exported macros.
//!
//! This module is public because macros expanded in downstream crates must be
//! able to reach it. Its contents are not part of doxsync's stable public API.

pub use std::{
    collections::BTreeMap,
    result::Result::{Err, Ok},
    string::String,
    sync::Arc,
    vec::Vec,
};

use crate::{Error, ErrorKind, Result, Value};

/// Converts a supported Rust value into a doxsync [`Value`].
#[doc(hidden)]
pub trait IntoValue {
    fn into_value(self) -> Result<Value>;
}

/// Converts a supported Rust value into a doxsync [`Value`].
#[doc(hidden)]
pub fn into_value<T>(value: T) -> Result<Value>
where
    T: IntoValue,
{
    value.into_value()
}

/// Converts a non-negative literal while giving unsuffixed integer literals a
/// `u128` context.
#[doc(hidden)]
pub trait PositiveLiteralIntoValue {
    fn positive_literal_into_value(self) -> Result<Value>;
}

impl PositiveLiteralIntoValue for &u128 {
    fn positive_literal_into_value(self) -> Result<Value> {
        (*self).into_value()
    }
}

impl<T> PositiveLiteralIntoValue for &&T
where
    T: Copy + IntoValue,
{
    fn positive_literal_into_value(self) -> Result<Value> {
        (**self).into_value()
    }
}

impl PositiveLiteralIntoValue for &&str {
    fn positive_literal_into_value(self) -> Result<Value> {
        Value::tstr(*self)
    }
}

/// Converts a negative literal while giving unsuffixed integer literals an
/// `i128` context.
#[doc(hidden)]
pub trait NegativeLiteralIntoValue {
    fn negative_literal_into_value(self) -> Result<Value>;
}

impl NegativeLiteralIntoValue for &i128 {
    fn negative_literal_into_value(self) -> Result<Value> {
        (*self).into_value()
    }
}

impl<T> NegativeLiteralIntoValue for &&T
where
    T: Copy + IntoValue,
{
    fn negative_literal_into_value(self) -> Result<Value> {
        (**self).into_value()
    }
}

impl IntoValue for Value {
    fn into_value(self) -> Result<Value> {
        Ok(self)
    }
}

impl IntoValue for &Value {
    fn into_value(self) -> Result<Value> {
        Ok(self.clone())
    }
}

impl IntoValue for bool {
    fn into_value(self) -> Result<Value> {
        Value::bool(self)
    }
}

macro_rules! impl_signed_integer {
    ($($ty:ty),* $(,)?) => {
        $(
            impl IntoValue for $ty {
                fn into_value(self) -> Result<Value> {
                    let value = i128::try_from(self).map_err(|_| {
                        Error::new(ErrorKind::InvalidData, "integer value out of range")
                    })?;
                    Value::int(value)
                }
            }
        )*
    };
}

macro_rules! impl_unsigned_integer {
    ($($ty:ty),* $(,)?) => {
        $(
            impl IntoValue for $ty {
                fn into_value(self) -> Result<Value> {
                    let value = i128::try_from(self).map_err(|_| {
                        Error::new(ErrorKind::InvalidData, "integer value out of range")
                    })?;
                    Value::int(value)
                }
            }
        )*
    };
}

impl_signed_integer!(i8, i16, i32, i64, i128, isize);
impl_unsigned_integer!(u8, u16, u32, u64, u128, usize);

impl IntoValue for f32 {
    fn into_value(self) -> Result<Value> {
        Value::float(f64::from(self))
    }
}

impl IntoValue for f64 {
    fn into_value(self) -> Result<Value> {
        Value::float(self)
    }
}

impl IntoValue for String {
    fn into_value(self) -> Result<Value> {
        Value::tstr(self)
    }
}

impl IntoValue for &String {
    fn into_value(self) -> Result<Value> {
        Value::tstr(self.clone())
    }
}

impl IntoValue for &str {
    fn into_value(self) -> Result<Value> {
        Value::tstr(self)
    }
}

impl IntoValue for Vec<u8> {
    fn into_value(self) -> Result<Value> {
        Value::bstr(self)
    }
}

impl IntoValue for &Vec<u8> {
    fn into_value(self) -> Result<Value> {
        Value::bstr(self.clone())
    }
}

impl IntoValue for &[u8] {
    fn into_value(self) -> Result<Value> {
        Value::bstr(self)
    }
}

impl<const N: usize> IntoValue for [u8; N] {
    fn into_value(self) -> Result<Value> {
        Value::bstr(self)
    }
}

impl<const N: usize> IntoValue for &[u8; N] {
    fn into_value(self) -> Result<Value> {
        Value::bstr(self.as_slice())
    }
}

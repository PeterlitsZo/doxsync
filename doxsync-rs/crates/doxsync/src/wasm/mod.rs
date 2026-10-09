//! JS bindings only; synchronization and protocol state live in the core types.

use js_sys::{Array, Function, Reflect, Uint8Array};
use wasm_bindgen::prelude::*;

use crate::{Document, Error, ErrorKind, PackedMessage};

mod value;

#[wasm_bindgen(js_name = supportedProtocols)]
pub fn supported_protocols() -> Array {
    crate::supported_protocols()
        .iter()
        .map(|&version| JsValue::from(version))
        .collect()
}

fn js_error(error: Error) -> JsValue {
    let result = js_sys::Error::new(&error.to_string());
    let kind = match error.kind() {
        ErrorKind::Internal => "Internal",
        ErrorKind::InvalidData => "InvalidData",
        ErrorKind::UnexpectedType => "UnexpectedType",
    };
    // A fresh, extensible Error always accepts this own property.
    let _ = Reflect::set(&result, &"kind".into(), &kind.into());
    result.into()
}

fn invalid(message: &'static str) -> JsValue {
    js_error(Error::new(ErrorKind::InvalidData, message))
}

fn unexpected(message: &'static str) -> JsValue {
    js_error(Error::new(ErrorKind::UnexpectedType, message))
}

#[wasm_bindgen]
pub struct Producer {
    inner: crate::Producer,
    decimal_to_string: Function,
}

#[wasm_bindgen]
impl Producer {
    #[wasm_bindgen(constructor)]
    pub fn new(
        value: JsValue,
        protocols: JsValue,
        decimal_to_string: Function,
    ) -> Result<Producer, JsValue> {
        if !Array::is_array(&protocols) {
            return Err(invalid("protocols must be an array of u32 integers"));
        }
        let protocols = Array::from(&protocols)
            .iter()
            .map(|value| {
                value
                    .as_f64()
                    .filter(|value| {
                        value.is_finite()
                            && value.fract() == 0.0
                            && *value >= 0.0
                            && *value <= u32::MAX as f64
                    })
                    .map(|value| value as u32)
                    .ok_or_else(|| invalid("protocol versions must be u32 integers"))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let document = Document::new(value::from_js(&value, &decimal_to_string)?);
        Ok(Self {
            inner: crate::Producer::new(document, &protocols).map_err(js_error)?,
            decimal_to_string,
        })
    }

    pub fn replace(&mut self, value: JsValue) -> Result<(), JsValue> {
        // Convert completely before touching the current document or pools.
        let document = Document::new(value::from_js(&value, &self.decimal_to_string)?);
        self.inner.replace(document);
        Ok(())
    }

    #[wasm_bindgen(js_name = produceDiff)]
    pub fn produce_diff(&mut self) -> Result<JsValue, JsValue> {
        match self.inner.produce_diff().map_err(js_error)? {
            Some(message) => Ok(Uint8Array::from(message.bytes()).into()),
            None => Ok(JsValue::UNDEFINED),
        }
    }
}

#[wasm_bindgen]
pub struct Consumer {
    inner: crate::Consumer,
}

#[wasm_bindgen]
impl Consumer {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self {
            inner: crate::Consumer::new(),
        }
    }

    #[wasm_bindgen(js_name = consumeDiff)]
    pub fn consume_diff(&mut self, bytes: JsValue) -> Result<(), JsValue> {
        let bytes = bytes
            .dyn_ref::<Uint8Array>()
            .ok_or_else(|| unexpected("expected a Uint8Array message"))?;
        self.inner
            .consume_diff(PackedMessage::from_bytes(&bytes.to_vec()))
            .map_err(js_error)
    }

    pub fn document(&self, decimal_from_string: Function) -> Result<JsValue, JsValue> {
        match self.inner.document() {
            Some(document) => value::to_js(&document.value(), &decimal_from_string),
            None => Ok(JsValue::UNDEFINED),
        }
    }
}

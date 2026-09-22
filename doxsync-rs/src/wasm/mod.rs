//! JS bindings only; synchronization and protocol state live in the core types.

use js_sys::{Reflect, Uint8Array};
use wasm_bindgen::prelude::*;

use crate::{Document, Error, ErrorKind, PackedMessage};

mod value;

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
}

#[wasm_bindgen]
impl Producer {
    #[wasm_bindgen(constructor)]
    pub fn new(value: JsValue) -> Result<Producer, JsValue> {
        let document = Document::new(value::from_js(&value)?);
        Ok(Self {
            inner: crate::Producer::new(document),
        })
    }

    pub fn replace(&mut self, value: JsValue) -> Result<(), JsValue> {
        // Convert completely before touching the current document or pools.
        let document = Document::new(value::from_js(&value)?);
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

    pub fn document(&self) -> Result<JsValue, JsValue> {
        match self.inner.document() {
            Some(document) => value::to_js(&document.value()),
            None => Ok(JsValue::UNDEFINED),
        }
    }
}

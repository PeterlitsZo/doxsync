use crate::Inspector;
use wasm_bindgen::prelude::*;
#[wasm_bindgen]
pub struct MessageInspector {
    inner: Inspector,
}
#[wasm_bindgen]
impl MessageInspector {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self {
            inner: Inspector::default(),
        }
    }
    pub fn inspect(&mut self, bytes: &[u8]) -> Result<String, JsValue> {
        let result = self
            .inner
            .inspect(bytes)
            .map_err(|error| JsValue::from_str(&error.to_string()))?;
        serde_json::to_string(&result).map_err(|error| JsValue::from_str(&error.to_string()))
    }
}

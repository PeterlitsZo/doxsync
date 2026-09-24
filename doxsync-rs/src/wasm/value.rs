use std::{collections::BTreeMap, sync::Arc};

use js_sys::{Array, Function, JsString, Object, Reflect, Set, Uint8Array};
use wasm_bindgen::{JsCast, JsValue};

use super::{invalid, js_error, unexpected};
use crate::{Decimal, Value, ValueInner};

fn text(value: &JsValue) -> Result<String, JsValue> {
    let value = value
        .dyn_ref::<JsString>()
        .ok_or_else(|| unexpected("expected a string"))?;
    if !value.is_valid_utf16() {
        return Err(invalid(
            "strings must not contain unpaired UTF-16 surrogates",
        ));
    }
    Ok(String::from(value))
}

fn descriptor(object: &JsValue, key: &JsValue) -> Result<JsValue, JsValue> {
    Reflect::get_own_property_descriptor(object.unchecked_ref::<Object>(), key)
}

fn data_value(descriptor: &JsValue) -> Result<JsValue, JsValue> {
    if descriptor.is_undefined() {
        return Err(invalid(
            "sparse arrays and missing properties are not supported",
        ));
    }
    if !Reflect::has(descriptor, &"value".into())? {
        return Err(unexpected("accessor properties are not supported"));
    }
    Reflect::get(descriptor, &"value".into())
}

pub(super) fn from_js(value: &JsValue, decimal_to_string: &Function) -> Result<Value, JsValue> {
    from_js_inner(value, &Set::new(&JsValue::UNDEFINED), decimal_to_string)
}

fn from_js_inner(
    value: &JsValue,
    ancestors: &Set,
    decimal_to_string: &Function,
) -> Result<Value, JsValue> {
    if value.is_null() {
        return Value::null().map_err(js_error);
    }
    if let Some(value) = value.as_bool() {
        return Value::bool(value).map_err(js_error);
    }
    if let Some(value) = value.as_f64() {
        return Value::float(value).map_err(js_error);
    }
    if value.is_bigint() {
        let value = i128::try_from(value.clone())
            .map_err(|_| invalid("integer outside the range -2^64..=2^64 - 1"))?;
        return Value::int(value).map_err(js_error);
    }
    if value.is_string() {
        return Value::tstr(text(value)?).map_err(js_error);
    }
    if let Some(value) = value.dyn_ref::<Uint8Array>() {
        return Value::bstr(value.to_vec()).map_err(js_error);
    }
    if !value.is_object() {
        return Err(unexpected("unsupported JS value"));
    }
    if ancestors.has(value) {
        return Err(invalid("cyclic documents are not supported"));
    }
    ancestors.add(value);
    let result = container_from_js(value, ancestors, decimal_to_string);
    ancestors.delete(value);
    result
}

fn container_from_js(
    value: &JsValue,
    ancestors: &Set,
    decimal_to_string: &Function,
) -> Result<Value, JsValue> {
    if Array::is_array(value) {
        let array = value.unchecked_ref::<Array>();
        let mut items = Vec::new();
        for index in 0..array.length() {
            let item = data_value(&descriptor(value, &index.to_string().into())?)?;
            items.push(from_js_inner(&item, ancestors, decimal_to_string)?);
        }
        return Value::array(items).map_err(js_error);
    }

    let prototype = Reflect::get_prototype_of(value)?;
    let object_prototype = Object::get_prototype_of(&Object::new());
    if !prototype.is_null() && prototype != object_prototype {
        // Check decimals only for class instances, leaving plain-object property
        // validation intact (including getters and enumerable symbol keys).
        let decimal = decimal_to_string.call1(&JsValue::UNDEFINED, value)?;
        if !decimal.is_undefined() {
            let decimal = Decimal::from_str_exact(&text(&decimal)?)
                .map_err(|_| invalid("decimal outside the supported coefficient or scale range"))?;
            return Value::decimal(decimal).map_err(js_error);
        }
        return Err(unexpected(
            "expected a plain object, array, Uint8Array, or Decimal",
        ));
    }

    let mut entries = BTreeMap::new();
    for key in Reflect::own_keys(value)?.iter() {
        let property = descriptor(value, &key)?;
        if property.is_undefined() {
            return Err(invalid("object changed during conversion"));
        }
        if Reflect::get(&property, &"enumerable".into())?.as_bool() != Some(true) {
            continue;
        }
        let key = Arc::new(text(&key)?);
        let item = from_js_inner(&data_value(&property)?, ancestors, decimal_to_string)?;
        entries.insert(key, item);
    }
    Value::map(entries).map_err(js_error)
}

pub(super) fn to_js(value: &Value, decimal_from_string: &Function) -> Result<JsValue, JsValue> {
    match value.inner() {
        ValueInner::Null => Ok(JsValue::NULL),
        ValueInner::Bool { inner } => Ok(JsValue::from_bool(*inner)),
        ValueInner::PosInt { inner } => Ok(JsValue::from(*inner)),
        ValueInner::NegInt { inner } => Ok(JsValue::from(-(*inner as i128) - 1)),
        ValueInner::Float { inner } => Ok(JsValue::from_f64(*inner)),
        ValueInner::Decimal { inner } => {
            decimal_from_string.call1(&JsValue::UNDEFINED, &inner.to_string().into())
        }
        ValueInner::TStr { inner } => Ok(JsValue::from_str(inner)),
        ValueInner::BStr { inner } => Ok(Uint8Array::from(inner.as_slice()).into()),
        ValueInner::Array { inner } => {
            let array = Array::new();
            for item in inner {
                array.push(&to_js(item, decimal_from_string)?);
            }
            Ok(array.into())
        }
        ValueInner::Map { inner } => {
            let object = Object::new();
            for (key, item) in inner {
                // Define data properties so even "__proto__" remains a map key.
                let property = Object::new();
                Reflect::set(
                    &property,
                    &"value".into(),
                    &to_js(item, decimal_from_string)?,
                )?;
                for flag in ["enumerable", "writable", "configurable"] {
                    Reflect::set(&property, &flag.into(), &JsValue::TRUE)?;
                }
                if !Reflect::define_property(&object, &key.as_str().into(), &property)? {
                    return Err(invalid("could not create document property"));
                }
            }
            Ok(object.into())
        }
    }
}

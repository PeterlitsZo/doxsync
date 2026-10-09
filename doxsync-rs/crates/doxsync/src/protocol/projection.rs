//! Project application data before planning or preparing wire pools.

use std::{borrow::Cow, collections::BTreeMap, sync::Arc};

use super::consts::SUPPORTED_PROTOCOLS;
use crate::producer::PlannedActions;
use crate::{Document, Error, ErrorKind, Message, Result, Value, ValueInner, patch::Action};

/// The sole owner of protocol-dependent value representation rules.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ProtocolProjection {
    protocol: u32,
}

impl ProtocolProjection {
    pub(crate) fn new(protocol: u32) -> Result<Self> {
        if !SUPPORTED_PROTOCOLS.contains(&protocol) {
            return Err(Error::new(
                ErrorKind::InvalidData,
                "unsupported projection protocol",
            ));
        }
        Ok(Self { protocol })
    }

    pub(crate) fn ensure_protocol(self, protocol: u32) -> Result<()> {
        if self.protocol != protocol {
            return Err(Error::new(
                ErrorKind::InvalidData,
                "projection protocol mismatch",
            ));
        }
        Ok(())
    }

    pub(crate) fn document(self, document: Document) -> ProjectedDocument {
        let document = if self.protocol == 1 {
            lower_decimals(&document.value())
                .map(Document::new)
                .unwrap_or(document)
        } else {
            document
        };
        ProjectedDocument {
            document,
            projection: self,
        }
    }

    pub(crate) fn message(self, message: &Message) -> ProjectedMessage<'_> {
        let mut actions = Cow::Borrowed(message.actions());
        if self.protocol == 1 {
            for (index, action) in message.actions().iter().enumerate() {
                if let Some(replacement) = lower_action_decimals(action) {
                    actions.to_mut()[index] = replacement;
                }
            }
        }
        ProjectedMessage {
            actions,
            projection: self,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ProjectedDocument {
    document: Document,
    projection: ProtocolProjection,
}

impl ProjectedDocument {
    pub(crate) fn document(&self) -> &Document {
        &self.document
    }

    pub(crate) fn projection(&self) -> ProtocolProjection {
        self.projection
    }
}

pub(crate) struct ProjectedMessage<'a> {
    actions: Cow<'a, [Action]>,
    projection: ProtocolProjection,
}

impl ProjectedMessage<'_> {
    pub(crate) fn actions(&self) -> &[Action] {
        &self.actions
    }

    pub(crate) fn ensure_protocol(&self, protocol: u32) -> Result<()> {
        self.projection.ensure_protocol(protocol)
    }

    pub(crate) fn into_message(self) -> Message {
        Message::new(self.actions.into_owned())
    }
}

impl ProjectedMessage<'static> {
    /// Only the producer planner can construct PlannedActions. Its values already
    /// originate from projected documents, so no compatibility scan is needed.
    pub(crate) fn from_plan(plan: PlannedActions) -> Self {
        let (actions, projection) = plan.into_parts();
        Self {
            actions: Cow::Owned(actions),
            projection,
        }
    }
}

fn lower_action_decimals(action: &Action) -> Option<Action> {
    match action {
        Action::Snapshot { value } => lower_decimals(value).map(|value| Action::Snapshot { value }),
        Action::Add { path, value } => lower_decimals(value).map(|value| Action::Add {
            path: path.clone(),
            value,
        }),
        Action::Replace { path, value } => lower_decimals(value).map(|value| Action::Replace {
            path: path.clone(),
            value,
        }),
        Action::Delete { .. } | Action::Copy { .. } => None,
    }
}

/// None means the entire subtree can be shared, including its container storage.
fn lower_decimals(value: &Value) -> Option<Value> {
    match value.inner() {
        ValueInner::Decimal { inner } => Some(Value::inner_tstr(inner.to_string())),
        ValueInner::Array { inner } => {
            let mut replacement: Option<Vec<Value>> = None;
            for (index, child) in inner.iter().enumerate() {
                if let Some(child) = lower_decimals(child) {
                    replacement.get_or_insert_with(|| inner.clone())[index] = child;
                }
            }
            replacement.map(Value::inner_array)
        }
        ValueInner::Map { inner } => {
            let mut replacement: Option<BTreeMap<Arc<String>, Value>> = None;
            for (key, child) in inner {
                if let Some(child) = lower_decimals(child) {
                    replacement
                        .get_or_insert_with(|| inner.clone())
                        .insert(key.clone(), child);
                }
            }
            replacement.map(Value::inner_map)
        }
        _ => None,
    }
}

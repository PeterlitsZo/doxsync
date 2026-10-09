//! The trusted boundary between projected documents and planned actions.

use super::{cost::CostSession, differ::Differ};
use crate::{
    Result,
    patch::Action,
    protocol::{ProjectedDocument, ProjectedMessage, ProtocolProjection},
    state::ProducerStateTxn,
};

/// Private fields prevent callers outside this planner from certifying raw actions.
/// Every value-bearing action must come from the supplied projected documents.
pub(crate) struct PlannedActions {
    actions: Vec<Action>,
    projection: ProtocolProjection,
}

impl PlannedActions {
    pub(crate) fn into_parts(self) -> (Vec<Action>, ProtocolProjection) {
        (self.actions, self.projection)
    }
}

pub(super) fn plan(
    old: Option<&ProjectedDocument>,
    new: &ProjectedDocument,
    txn: &mut ProducerStateTxn,
) -> Result<ProjectedMessage<'static>> {
    let projection = new.projection();
    projection.ensure_protocol(txn.protocol())?;
    if let Some(old) = old {
        old.projection().ensure_protocol(txn.protocol())?;
    }
    let mut cost = CostSession::new(txn);
    let actions = match old {
        Some(old) => Differ::new(old, &mut cost).diff(new)?,
        None => {
            let action = Action::Snapshot {
                value: new.document().value(),
            };
            cost.append(&action)?;
            vec![action]
        }
    };
    Ok(ProjectedMessage::from_plan(PlannedActions {
        actions,
        projection,
    }))
}

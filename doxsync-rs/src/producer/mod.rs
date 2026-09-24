use crate::protocol::{
    ProjectedDocument, ProjectedMessage, ProtocolProjection, consts::SUPPORTED_PROTOCOLS,
};
use crate::state::ProducerStateTxn;
use crate::{Document, Error, ErrorKind, Message, PackedMessage, ProducerState, Result};

mod cost;
mod differ;
mod planning;
pub(crate) use planning::PlannedActions;

pub struct Producer {
    state: Option<ProducerState>,
    current_document: ProjectedDocument,
    last_emited_document: Option<ProjectedDocument>,
}

impl Producer {
    /// Selects the highest protocol version supported by both peers.
    /// Empty lists and lists without a common version return `InvalidData`.
    /// Protocol 1 projects decimals to strings before indexing and diffing.
    pub fn new(current_document: Document, protocols: &[u32]) -> Result<Self> {
        let protocol = SUPPORTED_PROTOCOLS
            .iter()
            .copied()
            .filter(|version| protocols.contains(version))
            .max()
            .ok_or_else(|| Error::new(ErrorKind::InvalidData, "no common protocol version"))?;
        let mut state = ProducerState::new();
        state.protocol = protocol;
        state.pending_protocol = Some(protocol);
        Ok(Self {
            state: Some(state),
            current_document: ProtocolProjection::new(protocol)?.document(current_document),
            last_emited_document: None,
        })
    }

    /// Replaces the document, applying the selected protocol's value conversion.
    pub fn replace(&mut self, new_document: Document) {
        self.current_document = self.current_document.projection().document(new_document);
    }

    /// Packs a message.
    ///
    /// The first successfully packed message declares the selected protocol.
    /// Internal state will be updated on success
    pub fn pack_diff(&mut self, diff: Message) -> Result<PackedMessage> {
        self.pack_with(|txn| diff.encode(txn))
    }

    fn pack_with(
        &mut self,
        encode: impl FnOnce(&mut ProducerStateTxn) -> Result<PackedMessage>,
    ) -> Result<PackedMessage> {
        let mut txn = self.state.take().expect("producer state").txn();
        let result = encode(&mut txn);
        self.state = Some(if result.is_ok() {
            txn.commit()
        } else {
            txn.rollback()
        });
        result
    }

    /// Packs an encodable change selected by exact encoded cost. On failure the baseline and pools
    /// remain unchanged, so the same update can be retried.
    pub fn produce_diff(&mut self) -> Result<Option<PackedMessage>> {
        let Some(message) = self.next_message()? else {
            return Ok(None);
        };
        let packed = self.pack_with(|txn| message.encode(txn))?;
        self.last_emited_document = Some(self.current_document.clone());
        Ok(Some(packed))
    }

    /// Produces a structured message and advances the document baseline.
    /// Cost calculation and resource validation leave pools unchanged. Pack and deliver
    /// each returned message before requesting another; retain a clone for retry
    /// if packing fails. Prefer `produce_diff` for atomic baseline advancement.
    /// Under protocol 1 the returned values already contain strings in place of decimals.
    pub fn produce_diff_unpacked(&mut self) -> Result<Option<Message>> {
        let message = self.next_message()?;
        if message.is_some() {
            self.last_emited_document = Some(self.current_document.clone());
        }
        Ok(message.map(ProjectedMessage::into_message))
    }

    fn next_message(&mut self) -> Result<Option<ProjectedMessage<'static>>> {
        if self.last_emited_document.as_ref() == Some(&self.current_document) {
            return Ok(None);
        }
        let mut txn = self.state.take().expect("producer state").txn();
        let result = planning::plan(
            self.last_emited_document.as_ref(),
            &self.current_document,
            &mut txn,
        );
        self.state = Some(txn.rollback());
        result.map(Some)
    }
}

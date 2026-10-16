//! Builds exact byte spans and controls annotated-listing presentation.
use super::Parser;
use super::limits;
use crate::{Field, Result};

impl Parser<'_> {
    pub(super) fn group(
        &mut self,
        out: &mut Vec<Field>,
        start: usize,
        label: String,
        explanation: String,
        children: Vec<Field>,
    ) -> Result<()> {
        self.fields += 1;
        if self.fields > limits::FIELDS {
            return self.fail("Inspector field limit exceeded");
        }
        out.push(Field {
            label,
            offset: start,
            length: self.cursor.pos - start,
            hex: self.cursor.bytes[start..self.cursor.pos]
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<Vec<_>>()
                .join(" "),
            annotation: explanation.clone(),
            prefix: 0,
            explanation,
            children,
        });
        Ok(())
    }
    pub(super) fn leaf(
        &mut self,
        out: &mut Vec<Field>,
        start: usize,
        label: &str,
        explanation: String,
    ) -> Result<()> {
        self.group(out, start, label.into(), explanation, vec![])
    }
}

pub(super) fn annotate(fields: &mut [Field], annotation: String, prefix: usize) {
    let field = fields.last_mut().expect("annotated field");
    field.annotation = annotation;
    field.prefix = prefix.min(field.children.len());
}

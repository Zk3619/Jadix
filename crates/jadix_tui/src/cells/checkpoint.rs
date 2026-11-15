//! Application recovery uses explicit stable keys, never serialized NodeIds.
use super::*;
impl CellSurface {
    pub(crate) fn checkpoint_field(&self, id: NodeId) -> io::Result<Vec<u8>> {
        self.checkpoint_target(id)?;
        self.fields[&id].editor.checkpoint_bytes()
    }
    pub(crate) fn checkpoint_target(&self, id: NodeId) -> io::Result<()> {
        self.live()?;
        let index = self
            .node_indices
            .get(&id)
            .copied()
            .ok_or_else(|| invalid("missing checkpoint field"))?;
        if !matches!(
            self.nodes[index].descriptor.content,
            NodeContent::Input {
                enabled: true,
                password: false,
                ..
            } | NodeContent::Textarea { enabled: true, .. }
        ) || !self.fields.contains_key(&id)
        {
            return Err(invalid(
                "checkpoint requires an enabled non-password editor",
            ));
        }
        Ok(())
    }
    /// Decode and budget every editor before publishing any restored state.
    pub(crate) fn restore_checkpoint_fields(
        &mut self,
        saved: &[(NodeId, String)],
    ) -> io::Result<Vec<PlatformEvent>> {
        self.live()?;
        if self.event_revision_exhausted {
            return Err(invalid("cell event revision exhausted"));
        }
        let mut staged = BTreeMap::new();
        for (id, bytes) in saved {
            self.checkpoint_target(*id)?;
            if !self.event_target(*id) || staged.contains_key(id) {
                return Err(invalid("inactive or duplicate checkpoint field"));
            }
            let field = &self.fields[id];
            let revision = field
                .revision
                .max(self.event_revisions.get(id).copied().unwrap_or(0))
                .checked_add(1)
                .ok_or_else(|| invalid("editor revision exhausted"))?;
            let mut editor = TerminalEditor::new("")?;
            editor.restore_checkpoint(editor.document().revision(), bytes.as_bytes())?;
            editor.set_max_length(field.editor.max_length())?;
            staged.insert(*id, (editor, revision));
        }
        let mut retained = self.editor_bytes;
        for (id, (editor, _)) in &staged {
            retained = retained - self.fields[id].editor.retained_bytes() + editor.retained_bytes();
        }
        if retained > Self::EDITOR_BYTES {
            return Err(invalid("checkpoint aggregate editor budget"));
        }
        let mut events = Vec::new();
        for (id, (editor, revision)) in staged {
            let field = self.fields.get_mut(&id).unwrap();
            field.controlled = editor.document().text().into();
            field.editor = editor;
            field.revision = revision;
            field.top_row = 0;
            field.preferred_column = None;
            field.visual_caret = None;
            self.event_revisions.insert(id, revision);
            self.event_revision_exhausted |= revision == u64::MAX;
            events.push(edit_event(id, field));
            self.search.touch(id);
        }
        self.editor_bytes = retained;
        self.dirty = true;
        Ok(events)
    }
}

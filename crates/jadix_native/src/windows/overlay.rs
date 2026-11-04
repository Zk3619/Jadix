use super::*;

#[derive(Clone, Copy)]
pub(super) struct ActivePopover {
    pub node: NodeId,
    pub window: HWND,
    pub anchor: HWND,
}

impl WindowsHost {
    pub(super) fn portal_parent(&self, mut parent: Option<NodeId>) -> Result<HWND, String> {
        while let Some(identifier) = parent {
            let control = self
                .controls
                .get(&identifier)
                .ok_or("unknown native portal parent")?;
            if matches!(control.descriptor.content, NodeContent::Dialog { .. }) {
                return self
                    .backdrops
                    .get(&identifier)
                    .copied()
                    .ok_or_else(|| "missing native modal backdrop".into());
            }
            parent = control.parent;
        }
        Ok(self.window)
    }

    pub(super) fn sync_popovers(&mut self, focused: Option<NodeId>) -> Result<(), String> {
        if self.popovers.is_empty() && self.context.popovers.borrow().is_empty() {
            return Ok(());
        }
        let metadata = self
            .controls
            .iter()
            .map(|(identifier, control)| {
                (*identifier, (control.parent, &control.descriptor.content))
            })
            .collect();
        let ordered = crate::popover::overlay_order(&metadata)
            .map_err(|_| "invalid native overlay dependencies")?;
        let mut active = Vec::new();
        for identifier in ordered {
            let control = &self.controls[&identifier];
            let NodeContent::Popover {
                anchor, open: true, ..
            } = control.descriptor.content
            else {
                continue;
            };
            if control.shown && control.effective_enabled {
                let parent = self.portal_parent(control.parent)?;
                if unsafe { GetParent(control.window) } != parent
                    && unsafe { SetParent(control.window, parent) }.is_null()
                {
                    return Err(native_error("move native popover portal"));
                }
                active.push(ActivePopover {
                    node: identifier,
                    window: control.window,
                    anchor: self.controls[&anchor].window,
                });
            }
        }
        let previous = std::mem::replace(&mut *self.context.popovers.borrow_mut(), active.clone());
        for panel in previous
            .iter()
            .rev()
            .filter(|panel| !active.iter().any(|current| current.node == panel.node))
        {
            if focused == Some(panel.node)
                && unsafe { IsWindow(panel.anchor) } != 0
                && unsafe { IsWindowVisible(panel.anchor) } != 0
                && unsafe { IsWindowEnabled(panel.anchor) } != 0
            {
                unsafe { SetFocus(panel.anchor) };
            }
        }
        for panel in active {
            self.position(panel.node, None)?;
        }
        Ok(())
    }

    pub(super) fn raise_overlays(&mut self) -> Result<(), String> {
        if let Some(backdrop) = self
            .context
            .modal
            .get()
            .and_then(|identifier| self.backdrops.get(&identifier))
            .copied()
        {
            if unsafe {
                SetWindowPos(
                    backdrop,
                    HWND_TOP,
                    0,
                    0,
                    0,
                    0,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
                )
            } == 0
            {
                return Err(native_error("raise native modal backdrop"));
            }
        }
        let panels = self.context.popovers.borrow().clone();
        for panel in panels {
            if self.controls.contains_key(&panel.node) {
                self.position(panel.node, None)?;
            }
        }
        Ok(())
    }
}

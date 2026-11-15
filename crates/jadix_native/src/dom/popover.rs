use super::*;

#[derive(Clone)]
pub(super) struct ActivePopover {
    id: NodeId,
    element: HtmlElement,
    anchor: HtmlElement,
    revision: Rc<Cell<u64>>,
    placement: PopoverPlacement,
    gap: u16,
    size: [f64; 2],
    close_on_outside_click: bool,
}

pub(super) fn dismiss(
    sink: &EventSink,
    id: NodeId,
    revision: &Cell<u64>,
    backdrop: bool,
) -> Result<(), String> {
    let next = revision
        .get()
        .checked_add(1)
        .ok_or("Popover revision exhausted")?;
    revision.set(next);
    let mut callback = sink.try_borrow_mut().map_err(|_| "native root is busy")?;
    callback(serde_json::json!({"root": id.root.token(), "node": id.token(), "payload": {"kind": "dismiss", "backdrop": backdrop, "revision": next.to_string()}}).to_string())
}

fn eligible(element: &HtmlElement, modal: Option<NodeId>) -> bool {
    let enclosing = element.closest("dialog").ok().flatten();
    enclosing.and_then(|element| element.get_attribute("data-jadix-node"))
        == modal.map(NodeId::token)
}

fn position(container: &HtmlElement, panel: &ActivePopover) -> Result<(), String> {
    let root = container.get_bounding_client_rect();
    let window = container
        .owner_document()
        .and_then(|document| document.default_view())
        .ok_or("missing Popover window")?;
    let width = window
        .inner_width()
        .map_err(error)?
        .as_f64()
        .ok_or("invalid viewport width")?;
    let height = window
        .inner_height()
        .map_err(error)?
        .as_f64()
        .ok_or("invalid viewport height")?;
    let origin = [root.left().max(0.0), root.top().max(0.0)];
    let viewport = [
        (root.right().min(width) - origin[0]).max(0.0),
        (root.bottom().min(height) - origin[1]).max(0.0),
    ];
    for (axis, dimension) in ["width", "height"].into_iter().enumerate() {
        panel
            .element
            .style()
            .set_property(
                dimension,
                &format!(
                    "{}px",
                    panel.size[axis].min((viewport[axis] - 16.0).max(0.0))
                ),
            )
            .map_err(error)?;
    }
    let anchor = panel.anchor.get_bounding_client_rect();
    let bounds = panel.element.get_bounding_client_rect();
    let offset = crate::popover::position(
        Rect {
            x: anchor.left() - origin[0],
            y: anchor.top() - origin[1],
            width: anchor.width(),
            height: anchor.height(),
        },
        [bounds.width(), bounds.height()],
        viewport.map(|value| value as f32),
        panel.placement,
        panel.gap,
    );
    panel
        .element
        .style()
        .set_property("left", &format!("{}px", origin[0] + offset[0]))
        .map_err(error)?;
    panel
        .element
        .style()
        .set_property("top", &format!("{}px", origin[1] + offset[1]))
        .map_err(error)
}

fn hide(document: &Document, panel: &ActivePopover, restore: bool) -> Result<(), String> {
    let focused = document
        .active_element()
        .is_some_and(|active| panel.element.contains(Some(active.as_ref())));
    if panel.element.matches(":popover-open").unwrap_or(false) {
        panel.element.hide_popover().map_err(error)?;
    }
    if restore
        && focused
        && panel.anchor.is_connected()
        && panel.anchor.get_bounding_client_rect().width() > 0.0
    {
        let options = FocusOptions::new();
        options.set_prevent_scroll(true);
        panel.anchor.focus_with_options(&options).map_err(error)?;
    }
    Ok(())
}

fn visible(roots: &[PresentationNode]) -> Vec<&PresentationNode> {
    let mut result = Vec::new();
    let mut pending: Vec<_> = roots.iter().rev().collect();
    while let Some(node) = pending.pop() {
        if matches!(
            node.descriptor.content,
            NodeContent::Dialog { open: false, .. } | NodeContent::Popover { open: false, .. }
        ) {
            continue;
        }
        result.push(node);
        pending.extend(node.children.iter().rev());
    }
    result
}

impl DomHost {
    pub(super) fn install_popover_events(&mut self) -> Result<(), String> {
        for name in ["pointerdown", "keydown", "scroll"] {
            let live = self.live.clone();
            let publishing = self.publishing.clone();
            let states = self.popovers.clone();
            let sink = self.sink.clone();
            let modal = self.active_modal.clone();
            let container = self.container.clone();
            let callback = Closure::wrap(Box::new(move |event: Event| {
                if !live.get() || publishing.get() || event.default_prevented() {
                    return;
                }
                let panels = states.borrow().clone();
                if name == "scroll" {
                    for panel in &panels {
                        let _ = position(&container, panel);
                    }
                    return;
                }
                let Some(panel) = panels
                    .iter()
                    .rev()
                    .find(|panel| eligible(&panel.element, modal.get()))
                else {
                    return;
                };
                let backdrop = name == "pointerdown";
                if backdrop {
                    let target = event
                        .target()
                        .and_then(|target| target.dyn_into::<web_sys::Node>().ok());
                    if target.as_ref().is_some_and(|target| {
                        panel.element.contains(Some(target)) || panel.anchor.contains(Some(target))
                    }) || !panel.close_on_outside_click
                    {
                        return;
                    }
                } else {
                    let Some(key) = event.dyn_ref::<web_sys::KeyboardEvent>() else {
                        return;
                    };
                    let target = event
                        .target()
                        .and_then(|target| target.dyn_into::<web_sys::Node>().ok());
                    if key.key() != "Escape"
                        || !target
                            .as_ref()
                            .is_some_and(|target| container.contains(Some(target)))
                    {
                        return;
                    }
                    event.prevent_default();
                    event.stop_propagation();
                }
                let _ = dismiss(&sink, panel.id, &panel.revision, backdrop);
            }) as Box<dyn FnMut(Event)>);
            let target: web_sys::EventTarget = self.document.clone().into();
            target
                .add_event_listener_with_callback_and_bool(
                    name,
                    callback.as_ref().unchecked_ref(),
                    true,
                )
                .map_err(error)?;
            self.overlay_listeners.push(Listener {
                target,
                name,
                callback,
                capture: true,
            });
        }
        Ok(())
    }

    pub(super) fn retire_popovers(&mut self, roots: &[PresentationNode]) -> Result<(), String> {
        let visible: BTreeSet<_> = visible(roots)
            .into_iter()
            .filter(|node| {
                matches!(
                    node.descriptor.content,
                    NodeContent::Popover { open: true, .. }
                )
            })
            .map(|node| node.id)
            .collect();
        let previous = self.popovers.borrow().clone();
        for panel in previous
            .iter()
            .rev()
            .filter(|panel| !visible.contains(&panel.id))
        {
            hide(&self.document, panel, true)?;
            panel
                .anchor
                .remove_attribute("aria-expanded")
                .map_err(error)?;
            panel
                .anchor
                .remove_attribute("data-jadix-popover-controls")
                .map_err(error)?;
            panel
                .anchor
                .remove_attribute("aria-haspopup")
                .map_err(error)?;
        }
        Ok(())
    }

    pub(super) fn sync_popovers(&mut self, roots: &[PresentationNode]) -> Result<(), String> {
        for anchor in self.popover_anchors.drain(..) {
            for name in [
                "aria-haspopup",
                "aria-expanded",
                "data-jadix-popover-controls",
            ] {
                anchor.remove_attribute(name).map_err(error)?;
            }
        }
        let visible: BTreeSet<_> = visible(roots).into_iter().map(|node| node.id).collect();
        let mut metadata = BTreeMap::new();
        let mut descriptors = BTreeMap::new();
        let mut pending: Vec<_> = roots.iter().map(|node| (node, None)).collect();
        while let Some((node, parent)) = pending.pop() {
            metadata.insert(node.id, (parent, &node.descriptor.content));
            descriptors.insert(node.id, node);
            pending.extend(node.children.iter().map(|child| (child, Some(node.id))));
        }
        let order =
            crate::popover::overlay_order(&metadata).map_err(|_| "invalid Popover anchors")?;
        let mut next = Vec::new();
        let mut anchors = BTreeMap::<NodeId, (Vec<String>, bool)>::new();
        for id in order {
            let NodeContent::Popover {
                anchor,
                placement,
                gap,
                open,
                close_on_outside_click,
                ..
            } = descriptors[&id].descriptor.content
            else {
                continue;
            };
            let view = &self.views[&id];
            let anchor_element = self.views[&anchor].element.clone();
            let html_id = super::node_html_id(id);
            view.element.set_id(&html_id);
            let shown =
                open && visible.contains(&id) && eligible(&view.element, self.active_modal.get());
            let accessibility = anchors.entry(anchor).or_default();
            accessibility.0.push(html_id);
            accessibility.1 |= shown;
            let panel = ActivePopover {
                id,
                element: view.element.clone(),
                anchor: anchor_element,
                revision: view.revision.clone(),
                placement,
                gap,
                close_on_outside_click,
                size: [
                    descriptors[&id].descriptor.frame.width,
                    descriptors[&id].descriptor.frame.height,
                ],
            };
            if !shown {
                hide(&self.document, &panel, true)?;
                continue;
            }
            let already_open = panel.element.matches(":popover-open").map_err(error)?;
            if !already_open {
                panel.element.show_popover().map_err(error)?;
            }
            position(&self.container, &panel)?;
            if !already_open {
                let candidates = panel.element.query_selector_all("button:not([disabled]),input:not([disabled]),textarea:not([disabled]),select:not([disabled]),[tabindex='0']").map_err(error)?;
                let focus = (0..candidates.length())
                    .filter_map(|index| candidates.item(index))
                    .filter_map(|node| node.dyn_into::<HtmlElement>().ok())
                    .find(|element| element.get_bounding_client_rect().width() > 0.0)
                    .unwrap_or_else(|| panel.element.clone());
                let options = FocusOptions::new();
                options.set_prevent_scroll(true);
                focus.focus_with_options(&options).map_err(error)?;
            }
            next.push(panel);
        }
        for (id, (controls, open)) in anchors {
            let anchor = &self.views[&id].element;
            anchor
                .set_attribute("aria-haspopup", "dialog")
                .map_err(error)?;
            anchor
                .set_attribute("data-jadix-popover-controls", &controls.join(" "))
                .map_err(error)?;
            anchor
                .set_attribute("aria-expanded", if open { "true" } else { "false" })
                .map_err(error)?;
            self.popover_anchors.push(anchor.clone());
        }
        let previous: Vec<_> = self
            .popovers
            .borrow()
            .iter()
            .map(|panel| panel.id)
            .collect();
        next.sort_by_key(|panel| {
            previous
                .iter()
                .position(|id| *id == panel.id)
                .unwrap_or(usize::MAX)
        });
        *self.popovers.borrow_mut() = next;
        Ok(())
    }

    pub(super) fn dispose_popovers(&mut self) {
        for panel in self.popovers.borrow().iter().rev() {
            let _ = hide(&self.document, panel, false);
        }
        self.popovers.borrow_mut().clear();
        for anchor in self.popover_anchors.drain(..) {
            for name in [
                "aria-haspopup",
                "aria-expanded",
                "data-jadix-popover-controls",
            ] {
                let _ = anchor.remove_attribute(name);
            }
        }
        self.overlay_listeners.clear();
    }
}

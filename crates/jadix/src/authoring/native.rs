use std::{cell::RefCell, collections::HashMap};

use crate::core::{reactivity::Signal, tree::Element};
use crate::presentation::{hybrid::*, *};

#[cfg(test)]
mod description_tests {
    use super::*;

    #[test]
    fn native_description_precedence_and_clear_preserve_name_content_and_identity() {
        let mut element = Element::new().text("Visible");
        element.set_attr("aria-label", "Name");
        element.set_attr("aria-description", "ARIA help 中文");
        element.set_attr("accesskit_description", "Explicit help 😀");
        let projection = ElementProjection::new(RootId::new());
        let first = projection.snapshot(&mut element).unwrap();
        assert_eq!(
            first.descriptor.accessible_description.as_deref(),
            Some("Explicit help 😀")
        );
        element.attrs.remove("accesskit_description");
        let fallback = projection.snapshot(&mut element).unwrap();
        assert_eq!(
            fallback.descriptor.accessible_description.as_deref(),
            Some("ARIA help 中文")
        );
        element.attrs.remove("aria-description");
        let cleared = projection.snapshot(&mut element).unwrap();
        assert_eq!(cleared.descriptor.accessible_description, None);
        for node in [fallback, cleared] {
            assert_eq!(node.id, first.id);
            assert_eq!(node.descriptor.accessible_name.as_deref(), Some("Name"));
            assert_eq!(node.descriptor.content, NodeContent::Text("Visible".into()));
        }
    }

    #[test]
    fn accessibility_relations_resolve_late_targets_and_clear_removed_or_ambiguous_ids() {
        let mut element = Element::new();
        element.set_attr("aria-activedescendant", "choice");
        element.set_attr("aria-controls", "panel missing");
        let projection = ElementProjection::new(RootId::new());

        let unresolved = projection.snapshot(&mut element).unwrap();
        assert_eq!(
            unresolved
                .descriptor
                .accessibility_relations
                .active_descendant,
            None
        );
        assert!(
            unresolved
                .descriptor
                .accessibility_relations
                .controls
                .is_empty()
        );

        let mut choice = Element::new();
        choice.set_attr("id", "choice");
        let mut panel = Element::new();
        panel.set_attr("id", "panel");
        element.children = vec![choice, panel];
        let resolved = projection.snapshot(&mut element).unwrap();
        assert_eq!(
            resolved
                .descriptor
                .accessibility_relations
                .active_descendant,
            Some(resolved.children[0].id)
        );
        assert_eq!(
            resolved.descriptor.accessibility_relations.controls,
            vec![resolved.children[1].id]
        );

        element.children.remove(0);
        let removed = projection.snapshot(&mut element).unwrap();
        assert_eq!(
            removed.descriptor.accessibility_relations.active_descendant,
            None
        );
        assert_eq!(
            removed.descriptor.accessibility_relations.controls,
            vec![removed.children[0].id]
        );

        let mut duplicate_panel = Element::new();
        duplicate_panel.set_attr("id", "panel");
        element.children.push(duplicate_panel);
        let duplicate = projection.snapshot(&mut element).unwrap();
        assert_eq!(
            duplicate.descriptor.accessibility_relations.controls,
            Vec::<NodeId>::new()
        );
    }
}

#[derive(Clone, Copy)]
enum Control {
    Button,
    Input,
}

#[derive(Clone)]
struct InputBinding {
    field: String,
    store: Signal<String>,
    external: Option<Signal<String>>,
}

#[derive(Clone)]
struct Identity {
    node: NodeId,
    edit_revision: u64,
    draft: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProjectionError {
    Unsupported(&'static str),
    InvalidGeometry,
    InvalidEvent,
    StaleNode,
    Disposed,
}

pub fn mark_button(element: &mut Element) {
    element.authoring_state.set(Control::Button);
}

pub fn mark_input(
    element: &mut Element,
    field: String,
    store: Signal<String>,
    external: Option<Signal<String>>,
) {
    element.authoring_state.set(Control::Input);
    element.authoring_state.set(InputBinding {
        field,
        store,
        external,
    });
}

pub(crate) fn copy_semantics(source: &Element, target: &mut Element) {
    if let Some(control) = source.authoring_state.get::<Control>() {
        target.authoring_state.set(*control);
    }
}

pub struct ElementProjection {
    lifetime: RootLifetime,
    /// Paths recorded by the last successful snapshot. Dispatch then walks only
    /// the target path instead of scanning every element in the authoring tree.
    dispatch_paths: RefCell<HashMap<NodeId, Vec<usize>>>,
}

impl ElementProjection {
    pub fn new(root: RootId) -> Self {
        Self::with_lifetime(RootLifetime::new(root))
    }

    pub fn with_lifetime(lifetime: RootLifetime) -> Self {
        Self {
            lifetime,
            dispatch_paths: RefCell::new(HashMap::new()),
        }
    }

    pub fn dispose(&self) {
        self.lifetime.dispose();
    }

    pub fn snapshot(&self, element: &mut Element) -> Result<PresentationNode, ProjectionError> {
        if self.lifetime.is_disposed() {
            return Err(ProjectionError::Disposed);
        }
        let mut targets = HashMap::new();
        collect_accessibility_targets(element, self.lifetime.id(), 0, &mut targets)?;
        let mut paths = HashMap::new();
        let result = snapshot(
            element,
            self.lifetime.id(),
            0,
            &targets,
            &mut Vec::new(),
            &mut paths,
        );
        if result.is_ok() {
            *self.dispatch_paths.borrow_mut() = paths;
        }
        result
    }

    pub fn geometry(
        &self,
        snapshot: &PresentationNode,
        revision: u64,
    ) -> Result<Vec<HybridGeometry>, ProjectionError> {
        if self.lifetime.is_disposed() {
            return Err(ProjectionError::Disposed);
        }
        let mut output = Vec::new();
        let mut pending = vec![snapshot];
        while let Some(node) = pending.pop() {
            if node.id.root != self.lifetime.id() || !node.descriptor.frame.is_valid() {
                return Err(ProjectionError::InvalidGeometry);
            }
            output.push(HybridGeometry {
                node: node.id,
                revision,
                presentation: Presentation::Native,
                frame: node.descriptor.frame,
                clips: Vec::new(),
                visible: true,
                interactive: matches!(
                    node.descriptor.content,
                    NodeContent::Button { enabled: true, .. }
                        | NodeContent::Input { enabled: true, .. }
                ),
                z_index: 0,
            });
            pending.extend(node.children.iter().rev());
        }
        Ok(output)
    }

    pub fn dispatch(
        &self,
        element: &mut Element,
        event: &PlatformEvent,
    ) -> Result<(), ProjectionError> {
        if self.lifetime.is_disposed() {
            return Err(ProjectionError::Disposed);
        }
        if event.node.root != self.lifetime.id() {
            return Err(ProjectionError::StaleNode);
        }
        if let Some(path) = self.dispatch_paths.borrow().get(&event.node).cloned() {
            if let Some(node) = element_at_path_mut(element, &path) {
                if node
                    .authoring_state
                    .get::<Identity>()
                    .is_some_and(|identity| identity.node == event.node)
                {
                    return dispatch(node, event);
                }
            }
        }
        let mut pending = vec![element];
        while let Some(node) = pending.pop() {
            if node
                .authoring_state
                .get::<Identity>()
                .is_some_and(|identity| identity.node == event.node)
            {
                return dispatch(node, event);
            }
            pending.extend(node.children.iter_mut());
        }
        Err(ProjectionError::StaleNode)
    }
}

fn validate_geometry(element: &Element) -> Result<Rect, ProjectionError> {
    let style = &element.style;
    if style.rotate_deg != 0.0
        || style.scale != [1.0; 2]
        || style.translate != [0.0; 2]
        || style.translate_pct != [0.0; 2]
        || style.transform.is_some()
        || style.opacity != 1.0
        || style.zoom != 1.0
        || style.z_index != 0
        || element.layout_motion.is_some()
        || element.scroll_offset != [0.0; 2]
        || element.sticky_visual_offset != [0.0; 2]
        || style.clip_path != crate::style::ClipPath::None
        || style.clip_path_name.is_some()
        || element.pointer_events_mode() != crate::core::tree::PointerEvents::Auto
        || style.display == taffy::style::Display::None
    {
        return Err(ProjectionError::Unsupported(
            "transform, visibility, motion or scrolling",
        ));
    }
    let [x, y, width, height] = element.layout_bounds;
    let frame = Rect {
        x: x.into(),
        y: y.into(),
        width: width.into(),
        height: height.into(),
    };
    if !frame.is_valid() {
        return Err(ProjectionError::InvalidGeometry);
    }
    Ok(frame)
}

fn caption(element: &Element) -> Result<String, ProjectionError> {
    let mut text = String::new();
    let mut pending = vec![element];
    while let Some(node) = pending.pop() {
        if has_drawing(node) || node.text_spans.is_some() {
            return Err(ProjectionError::Unsupported("structured button caption"));
        }
        if let Some(value) = &node.text {
            text.push_str(value);
        }
        pending.extend(node.children.iter().rev());
    }
    Ok(text)
}

fn has_drawing(element: &Element) -> bool {
    #[cfg(feature = "svg")]
    if element
        .attrs
        .contains_key(crate::primitives::svg::ATTR_SVG_KIND)
    {
        return true;
    }
    element.image_texture.is_some()
        || element
            .attrs
            .contains_key(crate::primitives::canvas::ATTR_CANVAS_PAINT)
        || element.attrs.contains_key(crate::utils::ATTR_ICON_NAME)
}

fn content(element: &Element) -> Result<NodeContent, ProjectionError> {
    let enabled = element
        .get_attr("disabled")
        .is_none_or(|disabled| disabled == "false");
    match element.authoring_state.get::<Control>() {
        Some(Control::Button) => Ok(NodeContent::Button {
            label: caption(element)?,
            enabled,
        }),
        Some(Control::Input) => {
            let sync = element
                .input_paint_sync
                .as_ref()
                .ok_or(ProjectionError::Unsupported("unbound input"))?;
            if sync.multiline
                || !matches!(sync.input_type.as_str(), "text" | "password")
                || sync.clearable
                || sync.password_toggle
                || sync.resizable
            {
                return Err(ProjectionError::Unsupported("input kind or adornments"));
            }
            let binding = element
                .authoring_state
                .get::<InputBinding>()
                .ok_or(ProjectionError::Unsupported("unbound input"))?;
            let identity = element.authoring_state.get::<Identity>();
            let value = identity
                .and_then(|identity| identity.draft.clone())
                .or_else(|| binding.external.and_then(|signal| signal.try_peek()))
                .or_else(|| binding.store.try_peek())
                .ok_or(ProjectionError::StaleNode)?;
            Ok(NodeContent::Input {
                max_length: crate::utils::text_field::input_max_length(&sync.base_id),
                value,
                placeholder: sync.placeholder.clone(),
                enabled: enabled && !sync.disabled,
                password: sync.input_type == "password",
                revision: identity.map_or(0, |identity| identity.edit_revision),
            })
        }
        None => {
            if element.style.overflow.x != taffy::style::Overflow::Visible
                || element.style.overflow.y != taffy::style::Overflow::Visible
            {
                return Err(ProjectionError::Unsupported(
                    "container clipping or scrolling",
                ));
            }
            if element.on_click.is_some()
                || element.on_wheel.is_some()
                || element.on_hover.is_some()
                || element.on_mousedown.is_some()
                || element.on_mouseup.is_some()
                || element.on_context_menu.is_some()
                || element.on_double_click.is_some()
                || element.on_mousemove.is_some()
                || element.on_mouseover.is_some()
                || element.on_mouseout.is_some()
                || element.on_mouseenter.is_some()
                || element.on_mouseleave.is_some()
                || element.on_key_down.is_some()
                || !element.window_key_handlers.is_empty()
                || element.text_edit_key_handler.is_some()
                || element.input_paint_sync.is_some()
                || has_drawing(element)
                || element.text_spans.is_some()
                || element.focusable
                || element.presentation_sync.is_some()
                || element
                    .attrs
                    .contains_key(crate::layout::layout_tree::ATTR_HOISTED_PORTAL)
                || element
                    .attrs
                    .contains_key(crate::layout::layout_tree::ATTR_PORTAL_MOUNT)
            {
                return Err(ProjectionError::Unsupported(
                    "unregistered element capability",
                ));
            }
            if let Some(text) = &element.text {
                if !element.children.is_empty() {
                    return Err(ProjectionError::Unsupported("mixed text content"));
                }
                Ok(NodeContent::Text(text.clone()))
            } else {
                Ok(NodeContent::Container)
            }
        }
    }
}

fn snapshot(
    element: &mut Element,
    root: RootId,
    depth: usize,
    targets: &HashMap<String, Option<NodeId>>,
    path: &mut Vec<usize>,
    paths: &mut HashMap<NodeId, Vec<usize>>,
) -> Result<PresentationNode, ProjectionError> {
    if depth >= 256 {
        return Err(ProjectionError::Unsupported("tree depth"));
    }
    let frame = validate_geometry(element)?;
    let identity = match element.authoring_state.get::<Identity>() {
        Some(identity) if identity.node.root == root => identity.clone(),
        Some(_) => {
            return Err(ProjectionError::Unsupported(
                "element already belongs to another root",
            ));
        }
        _ => Identity {
            node: NodeId::new(root),
            edit_revision: 0,
            draft: None,
        },
    };
    element.authoring_state.set(identity.clone());
    paths.insert(identity.node, path.clone());
    let content = content(element)?;
    let mut children = Vec::new();
    if content.accepts_children() {
        for (index, child) in element.children.iter_mut().enumerate() {
            if child.template_id.as_deref() == Some(crate::layout::layout_tree::PORTAL_LAYER_ID) {
                if !child.children.is_empty() {
                    return Err(ProjectionError::Unsupported("portal composition"));
                }
                continue;
            }
            path.push(index);
            let child_snapshot = snapshot(child, root, depth + 1, targets, path, paths);
            path.pop();
            children.push(child_snapshot?);
        }
    }
    Ok(PresentationNode {
        id: identity.node,
        descriptor: NodeDescriptor {
            content,
            frame,
            accessible_role: None,
            accessible_selected: None,
            accessible_expanded: None,
            accessibility_relations: AccessibilityRelations {
                active_descendant: element
                    .get_attr("aria-activedescendant")
                    .and_then(|id| targets.get(id))
                    .copied()
                    .flatten(),
                controls: element
                    .get_attr("aria-controls")
                    .into_iter()
                    .flat_map(|ids| ids.split_ascii_whitespace())
                    .filter_map(|id| targets.get(id).copied().flatten())
                    .take(64)
                    .collect(),
            },
            accessible_description: element
                .get_attr("accesskit_description")
                .or_else(|| element.get_attr("aria-description"))
                .map(String::from),
            accessible_name: element.get_attr("aria-label").map(String::from),
            style: jadix_core::presentation::style::NodeStyle {
                width: Some(frame.width as f32),
                height: Some(frame.height as f32),
                foreground: element.text_style.color.unwrap_or([0, 0, 0, 255]),
                background: element
                    .style
                    .bg_color
                    .map(|channel| (channel.clamp(0.0, 1.0) * 255.0).round() as u8),
                font_family: element
                    .text_style
                    .font_family
                    .clone()
                    .unwrap_or_else(|| String::from("system-ui")),
                font_size: element.text_style.size.unwrap_or(16.0),
                font_weight: element.text_style.weight.unwrap_or(400),
                ..Default::default()
            },
        },
        children,
    })
}

fn collect_accessibility_targets(
    element: &mut Element,
    root: RootId,
    depth: usize,
    targets: &mut HashMap<String, Option<NodeId>>,
) -> Result<(), ProjectionError> {
    if depth >= 256 {
        return Err(ProjectionError::Unsupported("tree depth"));
    }
    let identity = match element.authoring_state.get::<Identity>() {
        Some(identity) if identity.node.root == root => identity.clone(),
        Some(_) => {
            return Err(ProjectionError::Unsupported(
                "element already belongs to another root",
            ));
        }
        None => Identity {
            node: NodeId::new(root),
            edit_revision: 0,
            draft: None,
        },
    };
    element.authoring_state.set(identity.clone());
    if let Some(id) = element.get_attr("id") {
        targets
            .entry(id.clone())
            .and_modify(|target| *target = None)
            .or_insert(Some(identity.node));
    }
    if content(element)?.accepts_children() {
        for child in &mut element.children {
            if child.template_id.as_deref() != Some(crate::layout::layout_tree::PORTAL_LAYER_ID) {
                collect_accessibility_targets(child, root, depth + 1, targets)?;
            }
        }
    }
    Ok(())
}

fn element_at_path_mut<'a>(
    mut element: &'a mut Element,
    path: &[usize],
) -> Option<&'a mut Element> {
    for &index in path {
        element = element.children.get_mut(index)?;
    }
    Some(element)
}

fn dispatch(element: &mut Element, event: &PlatformEvent) -> Result<(), ProjectionError> {
    validate_geometry(element)?;
    match (content(element)?, &event.payload) {
        (NodeContent::Button { enabled: true, .. }, EventPayload::Activate) => {
            let Some(callback) = element.on_click.as_ref().cloned() else {
                return Ok(());
            };
            let mut callback = callback
                .try_borrow_mut()
                .map_err(|_| ProjectionError::InvalidEvent)?;
            callback();
        }
        (NodeContent::Input { enabled: true, .. }, EventPayload::Submit { revision }) => {
            let identity = element.authoring_state.get::<Identity>().unwrap();
            if *revision <= identity.edit_revision || identity.draft.is_some() {
                return Err(ProjectionError::InvalidEvent);
            }
            let handler = element
                .authoring_state
                .get::<crate::authoring::text::SubmitHandler>()
                .cloned();
            let mut callback = handler
                .as_ref()
                .map(|handler| handler.0.try_borrow_mut())
                .transpose()
                .map_err(|_| ProjectionError::InvalidEvent)?;
            let mut identity = identity.clone();
            identity.edit_revision = *revision;
            element.authoring_state.set(identity);
            if let Some(callback) = callback.as_mut() {
                callback();
            }
        }
        (
            NodeContent::Input { enabled: true, .. },
            EventPayload::Edit {
                value,
                revision,
                selection_utf16,
                composing_utf16,
            },
        ) => {
            let identity = element.authoring_state.get::<Identity>().unwrap();
            if *revision <= identity.edit_revision
                || !valid_utf16_range(value, *selection_utf16)
                || composing_utf16.is_some_and(|range| !valid_utf16_range(value, range))
            {
                return Err(ProjectionError::InvalidEvent);
            }
            let binding = element
                .authoring_state
                .get::<InputBinding>()
                .unwrap()
                .clone();
            if binding.store.try_peek().is_none() {
                return Err(ProjectionError::StaleNode);
            }
            let composition_handler = element
                .authoring_state
                .get::<crate::authoring::text::CompositionHandler>()
                .map(|handler| handler.0.clone());
            let mut composition_callback = composition_handler
                .as_ref()
                .map(|handler| handler.try_borrow_mut())
                .transpose()
                .map_err(|_| ProjectionError::InvalidEvent)?;
            let composition_changed = composing_utf16.is_some() || identity.draft.is_some();
            let mut identity = identity.clone();
            identity.edit_revision = *revision;
            identity.draft = composing_utf16.map(|_| value.clone());
            element.authoring_state.set(identity);
            if composing_utf16.is_none() {
                crate::utils::text_field::commit_value_sig(
                    &binding.field,
                    binding.store,
                    value.clone(),
                );
            }
            if composition_changed {
                if let Some(callback) = composition_callback.as_mut() {
                    callback(composing_utf16.map(|_| value.clone()));
                }
            }
        }
        _ => return Err(ProjectionError::InvalidEvent),
    }
    Ok(())
}

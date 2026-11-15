//! SDK presentation in the ordinary Jadix GPU App, sharing its text/IME engine.
use crate::application::{ApplicationError, ScriptApplication};
use jadix::{
    authoring::widgets::text_edit::{build_text_edit_element, TextEditBuildParams},
    core::reactivity::{
        create_effect, create_signal, dispose_effect, use_signal, EffectId, Signal,
    },
    core::tree::Element,
};
use jadix_core::presentation::{
    style::Axis, EventPayload, NodeContent, NodeId, PlatformEvent, PresentationNode,
};
use std::{
    cell::RefCell,
    collections::{BTreeMap, VecDeque},
    rc::Rc,
};
struct Field {
    value: Signal<String>,
    observed: jadix::authoring::text::EditObservation,
    name: String,
    revision: u64,
    observer: Option<EffectId>,
}
struct Control {
    value: Signal<String>,
    open: Signal<bool>,
    observed: String,
    expected_open: bool,
    revision: u64,
    select: bool,
    name: String,
    dismissal: Signal<Option<bool>>,
    observer: Option<EffectId>,
}
struct ScrollState {
    x: Signal<f32>,
    y: Signal<f32>,
    observed: [f32; 2],
    name: String,
    _observer: jadix::core::scroll_viewport::ScrollOffsetSubscription,
    effect: EffectId,
}
impl Drop for ScrollState {
    fn drop(&mut self) {
        dispose_effect(self.effect);
    }
}
struct EventBridge {
    queue: RefCell<VecDeque<PlatformEvent>>,
    dirty_fields: RefCell<std::collections::BTreeSet<NodeId>>,
    dirty_controls: RefCell<std::collections::BTreeSet<NodeId>>,
    dirty_scrolls: RefCell<std::collections::BTreeSet<NodeId>>,
    lifetime: jadix_core::presentation::RootLifetime,
}
impl EventBridge {
    fn mark_dirty(
        &self,
        dirty: &RefCell<std::collections::BTreeSet<NodeId>>,
        node: NodeId,
        wake: Signal<u64>,
    ) {
        // Release the queue borrow before waking an effect which may drain it.
        let inserted = dirty.borrow_mut().insert(node);
        if inserted {
            if let Some(value) = wake.try_peek() {
                wake.try_set(value.wrapping_add(1));
            }
        }
    }
    fn mark_field(&self, node: NodeId, wake: Signal<u64>) {
        self.mark_dirty(&self.dirty_fields, node, wake);
    }
    fn mark_control(&self, node: NodeId, wake: Signal<u64>) {
        self.mark_dirty(&self.dirty_controls, node, wake);
    }
    fn mark_scroll(&self, node: NodeId, wake: Signal<u64>) {
        self.mark_dirty(&self.dirty_scrolls, node, wake);
    }
}
impl Drop for Field {
    fn drop(&mut self) {
        if let Some(observer) = self.observer.take() {
            dispose_effect(observer);
        }
    }
}
impl Drop for Control {
    fn drop(&mut self) {
        if let Some(observer) = self.observer.take() {
            dispose_effect(observer);
        }
    }
}
struct ProjectedNode {
    name: String,
    descriptor: jadix_core::presentation::NodeDescriptor,
    children: Vec<NodeId>,
    path: Option<Vec<usize>>,
}
/// Projection work only; SDK validation/diff and renderer work are separate.
#[derive(Clone, Copy, Debug, Default)]
pub struct ProjectionStats {
    pub visited_nodes: usize,
    pub projected_nodes: usize,
    pub leaf_patches: usize,
    pub full_projection: bool,
}
#[derive(Clone, Copy, Debug, serde::Serialize)]
pub struct WindowUpdate {
    pub full_build: bool,
    pub patched_nodes: usize,
    /// CPU SDK dispatch/publication and candidate construction; sparse updates
    /// also include in-place tree writes. Full App reconciliation, queue wait
    /// and presentation are outside this measurement.
    pub elapsed_ms: f64,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PumpStats {
    pub field_candidates: usize,
    pub control_candidates: usize,
    pub scroll_candidates: usize,
}
pub struct GpuApplication {
    app: ScriptApplication,
    events: Rc<EventBridge>,
    fields: BTreeMap<NodeId, Field>,
    wake: Option<Signal<u64>>,
    controls: BTreeMap<NodeId, Control>,
    scrolls: BTreeMap<NodeId, ScrollState>,
    projected: BTreeMap<NodeId, ProjectedNode>,
    last_pump_stats: PumpStats,
}
impl GpuApplication {
    /// Build once in the ordinary App, then apply ordered SDK leaf packets
    /// through its existing window-scoped bind runtime. Structure/control
    /// updates rebuild normally. Borrow conflicts retry at idle; no reentrant
    /// AppState borrow, worker VM, or independently owned UI tree is introduced.
    pub fn retained_window_element<F>(application: Rc<RefCell<Self>>, observe: Rc<F>) -> Element
    where
        F: Fn(WindowUpdate, &Self) + 'static,
    {
        use jadix_core::{
            create_effect, create_signal, queue_running_effect_for_idle_retry, untrack,
        };
        let started = std::time::Instant::now();
        fn leaf_window(nodes: &[PresentationNode]) -> bool {
            nodes.iter().all(|node| {
                matches!(
                    node.descriptor.content,
                    NodeContent::Container
                        | NodeContent::Text(_)
                        | NodeContent::Button { .. }
                        | NodeContent::Checkbox { .. }
                        | NodeContent::Switch { .. }
                ) && leaf_window(&node.children)
            })
        }
        // Editors, selects and overlays rely on the ordinary root builder's
        // deferred observation order. Select it before constructing editor
        // effects; a probe Element would duplicate those effects.
        if !leaf_window(application.borrow().app.roots()) {
            let element = application
                .borrow_mut()
                .element()
                .expect("stateful window projection");
            observe(
                WindowUpdate {
                    full_build: true,
                    patched_nodes: 0,
                    elapsed_ms: started.elapsed().as_secs_f64() * 1000.,
                },
                &application.borrow(),
            );
            return element;
        }
        let rebuild = create_signal(0_u64);
        let _ = rebuild.get();
        let element =
            untrack(|| application.borrow_mut().element()).expect("window application projection");
        let wake = application.borrow().wake.unwrap();
        application.borrow_mut().app.take_changes();
        observe(
            WindowUpdate {
                full_build: true,
                patched_nodes: 0,
                elapsed_ms: started.elapsed().as_secs_f64() * 1000.,
            },
            &application.borrow(),
        );
        let mut first = true;
        let mut pending: Option<Vec<Element>> = None;
        let mut elapsed = 0.;
        create_effect(move || {
            let _ = wake.get();
            if first {
                first = false;
                return;
            }
            if pending.is_none() {
                let started = std::time::Instant::now();
                let mut app = application.borrow_mut();
                untrack(|| app.pump()).expect("window SDK dispatch");
                let Some(paths) = app.app.take_changes() else {
                    rebuild.set(rebuild.peek().wrapping_add(1));
                    return;
                };
                // Controlled fields carry stateful properties beyond plain text.
                // Rebuild through the existing projection to apply all of them.
                if paths.iter().any(|path| {
                    crate::application::presentation_at(app.app.roots(), path).is_none_or(|node| {
                        !matches!(
                            node.descriptor.content,
                            NodeContent::Text(_)
                                | NodeContent::Button { .. }
                                | NodeContent::Checkbox { .. }
                                | NodeContent::Switch { .. }
                        )
                    })
                }) {
                    rebuild.set(rebuild.peek().wrapping_add(1));
                    return;
                }
                let nodes: Vec<_> = paths
                    .iter()
                    .filter_map(|p| {
                        crate::application::presentation_at(app.app.roots(), p).cloned()
                    })
                    .collect();
                let mut replacements = Vec::new();
                for node in nodes {
                    let Self {
                        events,
                        fields,
                        controls,
                        scrolls,
                        ..
                    } = &mut *app;
                    replacements.push(
                        project(
                            &node,
                            events,
                            wake,
                            fields,
                            controls,
                            scrolls,
                            &mut Default::default(),
                        )
                        .expect("validated leaf projection"),
                    );
                }
                elapsed = started.elapsed().as_secs_f64() * 1000.;
                pending = Some(replacements);
            }
            let started = std::time::Instant::now();
            let replacements = pending.as_ref().unwrap();
            for next in replacements {
                let mut applied = false;
                let status = jadix::app::runtime::try_update_element_by_template_id(
                    next.template_id.as_deref().unwrap(),
                    |mounted| {
                        applied = true;
                        if mounted.text == next.text && mounted.focusable == next.focusable {
                            return jadix::core::tree::LayoutStatus::Clean;
                        }
                        mounted.text = next.text.clone();
                        mounted.text_spans = next.text_spans.clone();
                        mounted.focusable = next.focusable;
                        mounted.on_click = next.on_click.clone();
                        mounted.on_key_down = next.on_key_down.clone();
                        jadix::core::tree::LayoutStatus::LayoutChanged
                    },
                );
                if status.is_none() {
                    queue_running_effect_for_idle_retry();
                    return;
                }
                if !applied {
                    rebuild.set(rebuild.peek().wrapping_add(1));
                    return;
                }
            }
            elapsed += started.elapsed().as_secs_f64() * 1000.;
            let count = replacements.len();
            pending = None;
            if count > 0 {
                jadix::app::runtime::request_redraw();
                observe(
                    WindowUpdate {
                        full_build: false,
                        patched_nodes: count,
                        elapsed_ms: elapsed,
                    },
                    &application.borrow(),
                );
            }
        });
        element
    }
    pub fn new(mut app: ScriptApplication) -> Result<Self, ApplicationError> {
        app.publish(|_| Ok(()))?;
        let events = Rc::new(EventBridge {
            queue: Default::default(),
            dirty_fields: Default::default(),
            dirty_controls: Default::default(),
            dirty_scrolls: Default::default(),
            lifetime: app.lifetime(),
        });
        Ok(Self {
            app,
            events,
            fields: BTreeMap::new(),
            wake: None,
            controls: Default::default(),
            scrolls: Default::default(),
            projected: Default::default(),
            last_pump_stats: PumpStats::default(),
        })
    }
    pub fn application(&self) -> &ScriptApplication {
        &self.app
    }
    /// Number of dirty SDK nodes examined by the most recent host pump.
    pub fn last_pump_stats(&self) -> PumpStats {
        self.last_pump_stats
    }
    /// Observed shared editor state, including real OS preedit when delivered.
    pub fn edit_observations(
        &self,
    ) -> impl Iterator<Item = (NodeId, &jadix::authoring::text::EditObservation)> {
        self.fields.iter().map(|(id, field)| (*id, &field.observed))
    }
    fn pump(&mut self) -> Result<(), ApplicationError> {
        if self.app.lifetime().is_disposed() {
            return Err(ApplicationError::Session(crate::ShellError::InvalidState));
        }
        let wake = *self.wake.get_or_insert_with(|| create_signal(0));
        let _ = wake.get();
        let mut events: Vec<_> = self.events.queue.borrow_mut().drain(..).collect();
        let dirty_fields = std::mem::take(&mut *self.events.dirty_fields.borrow_mut());
        let dirty_controls = std::mem::take(&mut *self.events.dirty_controls.borrow_mut());
        let dirty_scrolls = std::mem::take(&mut *self.events.dirty_scrolls.borrow_mut());
        self.last_pump_stats = PumpStats {
            field_candidates: dirty_fields.len(),
            control_candidates: dirty_controls.len(),
            scroll_candidates: dirty_scrolls.len(),
        };
        for id in dirty_fields {
            let Some(field) = self.fields.get_mut(&id) else {
                continue;
            };
            let observation = jadix::authoring::text::observe_edit(&field.name);
            if observation != field.observed {
                field.revision = field
                    .revision
                    .checked_add(1)
                    .ok_or(ApplicationError::Protocol)?;
                events.push(PlatformEvent {
                    node: id,
                    payload: EventPayload::Edit {
                        value: observation.value.clone(),
                        revision: field.revision,
                        selection_utf16: observation.selection,
                        composing_utf16: observation.composing,
                    },
                });
                field.observed = observation;
            }
        }
        for id in dirty_controls {
            let Some(control) = self.controls.get_mut(&id) else {
                continue;
            };
            let value = control.value.get();
            let open = control.open.get();
            let changed = if control.select {
                value != control.observed
            } else {
                control.expected_open && !open
            };
            if changed {
                control.revision = control
                    .revision
                    .checked_add(1)
                    .ok_or(ApplicationError::Protocol)?;
                events.push(PlatformEvent {
                    node: id,
                    payload: if control.select {
                        EventPayload::Select {
                            value: value.clone(),
                            revision: control.revision,
                        }
                    } else {
                        EventPayload::Dismiss {
                            backdrop: control.dismissal.peek().unwrap_or(false),
                            revision: control.revision,
                        }
                    },
                });
                control.observed = value;
                control.expected_open = open;
            }
        }
        for id in dirty_scrolls {
            let Some(scroll) = self.scrolls.get_mut(&id) else {
                continue;
            };
            let offset = [scroll.x.get(), scroll.y.get()];
            if offset != scroll.observed {
                events.push(PlatformEvent {
                    node: id,
                    payload: EventPayload::Scroll {
                        x: f64::from(offset[0]),
                        y: f64::from(offset[1]),
                    },
                });
                scroll.observed = offset;
            }
        }
        if events.is_empty() {
            return Ok(());
        }
        self.app.events(events)?;
        self.app.publish(|_| Ok(()))?;
        Ok(())
    }
    /// Call inside the App's build owner. Static content also works in JadixScene;
    /// animated portal opacity requires the ordinary window compositor.
    pub fn element(&mut self) -> Result<Element, ApplicationError> {
        self.pump()?;
        // A caller may replace its root with this independently projected tree.
        // Re-admit it before a subsequent incremental update.
        self.projected.clear();
        self.project_root()
    }
    fn project_root(&mut self) -> Result<Element, ApplicationError> {
        let wake = self.wake.expect("pump initializes the wake signal");
        let mut live = std::collections::BTreeSet::new();
        let mut root = Element::new()
            .flex_col()
            .w("100%")
            .h("100%")
            .bg([0.94, 0.94, 0.94, 1.0]);
        for node in self.app.roots() {
            root = root.child(project(
                node,
                &self.events,
                wake,
                &mut self.fields,
                &mut self.controls,
                &mut self.scrolls,
                &mut live,
            )?);
        }
        self.fields.retain(|id, _| live.contains(id));
        self.controls.retain(|id, control| {
            if live.contains(id) {
                return true;
            }
            control.open.set(false);
            if control.select {
                use_signal(&format!("{}__select_open", control.name), || false).set(false);
            }
            jadix::app::portal_overlay::remove_overlay(&control.name);
            false
        });
        self.scrolls.retain(|id, state| {
            if live.contains(id) {
                return true;
            }
            jadix::core::scroll_viewport::remove_scroll_viewport_signals(&state.name);
            false
        });
        Ok(root)
    }
    fn remember_projection(&mut self) {
        self.projected.clear();
        let mut pending: Vec<_> = self
            .app
            .roots()
            .iter()
            .enumerate()
            .map(|(i, n)| (n, Some(vec![i])))
            .collect();
        while let Some((node, path)) = pending.pop() {
            self.projected.insert(
                node.id,
                ProjectedNode {
                    name: node_name(node.id),
                    descriptor: node.descriptor.clone(),
                    children: node.children.iter().map(|n| n.id).collect(),
                    path: path.clone(),
                },
            );
            for (i, child) in node.children.iter().enumerate() {
                let next = path
                    .as_ref()
                    .filter(|_| matches!(node.descriptor.content, NodeContent::Container))
                    .map(|p| {
                        let mut p = p.clone();
                        p.push(i);
                        p
                    });
                pending.push((child, next));
            }
        }
    }
    /// Patch changed plain leaves without reconstructing unrelated Elements.
    /// `root` must be the tree previously returned by this app, in this owner.
    /// SDK leaf packets admit and inspect only addressed paths. Plain text,
    /// Button, Checkbox and Switch leaves can be replaced in place;
    /// full mounts and structural/container changes use full projection.
    /// Layout, collection and rendering costs must be measured separately.
    pub fn update_root(
        &mut self,
        renderer: &mut jadix::renderer::offscreen::SceneRenderer,
        root: &mut Element,
    ) -> Result<ProjectionStats, ApplicationError> {
        self.pump()?;
        let changes = self.app.take_changes();
        let mut stats = ProjectionStats::default();
        let mut changed = Vec::new();
        let mut full = self.projected.is_empty();
        let incremental = changes.is_some() && !full;
        let mut pending: Vec<_> = if let Some(paths) = changes.filter(|_| !full) {
            paths
                .iter()
                .filter_map(|p| crate::application::presentation_at(self.app.roots(), p))
                .collect()
        } else {
            self.app.roots().iter().collect()
        };
        while let Some(node) = pending.pop() {
            stats.visited_nodes += 1;
            match self.projected.get(&node.id) {
                Some(old)
                    if old
                        .children
                        .iter()
                        .copied()
                        .eq(node.children.iter().map(|n| n.id)) =>
                {
                    if old.descriptor != node.descriptor {
                        if node.children.is_empty()
                            && old.path.is_some()
                            && matches!(
                                old.descriptor.content,
                                NodeContent::Text(_)
                                    | NodeContent::Button { .. }
                                    | NodeContent::Checkbox { .. }
                                    | NodeContent::Switch { .. }
                            )
                            && matches!(
                                node.descriptor.content,
                                NodeContent::Text(_)
                                    | NodeContent::Button { .. }
                                    | NodeContent::Checkbox { .. }
                                    | NodeContent::Switch { .. }
                            )
                        {
                            changed.push(node);
                        } else {
                            full = true;
                        }
                    }
                    if let Some(path) = &old.path {
                        if element_at(root, path)
                            .is_none_or(|e| e.template_id.as_deref() != Some(old.name.as_str()))
                        {
                            full = true;
                        }
                    }
                }
                _ => full = true,
            }
            if !incremental {
                pending.extend(node.children.iter());
            }
        }
        if !incremental {
            full |= stats.visited_nodes != self.projected.len();
        }
        if !full {
            // Stage every replacement before mutating the retained tree.
            let mut replacements = Vec::new();
            for node in changed {
                let next = project(
                    node,
                    &self.events,
                    self.wake.unwrap(),
                    &mut self.fields,
                    &mut self.controls,
                    &mut self.scrolls,
                    &mut Default::default(),
                )?;
                replacements.push((node.id, node.descriptor.clone(), next));
            }
            for (id, descriptor, next) in replacements {
                let old = self.projected.get_mut(&id).unwrap();
                assert!(
                    renderer.replace_leaf(root, old.path.as_ref().unwrap(), next),
                    "validated plain leaf"
                );
                old.descriptor = descriptor;
                stats.leaf_patches += 1;
            }
            stats.projected_nodes = stats.leaf_patches;
            renderer.reuse_collect_next_frame();
        } else {
            let next = self.project_root()?;
            self.remember_projection();
            renderer.replace_root(root, next);
            stats.projected_nodes = self.projected.len();
            stats.full_projection = true;
        }
        Ok(stats)
    }
    /// Revoke script callbacks before releasing presentation resources. Hosts
    /// should also remove their old Element tree from paint and input dispatch.
    pub fn dispose(&mut self) {
        self.app.dispose();
        self.events.queue.borrow_mut().clear();
        self.events.dirty_fields.borrow_mut().clear();
        self.events.dirty_controls.borrow_mut().clear();
        self.events.dirty_scrolls.borrow_mut().clear();
        for control in self.controls.values() {
            // Teardown must not synchronously rebuild the host's borrowed tree.
            control.open.try_set_silent(false);
        }
        for control in self.controls.values() {
            jadix::app::portal_overlay::remove_overlay(&control.name);
        }
        for scroll in self.scrolls.values() {
            jadix::core::scroll_viewport::remove_scroll_viewport_signals(&scroll.name);
        }
        self.controls.clear();
        self.scrolls.clear();
        self.fields.clear();
        self.projected.clear();
    }
    /// Uses the production window/runtime, renderer, focus and OS IME path.
    pub fn run(mut self, options: jadix::app::WindowOptions) {
        jadix::run_with(options, move || match self.element() {
            Ok(element) => element,
            Err(error) => {
                self.app.dispose();
                Element::new().text(format!("Application stopped: {error}"))
            }
        });
    }
}
fn node_name(id: NodeId) -> String {
    format!("shell:{}:{}", id.root.token(), id.generation())
}
fn element_at<'a>(mut root: &'a Element, path: &[usize]) -> Option<&'a Element> {
    for i in path {
        root = root.children.get(*i)?;
    }
    Some(root)
}

impl Drop for GpuApplication {
    fn drop(&mut self) {
        self.dispose();
    }
}
fn project(
    node: &PresentationNode,
    events: &Rc<EventBridge>,
    wake: Signal<u64>,
    fields: &mut BTreeMap<NodeId, Field>,
    controls: &mut BTreeMap<NodeId, Control>,
    scrolls: &mut BTreeMap<NodeId, ScrollState>,
    live: &mut std::collections::BTreeSet<NodeId>,
) -> Result<Element, ApplicationError> {
    let id = node.id;
    let descriptor = &node.descriptor;
    let name = node_name(id);
    let mut payload = None;
    let mut el = match &descriptor.content {
        NodeContent::Container | NodeContent::Dialog { .. } | NodeContent::Popover { .. } => {
            Element::new()
        }
        NodeContent::Scroll => {
            live.insert(id);
            use jadix::core::scroll_viewport::*;
            scrolls.entry(id).or_insert_with(|| {
                let x = use_signal(&scroll_offset_x_signal_key(&name), || 0.0);
                let y = use_signal(&scroll_offset_y_signal_key(&name), || 0.0);
                let previous = [x.peek(), y.peek()];
                // Scroll collect can repaint without rebuilding the application.
                // Wake its event bridge even when only retained geometry changed.
                let notifications = events.clone();
                let mut first = true;
                let effect = create_effect(move || {
                    let _ = (x.get(), y.get());
                    if first {
                        first = false;
                        return;
                    }
                    notifications.mark_scroll(id, wake);
                });
                let events = events.clone();
                let observer = observe_scroll_offset(&name, move |_| {
                    events.mark_scroll(id, wake);
                });
                ScrollState {
                    x,
                    y,
                    observed: previous,
                    name: name.clone(),
                    _observer: observer,
                    effect,
                }
            });
            activate_scroll_viewport(Element::new(), &name, "auto", "auto", "jump", "area")
        }
        NodeContent::Image { source, alt } => {
            use base64::Engine;
            if descriptor.content.image_dimensions().is_none() {
                return Err(ApplicationError::Host(
                    "GPU SDK image must be a bounded PNG data URL".into(),
                ));
            }
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(source.strip_prefix("data:image/png;base64,").unwrap())
                .map_err(|_| ApplicationError::Protocol)?;
            let key = format!("shell-image:{}", crate::package::sha256(&bytes));
            if !jadix::app::pending_images::queue_image_from_src(key.clone(), source) {
                return Err(ApplicationError::Host("PNG decode rejected".into()));
            }
            let mut image = Element::new();
            image.image_texture = Some(key);
            image.attrs.insert("accesskit_label".into(), alt.clone());
            image
        }
        NodeContent::Select {
            options,
            value,
            placeholder,
            enabled,
            revision,
        } => {
            live.insert(id);
            let control = controls.entry(id).or_insert_with(|| Control {
                value: create_signal(value.clone()),
                open: use_signal(&format!("{name}__select_open"), || false),
                observed: value.clone(),
                expected_open: false,
                revision: *revision,
                select: true,
                name: name.clone(),
                dismissal: create_signal(None),
                observer: None,
            });
            if control.revision != *revision || control.observed != *value {
                control.value.set(value.clone());
                control.observed = value.clone();
                control.revision = *revision;
            }
            if control.observer.is_none() {
                let value = control.value;
                let open = control.open;
                let events = events.clone();
                let mut first = true;
                control.observer = Some(create_effect(move || {
                    let _ = value.get();
                    let _ = open.get();
                    if first {
                        first = false;
                        return;
                    }
                    events.mark_control(id, wake);
                }));
            }
            let _ = (control.open.get(), control.value.get());
            if !enabled {
                Element::new().text(
                    options
                        .iter()
                        .find(|o| o.value == *value)
                        .map_or(placeholder.as_str(), |o| o.label.as_str()),
                )
            } else {
                let mut builder = jadix_components::select(&name)
                    .retained(true)
                    .value_signal(control.value)
                    .placeholder(placeholder)
                    .trigger_width("100%");
                for option in options {
                    builder = builder.item(
                        jadix_components::SelectItemData::new(&option.value, &option.label)
                            .disabled(!option.enabled),
                    );
                }
                builder.into()
            }
        }
        NodeContent::Text(text) => {
            let mut element = Element::new().text(text.clone());
            if !descriptor.style.inline_styles.is_empty() {
                let mut spans = Vec::new();
                let mut at = 0;
                for inline in &descriptor.style.inline_styles {
                    if at < inline.range[0] {
                        spans.push(jadix::style::TextSpan::new(&text[at..inline.range[0]]));
                    }
                    let mut span =
                        jadix::style::TextSpan::new(&text[inline.range[0]..inline.range[1]]);
                    span.color = inline.foreground;
                    span.background = inline.background;
                    span.weight = inline.bold.map(|bold| if bold { 700 } else { 400 });
                    span.italic = inline.italic.unwrap_or(false);
                    span.underline = inline.underline.unwrap_or(false);
                    span.strikethrough = inline.strikethrough.unwrap_or(false);
                    spans.push(span);
                    at = inline.range[1];
                }
                if at < text.len() {
                    spans.push(jadix::style::TextSpan::new(&text[at..]));
                }
                element = element.text_spans(spans);
            }
            element
        }
        NodeContent::Button { label, enabled } => {
            if *enabled {
                payload = Some(EventPayload::Activate);
            }
            Element::new().text(label.clone()).focusable(*enabled)
        }
        NodeContent::Checkbox {
            label,
            checked,
            enabled,
            revision,
        }
        | NodeContent::Switch {
            label,
            checked,
            enabled,
            revision,
        } => {
            if *enabled {
                payload = Some(EventPayload::Toggle {
                    checked: !*checked,
                    revision: revision.checked_add(1).ok_or(ApplicationError::Protocol)?,
                });
            }
            Element::new()
                .text(format!("{} {label}", if *checked { "[x]" } else { "[ ]" }))
                .focusable(*enabled)
        }
        NodeContent::Input {
            value,
            placeholder,
            enabled,
            revision,
            ..
        }
        | NodeContent::Textarea {
            max_length: _,
            value,
            placeholder,
            enabled,
            revision,
        } => {
            live.insert(id);
            let field = fields.entry(id).or_insert_with(|| Field {
                value: use_signal(&format!("shell-value-{name}"), || value.clone()),
                observed: jadix::authoring::text::EditObservation {
                    value: value.clone(),
                    selection: [0, 0],
                    composing: None,
                },
                name: name.clone(),
                revision: *revision,
                observer: None,
            });
            if field.observed.composing.is_none()
                && (field.revision != *revision || &field.observed.value != value)
            {
                field.value.set(value.clone());
                field.observed.value = value.clone();
                field.revision = *revision;
            }
            if field.observer.is_none() {
                let field_id = field.name.clone();
                let events = events.clone();
                let mut first = true;
                field.observer = Some(create_effect(move || {
                    let _ = jadix::authoring::text::observe_edit(&field_id);
                    if first {
                        first = false;
                        return;
                    }
                    events.mark_field(id, wake);
                }));
            }
            let _ = field.value.get();
            let multiline = matches!(descriptor.content, NodeContent::Textarea { .. });
            let password = matches!(
                descriptor.content,
                NodeContent::Input { password: true, .. }
            );
            build_text_edit_element(TextEditBuildParams {
                field_id: name.clone(),
                placeholder: placeholder.clone(),
                disabled: !*enabled,
                invalid: false,
                input_type: if password { "password" } else { "text" }.into(),
                clearable: false,
                password_toggle: false,
                number_steppers: false,
                step: 1.,
                text_size_px: descriptor.style.font_size,
                font_weight: descriptor.style.font_weight,
                line_height_px: None,
                leading_scale: None,
                value: Some(field.value),
                max_length: descriptor.content.input_max_length(),
                text_align: Default::default(),
                single_line_container: Some(
                    jadix::primitives::hstack()
                        .h(36.0)
                        .w("100%")
                        .min_w(0.0)
                        .items_center()
                        .rounded(6.0),
                ),
                multiline,
                rows: 4,
                resizable: false,
                resize_max_h: 480.,
                wrap_mode: Default::default(),
                plain_chrome: false,
                caret_color: None,
                selection_bg: None,
                selection_foreground: None,
                selection_on_blur: jadix::utils::text_field::SelectionOnBlur::Clear,
                undo: jadix::utils::text_edit_history::TextUndoConfig::input_default(),
                drop_undo_on_unmount: true,
            })
        }
    };
    if !matches!(
        descriptor.content,
        NodeContent::Input { .. } | NodeContent::Textarea { .. } | NodeContent::Scroll
    ) {
        el = el.focus_id(name.clone());
    }
    el.template_id = Some(name.clone());
    if let Some(payload) = payload {
        let queue = events.clone();
        let fire = Rc::new(move || {
            if queue.lifetime.is_disposed() || !wake.is_alive() {
                return;
            }
            let mut pending = queue.queue.borrow_mut();
            if pending.len() < 1024 {
                pending.push_back(PlatformEvent {
                    node: id,
                    payload: payload.clone(),
                });
            }
            drop(pending);
            wake.set(wake.peek().wrapping_add(1));
        });
        let click = fire.clone();
        el = el.on_click(move || click()).on_key_down(move |key| {
            use winit::{
                event::ElementState,
                keyboard::{Key, NamedKey},
            };
            if key.state == ElementState::Pressed
                && !key.repeat
                && (matches!(key.logical_key, Key::Named(NamedKey::Enter))
                    || key.logical_key.to_text() == Some(" "))
            {
                fire();
                true
            } else {
                false
            }
        });
    }
    let s = &descriptor.style;
    el = if s.axis == Axis::Column && !matches!(descriptor.content, NodeContent::Input { .. }) {
        el.flex_col()
    } else {
        el.flex_row()
    };
    // NodeStyle contains independent flex fields. Element::flex_grow is an
    // Input authoring convenience that also forces basis=0 and width=auto;
    // applying it to a zero-grow SDK editor collapses its requested height.
    el.style.flex_grow = s.grow;
    el = el
        .gap(s.gap)
        .flex_shrink(s.shrink)
        .text_size(s.font_size)
        .font_weight(s.font_weight)
        .text_color(s.foreground)
        .bg(s.background.map(|v| f32::from(v) / 255.));
    el = el
        .pt(s.padding[0])
        .pr(s.padding[1])
        .pb(s.padding[2])
        .pl(s.padding[3])
        .font_family(s.font_family.clone());
    if let Some(v) = s.width {
        el = el.w(v);
    }
    if let Some(v) = s.height {
        el = el.h(v);
    }
    el = el.min_w(s.min_width).min_h(s.min_height);
    if let Some(v) = s.max_width {
        el = el.max_w(v);
    }
    if let Some(v) = s.max_height {
        el = el.max_h(v);
    }
    for child in &node.children {
        el = el.child(project(
            child, events, wake, fields, controls, scrolls, live,
        )?);
    }
    if let NodeContent::Text(_) = &descriptor.content {
        use jadix_core::presentation::style::TextOverflow;
        el = match descriptor.style.text_overflow {
            TextOverflow::Wrap => el,
            // A multi-line clip/ellipsis still needs normal wrapping. The
            // single-line form keeps nowrap so the text engine can place the
            // truncation marker against the fixed inline box.
            TextOverflow::Clip if descriptor.style.max_lines.is_some_and(|lines| lines > 1) => {
                el.text_overflow("clip")
            }
            TextOverflow::Ellipsis if descriptor.style.max_lines.is_some_and(|lines| lines > 1) => {
                el.text_overflow("ellipsis")
            }
            TextOverflow::Clip => el.whitespace_nowrap().text_overflow("clip"),
            TextOverflow::Ellipsis => el.whitespace_nowrap().text_overflow("ellipsis"),
        };
        if let Some(lines) = descriptor.style.max_lines.filter(|lines| *lines > 0) {
            // The portable protocol intentionally carries no line-height
            // field. Match the renderer's default text leading and cap the
            // layout box so wrapped glyphs beyond the requested rows are
            // clipped by the normal scene damage/overflow path.
            let line_height = descriptor.style.font_size * 1.25;
            let line_cap = line_height * f32::from(lines);
            let max_height = descriptor
                .style
                .height
                .into_iter()
                .chain(descriptor.style.max_height)
                .fold(line_cap, f32::min);
            el = el
                .text_leading(1.25)
                .text_max_lines(lines)
                .max_h(max_height)
                .overflow("hidden");
        }
    }
    match &descriptor.content {
        NodeContent::Dialog {
            open,
            close_on_overlay_click,
            revision,
        }
        | NodeContent::Popover {
            open,
            close_on_outside_click: close_on_overlay_click,
            revision,
            ..
        } => {
            live.insert(id);
            let control = controls.entry(id).or_insert_with(|| Control {
                value: create_signal(String::new()),
                open: create_signal(*open),
                observed: String::new(),
                expected_open: *open,
                revision: *revision,
                select: false,
                name: name.clone(),
                dismissal: create_signal(None),
                observer: None,
            });
            if control.expected_open != *open || control.revision != *revision {
                control.dismissal.set(None);
                control.open.set(*open);
                control.expected_open = *open;
                control.revision = *revision;
            }
            if control.observer.is_none() {
                let value = control.value;
                let open = control.open;
                let events = events.clone();
                let mut first = true;
                control.observer = Some(create_effect(move || {
                    let _ = value.get();
                    let _ = open.get();
                    if first {
                        first = false;
                        return;
                    }
                    events.mark_control(id, wake);
                }));
            }
            if matches!(descriptor.content, NodeContent::Dialog { .. }) {
                use jadix::utils::{
                    modal_shell::ShellOverlayOpts,
                    viewport_modal::{retained_viewport_modal_layer, ViewportModalLayerOpts},
                };
                let overlay = ShellOverlayOpts {
                    color: [0., 0., 0., 0.4],
                    close_on_click: *close_on_overlay_click,
                    backdrop_blur: 0.,
                };
                el = retained_viewport_modal_layer(
                    &name,
                    control.open,
                    &overlay,
                    ViewportModalLayerOpts::dialog_fade(),
                    el,
                );
            } else if let NodeContent::Popover {
                anchor,
                placement,
                gap,
                ..
            } = &descriptor.content
            {
                use jadix::utils::{
                    anchored_overlay::FloatingStrategy,
                    anchored_overlay::{
                        retained_anchored_panel, AnchoredOverlayContentOpts, Placement,
                    },
                    popup::PopupPlacement,
                };
                let placement = match placement {
                    jadix_core::presentation::PopoverPlacement::TopStart => {
                        PopupPlacement::TopStart
                    }
                    jadix_core::presentation::PopoverPlacement::Top => PopupPlacement::TopCenter,
                    jadix_core::presentation::PopoverPlacement::TopEnd => PopupPlacement::TopEnd,
                    jadix_core::presentation::PopoverPlacement::BottomStart => {
                        PopupPlacement::BottomStart
                    }
                    jadix_core::presentation::PopoverPlacement::Bottom => {
                        PopupPlacement::BottomCenter
                    }
                    jadix_core::presentation::PopoverPlacement::BottomEnd => {
                        PopupPlacement::BottomEnd
                    }
                    jadix_core::presentation::PopoverPlacement::LeftStart => {
                        PopupPlacement::LeftStart
                    }
                    jadix_core::presentation::PopoverPlacement::Left => PopupPlacement::LeftCenter,
                    jadix_core::presentation::PopoverPlacement::LeftEnd => PopupPlacement::LeftEnd,
                    jadix_core::presentation::PopoverPlacement::RightStart => {
                        PopupPlacement::RightStart
                    }
                    jadix_core::presentation::PopoverPlacement::Right => {
                        PopupPlacement::RightCenter
                    }
                    jadix_core::presentation::PopoverPlacement::RightEnd => {
                        PopupPlacement::RightEnd
                    }
                };
                let opts = AnchoredOverlayContentOpts {
                    placement: Some(Placement::Trigger {
                        focus_id: format!("shell:{}:{}", anchor.root.token(), anchor.generation()),
                        placement,
                        gap: f32::from(*gap),
                        strategy: FloatingStrategy::Auto,
                    }),
                    ..Default::default()
                };
                el = retained_anchored_panel(&name, control.open, &opts, el);
                jadix::app::portal_overlay::set_outside_dismiss(&name, *close_on_overlay_click);
            }
            jadix::app::portal_overlay::observe_dismissal(&name, control.dismissal);
        }
        _ => {}
    }
    Ok(el)
}

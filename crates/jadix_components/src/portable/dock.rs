//! Dock presentation over the shared transactional model. Pane content keeps
//! its scope and native node when tabs move between stacks on the same surface.
use super::{button, resizable, Element};
pub use jadix::portable::dock::{Axis as DockAxis, Bounds as DockBounds, Dock, DockError,
    DockLimits, DockNode, DropLocation, Edge as DockEdge, Pane as DockPane};
use jadix::{portable::{create_effect, create_signal, KeyedElements, Signal},
    presentation::{Rect, NavigationKey, accessibility::SemanticRole,
        interaction::{InteractionKind, InteractionPhase, InteractionPolicy, PanAxis}}};
use std::{cell::RefCell, collections::BTreeMap, rc::Rc};

#[derive(Clone)]
pub struct DockState { model: Rc<RefCell<Dock>>, revision: Signal<u64>, error: Signal<String> }
impl DockState {
    pub fn new(model: Dock) -> Self {
        Self { model: Rc::new(RefCell::new(model)), revision: create_signal(0), error: create_signal(String::new()) }
    }
    pub fn read<R>(&self, read: impl FnOnce(&Dock) -> R) -> R {
        self.revision.get(); read(&self.model.borrow())
    }
    /// Publish after releasing the model borrow, so effects may read it safely.
    pub fn update<R>(&self, update: impl FnOnce(&mut Dock) -> Result<R, DockError>) -> Result<R, DockError> {
        let (before, result, after) = {
            let mut model = self.model.borrow_mut();
            let before = (model.revision(), model.dragging());
            let result = update(&mut model);
            (before, result, (model.revision(), model.dragging()))
        };
        let error = result.as_ref().err().map(ToString::to_string).unwrap_or_default();
        if self.error.peek() != error { self.error.set(error); }
        if before != after { self.revision.set(self.revision.peek().wrapping_add(1)); }
        result
    }
    pub fn error(&self) -> String { self.error.get() }
}

type FloatHandler = Rc<dyn Fn(u64, DockBounds)>;
pub struct DockBuilder { id: String, state: DockState, surface: u64, pane: Rc<dyn Fn(u64) -> Element>, float: Option<FloatHandler> }
pub fn dock(id: impl Into<String>, state: DockState) -> DockBuilder {
    DockBuilder { id: id.into(), state, surface: 1,
        pane: Rc::new(|key| Element::new().text(format!("Pane {key}"))), float: None }
}
impl DockBuilder {
    pub fn surface(mut self, key: u64) -> Self { self.surface = key; self }
    pub fn pane(mut self, render: impl Fn(u64) -> Element + 'static) -> Self { self.pane = Rc::new(render); self }
    /// The window host owns floating surfaces and their coordinate conversion.
    /// Without a host callback, releasing outside this surface cancels the drag.
    pub fn on_float(mut self, callback: impl Fn(u64, DockBounds) + 'static) -> Self { self.float = Some(Rc::new(callback)); self }
    pub fn build(self) -> Element {
        let Self { id, state, surface, pane, float } = self;
        let layout = state.clone(); let view = state.clone(); let prefix = id.clone();
        let geometry = Rc::new(RefCell::new(Geometry::default()));
        let measure = geometry.clone(); let regions = geometry.clone();
        let mut root = Element::new().id(&id).flex_col().flex_grow(1.).min_w(0.).min_h(0.)
            .on_layout(move |frame| { measure.borrow_mut().root = frame; false })
            .bind_keyed_tree(move || layout.read(|model| {
                model.surfaces().iter().find(|item| item.key == surface).map(|item| {
                    let mut node = item.root.clone(); topology(&mut node); (node, model.panes().to_vec())
                })
            }), move |tree, pool| {
                { let mut regions = regions.borrow_mut(); regions.stacks.clear(); regions.tabs.clear(); }
                let Some((node, _)) = tree else { return vec![]; };
                vec![render_node(&prefix, &node, &view, pool, &pane, &regions, &float)]
            });
        root.descriptor.accessibility.role = Some(SemanticRole::Region);
        let mut status = Element::new().id(format!("{id}__status"));
        status.text_binding = Some(Rc::new(move || state.error()));
        status.descriptor.accessibility.live = jadix::presentation::accessibility::LiveRegion::Polite;
        // Keep status outside the retained arrangement.
        Element::new().flex_col().flex_grow(1.).child(root).child(status)
    }
}

#[derive(Default)]
struct Geometry { root: Rect, stacks: BTreeMap<u64, Rect>, tabs: BTreeMap<(u64, usize), Rect> }
fn topology(node: &mut DockNode) {
    match node {
        DockNode::Stack { active, .. } => *active = 0,
        DockNode::Split { ratio, first, second, .. } => { *ratio = 0.5; topology(first); topology(second); }
    }
}
fn find_node(node: &DockNode, key: u64) -> Option<&DockNode> {
    if node.key() == key { return Some(node); }
    if let DockNode::Split { first, second, .. } = node { find_node(first, key).or_else(|| find_node(second, key)) }
    else { None }
}
fn model_node<R>(state: &DockState, key: u64, read: impl FnOnce(&DockNode) -> R) -> Option<R> {
    state.read(|model| model.surfaces().iter().find_map(|surface| find_node(&surface.root, key)).map(read))
}
fn render_node(id: &str, node: &DockNode, state: &DockState, pool: &mut KeyedElements,
    pane: &Rc<dyn Fn(u64) -> Element>, geometry: &Rc<RefCell<Geometry>>, float: &Option<FloatHandler>) -> Element {
    match node {
        DockNode::Split { key, axis, first, second, .. } => {
            let key = *key;
            let ratio = model_node(state, key, |node| match node { DockNode::Split {ratio, ..} => *ratio, _ => 0.5 }).unwrap_or(0.5);
            let sizes = create_signal(vec![ratio, 1. - ratio]);
            let sync = state.clone();
            create_effect(move || {
                if let Some(ratio) = model_node(&sync, key, |node| match node { DockNode::Split {ratio, ..} => *ratio, _ => 0.5 }) {
                    let next = vec![ratio, 1. - ratio]; if sizes.peek() != next { sizes.set(next); }
                }
            });
            let first = render_node(id, first, state, pool, pane, geometry, float);
            let second = render_node(id, second, state, pool, pane, geometry, float);
            let update = state.clone();
            let gesture = state.clone();
            resizable(format!("{id}__split_{key}"), sizes).vertical(*axis == DockAxis::Vertical)
                .on_resize_phase(move |phase| { let _ = gesture.update(|model| match phase {
                    InteractionPhase::Started => model.begin_resize(key).map(|_| false),
                    InteractionPhase::Ended => model.finish_resize(false),
                    InteractionPhase::Cancelled => model.finish_resize(true),
                    _ => Ok(false),
                }); })
                .limits(vec![(0.1, 0.9); 2]).on_resize(move |values| {
                    let _ = update.update(|model| model.resize_split(key, values[0] / (values[0] + values[1])));
                }).children(vec![first, second]).expect("validated dock split").flex_grow(1.).min_w(0.).min_h(0.)
        }
        DockNode::Stack { key, panes, .. } => {
            let stack = *key; let measured = geometry.clone();
            let mut root = Element::new().id(format!("{id}__stack_{stack}")).flex_col().flex_grow(1.).min_w(0.).min_h(0.)
                .on_layout(move |frame| { measured.borrow_mut().stacks.insert(stack, frame); false });
            let mut tabs = Element::new().id(format!("{id}__tabs_{stack}")).flex_row().flex_shrink(0.);
            tabs.descriptor.accessibility.role = Some(SemanticRole::Tablist);
            let mut content = Element::new().flex_col().flex_grow(1.).min_w(0.).min_h(0.);
            for (index, &key) in panes.iter().enumerate() {
                let record = state.read(|model| model.panes().iter().find(|pane| pane.key == key).cloned()).expect("validated dock pane");
                let activate = state.clone(); let selected = state.clone(); let measured = geometry.clone();
                let mut tab: Element = button(&format!("{id}__tab_{key}")).text(record.title.clone())
                    .on_click(move || { let _ = activate.update(|model| model.activate(key)); }).into();
                tab = tab.on_layout(move |frame| { measured.borrow_mut().tabs.insert((stack, index), frame); false });
                tab.descriptor.accessibility.role = Some(SemanticRole::Tab);
                tab.descriptor_binding = Some(Rc::new(move |descriptor| {
                    descriptor.accessible_selected = model_node(&selected, stack, |node| matches!(node, DockNode::Stack {active, ..} if *active == key));
                }));
                tab.descriptor.navigation_keys = vec![NavigationKey::Left, NavigationKey::Right, NavigationKey::Home, NavigationKey::End, NavigationKey::Escape];
                let drag = Rc::new(RefCell::new(None));
                let cancel_drag = drag.clone();
                let navigate = state.clone(); let prefix = id.to_owned(); let choices = panes.clone();
                tab.on_navigation = Some(Rc::new(RefCell::new(Box::new(move |event| {
                    if event == NavigationKey::Escape {
                        let token = cancel_drag.borrow_mut().take();
                        if let Some(token) = token { let _ = navigate.update(|model| model.cancel_drag(&token)); }
                        return None;
                    }
                    let next = match event { NavigationKey::Left => jadix::portable::roving::roving_move(choices.len(), index, -1),
                        NavigationKey::Right => jadix::portable::roving::roving_move(choices.len(), index, 1),
                        NavigationKey::Home => Some(0), NavigationKey::End => choices.len().checked_sub(1), _ => None }?;
                    let _ = navigate.update(|model| model.activate(choices[next])); Some(format!("{prefix}__tab_{}", choices[next]))
                }))));
                tab.descriptor.interaction = InteractionPolicy { pan: Some(PanAxis::Both), ..Default::default() };
                let token = drag.clone(); let model = state.clone();
                let targets = geometry.clone(); let floating = float.clone();
                tab = tab.on_interaction(move |event| {
                    if event.kind != InteractionKind::Pan { return; }
                    match event.phase {
                        InteractionPhase::Started => {
                            if token.borrow().is_none() { *token.borrow_mut() = model.update(|model| model.begin_drag(key)).ok(); }
                        }
                        InteractionPhase::Cancelled => { if let Some(token) = token.borrow_mut().take() { let _ = model.update(|model| model.cancel_drag(&token)); } }
                        InteractionPhase::Ended => {
                            let Some(token) = token.borrow_mut().take() else { return; };
                            let target = drop_target(&targets.borrow(), event.position);
                            if let Some(target) = target {
                                if model.update(|model| model.drop_drag(&token, target)).is_err() {
                                    let _ = model.update(|model| model.cancel_drag(&token));
                                }
                            } else {
                                let cancelled = model.update(|model| model.cancel_drag(&token)).is_ok();
                                if cancelled && !contains(targets.borrow().root, event.position)
                                    && event.position.iter().all(|value| value.is_finite() && value.abs() <= 1e6) {
                                    if let Some(float) = &floating {
                                    float(key, DockBounds { origin: event.position, size: [640., 480.] });
                                } }
                            }
                        }
                        _ => {}
                    }
                });
                tab.descriptor.keyboard.key_down = true;
                let cleanup_drag = drag.clone(); let cleanup_model = state.model.clone();
                jadix::portable::on_cleanup(move || {
                    let token = cleanup_drag.borrow_mut().take();
                    if let Some(token) = token { let _ = cleanup_model.borrow_mut().cancel_drag(&token); }
                });
                let cancel = state.clone();
                tab = tab.and_on_key(move |event| { if event.key == "Escape" {
                    let token = drag.borrow_mut().take(); if let Some(token) = token { let _ = cancel.update(|model| model.cancel_drag(&token)); }
                } });
                tabs.children.push(tab);
                if record.closable {
                    let close = state.clone();
                    tabs.children.push(button(&format!("{id}__close_{key}")).text(format!("Close {}", record.title))
                        .on_click(move || { let _ = close.update(|model| model.close(key)); }).into());
                }
                let visible = state.clone();
                let mut panel = Element::new().id(format!("{id}__panel_{key}")).flex_grow(1.).min_w(0.).min_h(0.)
                    .child(pool.take(key.to_string(), || pane(key)));
                panel.descriptor.accessibility.role = Some(SemanticRole::Region);
                panel.descriptor.accessible_name = Some(record.title);
                panel.visibility = Some(Rc::new(move || model_node(&visible, stack,
                    |node| matches!(node, DockNode::Stack {active, ..} if *active == key)).unwrap_or(false)));
                content.children.push(panel);
            }
            root.children.extend([tabs, content]); root
        }
    }
}
fn contains(frame: Rect, point: [f64; 2]) -> bool {
    point.iter().all(|value| value.is_finite()) && frame.width > 0. && frame.height > 0.
        && point[0] >= frame.x && point[0] <= frame.x + frame.width && point[1] >= frame.y && point[1] <= frame.y + frame.height
}
fn drop_target(geometry: &Geometry, point: [f64; 2]) -> Option<DropLocation> {
    for (&(stack, index), &frame) in &geometry.tabs {
        if contains(frame, point) { return Some(DropLocation::Tab { stack, index: index + usize::from(point[0] > frame.x + frame.width / 2.) }); }
    }
    for (&stack, &frame) in &geometry.stacks {
        if !contains(frame, point) { continue; }
        let x = (point[0] - frame.x) / frame.width; let y = (point[1] - frame.y) / frame.height;
        let edge = if x < 0.2 { Some(DockEdge::Left) } else if x > 0.8 { Some(DockEdge::Right) }
            else if y < 0.2 { Some(DockEdge::Top) } else if y > 0.8 { Some(DockEdge::Bottom) } else { None };
        return Some(if let Some(edge) = edge { DropLocation::Split { stack, edge } }
            else { DropLocation::Tab { stack, index: geometry.tabs.keys().filter(|(key, _)| *key == stack).count() } });
    }
    None
}

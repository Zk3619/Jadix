use jadix_core::presentation::{
    NodeContent, NodeDescriptor, NodeId, PresentationNode, Rect, RootId, RootLifetime,
};
use std::{cell::RefCell, collections::BTreeMap, rc::Rc};

pub use jadix_core::presentation;
pub use jadix_core::hosted;
pub use jadix_core::emoji;
pub use jadix_core::clock;
pub use jadix_core::error_boundary;
use error_boundary::run_owned_callback;
pub use jadix_core::range;
pub use jadix_core::{commands, dock, resizable, roving};
pub use jadix_core::schema;
pub use jadix_core::toast;
pub use jadix_core::hover_intent;
pub use jadix_core::tree as tree_model;
pub use jadix_core::table as table_model;
pub use jadix_core::reactivity::*;
pub mod virtual_list;
pub use virtual_list::{virtual_list, VirtualListBuilder, VirtualListState};
pub mod window;
pub use window::{window, Window, WindowBuilder, WindowProps};

#[cfg(feature = "macros")]
#[doc(hidden)]
pub use jadix_macros::ui_with_runtime as __ui_with_runtime;
#[cfg(all(feature = "macros", not(feature = "gpu")))]
#[macro_export]
macro_rules! ui {
    ($($tokens:tt)*) => { $crate::__ui_with_runtime!($crate; $($tokens)*) };
}

#[cfg(feature = "macros")]
#[macro_export]
macro_rules! portable_ui {
    ($($tokens:tt)*) => { $crate::portable::__ui_with_runtime!($crate::portable; $($tokens)*) };
}

pub use jadix_core::event::{Handler, ZeroArgFn};
pub type Base = Option<Box<dyn FnOnce(Element) -> Element>>;
pub type DescriptorBinding = Rc<dyn Fn(&mut NodeDescriptor)>;
pub type NavigationHandler =
    Rc<RefCell<Box<dyn FnMut(presentation::NavigationKey) -> Option<String>>>>;
type ImageLoadHandler = Box<dyn FnMut(&presentation::ImageLoadEvent)>;

#[derive(Clone)]
pub struct PopoverBinding {
    pub anchor: Signal<String>,
    pub open: Signal<bool>,
    pub placement: presentation::PopoverPlacement,
    pub gap: u16,
    pub close_on_outside_click: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PopoverOpenChangeReason {
    TriggerPress,
    PressOutside,
    Cancel,
}

pub struct Element {
    pub children: Vec<Element>,
    pub template_id: Option<String>,
    pub descriptor: NodeDescriptor,
    pub on_click: Option<Rc<RefCell<Box<dyn FnMut()>>>>,
    pub on_hover: Option<Rc<RefCell<Box<dyn FnMut(bool)>>>>,
    on_focus_change: Option<Box<dyn FnMut(bool, bool)>>,
    pub on_pointer_move: Option<Rc<RefCell<Box<dyn FnMut([f64; 2])>>>>,
    pub on_submit: Option<Rc<RefCell<Box<dyn FnMut()>>>>,
    pub on_composition_change: Option<Rc<RefCell<Box<dyn FnMut(Option<String>)>>>>,
    pub input: Option<Signal<String>>,
    pub checked: Option<Signal<bool>>,
    pub dialog_open: Option<Signal<bool>>,
    pub popover: Option<PopoverBinding>,
    pub deferred_children: Option<Box<dyn Fn() -> Element>>,
    pub text_binding: Option<Rc<dyn Fn() -> String>>,
    pub visibility: Option<Rc<dyn Fn() -> bool>>,
    pub focus_restore_target: Option<String>,
    pub descriptor_binding: Option<DescriptorBinding>,
    /// Evaluated after all children refresh, then resolved against live IDs.
    pub accessibility_binding: Option<Rc<dyn Fn() -> (Option<String>, Vec<String>)>>,
    pub on_navigation: Option<NavigationHandler>,
    navigation_group: Option<bool>,
    on_interaction: Option<Box<dyn FnMut(&presentation::interaction::InteractionEvent)>>,
    on_file_drop: Option<Box<dyn FnMut(&presentation::interaction::FileDropEvent)>>,
    on_key: Option<Box<dyn FnMut(&presentation::keyboard::KeyEvent)>>,
    on_shortcut: Option<Box<dyn FnMut(&str)>>,
    on_accessibility_action: Option<Box<dyn FnMut(&str, Option<f64>)>>,
    /// A one-shot focus request consumed by the host after projection.
    pub focus_requested: Option<Signal<bool>>,
    pub attrs: BTreeMap<String, String>,
    identity: Option<NodeId>,
    edit_revision: u64,
    composing: Option<String>,
    projected_visibility: Option<bool>,
    input_committed: String,
    popover_open: bool,
    on_popover_dismiss: Option<Box<dyn FnMut(PopoverOpenChangeReason)>>,
    on_popover_cancel: Option<Box<dyn FnMut(PopoverOpenChangeReason)>>,
    on_popover_lifecycle: Option<Box<dyn FnMut(bool)>>,
    on_popover_position: Option<Box<dyn FnMut(Option<presentation::PopoverPosition>)>>,
    on_image_load_event: Option<ImageLoadHandler>,
    on_scroll: Option<Box<dyn FnMut(f64, f64)>>,
    on_scroll_zoom: Option<Box<dyn FnMut(f64)>>,
    on_scroll_activity: Option<Box<dyn FnMut(presentation::scroll::ScrollActivity)>>,
    on_hosted: Option<Box<dyn FnMut(&str, &serde_json::Value)>>,
    children_binding: Option<Box<dyn FnMut(&mut Vec<Element>)>>,
    on_viewport: Option<Box<dyn FnMut(f64, f64)>>,
    on_layout: Option<Box<dyn FnMut(Rect) -> bool>>,
    owner: Option<OwnerId>,
    owned_scopes: Vec<OwnerHandle>,
    retained_key: Option<(u64, String)>,
}

/// Content retained by one `bind_keyed_tree` region, independent of its layout
/// parents. Each key is used once per arrangement; omitted content is disposed.
pub struct KeyedElements {
    region: u64,
    elements: BTreeMap<String, Element>,
    used: std::collections::BTreeSet<String>,
    owner: Option<OwnerId>,
}
impl KeyedElements {
    pub fn take(&mut self, key: impl Into<String>, create: impl FnOnce() -> Element) -> Element {
        let key = key.into();
        assert!(self.used.insert(key.clone()), "duplicate retained tree key");
        if let Some(element) = self.elements.remove(&key) { return element; }
        let build = || {
            let (scope, mut element) = create_scope(create);
            element.owned_scopes.push(scope);
            element
        };
        let mut element = if let Some(owner) = self.owner {
            run_owned_callback(owner, build).expect("retained tree owner was disposed")
        } else { build() };
        element.retained_key = Some((self.region, key));
        element
    }
}

impl Default for Element {
    fn default() -> Self {
        Self::new()
    }
}

impl Element {
    pub fn new() -> Self {
        Self {
            children: Vec::new(),
            template_id: None,
            descriptor: NodeDescriptor {
                content: NodeContent::Container,
                frame: Rect::default(),
                accessible_description: None,
                accessible_name: None,
                accessible_role: None,
                accessible_selected: None,
                accessible_expanded: None,
                interaction: Default::default(),
                keyboard: Default::default(),
                navigation_keys: Vec::new(),
                accessibility: Default::default(),
                accessibility_relations: Default::default(),
                style: Default::default(),
            },
            on_click: None,
            on_hover: None,
            on_focus_change: None,
            on_pointer_move: None,
            on_submit: None,
            on_composition_change: None,
            input: None,
            checked: None,
            dialog_open: None,
            popover: None,
            deferred_children: None,
            text_binding: None,
            visibility: None,
            focus_restore_target: None,
            descriptor_binding: None,
            accessibility_binding: None,
            on_navigation: None,
            navigation_group: None,
            on_interaction: None,
            on_file_drop: None,
            on_key: None,
            on_shortcut: None,
            on_accessibility_action: None,
            focus_requested: None,
            attrs: BTreeMap::new(),
            identity: None,
            edit_revision: 0,
            composing: None,
            projected_visibility: None,
            input_committed: String::new(),
            popover_open: false,
            on_popover_dismiss: None,
            on_popover_cancel: None,
            on_popover_lifecycle: None,
            on_popover_position: None,
            on_image_load_event: None,
            on_scroll: None,
            on_scroll_zoom: None,
            on_scroll_activity: None,
            on_hosted: None,
            children_binding: None,
            on_viewport: None,
            on_layout: None,
            owner: current_owner(),
            owned_scopes: Vec::new(),
            retained_key: None,
        }
    }

    pub fn hit_slop(mut self, edges: [f64; 4]) -> Self {
        self.descriptor.interaction.hit_slop = edges; self
    }

    /// Share arrow/Home/End focus movement among current descendant controls.
    /// Nested groups are independent; editors and controls with authored
    /// navigation keys keep their own keyboard behavior.
    pub fn roving_navigation(mut self, vertical: bool) -> Self {
        self.navigation_group = Some(vertical);
        self
    }

    /// Observe the actual viewport after native layout or resizing.
    pub fn on_viewport(mut self, mut callback: impl FnMut(f64, f64) + 'static) -> Self {
        let owner = current_owner().or(self.owner);
        self.on_viewport = Some(Box::new(move |width, height| {
            if let Some(owner) = owner { run_owned_callback(owner, || callback(width, height)); }
            else { callback(width, height); }
        })); self
    }

    /// Observe committed layout. Return true only when another projection is
    /// needed, for example after updating a measured virtual row's extent.
    pub fn on_layout(mut self, mut callback: impl FnMut(Rect) -> bool + 'static) -> Self {
        let owner = current_owner().or(self.owner);
        self.on_layout = Some(Box::new(move |frame| {
            if let Some(owner) = owner { run_owned_callback(owner, || callback(frame)).unwrap_or(false) }
            else { callback(frame) }
        })); self
    }
    pub fn pointer_events(mut self, mode: presentation::interaction::PointerEvents) -> Self {
        self.descriptor.interaction.pointer_events = mode; self
    }

    pub fn interaction(mut self, policy: presentation::interaction::InteractionPolicy) -> Self {
        self.descriptor.interaction = policy; self
    }
    pub fn on_interaction(mut self, mut callback: impl FnMut(&presentation::interaction::InteractionEvent) + 'static) -> Self {
        let owner = current_owner().or(self.owner);
        self.on_interaction = Some(Box::new(move |event| {
            if let Some(owner) = owner { run_owned_callback(owner, || callback(event)); }
            else { callback(event); }
        })); self
    }

    /// Opt into local OS file drops. URI delivery grants no filesystem permission.
    pub fn on_file_drop(mut self, mut callback: impl FnMut(&presentation::interaction::FileDropEvent) + 'static) -> Self {
        self.descriptor.interaction.file_drop = true;
        let owner = current_owner().or(self.owner);
        self.on_file_drop = Some(Box::new(move |event| {
            if let Some(owner) = owner { run_owned_callback(owner, || callback(event)); }
            else { callback(event); }
        })); self
    }

    /// Add component behavior without replacing the content's input handler.
    pub fn and_on_interaction(mut self, mut callback: impl FnMut(&presentation::interaction::InteractionEvent) + 'static) -> Self {
        let mut previous = self.on_interaction.take();
        self.on_interaction(move |event| {
            if let Some(previous) = &mut previous { previous(event); }
            callback(event);
        })
    }

    pub fn accessibility(mut self, semantics: presentation::accessibility::Accessibility) -> Self {
        self.descriptor.accessibility = semantics; self
    }
    pub fn on_accessibility_action(mut self, mut callback: impl FnMut(&str, Option<f64>) + 'static) -> Self {
        let owner = current_owner().or(self.owner);
        self.on_accessibility_action = Some(Box::new(move |id, value| {
            if let Some(owner) = owner { run_owned_callback(owner, || callback(id, value)); }
            else { callback(id, value); }
        })); self
    }

    pub fn keyboard(mut self, policy: presentation::keyboard::KeyboardPolicy) -> Self {
        self.descriptor.keyboard = policy; self
    }
    pub fn on_key(mut self, mut callback: impl FnMut(&presentation::keyboard::KeyEvent) + 'static) -> Self {
        let owner = current_owner().or(self.owner);
        self.on_key = Some(Box::new(move |event| {
            if let Some(owner) = owner { run_owned_callback(owner, || callback(event)); }
            else { callback(event); }
        })); self
    }

    /// Compose keyboard behavior while retaining the original owner context.
    pub fn and_on_key(mut self, mut callback: impl FnMut(&presentation::keyboard::KeyEvent) + 'static) -> Self {
        let mut previous = self.on_key.take();
        self.on_key(move |event| {
            if let Some(previous) = &mut previous { previous(event); }
            callback(event);
        })
    }
    pub fn on_shortcut(mut self, mut callback: impl FnMut(&str) + 'static) -> Self {
        let owner = current_owner().or(self.owner);
        self.on_shortcut = Some(Box::new(move |id| {
            if let Some(owner) = owner { run_owned_callback(owner, || callback(id)); }
            else { callback(id); }
        })); self
    }

    pub fn hosted(mut self, content: jadix_core::hosted::HostedContent) -> Result<Self, &'static str> {
        self.descriptor.content = NodeContent::Hosted { specification: content.to_json()? };
        Ok(self)
    }
    pub fn on_hosted(mut self, mut callback: impl FnMut(&str, &serde_json::Value) + 'static) -> Self {
        let owner = current_owner().or(self.owner);
        self.on_hosted = Some(Box::new(move |event, data| {
            if let Some(owner) = owner { run_owned_callback(owner, || callback(event, data)); }
            else { callback(event, data); }
        })); self
    }

    pub fn child(mut self, child: impl Into<Element>) -> Self {
        self.children.push(child.into());
        self
    }
    pub fn children(mut self, children: impl IntoIterator<Item = Element>) -> Self {
        self.children.extend(children);
        self
    }
    /// Retain a structural region until its key changes. Input fields outside the
    /// region keep their identity, selection and active composition.
    pub fn bind_children<K: Clone + PartialEq + 'static>(
        self,
        key: impl Fn() -> K + 'static,
        render: impl FnMut(K) -> Vec<Element> + 'static,
    ) -> Self {
        self.bind_children_with(key, render, |region| region)
    }
    /// Retain each keyed child's scope, native identity and local state across
    /// insertion, removal and reordering. The supplied index stays live.
    pub fn bind_keyed_children(
        mut self,
        keys: impl Fn() -> Vec<String> + 'static,
        mut render: impl FnMut(String, Signal<usize>) -> Element + 'static,
    ) -> Self {
        let mut previous: Vec<(String, Signal<usize>)> = Vec::new();
        self.children_binding = Some(Box::new(move |children| {
            let next = keys();
            let unique: std::collections::BTreeSet<_> = next.iter().collect();
            assert_eq!(unique.len(), next.len(), "duplicate child key");
            let mut old: BTreeMap<_, _> = previous.drain(..)
                .zip(std::mem::take(children)).map(|((key, index), child)| (key, (index, child))).collect();
            for (position, key) in next.into_iter().enumerate() {
                let (index, child) = old.remove(&key).unwrap_or_else(|| {
                    let (owner, (index, mut child)) = create_scope(|| {
                        let index = create_signal(position);
                        (index, render(key.clone(), index))
                    });
                    // Preserve the rendered layout and all scopes already owned by it.
                    child.owned_scopes.push(owner);
                    (index, child)
                });
                if index.get() != position { index.set(position); }
                previous.push((key, index));
                children.push(child);
            }
        }));
        self
    }
    /// Retain keyed content across changes to its layout parents. Arrangement
    /// scopes are rebuilt independently; omitted content and its effects die.
    pub fn bind_keyed_tree<K: Clone + PartialEq + 'static>(
        mut self,
        key: impl Fn() -> K + 'static,
        mut render: impl FnMut(K, &mut KeyedElements) -> Vec<Element> + 'static,
    ) -> Self {
        static NEXT_REGION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        let region = NEXT_REGION.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let owner = self.owner;
        let mut previous = None;
        fn collect(children: Vec<Element>, region: u64, pool: &mut BTreeMap<String, Element>) {
            for mut child in children {
                if let Some((id, key)) = &child.retained_key {
                    if *id == region {
                        let key = key.clone();
                        assert!(pool.insert(key, child).is_none(), "duplicate retained tree key");
                        continue;
                    }
                }
                collect(std::mem::take(&mut child.children), region, pool);
            }
        }
        self.children_binding = Some(Box::new(move |children| {
            let next = key();
            if previous.as_ref() == Some(&next) { return; }
            let mut pool = KeyedElements { region, elements: BTreeMap::new(),
                used: Default::default(), owner };
            collect(std::mem::take(children), region, &mut pool.elements);
            let (scope, elements) = create_scope(|| render(next.clone(), &mut pool));
            let mut arrangement = Element::new().flex_grow(1.).children(elements);
            arrangement.owned_scopes.push(scope);
            *children = vec![arrangement];
            previous = Some(next);
        }));
        self
    }
    /// Style the scope-owning region, for example to fill its parent's remaining height.
    pub fn bind_children_with<K: Clone + PartialEq + 'static>(
        mut self,
        key: impl Fn() -> K + 'static,
        mut render: impl FnMut(K) -> Vec<Element> + 'static,
        style: impl Fn(Element) -> Element + 'static,
    ) -> Self {
        let mut previous = None;
        self.children_binding = Some(Box::new(move |children| {
            let next = key();
            if previous.as_ref() != Some(&next) {
                let (scope, elements) = create_scope(|| render(next.clone()));
                // The replacement owns its bindings and disposes them with its subtree.
                let mut region = style(Element::new().children(elements));
                region.owned_scopes.push(scope);
                *children = vec![region];
                previous = Some(next);
            }
        }));
        self
    }
    pub fn w(mut self, width: f32) -> Self {
        self.descriptor.style.width = Some(width);
        self
    }
    pub fn h(mut self, height: f32) -> Self {
        self.descriptor.style.height = Some(height);
        self
    }
    pub fn flex_grow(mut self, grow: f32) -> Self {
        self.descriptor.style.grow = grow;
        self
    }
    pub fn flex_shrink(mut self, shrink: f32) -> Self {
        self.descriptor.style.shrink = shrink;
        self
    }
    pub fn min_w(mut self, width: f32) -> Self {
        self.descriptor.style.min_width = width;
        self
    }
    pub fn min_h(mut self, height: f32) -> Self {
        self.descriptor.style.min_height = height;
        self
    }
    pub fn gap(mut self, gap: f32) -> Self {
        self.descriptor.style.gap = gap;
        self
    }
    pub fn items(mut self, alignment: presentation::style::AlignItems) -> Self {
        self.descriptor.style.align_items = Some(alignment);
        self
    }

    /// Configure the AppKit popover panel's opacity transitions in milliseconds.
    pub fn popover_fade(mut self, enter_ms: u16, exit_ms: u16) -> Self {
        self.descriptor.style.popover_enter_ms = Some(enter_ms);
        self.descriptor.style.popover_exit_ms = Some(exit_ms);
        self
    }
    pub fn view_appearance(mut self, appearance: presentation::style::ViewAppearance) -> Self {
        self.descriptor.style.view = Some(appearance);
        self
    }
    pub fn text_appearance(mut self, appearance: presentation::style::TextAppearance) -> Self {
        self.descriptor.style.text = Some(appearance);
        self
    }
    /// Set the AppKit panel's endpoints for the configured enter and exit durations.
    pub fn popover_appearance(
        mut self,
        starting: Option<presentation::style::PopoverAppearance>,
        ending: Option<presentation::style::PopoverAppearance>,
    ) -> Self {
        self.descriptor.style.popover_starting = starting;
        self.descriptor.style.popover_ending = ending;
        self
    }
    pub fn p(mut self, padding: f32) -> Self {
        self.descriptor.style.padding = [padding; 4];
        self
    }
    /// Marks an AppKit window drag region or excludes this subtree from an inherited region.
    pub fn window_drag(mut self, enabled: bool) -> Self {
        self.descriptor.style.window_drag = Some(enabled);
        self
    }
    /// Sets the image fit in native hosts that support it.
    pub fn object_fit(mut self, fit: presentation::style::ImageFit) -> Self {
        self.descriptor.style.image_fit = Some(fit);
        self
    }

    pub fn object_position(mut self, position: presentation::style::ImagePosition) -> Self {
        self.descriptor.style.image_position = Some(position);
        self
    }
    pub fn image_placeholder(mut self, source: impl Into<String>) -> Self {
        self.descriptor.style.image_placeholder = Some(source.into());
        self
    }
    pub fn image_blur_radius(mut self, radius: f32) -> Self {
        self.descriptor.style.image_blur_radius = Some(radius);
        self
    }

    /// Menu viewports scroll without scrollers or elastic overshoot.
    pub fn menu_scroll(self) -> Self {
        self.scrollbars(false).scroll_bounces(false)
    }

    pub fn image_fade_ms(mut self, duration: u32) -> Self {
        self.descriptor.style.image_fade_ms = Some(duration);
        self
    }

    pub fn on_image_load_event(
        mut self,
        callback: impl FnMut(&presentation::ImageLoadEvent) + 'static,
    ) -> Self {
        self.on_image_load_event = Some(Box::new(callback));
        self
    }

    /// Observe the absolute scroll offset in logical pixels.
    pub fn on_scroll(mut self, callback: impl FnMut(f64, f64) + 'static) -> Self {
        self.on_scroll = Some(Box::new(callback));
        self
    }

    /// Show or hide both scrollers on a native scroll view.
    pub fn scrollbars(mut self, visible: bool) -> Self {
        self.descriptor.style.scrollbars = [visible; 2];
        self
    }

    /// Native indicator insets in logical points: top, right, bottom, left.
    pub fn scrollbar_inset(mut self, edges: [f32; 4]) -> Self {
        self.descriptor.style.scrollbar_inset = Some(edges);
        self
    }

    /// Set elastic scrolling on hosts that support it.
    pub fn scroll_bounces(mut self, enabled: bool) -> Self {
        self.descriptor.style.scroll_bounces = Some(enabled);
        self
    }

    pub fn scroll_refresh(mut self, policy: presentation::scroll::RefreshControl) -> Self {
        self.descriptor.style.scroll_refresh = Some(policy); self
    }
    pub fn on_scroll_activity(mut self, mut callback: impl FnMut(presentation::scroll::ScrollActivity) + 'static) -> Self {
        let owner = current_owner().or(self.owner);
        self.on_scroll_activity = Some(Box::new(move |activity| {
            if let Some(owner) = owner { run_owned_callback(owner, || callback(activity)); }
            else { callback(activity); }
        })); self
    }
    pub fn scroll_snap(mut self, policy: presentation::scroll::ScrollSnap) -> Self {
        self.descriptor.style.scroll_snap = Some(policy); self
    }
    pub fn scroll_enabled(mut self, enabled: bool) -> Self {
        self.descriptor.style.scroll_enabled = Some(enabled); self
    }
    pub fn keyboard_taps(mut self, policy: presentation::style::KeyboardTaps) -> Self {
        self.descriptor.style.keyboard_taps = Some(policy);
        self
    }
    pub fn keyboard_dismiss_mode(mut self, mode: presentation::style::KeyboardDismissMode) -> Self {
        self.descriptor.style.keyboard_dismiss_mode = Some(mode);
        self
    }

    /// Behavior of declarative scroll targets; user scrolling keeps the host's policy.
    pub fn scroll_behavior(mut self, behavior: presentation::style::ScrollBehavior) -> Self {
        self.descriptor.style.scroll_behavior = Some(behavior);
        self
    }

    pub fn scroll_zoom(mut self, policy: jadix_core::scroll_zoom::ScrollZoom) -> Self {
        self.descriptor.style.scroll_zoom = Some(policy);
        self
    }

    pub fn on_scroll_zoom(mut self, callback: impl FnMut(f64) + 'static) -> Self {
        self.on_scroll_zoom = Some(Box::new(callback));
        self
    }

    pub fn flex_col(mut self) -> Self {
        self.descriptor.style.axis = presentation::style::Axis::Column;
        self
    }
    pub fn flex_row(mut self) -> Self {
        self.descriptor.style.axis = presentation::style::Axis::Row;
        self
    }
    pub fn text_size(mut self, size: f32) -> Self {
        self.descriptor.style.font_size = size;
        self
    }
    pub fn text_color(mut self, color: [u8; 4]) -> Self {
        self.descriptor.style.foreground = color;
        self
    }
    pub fn bg(mut self, color: [u8; 4]) -> Self {
        self.descriptor.style.background = color;
        self
    }
    pub fn text(mut self, value: impl ToString) -> Self {
        self.descriptor.content = NodeContent::Text(value.to_string());
        self
    }
    pub fn id(mut self, id: impl ToString) -> Self {
        self.template_id = Some(id.to_string());
        self
    }
    pub fn template_id(self, id: impl ToString) -> Self {
        self.id(id)
    }
    pub fn set_attr(&mut self, name: impl Into<String>, value: impl ToString) {
        self.attrs.insert(name.into(), value.to_string());
    }
    pub fn template_hash(self, _: &str) -> Self {
        self
    }
    pub fn hot_reload_node_type(self, _: &str) -> Self {
        self
    }
    pub fn hot_reload_declaration_base(self, _: &[&str], _: bool) -> Self {
        self
    }
    pub fn hot_reload_description(self, _: core::tree::TemplateDescription) -> Self {
        self
    }

    pub fn on_click(mut self, mut callback: impl FnMut() + 'static) -> Self {
        let owner = current_owner().or(self.owner);
        self.on_click = Some(Rc::new(RefCell::new(Box::new(move || {
            if let Some(owner) = owner {
                run_owned_callback(owner, &mut callback);
            } else {
                callback();
            }
        }))));
        self
    }

    /// Pointer presence uses the same owner context as activation callbacks.
    /// Observe actual pointer motion without changing keyboard focus.
    pub fn on_pointer_move(mut self, mut handler: impl FnMut([f64; 2]) + 'static) -> Self {
        let owner = current_owner().or(self.owner);
        self.on_pointer_move = Some(Rc::new(RefCell::new(Box::new(move |point| {
            if let Some(owner) = owner { run_owned_callback(owner, || handler(point)); }
            else { handler(point); }
        }))));
        self
    }
    pub fn on_hover(mut self, mut callback: impl FnMut(bool) + 'static) -> Self {
        self.descriptor.interaction.hover = true;
        let owner = current_owner().or(self.owner);
        self.on_hover = Some(Rc::new(RefCell::new(Box::new(move |inside| {
            if let Some(owner) = owner { run_owned_callback(owner, || callback(inside)); }
            else { callback(inside); }
        }))));
        self
    }

    /// Observe logical focus and keyboard-visible focus in the component owner.
    pub fn and_on_focus_change(mut self, mut callback: impl FnMut(bool, bool) + 'static) -> Self {
        let owner = current_owner().or(self.owner);
        let mut previous = self.on_focus_change.take();
        self.on_focus_change = Some(Box::new(move |focused, visible| {
            if let Some(previous) = &mut previous { previous(focused, visible); }
            if let Some(owner) = owner { run_owned_callback(owner, || callback(focused, visible)); }
            else { callback(focused, visible); }
        }));
        self
    }

    #[doc(hidden)]
    pub fn on_popover_dismiss(
        mut self,
        callback: impl FnMut(PopoverOpenChangeReason) + 'static,
    ) -> Self {
        self.on_popover_dismiss = Some(Box::new(callback));
        self
    }

    #[doc(hidden)]
    pub fn on_popover_cancel(
        mut self,
        callback: impl FnMut(PopoverOpenChangeReason) + 'static,
    ) -> Self {
        self.on_popover_cancel = Some(Box::new(callback));
        self
    }

    #[doc(hidden)]
    pub fn on_popover_lifecycle(mut self, callback: impl FnMut(bool) + 'static) -> Self {
        self.on_popover_lifecycle = Some(Box::new(callback));
        self
    }

    #[doc(hidden)]
    pub fn on_popover_position(
        mut self,
        callback: impl FnMut(Option<presentation::PopoverPosition>) + 'static,
    ) -> Self {
        self.on_popover_position = Some(Box::new(callback));
        self
    }

    fn refresh(&mut self) {
        if let Some(owner) = self.owner {
            run_owned_callback(owner, || self.refresh_values());
        } else {
            self.refresh_values();
        }
    }

    fn refresh_values(&mut self) {
        if let Some(binding) = &mut self.children_binding {
            binding(&mut self.children);
        }
        if self.visibility.is_some() && self.is_visible() {
            if let Some(factory) = self.deferred_children.take() {
                self.children.push(component_root(factory));
            }
        }
        if let Some(binding) = &self.popover {
            let open = binding.open.get();
            if self.popover_open != open {
                self.edit_revision = self.edit_revision.saturating_add(1);
                self.popover_open = open;
            }
            if open {
                if let Some(factory) = self.deferred_children.take() {
                    self.children.push(component_root(factory));
                }
            }
        }
        if let (Some(binding), NodeContent::Dialog { open, revision, .. }) =
            (self.dialog_open, &mut self.descriptor.content)
        {
            let value = binding.get();
            if *open != value {
                self.edit_revision = self.edit_revision.saturating_add(1);
                *open = value;
            }
            *revision = self.edit_revision;
            if value {
                if let Some(factory) = self.deferred_children.take() {
                    self.children.push(component_root(factory));
                }
            }
        }
        if let Some(binding) = &self.text_binding {
            self.descriptor.content = NodeContent::Text(binding());
        }
        if let Some(value) = self.input {
            let committed = value.get();
            if committed != self.input_committed {
                self.input_committed = committed.clone();
                self.composing = None;
                self.edit_revision = self.edit_revision.saturating_add(1);
            }
            if let NodeContent::Input {
                value: current,
                revision,
                ..
            }
            | NodeContent::Textarea {
                value: current,
                revision,
                ..
            }
            | NodeContent::Select {
                value: current,
                revision,
                ..
            } = &mut self.descriptor.content
            {
                let next = self.composing.clone().unwrap_or(committed);
                if *current != next {
                    *current = next;
                }
                *revision = self.edit_revision;
            }
        }
        if let (
            Some(binding),
            NodeContent::Checkbox {
                checked, revision, ..
            }
            | NodeContent::Switch {
                checked, revision, ..
            },
        ) = (self.checked, &mut self.descriptor.content)
        {
            let value = binding.get();
            if *checked != value {
                self.edit_revision = self.edit_revision.saturating_add(1);
                *checked = value;
            }
            *revision = self.edit_revision;
        }
        if let Some(binding) = &self.descriptor_binding {
            binding(&mut self.descriptor);
        }
    }

    fn is_visible(&self) -> bool {
        self.projected_visibility
            .unwrap_or_else(|| self.evaluate_visibility())
    }

    fn evaluate_visibility(&self) -> bool {
        let evaluate = || self.visibility.as_ref().is_none_or(|binding| binding());
        if let Some(owner) = self.owner { run_owned_callback(owner, evaluate).unwrap_or(false) }
        else { evaluate() }
    }

    // Retain closed overlay children for host exit transitions, but exclude
    // them from input, focus and editor queries until the overlay opens again.
    fn children_are_visible(&self) -> bool {
        (self.popover.is_none() || self.popover_open)
            && !matches!(self.descriptor.content, NodeContent::Dialog { open: false, .. })
    }

    fn latch_visibility(&mut self, defer_changes: bool) {
        if !defer_changes || self.projected_visibility.is_none() {
            self.projected_visibility =
                Some(self.evaluate_visibility());
        }
    }

    fn has_active_composition(&self) -> bool {
        self.identity.is_some()
            && self.is_visible()
            && (self.composing.is_some()
                && self
                    .input
                    .is_some_and(|binding| binding.get() == self.input_committed)
                || self.children_are_visible() && self.children.iter().any(Self::has_active_composition))
    }

    fn forget_identities(&mut self) {
        self.identity = None;
        self.composing = None;
        for child in &mut self.children {
            child.forget_identities();
        }
    }
}

pub trait IntoElementList {
    fn into_element_list(self) -> Vec<Element>;
}
impl IntoElementList for Element {
    fn into_element_list(self) -> Vec<Element> {
        vec![self]
    }
}
impl IntoElementList for String {
    fn into_element_list(self) -> Vec<Element> {
        vec![Element::new().text(self)]
    }
}
impl IntoElementList for &str {
    fn into_element_list(self) -> Vec<Element> {
        vec![Element::new().text(self)]
    }
}
impl IntoElementList for Vec<Element> {
    fn into_element_list(self) -> Vec<Element> {
        self
    }
}
impl IntoElementList for Option<Element> {
    fn into_element_list(self) -> Vec<Element> {
        self.into_iter().collect()
    }
}

pub mod core {
    pub use jadix_core::{reactivity, store};
    pub mod tree {
        pub use super::super::{Element, Handler, IntoElementList, ZeroArgFn};
        pub use jadix_layout::{IntoF32, IntoF32Raw};
        pub fn parse_bool_like(value: impl ParseBoolLike) -> bool {
            value.to_bool()
        }
        pub trait ParseBoolLike {
            fn to_bool(self) -> bool;
        }
        impl ParseBoolLike for bool {
            fn to_bool(self) -> bool {
                self
            }
        }
        impl ParseBoolLike for &str {
            fn to_bool(self) -> bool {
                matches!(
                    self.trim().to_ascii_lowercase().as_str(),
                    "true" | "1" | "yes" | "on"
                )
            }
        }
        impl ParseBoolLike for String {
            fn to_bool(self) -> bool {
                self.as_str().to_bool()
            }
        }
        impl ParseBoolLike for &String {
            fn to_bool(self) -> bool {
                self.as_str().to_bool()
            }
        }
        pub fn build_ui_root(factory: impl FnOnce() -> Element) -> Element {
            factory()
        }
        pub fn build_ui_child<Child: IntoElementList>(
            factory: impl FnOnce() -> Child,
        ) -> Vec<Element> {
            factory().into_element_list()
        }
        pub use jadix_hot_reload::TemplateDescription;
    }
}

pub mod primitives {
    use super::Element;
    pub fn div() -> Element {
        Element::new()
    }
    pub fn vstack() -> Element {
        div().flex_col()
    }
    pub fn scroll() -> Element {
        let mut element = Element::new();
        element.descriptor.content = super::NodeContent::Scroll;
        element
    }
    pub fn hstack() -> Element {
        div().flex_row()
    }
    pub fn text(value: impl ToString) -> Element {
        Element::new().text(value)
    }
}

pub mod style {
    pub use jadix_theme::*;
    pub fn apply_stylesheet_rules_tree(element: super::Element) -> super::Element {
        element
    }
}

pub mod utils {
    pub mod popup {
        pub const FLOATING_TRIGGER_GAP: f32 = 4.0;
    }
    pub fn children_none_if_vacuous(children: Option<super::Element>) -> Option<super::Element> {
        children
    }
}

pub mod app {
    pub mod portal_overlay {
        #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
        pub enum ScrollFreeze {
            #[default]
            None,
            Page,
            Overlay,
            All,
        }

        pub fn parse_scroll_freeze(value: &str) -> ScrollFreeze {
            match value.trim().to_ascii_lowercase().as_str() {
                "page" | "page-only" | "page_only" | "true" | "1" | "yes" => ScrollFreeze::Page,
                "overlay" | "floating" | "float" => ScrollFreeze::Overlay,
                "all" | "app" | "program" | "full" | "both" => ScrollFreeze::All,
                "none" | "off" | "false" | "0" | "no" | "" => ScrollFreeze::None,
                _ => ScrollFreeze::Page,
            }
        }
    }
    pub mod reactive {
        use super::super::{Element, Rc};
        pub fn bind_text(_: impl ToString, binding: impl Fn() -> String + 'static) -> Element {
            let mut element = Element::new().text(binding());
            element.text_binding = Some(Rc::new(binding));
            element
        }
    }
}

pub mod authoring {
    pub type BaseComponentBuilder = super::Element;
    #[derive(Default)]
    pub struct SlotChrome {
        pub class: Option<String>,
        pub base: super::Base,
    }
    impl SlotChrome {
        pub fn from_opts(class: Option<String>, base: super::Base) -> Self {
            Self { class, base }
        }
        pub fn apply(self, mut element: super::Element) -> super::Element {
            if self.class.is_some_and(|class| !class.is_empty()) {
                element.set_attr("jadix_native_unsupported", "slot classes");
            }
            match self.base {
                Some(base) => base(element),
                None => element,
            }
        }
    }
    pub mod native {
        use super::super::*;

        #[derive(Clone, Debug, PartialEq, Eq)]
        pub enum ProjectionError {
            Disposed,
            StaleNode,
            InvalidEvent,
            InvalidGeometry,
            Unsupported(&'static str),
        }

        pub struct ElementProjection {
            lifetime: RootLifetime,
            focus_request: RefCell<Option<String>>,
            group_focus_request: RefCell<Option<NodeId>>,
            #[cfg(feature = "i18n")]
            locale_context: Option<Rc<jadix_core::i18n::LocaleContext>>,
        }
        impl ElementProjection {
            /// Feed actual layout back to mounted owners, after a successful
            /// host presentation. Foreign, retired and hidden nodes are ignored.
            pub fn apply_layout(&self, element: &mut Element, mut frame: impl FnMut(NodeId) -> Option<Rect>) -> bool {
                if self.lifetime.is_disposed() { return false; }
                fn visit(element: &mut Element, root: RootId, frame: &mut impl FnMut(NodeId) -> Option<Rect>, depth: usize) -> bool {
                    if depth > 256 || !element.is_visible() { return false; }
                    let mut changed = false;
                    if let Some(bounds) = element.identity.filter(|id| id.root == root)
                        .and_then(&mut *frame).filter(|bounds| bounds.is_valid()) {
                        if let Some(callback) = &mut element.on_layout { changed |= callback(bounds); }
                    }
                    if element.children_are_visible() {
                        for child in &mut element.children { changed |= visit(child, root, frame, depth + 1); }
                    }
                    changed
                }
                visit(element, self.lifetime.id(), &mut frame, 0)
            }

            pub fn new(root: RootId) -> Self {
                Self::with_lifetime(RootLifetime::new(root))
            }
            pub fn with_lifetime(lifetime: RootLifetime) -> Self {
                Self {
                    lifetime,
                    focus_request: RefCell::new(None),
                    group_focus_request: RefCell::new(None),
                    #[cfg(feature = "i18n")]
                    locale_context: None,
                }
            }
            /// Share the native root's locale for all reactive bindings and callbacks.
            #[cfg(feature = "i18n")]
            pub fn with_locale_context(mut self, context: Rc<jadix_core::i18n::LocaleContext>) -> Self {
                self.locale_context = Some(context);
                self
            }
            pub fn snapshot(
                &self,
                element: &mut Element,
            ) -> Result<PresentationNode, ProjectionError> {
                #[cfg(feature = "i18n")]
                if let Some(context) = &self.locale_context {
                    return context.with(|| self.snapshot_inner(element));
                }
                self.snapshot_inner(element)
            }
            fn snapshot_inner(&self, element: &mut Element) -> Result<PresentationNode, ProjectionError> {
                jadix_core::clock::flush();
                // Discard partially refreshed content after an error, including
                // a failed nested fallback. Bound malformed recovery cycles.
                for _ in 0..32 {
                    let revision = error_boundary::error_revision();
                    let result = self.snapshot_once(element);
                    if revision == error_boundary::error_revision() { return result; }
                }
                Err(ProjectionError::Unsupported("error fallback did not settle"))
            }
            fn snapshot_once(&self, element: &mut Element) -> Result<PresentationNode, ProjectionError> {
                if self.lifetime.is_disposed() {
                    return Err(ProjectionError::Disposed);
                }
                let defer_visibility = element.has_active_composition();
                element.latch_visibility(defer_visibility);
                if !element.is_visible() {
                    element.forget_identities();
                    return Err(ProjectionError::Unsupported("hidden projection root"));
                }
                let mut anchors = BTreeMap::new();
                let mut popovers = BTreeMap::new();
                let mut relations = BTreeMap::new();
                let mut tree = snapshot(
                    element,
                    self.lifetime.id(),
                    0,
                    &mut anchors,
                    &mut popovers,
                    &mut relations,
                    defer_visibility,
                    None,
                    false,
                )?;
                let mut pending = vec![&mut tree];
                while let Some(node) = pending.pop() {
                    if let Some(binding) = relations.get(&node.id) {
                        let (active, controls) = binding();
                        node.descriptor.accessibility_relations = presentation::AccessibilityRelations::resolve_named(
                            active.as_deref(), controls, |name| anchors.get(name).filter(|ids| ids.len() == 1).map(|ids| ids[0]),
                        );
                    }
                    if let Some((binding, revision, open)) = popovers.get(&node.id) {
                        let candidates = anchors
                            .get(&binding.anchor.get())
                            .ok_or(ProjectionError::Unsupported("missing Popover anchor"))?;
                        if candidates.len() != 1 {
                            return Err(ProjectionError::Unsupported("ambiguous Popover anchor"));
                        }
                        node.descriptor.content = NodeContent::Popover {
                            anchor: candidates[0],
                            placement: binding.placement,
                            gap: binding.gap,
                            open: *open,
                            close_on_outside_click: binding.close_on_outside_click,
                            revision: *revision,
                        };
                    }
                    pending.extend(node.children.iter_mut());
                }
                Ok(tree)
            }
            pub fn snapshot_with_focus(
                &self,
                element: &mut Element,
                focused: Option<NodeId>,
            ) -> Result<PresentationNode, ProjectionError> {
                let focused = focused.filter(|node| node.root == self.lifetime.id());
                let targets = focused
                    .and_then(|node| Self::focus_restore_targets(element, node, 0))
                    .unwrap_or_default();
                let tree = self.snapshot(element)?;
                let mut pending = vec![&tree];
                while let Some(node) = pending.pop() {
                    if Some(node.id) == focused {
                        return Ok(tree);
                    }
                    if !matches!(node.descriptor.content, NodeContent::Popover { open: false, .. } | NodeContent::Dialog { open: false, .. }) {
                        pending.extend(&node.children);
                    }
                }
                if let Some(target) = targets
                    .into_iter()
                    .find(|target| self.resolve_focus_target(element, target).is_some())
                {
                    self.focus_request.borrow_mut().get_or_insert(target);
                }
                Ok(tree)
            }

            fn focus_restore_targets(
                element: &Element,
                focused: NodeId,
                depth: usize,
            ) -> Option<Vec<String>> {
                if depth > 256 || !element.is_visible() {
                    return None;
                }
                let mut targets = if element.identity == Some(focused) {
                    Vec::new()
                } else {
                    element
                        .children
                        .iter()
                        .find_map(|child| Self::focus_restore_targets(child, focused, depth + 1))?
                };
                if let Some(target) = &element.focus_restore_target {
                    targets.push(target.clone());
                }
                Some(targets)
            }

            pub fn dispatch(
                &self,
                element: &mut Element,
                event: &presentation::PlatformEvent,
            ) -> Result<(), ProjectionError> {
                #[cfg(feature = "i18n")]
                if let Some(context) = &self.locale_context {
                    return context.with(|| self.dispatch_inner(element, event));
                }
                self.dispatch_inner(element, event)
            }
            fn dispatch_inner(&self, element: &mut Element, event: &presentation::PlatformEvent) -> Result<(), ProjectionError> {
                if self.lifetime.is_disposed() {
                    return Err(ProjectionError::Disposed);
                }
                if event.node.root != self.lifetime.id() {
                    return Err(ProjectionError::StaleNode);
                }
                let group_target = match &event.payload {
                    presentation::EventPayload::Navigate { key, shift: false, control: false } =>
                        navigation_target(element, event.node, *key),
                    _ => None,
                };
                let mut pending = vec![element];
                while let Some(element) = pending.pop() {
                    element.refresh();
                    let cancelling = matches!(&event.payload, presentation::EventPayload::Interaction(value)
                        if value.phase == presentation::interaction::InteractionPhase::Cancelled);
                    if (!element.is_visible() && !cancelling)
                        || (matches!(&event.payload, presentation::EventPayload::Interaction(_) | presentation::EventPayload::FileDrop(_)) && !cancelling
                            && element.descriptor.interaction.pointer_events == presentation::interaction::PointerEvents::None)
                        || (matches!(event.payload, presentation::EventPayload::AccessibilityAction { .. })
                        && element.descriptor.accessibility.hidden) {
                        continue;
                    }
                    if element.identity == Some(event.node) {
                        element.refresh();
                        if let presentation::EventPayload::Navigate {
                            key,
                            shift: false,
                            control: false,
                        } = &event.payload
                        {
                            if !element.descriptor.can_focus() || element.descriptor.accessibility.disabled
                                || element.composing.is_some()
                                || (group_target.is_none()
                                    && !matches!(element.descriptor.content, NodeContent::Button { .. })
                                    && !element.descriptor.navigation_keys.contains(key)) {
                                return Err(ProjectionError::InvalidEvent);
                            }
                            let target = if let Some(callback) = &element.on_navigation {
                                let invoke = || -> Result<_, ProjectionError> {
                                    Ok(callback.try_borrow_mut().map_err(|_| ProjectionError::InvalidEvent)?(*key))
                                };
                                match element.owner {
                                    Some(owner) => run_owned_callback(owner, invoke).unwrap_or(Ok(None))?,
                                    None => invoke()?,
                                }
                            } else if group_target.is_some() { None }
                            else { return Err(ProjectionError::InvalidEvent); };
                            *self.group_focus_request.borrow_mut() = if target.is_none() { group_target } else { None };
                            *self.focus_request.borrow_mut() = target;
                            return Ok(());
                        }
                        return dispatch(element, event);
                    }
                    if cancelling || element.children_are_visible() {
                        pending.extend(element.children.iter_mut());
                    }
                }
                Err(ProjectionError::StaleNode)
            }

            pub fn navigation_nodes(&self, element: &Element) -> Vec<NodeId> {
                if self.lifetime.is_disposed() {
                    return Vec::new();
                }
                let mut result = Vec::new();
                let mut pending = vec![(element, None)];
                while let Some(element) = pending.pop() {
                    let (element, navigation) = element;
                    if !element.is_visible() || element.descriptor.accessibility.hidden {
                        continue;
                    }
                    let navigation = element.navigation_group.or(navigation);
                    if element.on_navigation.is_some() || navigation.is_some() && navigation_member(&element.descriptor) {
                        if let Some(node) = element
                            .identity
                            .filter(|node| node.root == self.lifetime.id())
                        {
                            result.push(node);
                        }
                    }
                    if element.children_are_visible() {
                        pending.extend(element.children.iter().rev().map(|child| (child, navigation)));
                    }
                }
                result
            }

            pub fn take_focus_request(&self, element: &Element) -> Option<NodeId> {
                if self.lifetime.is_disposed() { return None; }
                let wanted = self.group_focus_request.borrow_mut().take();
                let mut group_target = None;
                let mut requested = None;
                let mut pending = vec![element];
                while let Some(element) = pending.pop() {
                    if !element.is_visible() || element.descriptor.accessibility.hidden { continue; }
                    if element.identity == wanted && element.descriptor.can_focus()
                        && !element.descriptor.accessibility.hidden && !element.descriptor.accessibility.disabled {
                        group_target = wanted;
                    }
                    if let Some(signal) = element.focus_requested.filter(|signal| signal.get()) {
                        signal.set(false);
                        requested = element.identity;
                    }
                    if element.children_are_visible() {
                        pending.extend(element.children.iter().rev());
                    }
                }
                let pending = self.focus_request.borrow_mut().take();
                requested.or(group_target).or_else(|| pending
                    .and_then(|wanted| self.resolve_focus_target(element, &wanted)))
            }

            /// Project visible editor insertion constraints. Values are UTF-16
            /// code units; malformed authoring attributes reject publication.
            pub fn input_limits(
                &self,
                element: &Element,
            ) -> Result<Vec<(NodeId, usize)>, ProjectionError> {
                let mut result = Vec::new();
                let mut pending = vec![element];
                while let Some(element) = pending.pop() {
                    if !element.is_visible() {
                        continue;
                    }
                    let limit = element
                        .attrs
                        .get("max_length")
                        .map(|value| {
                            value
                                .parse::<usize>()
                                .map_err(|_| ProjectionError::InvalidEvent)
                        })
                        .transpose()?
                        .or(element.descriptor.content.input_max_length());
                    if let Some(value) = limit {
                        if value > jadix_core::presentation::input::MAX_INPUT_LENGTH
                            || !matches!(
                                element.descriptor.content,
                                NodeContent::Input { .. } | NodeContent::Textarea { .. }
                            )
                        {
                            return Err(ProjectionError::InvalidEvent);
                        }
                        if let Some(id) =
                            element.identity.filter(|id| id.root == self.lifetime.id())
                        {
                            result.push((id, value));
                        }
                    }
                    if element.children_are_visible() { pending.extend(&element.children); }
                }
                Ok(result)
            }

            /// Resolve one visible, currently projected template identity. Hidden
            /// pages and ambiguous names return None; no stale identity is kept.
            pub fn node_by_template(&self, element: &Element, wanted: &str) -> Option<NodeId> {
                if self.lifetime.is_disposed() {
                    return None;
                }
                let mut found = None;
                let mut pending = vec![element];
                while let Some(element) = pending.pop() {
                    if !element.is_visible() {
                        continue;
                    }
                    if element.template_id.as_deref() == Some(wanted) {
                        let id = element
                            .identity
                            .filter(|id| id.root == self.lifetime.id())?;
                        if found.replace(id).is_some() {
                            return None;
                        }
                    }
                    if element.children_are_visible() { pending.extend(&element.children); }
                }
                found
            }

            fn resolve_focus_target(&self, element: &Element, wanted: &str) -> Option<NodeId> {
                if self.lifetime.is_disposed() {
                    return None;
                }
                let mut result = None;
                let mut pending = vec![element];
                while let Some(element) = pending.pop() {
                    if !element.is_visible() {
                        continue;
                    }
                    if element.template_id.as_deref() == Some(wanted) {
                        if !element.descriptor.can_focus() {
                            return None;
                        }
                        let node = element
                            .identity
                            .filter(|node| node.root == self.lifetime.id())?;
                        if result.replace(node).is_some() {
                            return None;
                        }
                    }
                    if element.children_are_visible() { pending.extend(&element.children); }
                }
                result
            }
            pub fn dispose(&self) {
                self.lifetime.dispose();
                self.focus_request.borrow_mut().take();
                self.group_focus_request.borrow_mut().take();
            }
        }

        fn snapshot(
            element: &mut Element,
            root: RootId,
            depth: usize,
            anchors: &mut BTreeMap<String, Vec<NodeId>>,
            popovers: &mut BTreeMap<NodeId, (PopoverBinding, u64, bool)>,
            relations: &mut BTreeMap<NodeId, Rc<dyn Fn() -> (Option<String>, Vec<String>)>>,
            defer_visibility: bool,
            inherited_navigation: Option<bool>,
            inherited_ax_hidden: bool,
        ) -> Result<PresentationNode, ProjectionError> {
            if depth > 256 {
                return Err(ProjectionError::Unsupported("tree depth"));
            }
            element.refresh();
            if element.attrs.contains_key("jadix_native_unsupported") {
                return Err(ProjectionError::Unsupported("native component options"));
            }
            if element.attrs.contains_key("jadix_native_select_slot") {
                return Err(ProjectionError::Unsupported("Select slot outside Select"));
            }
            if !element.descriptor.style.is_valid() {
                return Err(ProjectionError::InvalidGeometry);
            }
            let node = *element.identity.get_or_insert_with(|| NodeId::new(root));
            if node.root != root {
                return Err(ProjectionError::StaleNode);
            }
            let mut descriptor = element.descriptor.clone();
            let ax_hidden = inherited_ax_hidden || descriptor.accessibility.hidden;
            let navigation = element.navigation_group.or(inherited_navigation);
            if !ax_hidden && navigation_member(&descriptor) {
                if let Some(vertical) = navigation {
                    descriptor.navigation_keys.extend(navigation_keys(vertical));
                }
            }
            if let Some(binding) = &element.accessibility_binding {
                let binding = binding.clone();
                let owner = element.owner;
                let binding: Rc<dyn Fn() -> (Option<String>, Vec<String>)> = Rc::new(move || {
                    if let Some(owner) = owner { run_owned_callback(owner, || binding()).unwrap_or_default() }
                    else { binding() }
                });
                relations.insert(node, binding);
            } else if element.attrs.contains_key("aria-activedescendant") || element.attrs.contains_key("aria-controls") {
                let active = element.attrs.get("aria-activedescendant").cloned();
                let controls = element.attrs.get("aria-controls").map(|value| value.split_whitespace().map(str::to_owned).collect::<Vec<_>>()).unwrap_or_default();
                relations.insert(node, Rc::new(move || (active.clone(), controls.clone())));
            }
            if let Some(value) = element.attrs.get("max_length") {
                let value: usize = value.parse().map_err(|_| ProjectionError::InvalidEvent)?;
                match &mut descriptor.content {
                    NodeContent::Input { max_length, .. }
                    | NodeContent::Textarea { max_length, .. } => *max_length = Some(value),
                    _ => return Err(ProjectionError::InvalidEvent),
                }
            }
            if !descriptor.accessibility.is_valid() || !descriptor.content.is_valid() || !descriptor.keyboard.is_valid() || !descriptor.interaction.is_valid() {
                return Err(ProjectionError::InvalidEvent);
            }
            if let Some(binding) = &element.popover {
                if !matches!(descriptor.content, NodeContent::Container) {
                    return Err(ProjectionError::Unsupported("Popover panel content"));
                }
                popovers.insert(
                    node,
                    (binding.clone(), element.edit_revision, element.popover_open),
                );
            }
            if let Some(id) = &element.template_id {
                anchors.entry(id.clone()).or_default().push(node);
            }
            let mut children = Vec::new();
            let button_content = matches!(descriptor.content, NodeContent::Button { .. })
                && element.attrs.contains_key("button:content");
            if !button_content && matches!(descriptor.content,
                NodeContent::Button { .. } | NodeContent::Checkbox { .. } | NodeContent::Switch { .. }) {
                let (NodeContent::Button { label, .. } | NodeContent::Checkbox { label, .. }
                    | NodeContent::Switch { label, .. }) = &mut descriptor.content else { unreachable!() };
                label.clear();
                for child in &mut element.children {
                    caption(child, label)?;
                }
            } else if let NodeContent::Text(text) = &mut descriptor.content {
                for child in &mut element.children {
                    caption(child, text)?;
                }
            } else if descriptor.content.accepts_children() {
                for child in &mut element.children {
                    child.latch_visibility(defer_visibility);
                    if child.is_visible() {
                        children.push(snapshot(
                            child,
                            root,
                            depth + 1,
                            anchors,
                            popovers,
                            relations,
                            defer_visibility,
                            navigation,
                            ax_hidden,
                        )?);
                    } else {
                        child.forget_identities();
                    }
                }
            } else if !element.children.is_empty() {
                return Err(ProjectionError::Unsupported("leaf children"));
            }
            if button_content && descriptor.accessible_name.is_none() {
                fn content_name(nodes: &[PresentationNode], name: &mut String) {
                    for node in nodes {
                        if node.descriptor.accessibility.hidden { continue; }
                        if let Some(label) = &node.descriptor.accessible_name {
                            name.push_str(label);
                        } else {
                            match &node.descriptor.content {
                                NodeContent::Text(text) => name.push_str(text),
                                NodeContent::Image { alt, .. } => name.push_str(alt),
                                _ => content_name(&node.children, name),
                            }
                        }
                    }
                }
                let mut name = String::new();
                content_name(&children, &mut name);
                if !name.is_empty() { descriptor.accessible_name = Some(name); }
            }
            Ok(PresentationNode {
                id: node,
                descriptor,
                children,
            })
        }

        fn navigation_keys(vertical: bool) -> [presentation::NavigationKey; 4] {
            use presentation::NavigationKey::*;
            [if vertical { Up } else { Left }, if vertical { Down } else { Right }, Home, End]
        }

        fn navigation_member(descriptor: &NodeDescriptor) -> bool {
            descriptor.can_focus() && !descriptor.accessibility.disabled
                && descriptor.navigation_keys.is_empty()
                && descriptor.accessibility.role != Some(presentation::accessibility::SemanticRole::Adjustable)
                && matches!(descriptor.content, NodeContent::Button { .. } | NodeContent::Checkbox { .. }
                    | NodeContent::Switch { .. } | NodeContent::Container | NodeContent::Text(_) | NodeContent::Image { .. })
        }

        fn navigation_target(element: &mut Element, source: NodeId, key: presentation::NavigationKey) -> Option<NodeId> {
            fn visit(element: &mut Element, group: Option<NodeId>, groups: &mut BTreeMap<NodeId, (bool, Vec<NodeId>)>, depth: usize) {
                if depth > 256 { return; }
                element.refresh();
                if !element.is_visible() || element.descriptor.accessibility.hidden { return; }
                let group = if let Some(vertical) = element.navigation_group {
                    let Some(id) = element.identity else { return; };
                    groups.insert(id, (vertical, Vec::new())); Some(id)
                } else { group };
                if navigation_member(&element.descriptor) {
                    if let (Some(group), Some(id)) = (group, element.identity) {
                        if let Some((_, members)) = groups.get_mut(&group) { members.push(id); }
                    }
                }
                if element.children_are_visible() && element.descriptor.content.accepts_children() {
                    for child in &mut element.children { visit(child, group, groups, depth + 1); }
                }
            }
            let mut groups = BTreeMap::new();
            visit(element, None, &mut groups, 0);
            for (vertical, members) in groups.values() {
                let Some(current) = members.iter().position(|id| *id == source) else { continue; };
                let keys = navigation_keys(*vertical);
                let next = if key == keys[0] { roving::roving_move(members.len(), current, -1) }
                    else if key == keys[1] { roving::roving_move(members.len(), current, 1) }
                    else if key == keys[2] { roving::roving_edge(members.len(), false) }
                    else if key == keys[3] { roving::roving_edge(members.len(), true) }
                    else { None };
                return next.map(|index| members[index]);
            }
            None
        }

        fn caption(element: &mut Element, output: &mut String) -> Result<(), ProjectionError> {
            element.refresh();
            match &element.descriptor.content {
                NodeContent::Text(text) => output.push_str(text),
                NodeContent::Container => {}
                _ => return Err(ProjectionError::Unsupported("button caption")),
            }
            for child in &mut element.children {
                caption(child, output)?;
            }
            Ok(())
        }

        fn dispatch(
            element: &mut Element,
            event: &presentation::PlatformEvent,
        ) -> Result<(), ProjectionError> {
            if let Some(owner) = element.owner {
                return run_owned_callback(owner, || dispatch_value(element, event)).unwrap_or(Ok(()));
            }
            dispatch_value(element, event)
        }

        fn dispatch_value(
            element: &mut Element,
            event: &presentation::PlatformEvent,
        ) -> Result<(), ProjectionError> {
            use presentation::EventPayload;
            if let (Some(binding), EventPayload::PopoverLifecycle { open }) =
                (&element.popover, &event.payload)
            {
                if binding.open.get() != *open {
                    return Err(ProjectionError::InvalidEvent);
                }
                if let Some(callback) = &mut element.on_popover_lifecycle {
                    callback(*open);
                }
                return Ok(());
            }
            if let (Some(binding), EventPayload::PopoverPosition { position }) =
                (&element.popover, &event.payload)
            {
                if !binding.open.get() || position.is_some_and(|position| !position.is_valid()) {
                    return Err(ProjectionError::InvalidEvent);
                }
                if let Some(callback) = &mut element.on_popover_position {
                    callback(*position);
                }
                return Ok(());
            }
            if let (Some(binding), EventPayload::PopoverCancel { backdrop }) =
                (&element.popover, &event.payload)
            {
                if !binding.open.get() {
                    return Err(ProjectionError::InvalidEvent);
                }
                if let Some(callback) = &mut element.on_popover_cancel {
                    callback(if *backdrop {
                        PopoverOpenChangeReason::PressOutside
                    } else {
                        PopoverOpenChangeReason::Cancel
                    });
                }
                return Ok(());
            }
            if let (Some(binding), EventPayload::Dismiss { backdrop, revision }) =
                (&element.popover, &event.payload)
            {
                if !binding.open.get()
                    || *revision <= element.edit_revision
                    || (*backdrop && !binding.close_on_outside_click)
                    || (!*backdrop
                        && element.descriptor.style.popover_close_on_escape == Some(false))
                {
                    return Err(ProjectionError::InvalidEvent);
                }
                element.edit_revision = *revision;
                if let Some(callback) = &mut element.on_popover_dismiss {
                    callback(if *backdrop {
                        PopoverOpenChangeReason::PressOutside
                    } else {
                        PopoverOpenChangeReason::Cancel
                    });
                } else {
                    binding.open.set(false);
                }
                element.popover_open = binding.open.peek();
                return Ok(());
            }
            match (&element.descriptor.content, &event.payload) {
                (_, EventPayload::AccessibilityAction { id, value }) if element.descriptor.accepts_accessibility_action(id, *value) => {
                    if let Some(callback) = &mut element.on_accessibility_action { callback(id, *value); }
                    Ok(())
                }
                (_, EventPayload::FileDrop(drop)) if element.descriptor.accepts_interaction()
                    && element.descriptor.interaction.file_drop
                    && element.descriptor.interaction.pointer_events == presentation::interaction::PointerEvents::Auto
                    && drop.is_valid() => {
                    if let Some(callback) = &mut element.on_file_drop { callback(drop); }
                    Ok(())
                }
                (_, EventPayload::Interaction(interaction)) if element.descriptor.accepts_interaction()
                    && element.descriptor.interaction.accepts(interaction) => {
                    use presentation::interaction::{InteractionKind, InteractionPhase};
                    if interaction.kind == InteractionKind::Hover {
                        let inside = matches!(interaction.phase, InteractionPhase::Started | InteractionPhase::Changed);
                        if let Some(callback) = &element.on_hover { (callback.borrow_mut())(inside); }
                    }
                    if let Some(callback) = &mut element.on_interaction { callback(interaction); }
                    Ok(())
                }
                (_, EventPayload::Focus | EventPayload::FocusVisible | EventPayload::Blur)
                    if element.descriptor.can_focus() || (matches!(event.payload, EventPayload::Blur)
                        && matches!(element.descriptor.content, NodeContent::Button { .. } | NodeContent::Input { .. }
                            | NodeContent::Textarea { .. } | NodeContent::Checkbox { .. } | NodeContent::Switch { .. } | NodeContent::Select { .. })) => {
                    if let Some(callback) = &mut element.on_focus_change {
                        callback(!matches!(event.payload, EventPayload::Blur), matches!(event.payload, EventPayload::FocusVisible));
                    }
                    Ok(())
                }
                (_, EventPayload::Key(key)) if element.descriptor.accepts_keyboard()
                    && element.descriptor.keyboard.accepts(key) => {
                    if let Some(callback) = &mut element.on_key { callback(key); }
                    Ok(())
                }
                (_, EventPayload::Shortcut { id }) if element.descriptor.accepts_keyboard()
                    && element.descriptor.keyboard.shortcuts.iter().any(|shortcut| shortcut.id == *id) => {
                    if let Some(callback) = &mut element.on_shortcut { callback(id); }
                    Ok(())
                }
                (NodeContent::Hosted { specification }, EventPayload::Hosted { event, data }) => {
                    if !jadix_core::hosted::HostedContent::from_json(specification)
                        .is_ok_and(|content| content.accepts_event(event, data)) {
                        return Err(ProjectionError::InvalidEvent);
                    }
                    if let Some(callback) = &mut element.on_hosted { callback(event, data); }
                    Ok(())
                }
                (NodeContent::Scroll, EventPayload::ScrollActivity(activity)) if activity.is_valid(element.descriptor.style.scroll_refresh) => {
                    if let Some(callback) = &mut element.on_scroll_activity { callback(*activity); }
                    Ok(())
                }
                (NodeContent::Scroll, EventPayload::ScrollZoom { scale, .. }) => {
                    if let Some(callback) = &mut element.on_scroll_zoom { callback(*scale); }
                    Ok(())
                }
                (NodeContent::Scroll, EventPayload::ScrollViewport { x, y, width, height })
                    if [x, y, width, height].iter().all(|v| v.is_finite())
                        && *width >= 0.0 && *height >= 0.0 =>
                {
                    if let Some(callback) = &mut element.on_viewport { callback(*width, *height); }
                    if let Some(callback) = &mut element.on_scroll { callback(*x, *y); }
                    Ok(())
                }
                (NodeContent::Scroll, EventPayload::Scroll { x, y })
                    if x.is_finite() && y.is_finite() =>
                {
                    if let Some(callback) = &mut element.on_scroll {
                        callback(*x, *y);
                    }
                    Ok(())
                }
                (NodeContent::Image { .. }, EventPayload::ImageLoad(load)) => {
                    if let Some(callback) = &mut element.on_image_load_event {
                        callback(load);
                    }
                    Ok(())
                }
                (
                    NodeContent::Dialog {
                        open: true,
                        close_on_overlay_click,
                        ..
                    },
                    EventPayload::Dismiss { backdrop, revision },
                ) => {
                    if *revision <= element.edit_revision || (*backdrop && !close_on_overlay_click)
                    {
                        return Err(ProjectionError::InvalidEvent);
                    }
                    element
                        .dialog_open
                        .ok_or(ProjectionError::InvalidEvent)?
                        .set(false);
                    if let NodeContent::Dialog { open, .. } = &mut element.descriptor.content {
                        *open = false;
                    }
                    element.edit_revision = *revision;
                    Ok(())
                }
                (
                    NodeContent::Select {
                        options,
                        enabled: true,
                        ..
                    },
                    EventPayload::Select { value, revision },
                ) => {
                    if *revision <= element.edit_revision
                        || !options
                            .iter()
                            .any(|option| option.enabled && option.value == *value)
                    {
                        return Err(ProjectionError::InvalidEvent);
                    }
                    element
                        .input
                        .ok_or(ProjectionError::InvalidEvent)?
                        .set(value.clone());
                    element.input_committed = value.clone();
                    if let NodeContent::Select { value: current, .. } =
                        &mut element.descriptor.content
                    {
                        *current = value.clone();
                    }
                    element.edit_revision = *revision;
                    Ok(())
                }
                (
                    NodeContent::Button { enabled: true, .. },
                    EventPayload::Expansion { expanded },
                ) if element.descriptor.accessible_role.is_none()
                    && element.descriptor.accessible_expanded.is_some() =>
                {
                    if element.descriptor.accessible_expanded == Some(*expanded) {
                        return Ok(());
                    }
                    let callback = element
                        .on_click
                        .as_ref()
                        .cloned()
                        .ok_or(ProjectionError::InvalidEvent)?;
                    let mut callback = callback
                        .try_borrow_mut()
                        .map_err(|_| ProjectionError::InvalidEvent)?;
                    callback();
                    Ok(())
                }
                (NodeContent::Button { enabled: true, .. }, EventPayload::PointerMove { x, y }) if x.is_finite() && y.is_finite() => {
                    if let Some(callback) = &element.on_pointer_move {
                        let mut callback = callback.try_borrow_mut().map_err(|_| ProjectionError::InvalidEvent)?;
                        callback([*x, *y]);
                    }
                    Ok(())
                }
                (_, EventPayload::Hover { inside }) if element.descriptor.accepts_interaction() => {
                    if let Some(callback) = element.on_hover.as_ref() {
                        let mut callback = callback.try_borrow_mut().map_err(|_| ProjectionError::InvalidEvent)?;
                        callback(*inside);
                    }
                    Ok(())
                }
                (NodeContent::Button { enabled: true, .. }, EventPayload::Activate) => {
                    let Some(callback) = element.on_click.as_ref().cloned() else {
                        return Ok(());
                    };
                    let mut callback = callback
                        .try_borrow_mut()
                        .map_err(|_| ProjectionError::InvalidEvent)?;
                    callback();
                    Ok(())
                }
                (
                    NodeContent::Input { enabled: true, .. }
                    | NodeContent::Textarea { enabled: true, .. },
                    EventPayload::Submit { revision },
                ) => {
                    if *revision <= element.edit_revision || element.composing.is_some() {
                        return Err(ProjectionError::InvalidEvent);
                    }
                    let callback = element.on_submit.clone();
                    let mut callback = callback
                        .as_ref()
                        .map(|callback| callback.try_borrow_mut())
                        .transpose()
                        .map_err(|_| ProjectionError::InvalidEvent)?;
                    element.edit_revision = *revision;
                    if let Some(callback) = callback.as_mut() {
                        callback();
                    }
                    Ok(())
                }
                (
                    NodeContent::Input { enabled: true, .. }
                    | NodeContent::Textarea { enabled: true, .. },
                    EventPayload::Edit {
                        value,
                        revision,
                        selection_utf16,
                        composing_utf16,
                    },
                ) => {
                    if *revision <= element.edit_revision
                        || !presentation::valid_utf16_range(value, *selection_utf16)
                        || composing_utf16
                            .is_some_and(|range| !presentation::valid_utf16_range(value, range))
                    {
                        return Err(ProjectionError::InvalidEvent);
                    }
                    let mut callback = element
                        .on_composition_change
                        .as_ref()
                        .map(|callback| callback.try_borrow_mut())
                        .transpose()
                        .map_err(|_| ProjectionError::InvalidEvent)?;
                    let binding = element.input.ok_or(ProjectionError::InvalidEvent)?;
                    let composition_changed =
                        composing_utf16.is_some() || element.composing.is_some();
                    if composing_utf16.is_some() {
                        element.composing = Some(value.clone());
                    } else {
                        binding.set(value.clone());
                        element.input_committed = value.clone();
                        element.composing = None;
                    }
                    element.edit_revision = *revision;
                    if composition_changed {
                        if let Some(callback) = callback.as_mut() {
                            callback(composing_utf16.map(|_| value.clone()));
                        }
                    }
                    Ok(())
                }
                (
                    NodeContent::Checkbox { enabled: true, .. }
                    | NodeContent::Switch { enabled: true, .. },
                    EventPayload::Toggle { checked, revision },
                ) => {
                    if *revision <= element.edit_revision {
                        return Err(ProjectionError::InvalidEvent);
                    }
                    let binding = element.checked.ok_or(ProjectionError::InvalidEvent)?;
                    binding.set(*checked);
                    if let NodeContent::Checkbox {
                        checked: current, ..
                    }
                    | NodeContent::Switch {
                        checked: current, ..
                    } = &mut element.descriptor.content
                    {
                        *current = *checked;
                    }
                    element.edit_revision = *revision;
                    Ok(())
                }
                _ => Err(ProjectionError::InvalidEvent),
            }
        }
    }
}

pub mod prelude {
    pub use super::{primitives::*, virtual_list::*, window::*, Element};
    pub use jadix_core::presentation::style::{ImageFit, ImagePosition};
    #[cfg(feature = "macros")]
    pub use crate::portable_ui as ui;
    pub use jadix_core::reactivity::*;
}

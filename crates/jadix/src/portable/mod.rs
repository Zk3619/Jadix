use jadix_core::presentation::{
    NodeContent, NodeDescriptor, NodeId, PresentationNode, Rect, RootId, RootLifetime,
};
use std::{cell::RefCell, collections::BTreeMap, rc::Rc};

pub use jadix_core::presentation;
pub use jadix_core::reactivity::*;

#[cfg(feature = "macros")]
#[doc(hidden)]
pub use jadix_macros::ui_with_runtime as __ui_with_runtime;
#[cfg(feature = "macros")]
#[macro_export]
macro_rules! ui {
    ($($tokens:tt)*) => { $crate::__ui_with_runtime!($crate; $($tokens)*) };
}

pub use jadix_core::event::{Handler, ZeroArgFn};
pub type Base = Option<Box<dyn FnOnce(Element) -> Element>>;
pub type DescriptorBinding = Rc<dyn Fn(&mut NodeDescriptor)>;
pub type NavigationHandler =
    Rc<RefCell<Box<dyn FnMut(presentation::NavigationKey) -> Option<String>>>>;

#[derive(Clone)]
pub struct PopoverBinding {
    pub anchor: Signal<String>,
    pub open: Signal<bool>,
    pub placement: presentation::PopoverPlacement,
    pub gap: u16,
    pub close_on_outside_click: bool,
}

pub struct Element {
    pub children: Vec<Element>,
    pub template_id: Option<String>,
    pub descriptor: NodeDescriptor,
    pub on_click: Option<Rc<RefCell<Box<dyn FnMut()>>>>,
    pub input: Option<Signal<String>>,
    pub checked: Option<Signal<bool>>,
    pub dialog_open: Option<Signal<bool>>,
    pub popover: Option<PopoverBinding>,
    pub deferred_children: Option<Box<dyn Fn() -> Element>>,
    pub text_binding: Option<Rc<dyn Fn() -> String>>,
    pub visibility: Option<Rc<dyn Fn() -> bool>>,
    pub focus_restore_target: Option<String>,
    pub descriptor_binding: Option<DescriptorBinding>,
    pub on_navigation: Option<NavigationHandler>,
    pub attrs: BTreeMap<String, String>,
    identity: Option<NodeId>,
    edit_revision: u64,
    composing: Option<String>,
    projected_visibility: Option<bool>,
    input_committed: String,
    popover_open: bool,
    owner: Option<OwnerId>,
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
                accessible_name: None,
                accessible_role: None,
                accessible_selected: None,
                accessible_expanded: None,
                style: Default::default(),
            },
            on_click: None,
            input: None,
            checked: None,
            dialog_open: None,
            popover: None,
            deferred_children: None,
            text_binding: None,
            visibility: None,
            focus_restore_target: None,
            descriptor_binding: None,
            on_navigation: None,
            attrs: BTreeMap::new(),
            identity: None,
            edit_revision: 0,
            composing: None,
            projected_visibility: None,
            input_committed: String::new(),
            popover_open: false,
            owner: current_owner(),
        }
    }

    pub fn child(mut self, child: impl Into<Element>) -> Self {
        self.children.push(child.into());
        self
    }
    pub fn children(mut self, children: impl IntoIterator<Item = Element>) -> Self {
        self.children.extend(children);
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
    pub fn p(mut self, padding: f32) -> Self {
        self.descriptor.style.padding = [padding; 4];
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
                with_owner(owner, &mut callback);
            } else {
                callback();
            }
        }))));
        self
    }

    fn refresh(&mut self) {
        if let Some(owner) = self.owner {
            with_owner(owner, || self.refresh_values());
        } else {
            self.refresh_values();
        }
    }

    fn refresh_values(&mut self) {
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
            .unwrap_or_else(|| self.visibility.as_ref().is_none_or(|binding| binding()))
    }

    fn latch_visibility(&mut self, defer_changes: bool) {
        if !defer_changes || self.projected_visibility.is_none() {
            self.projected_visibility =
                Some(self.visibility.as_ref().is_none_or(|binding| binding()));
        }
    }

    fn has_active_composition(&self) -> bool {
        self.identity.is_some()
            && self.is_visible()
            && (self.composing.is_some()
                && self
                    .input
                    .is_some_and(|binding| binding.get() == self.input_committed)
                || self.children.iter().any(Self::has_active_composition))
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
        }
        impl ElementProjection {
            pub fn new(root: RootId) -> Self {
                Self::with_lifetime(RootLifetime::new(root))
            }
            pub fn with_lifetime(lifetime: RootLifetime) -> Self {
                Self {
                    lifetime,
                    focus_request: RefCell::new(None),
                }
            }
            pub fn snapshot(
                &self,
                element: &mut Element,
            ) -> Result<PresentationNode, ProjectionError> {
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
                let mut tree = snapshot(
                    element,
                    self.lifetime.id(),
                    0,
                    &mut anchors,
                    &mut popovers,
                    defer_visibility,
                )?;
                let mut pending = vec![&mut tree];
                while let Some(node) = pending.pop() {
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
                    pending.extend(&node.children);
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
                if self.lifetime.is_disposed() {
                    return Err(ProjectionError::Disposed);
                }
                if event.node.root != self.lifetime.id() {
                    return Err(ProjectionError::StaleNode);
                }
                let mut pending = vec![element];
                while let Some(element) = pending.pop() {
                    if !element.is_visible() {
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
                            if !matches!(
                                element.descriptor.content,
                                NodeContent::Button { enabled: true, .. }
                            ) {
                                return Err(ProjectionError::InvalidEvent);
                            }
                            let callback = element
                                .on_navigation
                                .as_ref()
                                .ok_or(ProjectionError::InvalidEvent)?;
                            *self.focus_request.borrow_mut() = callback
                                .try_borrow_mut()
                                .map_err(|_| ProjectionError::InvalidEvent)?(
                                *key
                            );
                            return Ok(());
                        }
                        return dispatch(element, event);
                    }
                    pending.extend(element.children.iter_mut());
                }
                Err(ProjectionError::StaleNode)
            }

            pub fn navigation_nodes(&self, element: &Element) -> Vec<NodeId> {
                if self.lifetime.is_disposed() {
                    return Vec::new();
                }
                let mut result = Vec::new();
                let mut pending = vec![element];
                while let Some(element) = pending.pop() {
                    if !element.is_visible() {
                        continue;
                    }
                    if element.on_navigation.is_some() {
                        if let Some(node) = element
                            .identity
                            .filter(|node| node.root == self.lifetime.id())
                        {
                            result.push(node);
                        }
                    }
                    pending.extend(element.children.iter().rev());
                }
                result
            }

            pub fn take_focus_request(&self, element: &Element) -> Option<NodeId> {
                let wanted = self.focus_request.borrow_mut().take()?;
                self.resolve_focus_target(element, &wanted)
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
                        if !matches!(
                            element.descriptor.content,
                            NodeContent::Button { enabled: true, .. }
                        ) {
                            return None;
                        }
                        let node = element
                            .identity
                            .filter(|node| node.root == self.lifetime.id())?;
                        if result.replace(node).is_some() {
                            return None;
                        }
                    }
                    pending.extend(&element.children);
                }
                result
            }
            pub fn dispose(&self) {
                self.lifetime.dispose();
                self.focus_request.borrow_mut().take();
            }
        }

        fn snapshot(
            element: &mut Element,
            root: RootId,
            depth: usize,
            anchors: &mut BTreeMap<String, Vec<NodeId>>,
            popovers: &mut BTreeMap<NodeId, (PopoverBinding, u64, bool)>,
            defer_visibility: bool,
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
            if let NodeContent::Button { label, .. }
            | NodeContent::Checkbox { label, .. }
            | NodeContent::Switch { label, .. } = &mut descriptor.content
            {
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
                            defer_visibility,
                        )?);
                    } else {
                        child.forget_identities();
                    }
                }
            } else if !element.children.is_empty() {
                return Err(ProjectionError::Unsupported("leaf children"));
            }
            Ok(PresentationNode {
                id: node,
                descriptor,
                children,
            })
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
            use presentation::EventPayload;
            if let (Some(binding), EventPayload::Dismiss { backdrop, revision }) =
                (&element.popover, &event.payload)
            {
                if !binding.open.get()
                    || *revision <= element.edit_revision
                    || (*backdrop && !binding.close_on_outside_click)
                {
                    return Err(ProjectionError::InvalidEvent);
                }
                binding.open.set(false);
                element.popover_open = false;
                element.edit_revision = *revision;
                return Ok(());
            }
            match (&element.descriptor.content, &event.payload) {
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
                    let binding = element.input.ok_or(ProjectionError::InvalidEvent)?;
                    if composing_utf16.is_some() {
                        element.composing = Some(value.clone());
                    } else {
                        binding.set(value.clone());
                        element.input_committed = value.clone();
                        element.composing = None;
                    }
                    element.edit_revision = *revision;
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
    pub use super::{primitives::*, Element};
    #[cfg(feature = "macros")]
    pub use crate::ui;
    pub use jadix_core::reactivity::*;
}

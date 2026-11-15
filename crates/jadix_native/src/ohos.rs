use std::{
    cell::{Cell, RefCell},
    collections::{BTreeMap, VecDeque},
    ffi::{c_char, c_void, CStr, CString},
    rc::Rc,
};

use crate::{
    environment::LayoutDirection,
    host::{ApplyReceipt, PlatformHost},
    layout::{LayoutError, MeasureRequest},
    presentation::{
        capabilities::{BackendCapabilities, NodeKind},
        *,
    },
};

unsafe extern "C" {
    fn jadix_ohos_content(
        environment: *mut c_void,
        value: *mut c_void,
        result: *mut *mut c_void,
    ) -> i32;
    fn jadix_ohos_accessibility_create(
        environment: *mut c_void,
        xcomponent: *mut c_void,
    ) -> *mut c_void;
    fn jadix_ohos_accessibility_dispose(provider: *mut c_void);
    fn jadix_ohos_accessibility_replace(
        provider: *mut c_void,
        elements: *const OhosAccessibilityElement,
        count: usize,
    ) -> i32;
    fn jadix_ohos_create(kind: i32, target: *mut c_void) -> *mut c_void;
    fn jadix_ohos_dispose(node: *mut c_void);
    fn jadix_ohos_dialog_supported() -> bool;
    fn jadix_ohos_dialog(node: *mut c_void, open: bool, backdrop: bool) -> i32;
    fn jadix_ohos_select_close(node: *mut c_void) -> i32;
    fn jadix_ohos_select(
        node: *mut c_void,
        values: *const *const c_char,
        labels: *const *const c_char,
        enabled: *const bool,
        count: usize,
        selected: i32,
        interactive: bool,
    ) -> i32;
    fn jadix_ohos_mount(content: *mut c_void, node: *mut c_void, attach: bool) -> i32;
    fn jadix_ohos_child(parent: *mut c_void, child: *mut c_void, index: i32) -> i32;
    fn jadix_ohos_geometry(node: *mut c_void, left: f32, top: f32, width: f32, height: f32) -> i32;
    fn jadix_ohos_update(
        node: *mut c_void,
        kind: i32,
        value: *const c_char,
        placeholder: *const c_char,
        enabled: bool,
        password: bool,
        checked: bool,
        size: f32,
        weight: u32,
        family: *const c_char,
        foreground: u32,
        background: u32,
        direction: i32,
        accessible: *const c_char,
        description: *const c_char,
        accessible_role: i32,
        accessible_selected: i32,
        accessible_expanded: i32,
    ) -> i32;
    fn jadix_ohos_radio_group(node: *mut c_void, group: *const c_char) -> i32;
    fn jadix_ohos_focus(node: *mut c_void, focused: bool) -> i32;
    fn jadix_ohos_max_length(node: *mut c_void, multiline: bool, limit: i32) -> i32;
    fn jadix_ohos_scroll(node: *mut c_void, horizontal: f32, vertical: f32) -> i32;
    fn jadix_ohos_measure(
        text: *const c_char,
        family: *const c_char,
        size: f64,
        weight: u32,
        maximum: f64,
        result: *mut f64,
    ) -> i32;
}

#[repr(C)]
pub(super) struct OhosAccessibilityElement {
    pub(super) id: i64,
    pub(super) parent_id: i64,
    pub(super) child_ids: *const i64,
    pub(super) child_count: usize,
    pub(super) component_type: *const c_char,
    pub(super) text: *const c_char,
    pub(super) description: *const c_char,
    pub(super) left: i32,
    pub(super) top: i32,
    pub(super) right: i32,
    pub(super) bottom: i32,
    pub(super) flags: u32,
    pub(super) target: *mut c_void,
}

const A11Y_ENABLED: u32 = 1 << 0;
const A11Y_SELECTED: u32 = 1 << 1;
const A11Y_EXPANDED: u32 = 1 << 2;
const A11Y_CHECKABLE: u32 = 1 << 3;
const A11Y_CHECKED: u32 = 1 << 4;
const A11Y_FOCUSABLE: u32 = 1 << 5;
const A11Y_CLICKABLE: u32 = 1 << 6;
const A11Y_SCROLLABLE: u32 = 1 << 7;
const A11Y_EDITABLE: u32 = 1 << 8;
const A11Y_PASSWORD: u32 = 1 << 9;

const MAX_EVENTS: usize = 1024;
const MAX_EVENT_BYTES: usize = 2 * 1024 * 1024;
type Notifier = dyn Fn() -> Result<(), String>;

#[derive(Default)]
struct Queue {
    events: VecDeque<PlatformEvent>,
    bytes: usize,
}

#[derive(Default)]
struct Events {
    queue: RefCell<Queue>,
    failed: Cell<bool>,
    suppress: Cell<bool>,
    closed: Cell<bool>,
    scheduled: Cell<bool>,
    notifier: RefCell<Option<Rc<Notifier>>>,
}

struct Target {
    node: NodeId,
    revision: Cell<u64>,
    composing: Cell<bool>,
    events: Rc<Events>,
    enabled: Cell<bool>,
    options: RefCell<Option<Vec<SelectOption>>>,
}

unsafe fn event_string(value: *const c_char) -> Result<String, String> {
    if value.is_null() {
        return Ok(String::new());
    }
    let value = unsafe { CStr::from_ptr(value) };
    if value.to_bytes().len() > MAX_EVENT_BYTES {
        return Err(String::from("ArkUI text event exceeds the byte budget"));
    }
    value
        .to_str()
        .map(String::from)
        .map_err(|_| String::from("invalid ArkUI UTF-8 event"))
}

#[no_mangle]
unsafe extern "C" fn jadix_ohos_receive(
    target: *mut c_void,
    kind: i32,
    text: *const c_char,
    preview: *const c_char,
    start: i32,
    selection_start: i32,
    selection_end: i32,
    x: f32,
    y: f32,
) {
    if target.is_null() {
        return;
    }
    let target = unsafe { &*target.cast::<Target>() };
    if target.events.closed.get()
        || target.events.suppress.get()
        || (!target.enabled.get() && kind != 8)
    {
        return;
    }
    let result =
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| -> Result<(), String> {
            let payload = match kind {
                0 => EventPayload::Activate,
                1 => EventPayload::Focus,
                8 => EventPayload::Blur,
                2 if start == 0 || start == 1 => {
                    let revision = target
                        .revision
                        .get()
                        .checked_add(1)
                        .ok_or("ArkUI toggle revision exhausted")?;
                    target.revision.set(revision);
                    EventPayload::Toggle {
                        checked: start == 1,
                        revision,
                    }
                }
                3 => {
                    let value = unsafe { event_string(text) }?;
                    let marked = unsafe { event_string(preview) }?;
                    let length = value.encode_utf16().count();
                    let selection = if selection_start < 0 && selection_end < 0 {
                        [length; 2]
                    } else {
                        [
                            usize::try_from(selection_start)
                                .map_err(|_| "invalid ArkUI selection")?,
                            usize::try_from(selection_end)
                                .map_err(|_| "invalid ArkUI selection")?,
                        ]
                    };
                    let composing = if marked.is_empty() {
                        None
                    } else {
                        let begin =
                            usize::try_from(start).map_err(|_| "invalid ArkUI composition")?;
                        let end = begin
                            .checked_add(marked.encode_utf16().count())
                            .ok_or("ArkUI composition overflow")?;
                        if !valid_utf16_range(&value, [begin, end])
                            || value
                                .encode_utf16()
                                .skip(begin)
                                .take(end - begin)
                                .ne(marked.encode_utf16())
                        {
                            return Err(String::from(
                                "ArkUI preview does not match the text snapshot",
                            ));
                        }
                        Some([begin, end])
                    };
                    if !valid_utf16_range(&value, selection) {
                        return Err(String::from("invalid ArkUI UTF-16 selection"));
                    }
                    let revision = target
                        .revision
                        .get()
                        .checked_add(1)
                        .ok_or("ArkUI edit revision exhausted")?;
                    target.revision.set(revision);
                    target.composing.set(composing.is_some());
                    EventPayload::Edit {
                        value,
                        revision,
                        selection_utf16: selection,
                        composing_utf16: composing,
                    }
                }
                9 => {
                    if target.composing.get() {
                        return Ok(());
                    }
                    let revision = target
                        .revision
                        .get()
                        .checked_add(1)
                        .ok_or("ArkUI submit revision exhausted")?;
                    target.revision.set(revision);
                    EventPayload::Submit { revision }
                }
                4 if x.is_finite() && y.is_finite() => EventPayload::Scroll {
                    x: x.into(),
                    y: y.into(),
                },
                5 if start == 0 || start == 1 => {
                    let revision = target
                        .revision
                        .get()
                        .checked_add(1)
                        .ok_or("ArkUI dismiss revision exhausted")?;
                    target.revision.set(revision);
                    EventPayload::Dismiss {
                        backdrop: start == 1,
                        revision,
                    }
                }
                6 => {
                    let value = unsafe { event_string(text) }?;
                    let options = target.options.borrow();
                    if !options.as_ref().is_some_and(|options| {
                        options
                            .iter()
                            .any(|option| option.value == value && option.enabled)
                    }) {
                        return Err(String::from("invalid ArkUI Select option"));
                    }
                    let revision = target
                        .revision
                        .get()
                        .checked_add(1)
                        .ok_or("ArkUI Select revision exhausted")?;
                    target.revision.set(revision);
                    EventPayload::Select { value, revision }
                }
                7 => EventPayload::Navigate {
                    key: match start {
                        0 => NavigationKey::Up,
                        1 => NavigationKey::Down,
                        2 => NavigationKey::Left,
                        3 => NavigationKey::Right,
                        4 => NavigationKey::Home,
                        5 => NavigationKey::End,
                        _ => return Err(String::from("invalid ArkUI navigation key")),
                    },
                    shift: false,
                    control: false,
                },
                _ => return Err(String::from("unsupported or invalid ArkUI event")),
            };
            let bytes = match &payload {
                EventPayload::Edit { value, .. } | EventPayload::Select { value, .. } => value
                    .len()
                    .checked_add(64)
                    .ok_or("ArkUI event size overflow")?,
                _ => 64,
            };
            let mut queue = target.events.queue.borrow_mut();
            if queue.events.len() == MAX_EVENTS
                || bytes > MAX_EVENT_BYTES.saturating_sub(queue.bytes)
            {
                return Err(String::from("ArkUI event queue exhausted"));
            }
            queue.bytes += bytes;
            queue.events.push_back(PlatformEvent {
                node: target.node,
                payload,
            });
            drop(queue);
            if !target.events.scheduled.replace(true) {
                let notify = target.events.notifier.borrow().clone();
                if let Some(notify) = notify {
                    notify()?;
                }
            }
            Ok(())
        }));
    if !matches!(result, Ok(Ok(()))) {
        target.events.failed.set(true);
    }
}

struct Control {
    handle: *mut c_void,
    parent: Option<NodeId>,
    descriptor: NodeDescriptor,
    radio_group: Option<NodeId>,
    target: Box<Target>,
}

pub struct OhosHost {
    content: *mut c_void,
    container: *mut c_void,
    controls: BTreeMap<NodeId, Control>,
    children: BTreeMap<Option<NodeId>, Vec<NodeId>>,
    dialogs: Vec<NodeId>,
    events: Rc<Events>,
    root: Option<RootId>,
    revision: u64,
    viewport: [f32; 2],
    density: f64,
    accessibility: *mut c_void,
}

fn check(result: i32) -> Result<(), String> {
    if result == 0 {
        Ok(())
    } else {
        Err(format!("ArkUI operation failed: {result}"))
    }
}

fn cstring(value: &str) -> Result<CString, String> {
    CString::new(value).map_err(|_| String::from("ArkUI text cannot contain NUL"))
}

fn color(channels: [u8; 4]) -> u32 {
    u32::from_be_bytes([channels[3], channels[0], channels[1], channels[2]])
}

fn native_kind(descriptor: &NodeDescriptor) -> Result<i32, String> {
    if matches!(
        descriptor.accessible_role,
        Some(AccessibleRole::RadioItem { .. })
    ) {
        return if matches!(descriptor.content, NodeContent::Button { .. }) {
            Ok(12)
        } else {
            Err(String::from("ArkUI RadioItem requires Button content"))
        };
    }
    kind(&descriptor.content)
}

fn kind(content: &NodeContent) -> Result<i32, String> {
    match content {
        NodeContent::Container => Ok(0),
        NodeContent::Text(_) => Ok(1),
        NodeContent::Button { .. } => Ok(2),
        NodeContent::Input { .. } => Ok(3),
        NodeContent::Textarea { .. } => Ok(4),
        NodeContent::Scroll => Ok(5),
        NodeContent::Checkbox { .. } => Ok(6),
        NodeContent::Switch { .. } => Ok(7),
        NodeContent::Image { .. } => Ok(8),
        NodeContent::Dialog { .. } => Ok(9),
        NodeContent::Popover { .. } => Ok(10),
        NodeContent::Select { .. } => Ok(11),
    }
}

impl OhosHost {
    fn accessibility_component_type(node: &PresentationNode) -> &'static str {
        match node.descriptor.accessible_role {
            Some(AccessibleRole::RadioGroup) => "radiogroup",
            Some(AccessibleRole::RadioItem { .. }) => "radio",
            Some(AccessibleRole::TabList) => "tablist",
            Some(AccessibleRole::TabItem) => "tab",
            Some(AccessibleRole::TabPanel) => "tabpanel",
            Some(AccessibleRole::Tree) => "tree",
            Some(AccessibleRole::TreeItem) => "treeitem",
            Some(AccessibleRole::Grid { .. }) => "grid",
            Some(AccessibleRole::GridCell { .. }) => "gridcell",
            Some(AccessibleRole::GridColumnHeader { .. }) => "columnheader",
            None => match &node.descriptor.content {
                NodeContent::Text(_) => "text",
                NodeContent::Button { .. } => "button",
                NodeContent::Input { .. } => "input",
                NodeContent::Textarea { .. } => "textarea",
                NodeContent::Checkbox { .. } => "checkbox",
                NodeContent::Switch { .. } => "switch",
                NodeContent::Select { .. } => "select",
                NodeContent::Scroll => "scroll",
                NodeContent::Image { .. } => "image",
                NodeContent::Dialog { .. } => "dialog",
                NodeContent::Popover { .. } => "popover",
                NodeContent::Container => "group",
            },
        }
    }

    fn accessibility_text(node: &PresentationNode) -> String {
        if let Some(name) = &node.descriptor.accessible_name {
            return name.clone();
        }
        match &node.descriptor.content {
            NodeContent::Text(text) => text.clone(),
            NodeContent::Button { label, .. }
            | NodeContent::Checkbox { label, .. }
            | NodeContent::Switch { label, .. } => label.clone(),
            NodeContent::Input {
                value, placeholder, ..
            }
            | NodeContent::Textarea {
                value, placeholder, ..
            } => {
                if value.is_empty() {
                    placeholder.clone()
                } else {
                    value.clone()
                }
            }
            NodeContent::Select {
                options,
                value,
                placeholder,
                ..
            } => options
                .iter()
                .find(|option| option.value == *value)
                .map(|option| option.label.clone())
                .unwrap_or_else(|| placeholder.clone()),
            NodeContent::Image { alt, .. } => alt.clone(),
            _ => String::new(),
        }
    }

    fn accessibility_flags(node: &PresentationNode) -> u32 {
        let mut flags = 0;
        let content = &node.descriptor.content;
        let enabled = match content {
            NodeContent::Button { enabled, .. }
            | NodeContent::Input { enabled, .. }
            | NodeContent::Textarea { enabled, .. }
            | NodeContent::Checkbox { enabled, .. }
            | NodeContent::Switch { enabled, .. }
            | NodeContent::Select { enabled, .. } => *enabled,
            _ => true,
        };
        if enabled {
            flags |= A11Y_ENABLED;
        }
        if node.descriptor.accessible_selected == Some(true) {
            flags |= A11Y_SELECTED;
        }
        if node.descriptor.accessible_expanded == Some(true) {
            flags |= A11Y_EXPANDED;
        }
        if matches!(
            content,
            NodeContent::Checkbox { .. } | NodeContent::Switch { .. }
        ) {
            flags |= A11Y_CHECKABLE;
        }
        if matches!(
            content,
            NodeContent::Checkbox { checked: true, .. } | NodeContent::Switch { checked: true, .. }
        ) {
            flags |= A11Y_CHECKED;
        }
        if matches!(
            content,
            NodeContent::Button { .. }
                | NodeContent::Input { .. }
                | NodeContent::Textarea { .. }
                | NodeContent::Checkbox { .. }
                | NodeContent::Switch { .. }
                | NodeContent::Select { .. }
        ) {
            flags |= A11Y_FOCUSABLE;
        }
        if matches!(
            content,
            NodeContent::Button { .. }
                | NodeContent::Checkbox { .. }
                | NodeContent::Switch { .. }
                | NodeContent::Select { .. }
        ) {
            flags |= A11Y_CLICKABLE;
        }
        if matches!(content, NodeContent::Scroll) {
            flags |= A11Y_SCROLLABLE;
        }
        if matches!(
            content,
            NodeContent::Input { .. } | NodeContent::Textarea { .. }
        ) {
            flags |= A11Y_EDITABLE;
        }
        if matches!(content, NodeContent::Input { password: true, .. }) {
            flags |= A11Y_PASSWORD;
        }
        flags
    }

    fn publish_accessibility(&self, roots: &[PresentationNode]) -> Result<(), String> {
        if self.accessibility.is_null() {
            return Ok(());
        }
        let mut nodes = Vec::new();
        let mut parents = Vec::new();
        let mut pending: Vec<(i64, &PresentationNode)> =
            roots.iter().rev().map(|node| (0, node)).collect();
        while let Some((parent_id, node)) = pending.pop() {
            let id = i64::try_from(node.id.generation())
                .ok()
                .filter(|id| *id <= i64::from(i32::MAX))
                .ok_or_else(|| String::from("OHOS accessibility element ID exceeds ArkUI range"))?;
            let child_ids = node
                .children
                .iter()
                .map(|child| {
                    i64::try_from(child.id.generation())
                        .ok()
                        .filter(|id| *id <= i64::from(i32::MAX))
                        .ok_or_else(|| {
                            String::from("OHOS accessibility child ID exceeds ArkUI range")
                        })
                })
                .collect::<Result<Vec<_>, _>>()?;
            let text = cstring(&Self::accessibility_text(node))?;
            let description = cstring(
                node.descriptor
                    .accessible_description
                    .as_deref()
                    .unwrap_or(""),
            )?;
            let component_type = cstring(Self::accessibility_component_type(node))?;
            let rect = node.descriptor.frame;
            let scale = self.density;
            let pixel = |value: f64| -> i32 {
                (value * scale)
                    .round()
                    .clamp(i32::MIN as f64, i32::MAX as f64) as i32
            };
            let target = self
                .controls
                .get(&node.id)
                .map(|control| (&*control.target as *const Target).cast_mut().cast())
                .unwrap_or(std::ptr::null_mut());
            parents.push((child_ids, component_type, text, description));
            nodes.push(OhosAccessibilityElement {
                id,
                parent_id,
                child_ids: std::ptr::null(),
                child_count: 0,
                component_type: std::ptr::null(),
                text: std::ptr::null(),
                description: std::ptr::null(),
                left: pixel(rect.x),
                top: pixel(rect.y),
                right: pixel(rect.x + rect.width),
                bottom: pixel(rect.y + rect.height),
                flags: Self::accessibility_flags(node),
                target,
            });
            pending.extend(node.children.iter().rev().map(|child| (id, child)));
        }
        // The owned strings and child ID vectors outlive the synchronous native call.
        for (element, (children, component, text, description)) in nodes.iter_mut().zip(&parents) {
            element.child_ids = children.as_ptr();
            element.child_count = children.len();
            element.component_type = component.as_ptr();
            element.text = text.as_ptr();
            element.description = description.as_ptr();
        }
        check(unsafe {
            jadix_ohos_accessibility_replace(self.accessibility, nodes.as_ptr(), nodes.len())
        })
    }

    pub fn inspection(&self) -> Result<serde_json::Value, String> {
        let queue = self
            .events
            .queue
            .try_borrow()
            .map_err(|_| "ArkUI event queue is busy")?;
        Ok(serde_json::json!({
            "authority": "Rust adapter bookkeeping; platform accessibility and visibility require separate observation",
            "root": self.root.map(|root| root.token()), "revision": self.revision.to_string(),
            "viewport": self.viewport, "density": self.density, "controls": self.controls.len(),
            "overlay_order": self.dialogs.iter().map(|node| node.token()).collect::<Vec<_>>(),
            "queue": {"events": queue.events.len(), "bytes": queue.bytes,
                "event_limit": MAX_EVENTS, "byte_limit": MAX_EVENT_BYTES,
                "scheduled": self.events.scheduled.get(), "failed": self.events.failed.get()},
            "closed": self.events.closed.get()
        }))
    }

    pub fn set_notifier(&mut self, notify: impl Fn() -> Result<(), String> + 'static) {
        *self.events.notifier.borrow_mut() = Some(Rc::new(notify));
    }
    /// Creates a mounted ArkUI root from values owned by the active N-API call.
    ///
    /// # Safety
    /// `environment` and `value` must be valid handles from the same live N-API environment.
    pub unsafe fn from_napi(
        environment: *mut c_void,
        value: *mut c_void,
        viewport: [f32; 2],
        density: f64,
    ) -> Result<Self, String> {
        if environment.is_null()
            || value.is_null()
            || !density.is_finite()
            || density <= 0.0
            || viewport
                .iter()
                .any(|value| !value.is_finite() || *value < 0.0)
        {
            return Err(String::from("invalid OHOS native mount"));
        }
        let mut content = std::ptr::null_mut();
        check(unsafe { jadix_ohos_content(environment, value, &mut content) })?;
        if content.is_null() {
            return Err(String::from("missing ArkUI NodeContent"));
        }
        let container = unsafe { jadix_ohos_create(0, std::ptr::null_mut()) };
        if container.is_null() {
            return Err(String::from("ArkUI NativeNode interface is unavailable"));
        }
        if let Err(error) = check(unsafe { jadix_ohos_mount(content, container, true) }) {
            unsafe {
                jadix_ohos_dispose(container);
            }
            return Err(error);
        }
        Ok(Self {
            content,
            container,
            controls: BTreeMap::new(),
            children: BTreeMap::new(),
            dialogs: Vec::new(),
            events: Rc::new(Events::default()),
            root: None,
            revision: 0,
            viewport,
            density,
            accessibility: std::ptr::null_mut(),
        })
    }

    /// Creates an ArkUI host with an XComponent-rooted virtual accessibility provider.
    ///
    /// # Safety
    /// All N-API handles must belong to the same live environment. `xcomponent` must be an
    /// XComponent FrameNode owned by the caller and must outlive this host.
    pub unsafe fn from_napi_with_accessibility(
        environment: *mut c_void,
        node_content: *mut c_void,
        xcomponent: *mut c_void,
        viewport: [f32; 2],
        density: f64,
    ) -> Result<Self, String> {
        if xcomponent.is_null() {
            return Err(String::from("missing ArkUI XComponent FrameNode"));
        }
        let mut host = unsafe { Self::from_napi(environment, node_content, viewport, density) }?;
        let provider = unsafe { jadix_ohos_accessibility_create(environment, xcomponent) };
        if provider.is_null() {
            return Err(String::from("ArkUI accessibility provider creation failed"));
        }
        host.accessibility = provider;
        Ok(host)
    }

    pub fn resize(&mut self, viewport: [f32; 2], density: f64) -> Result<(), String> {
        if !density.is_finite()
            || density <= 0.0
            || viewport
                .iter()
                .any(|value| !value.is_finite() || *value < 0.0)
        {
            return Err(String::from("invalid ArkUI environment"));
        }
        self.viewport = viewport;
        self.density = density;
        Ok(())
    }

    fn parent(&self, parent: Option<NodeId>) -> Result<*mut c_void, String> {
        parent.map_or(Ok(self.container), |parent| {
            self.controls
                .get(&parent)
                .map(|control| control.handle)
                .ok_or_else(|| String::from("unknown ArkUI native parent"))
        })
    }

    fn attach(&mut self, node: NodeId, parent: Option<NodeId>, index: usize) -> Result<(), String> {
        let siblings = self.children.get(&parent).map(Vec::as_slice).unwrap_or(&[]);
        if index > siblings.len() {
            return Err(String::from("invalid ArkUI child index"));
        }
        let native_index = siblings[..index]
            .iter()
            .filter(|sibling| {
                !matches!(
                    self.controls[sibling].descriptor.content,
                    NodeContent::Dialog { .. } | NodeContent::Popover { .. }
                )
            })
            .count();
        check(unsafe {
            jadix_ohos_child(
                self.parent(parent)?,
                self.controls[&node].handle,
                i32::try_from(native_index).map_err(|_| "ArkUI child index overflow")?,
            )
        })?;
        self.children.entry(parent).or_default().insert(index, node);
        Ok(())
    }

    fn detach(&mut self, node: NodeId) -> Result<(), String> {
        let control = &self.controls[&node];
        check(unsafe { jadix_ohos_child(self.parent(control.parent)?, control.handle, -1) })?;
        let siblings = self
            .children
            .get_mut(&control.parent)
            .ok_or("missing ArkUI siblings")?;
        let index = siblings
            .iter()
            .position(|sibling| *sibling == node)
            .ok_or("missing ArkUI child")?;
        siblings.remove(index);
        Ok(())
    }

    fn sync_dialogs(&mut self) -> Result<(), String> {
        let mut desired = Vec::new();
        let mut pending = self.children.get(&None).cloned().unwrap_or_default();
        pending.reverse();
        while let Some(node) = pending.pop() {
            let control = &self.controls[&node];
            if let NodeContent::Dialog { open, .. } | NodeContent::Popover { open, .. } =
                control.descriptor.content
            {
                if !open {
                    continue;
                }
                desired.push(node);
            }
            pending.extend(self.children.get(&Some(node)).into_iter().flatten().rev());
        }
        let metadata = self
            .controls
            .iter()
            .map(|(node, control)| (*node, (control.parent, &control.descriptor.content)))
            .collect();
        let priority = desired
            .iter()
            .enumerate()
            .map(|(index, node)| (*node, index))
            .collect();
        let desired = crate::popover::overlay_order_with_priority(&metadata, &priority)
            .map_err(|_| "invalid ArkUI overlay dependencies")?
            .into_iter()
            .filter(|node| priority.contains_key(node))
            .collect::<Vec<_>>();
        let common = self
            .dialogs
            .iter()
            .zip(&desired)
            .take_while(|(before, after)| before == after)
            .count();
        if self.dialogs != desired {
            self.close_selects()?;
        }
        for node in self.dialogs[common..].iter().rev() {
            check(unsafe { jadix_ohos_dialog(self.controls[node].handle, false, false) })?;
        }
        self.dialogs.truncate(common);
        for (index, node) in desired.iter().enumerate() {
            let control = &self.controls[node];
            let close_on_outside_click = match control.descriptor.content {
                NodeContent::Dialog {
                    close_on_overlay_click,
                    ..
                } => close_on_overlay_click,
                NodeContent::Popover {
                    close_on_outside_click,
                    ..
                } => close_on_outside_click,
                _ => unreachable!(),
            };
            check(unsafe { jadix_ohos_dialog(control.handle, true, close_on_outside_click) })?;
            if index >= common {
                self.dialogs.push(*node);
            }
        }
        Ok(())
    }

    fn close_selects(&self) -> Result<(), String> {
        for control in self.controls.values() {
            if matches!(control.descriptor.content, NodeContent::Select { .. }) {
                check(unsafe { jadix_ohos_select_close(control.handle) })?;
            }
        }
        Ok(())
    }

    fn update(&mut self, node: NodeId, descriptor: &NodeDescriptor) -> Result<(), String> {
        let control = self.controls.get_mut(&node).ok_or("unknown ArkUI update")?;
        let (text, placeholder, enabled, password, revision) = match &descriptor.content {
            NodeContent::Text(value) => (value.as_str(), "", true, false, 0),
            NodeContent::Button { label, enabled } => (label.as_str(), "", *enabled, false, 0),
            NodeContent::Checkbox {
                label,
                enabled,
                revision,
                ..
            }
            | NodeContent::Switch {
                label,
                enabled,
                revision,
                ..
            } => (label.as_str(), "", *enabled, false, *revision),
            NodeContent::Input {
                value,
                placeholder,
                enabled,
                password,
                revision,
                ..
            } => (
                value.as_str(),
                placeholder.as_str(),
                *enabled,
                *password,
                *revision,
            ),
            NodeContent::Textarea {
                value,
                placeholder,
                enabled,
                revision,
                ..
            } => (
                value.as_str(),
                placeholder.as_str(),
                *enabled,
                false,
                *revision,
            ),
            NodeContent::Image { source, alt } => (source.as_str(), alt.as_str(), true, false, 0),
            NodeContent::Select {
                options,
                value,
                placeholder,
                enabled,
                revision,
            } => (
                options
                    .iter()
                    .find(|option| option.value == *value)
                    .map_or(placeholder.as_str(), |option| option.label.as_str()),
                placeholder.as_str(),
                *enabled,
                false,
                *revision,
            ),
            NodeContent::Container | NodeContent::Scroll => ("", "", true, false, 0),
            NodeContent::Dialog { revision, .. } | NodeContent::Popover { revision, .. } => {
                ("", "", true, false, *revision)
            }
        };
        let text = cstring(text)?;
        let placeholder = cstring(placeholder)?;
        let family = cstring(if descriptor.style.font_family == "system-ui" {
            ""
        } else {
            &descriptor.style.font_family
        })?;
        let accessible = cstring(descriptor.accessible_name.as_deref().unwrap_or(""))?;
        let description = cstring(descriptor.accessible_description.as_deref().unwrap_or(""))?;
        let accessible_role = match descriptor.accessible_role {
            Some(AccessibleRole::Tree) => 1,
            Some(AccessibleRole::TreeItem) => 2,
            Some(AccessibleRole::RadioGroup) => 3,
            Some(AccessibleRole::RadioItem { tab_stop: false }) => 4,
            Some(AccessibleRole::RadioItem { tab_stop: true }) => 5,
            _ => 0,
        };
        let accessible_selected = descriptor.accessible_selected.map(i32::from).unwrap_or(-1);
        let accessible_expanded = descriptor.accessible_expanded.map(i32::from).unwrap_or(-1);
        match &descriptor.content {
            NodeContent::Input { max_length, .. } | NodeContent::Textarea { max_length, .. } => {
                let limit = max_length
                    .map(i32::try_from)
                    .transpose()
                    .map_err(|_| "ArkUI max length overflow")?
                    .unwrap_or(-1);
                check(unsafe {
                    jadix_ohos_max_length(
                        control.handle,
                        matches!(descriptor.content, NodeContent::Textarea { .. }),
                        limit,
                    )
                })?;
            }
            _ => {}
        }
        check(unsafe {
            jadix_ohos_update(
                control.handle,
                native_kind(descriptor)?,
                text.as_ptr(),
                placeholder.as_ptr(),
                enabled,
                password,
                matches!(
                    descriptor.accessible_role,
                    Some(AccessibleRole::RadioItem { .. })
                ) && descriptor.accessible_selected == Some(true)
                    || matches!(
                        descriptor.content,
                        NodeContent::Checkbox { checked: true, .. }
                            | NodeContent::Switch { checked: true, .. }
                    ),
                descriptor.style.font_size,
                descriptor.style.font_weight.into(),
                family.as_ptr(),
                color(descriptor.style.foreground),
                color(descriptor.style.background),
                match descriptor.style.direction {
                    Some(LayoutDirection::LeftToRight) => 0,
                    Some(LayoutDirection::RightToLeft) => 1,
                    None => 3,
                },
                accessible.as_ptr(),
                description.as_ptr(),
                accessible_role,
                accessible_selected,
                accessible_expanded,
            )
        })?;
        if let NodeContent::Select {
            options,
            value,
            enabled,
            ..
        } = &descriptor.content
        {
            let values = options
                .iter()
                .map(|option| cstring(&option.value))
                .collect::<Result<Vec<_>, _>>()?;
            let labels = options
                .iter()
                .map(|option| cstring(&option.label))
                .collect::<Result<Vec<_>, _>>()?;
            let value_pointers = values
                .iter()
                .map(|value| value.as_ptr())
                .collect::<Vec<_>>();
            let label_pointers = labels
                .iter()
                .map(|label| label.as_ptr())
                .collect::<Vec<_>>();
            let option_enabled = options
                .iter()
                .map(|option| option.enabled)
                .collect::<Vec<_>>();
            let selected = options
                .iter()
                .position(|option| option.value == *value)
                .map_or(-1, |index| index as i32);
            check(unsafe {
                jadix_ohos_select(
                    control.handle,
                    value_pointers.as_ptr(),
                    label_pointers.as_ptr(),
                    option_enabled.as_ptr(),
                    options.len(),
                    selected,
                    *enabled,
                )
            })?;
            *control.target.options.borrow_mut() = Some(options.clone());
        } else {
            control.target.options.borrow_mut().take();
        }
        control.target.enabled.set(enabled);
        if revision > control.target.revision.get() {
            control.target.composing.set(false);
        }
        control
            .target
            .revision
            .set(control.target.revision.get().max(revision));
        control.descriptor = descriptor.clone();
        Ok(())
    }

    fn sync_radio_groups(&mut self) -> Result<(), String> {
        let mut updates = Vec::new();
        for (node, control) in &self.controls {
            if !matches!(
                control.descriptor.accessible_role,
                Some(AccessibleRole::RadioItem { .. })
            ) {
                continue;
            }
            let mut parent = control.parent;
            let mut group = None;
            while let Some(ancestor) = parent {
                let ancestor_control = self
                    .controls
                    .get(&ancestor)
                    .ok_or("unknown ArkUI radio ancestor")?;
                if ancestor_control.descriptor.accessible_role == Some(AccessibleRole::RadioGroup) {
                    group = Some(ancestor);
                    break;
                }
                parent = ancestor_control.parent;
            }
            if group != control.radio_group {
                updates.push((*node, control.handle, group));
            }
        }
        for (node, handle, group) in updates {
            let group_name = group.map(|id| id.token()).unwrap_or_default();
            let group_name = cstring(&group_name)?;
            check(unsafe { jadix_ohos_radio_group(handle, group_name.as_ptr()) })?;
            self.controls
                .get_mut(&node)
                .ok_or("unknown ArkUI radio control")?
                .radio_group = group;
        }
        Ok(())
    }

    fn apply(&mut self, batch: &CommitBatch) -> Result<ApplyReceipt, String> {
        // Changing the platform node type under a live identity would lose input state.
        // Validate the whole batch before touching the platform tree.
        for operation in &batch.operations {
            match operation {
                Operation::Create { descriptor, .. } => {
                    native_kind(descriptor)?;
                }
                Operation::Update { node, descriptor } => {
                    let next = native_kind(descriptor)?;
                    if let Some(control) = self.controls.get(node) {
                        if next != native_kind(&control.descriptor)? {
                            return Err(String::from(
                                "ArkUI node type change requires a new identity",
                            ));
                        }
                    }
                }
                _ => {}
            }
        }
        if self.events.closed.get()
            || self.root.is_some_and(|root| root != batch.root)
            || batch.previous != self.revision
            || self.revision.checked_add(1) != Some(batch.revision)
        {
            return Err(String::from("stale, foreign or closed ArkUI batch"));
        }
        check(unsafe {
            jadix_ohos_geometry(self.container, 0.0, 0.0, self.viewport[0], self.viewport[1])
        })?;
        let mut focus = None;
        for operation in &batch.operations {
            match operation {
                Operation::Create {
                    node,
                    parent,
                    index,
                    descriptor,
                } => {
                    let mut target = Box::new(Target {
                        node: *node,
                        revision: Cell::new(0),
                        composing: Cell::new(false),
                        events: self.events.clone(),
                        enabled: Cell::new(true),
                        options: RefCell::new(None),
                    });
                    let handle = unsafe {
                        jadix_ohos_create(
                            native_kind(descriptor)?,
                            (&mut *target as *mut Target).cast(),
                        )
                    };
                    if handle.is_null() {
                        return Err(String::from("ArkUI node creation failed"));
                    }
                    self.controls.insert(
                        *node,
                        Control {
                            handle,
                            parent: *parent,
                            descriptor: descriptor.clone(),
                            radio_group: None,
                            target,
                        },
                    );
                    self.attach(*node, *parent, *index)?;
                    self.update(*node, descriptor)?;
                }
                Operation::Update { node, descriptor } => self.update(*node, descriptor)?,
                Operation::Move {
                    node,
                    parent,
                    index,
                } => {
                    if !self.controls.contains_key(node) {
                        return Err(String::from("unknown ArkUI move"));
                    }
                    self.detach(*node)?;
                    self.attach(*node, *parent, *index)?;
                    self.controls.get_mut(node).unwrap().parent = *parent;
                }
                Operation::Remove { node } => {
                    if !self.controls.contains_key(node) {
                        return Err(String::from("unknown ArkUI removal"));
                    }
                    self.detach(*node)?;
                    let mut removed = vec![*node];
                    let mut cursor = 0;
                    while cursor < removed.len() {
                        removed.extend(
                            self.controls
                                .iter()
                                .filter(|(_, child)| child.parent == Some(removed[cursor]))
                                .map(|(node, _)| *node)
                                .collect::<Vec<_>>(),
                        );
                        cursor += 1;
                    }
                    for node in removed.iter().rev() {
                        let control = &self.controls[node];
                        if matches!(control.descriptor.content, NodeContent::Select { .. }) {
                            check(unsafe { jadix_ohos_select_close(control.handle) })?;
                        }
                        if matches!(
                            control.descriptor.content,
                            NodeContent::Dialog { .. } | NodeContent::Popover { .. }
                        ) {
                            check(unsafe { jadix_ohos_dialog(control.handle, false, false) })?;
                        }
                    }
                    for node in removed.into_iter().rev() {
                        self.children.remove(&Some(node));
                        self.dialogs.retain(|dialog| *dialog != node);
                        if let Some(control) = self.controls.remove(&node) {
                            unsafe {
                                jadix_ohos_dispose(control.handle);
                            }
                        }
                    }
                }
                Operation::Focus { node } => {
                    focus = Some(*node);
                }
            }
        }
        self.sync_radio_groups()?;
        for control in self.controls.values() {
            let frame = control.descriptor.frame;
            let parent = control
                .parent
                .filter(|_| {
                    !matches!(
                        control.descriptor.content,
                        NodeContent::Dialog { .. } | NodeContent::Popover { .. }
                    )
                })
                .map(|parent| self.controls[&parent].descriptor.frame)
                .unwrap_or_default();
            check(unsafe {
                jadix_ohos_geometry(
                    control.handle,
                    (frame.x - parent.x) as f32,
                    (frame.y - parent.y) as f32,
                    frame.width as f32,
                    frame.height as f32,
                )
            })?;
        }
        self.sync_dialogs()?;
        if let Some(node) = focus {
            check(unsafe {
                jadix_ohos_focus(
                    node.map_or(Ok(self.container), |node| {
                        self.controls
                            .get(&node)
                            .map(|control| control.handle)
                            .ok_or("unknown ArkUI focus")
                    })?,
                    node.is_some(),
                )
            })?;
        }
        self.root = Some(batch.root);
        self.revision = batch.revision;
        Ok(ApplyReceipt::for_batch(batch))
    }
}

impl PlatformHost for OhosHost {
    fn is_alive(&self) -> bool {
        !self.events.closed.get()
    }

    fn scroll_to(&mut self, node: NodeId, offset: [f64; 2]) -> Result<(), String> {
        if self.events.closed.get()
            || offset
                .iter()
                .any(|value| !value.is_finite() || *value < 0.0 || *value > f64::from(f32::MAX))
        {
            return Err(String::from("invalid or closed ArkUI scroll request"));
        }
        let control = self
            .controls
            .get(&node)
            .filter(|control| matches!(control.descriptor.content, NodeContent::Scroll))
            .ok_or("unknown ArkUI scroll target")?;
        check(unsafe { jadix_ohos_scroll(control.handle, offset[0] as f32, offset[1] as f32) })
    }

    fn capabilities(&self) -> BackendCapabilities {
        let mut node_kinds = vec![
            NodeKind::Container,
            NodeKind::Text,
            NodeKind::Button,
            NodeKind::Input,
            NodeKind::Textarea,
            NodeKind::Scroll,
            NodeKind::Checkbox,
            NodeKind::Switch,
            NodeKind::Image,
        ];
        if unsafe { jadix_ohos_dialog_supported() } {
            node_kinds.push(NodeKind::Dialog);
        }
        if unsafe { jadix_ohos_dialog_supported() } {
            node_kinds.push(NodeKind::Popover);
            node_kinds.push(NodeKind::Select);
        }
        BackendCapabilities {
            node_kinds,
            text_editing: true,
            focus: true,
            scrolling: true,
        }
    }

    fn measure(
        &mut self,
        request: MeasureRequest,
        descriptor: &NodeDescriptor,
    ) -> Result<MeasureResult, LayoutError> {
        let fail = || LayoutError::MeasurementUnavailable(request.key.node);
        if let NodeContent::Image { .. } = &descriptor.content {
            let [width, height] = descriptor.content.image_dimensions().ok_or_else(fail)?;
            let width = f64::from(width) / self.density;
            let height = f64::from(height) / self.density;
            let scale = (request.key.max_width / width)
                .min(request.key.max_height / height)
                .min(1.0);
            return Ok(MeasureResult {
                key: request.key,
                width: width * scale,
                height: height * scale,
                baseline: None,
            });
        }
        let text = match &descriptor.content {
            NodeContent::Text(text)
            | NodeContent::Input { value: text, .. }
            | NodeContent::Textarea { value: text, .. } => text.as_str(),
            NodeContent::Button { label, .. }
            | NodeContent::Checkbox { label, .. }
            | NodeContent::Switch { label, .. } => label.as_str(),
            NodeContent::Select {
                options,
                value,
                placeholder,
                ..
            } => options
                .iter()
                .find(|option| option.value == *value)
                .map_or(placeholder.as_str(), |option| option.label.as_str()),
            _ => "",
        };
        let text = cstring(text).map_err(|_| fail())?;
        let family = cstring(if descriptor.style.font_family == "system-ui" {
            ""
        } else {
            &descriptor.style.font_family
        })
        .map_err(|_| fail())?;
        let mut result = [0.0; 3];
        check(unsafe {
            jadix_ohos_measure(
                text.as_ptr(),
                family.as_ptr(),
                f64::from(descriptor.style.font_size) * self.density,
                descriptor.style.font_weight.into(),
                request.key.max_width * self.density,
                result.as_mut_ptr(),
            )
        })
        .map_err(|_| fail())?;
        let chrome = if matches!(
            descriptor.accessible_role,
            Some(AccessibleRole::RadioItem { .. })
        ) || matches!(
            descriptor.content,
            NodeContent::Checkbox { .. } | NodeContent::Switch { .. }
        ) {
            52.0
        } else if matches!(
            descriptor.content,
            NodeContent::Button { .. }
                | NodeContent::Input { .. }
                | NodeContent::Textarea { .. }
                | NodeContent::Select { .. }
        ) {
            12.0
        } else {
            0.0
        };
        Ok(MeasureResult {
            key: request.key,
            width: (result[0] / self.density + chrome).min(request.key.max_width),
            height: (result[1] / self.density + chrome.min(12.0))
                .max(if chrome == 52.0 { 28.0 } else { 0.0 })
                .min(request.key.max_height),
            baseline: Some((result[2] / self.density).min(request.key.max_height)),
        })
    }

    fn publish(&mut self, _: RootId, _: u64, _: &[PresentationNode]) -> Result<(), String> {
        Err(String::from("ArkUI host requires incremental batches"))
    }

    fn publish_batch(
        &mut self,
        batch: &CommitBatch,
        roots: &[PresentationNode],
    ) -> Result<ApplyReceipt, String> {
        self.events.suppress.set(true);
        let result = self.apply(batch);
        self.events.suppress.set(false);
        let receipt = result?;
        self.publish_accessibility(roots)?;
        Ok(receipt)
    }

    fn take_events(&mut self) -> Result<Vec<PlatformEvent>, String> {
        if self.events.closed.get() || self.events.failed.get() {
            return Err(String::from("ArkUI event queue failed or root is closed"));
        }
        let mut queue = self.events.queue.borrow_mut();
        self.events.scheduled.set(false);
        queue.bytes = 0;
        Ok(std::mem::take(&mut queue.events)
            .into_iter()
            .filter(|event| self.controls.contains_key(&event.node))
            .collect())
    }

    fn dispose(&mut self) {
        if self.events.closed.replace(true) {
            return;
        }
        if !self.accessibility.is_null() {
            unsafe { jadix_ohos_accessibility_dispose(self.accessibility) };
            self.accessibility = std::ptr::null_mut();
        }
        unsafe {
            jadix_ohos_mount(self.content, self.container, false);
        }
        let _ = self.close_selects();
        for node in self.dialogs.iter().rev() {
            unsafe {
                jadix_ohos_dialog(self.controls[node].handle, false, false);
            }
        }
        self.dialogs.clear();
        while !self.controls.is_empty() {
            let leaves: Vec<_> = self
                .controls
                .keys()
                .filter(|node| {
                    !self
                        .controls
                        .values()
                        .any(|child| child.parent == Some(**node))
                })
                .copied()
                .collect();
            for node in leaves {
                if let Some(control) = self.controls.remove(&node) {
                    unsafe {
                        jadix_ohos_dispose(control.handle);
                    }
                }
            }
        }
        unsafe {
            jadix_ohos_dispose(self.container);
        }
        self.container = std::ptr::null_mut();
        self.children.clear();
        self.events.queue.borrow_mut().events.clear();
        self.events.notifier.borrow_mut().take();
    }
}

impl Drop for OhosHost {
    fn drop(&mut self) {
        self.dispose();
    }
}

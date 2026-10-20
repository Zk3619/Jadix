//! Retained workbench docking, built on Jadix's resizable and drag primitives.
use crate::primitives::{
    interactive::interactive,
    r#box::{hstack, vstack},
};
use crate::utils::drag::{self, CollisionAlg, DragEvent};
use crate::{
    app::reactive::reactive_view,
    core::{
        reactivity::{use_signal, Signal},
        tree::Element,
    },
};
use std::{collections::HashMap, rc::Rc};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DockOrientation {
    Horizontal,
    Vertical,
}
impl DockOrientation {
    fn as_str(self) -> &'static str {
        match self {
            Self::Horizontal => "horizontal",
            Self::Vertical => "vertical",
        }
    }
}

#[derive(Clone)]
pub struct DockTab {
    key: String,
    title: String,
    closable: bool,
    content: Rc<dyn Fn() -> Element>,
}
impl PartialEq for DockTab {
    fn eq(&self, other: &Self) -> bool {
        self.key == other.key
            && self.title == other.title
            && self.closable == other.closable
            && Rc::ptr_eq(&self.content, &other.content)
    }
}
impl DockTab {
    pub fn new(
        key: impl Into<String>,
        title: impl Into<String>,
        content: impl Fn() -> Element + 'static,
    ) -> Self {
        Self {
            key: key.into(),
            title: title.into(),
            closable: true,
            content: Rc::new(content),
        }
    }
    pub fn closable(mut self, value: bool) -> Self {
        self.closable = value;
        self
    }
}

#[derive(Clone, PartialEq)]
pub enum DockLayout {
    Tabs {
        id: String,
        tabs: Vec<DockTab>,
    },
    Split {
        id: String,
        orientation: DockOrientation,
        sizes: Vec<f32>,
        children: Vec<DockLayout>,
    },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DockPlacement {
    Center,
    Left,
    Right,
    Top,
    Bottom,
}
impl DockPlacement {
    fn at([x, y, w, h]: [f32; 4], p: [f32; 2], center_ratio: f32) -> Self {
        let x = (p[0] - x) / w.max(1.0);
        let y = (p[1] - y) / h.max(1.0);
        let (distance, edge) = [
            (x, Self::Left),
            (1.0 - x, Self::Right),
            (y, Self::Top),
            (1.0 - y, Self::Bottom),
        ]
        .into_iter()
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .unwrap();
        if distance < (1.0 - center_ratio) / 2.0 {
            edge
        } else {
            Self::Center
        }
    }
    fn preview(self, [x, y, w, h]: [f32; 4]) -> [f32; 4] {
        match self {
            Self::Center => [x, y, w, h],
            Self::Left => [x, y, w / 2.0, h],
            Self::Right => [x + w / 2.0, y, w / 2.0, h],
            Self::Top => [x, y, w, h / 2.0],
            Self::Bottom => [x, y + h / 2.0, w, h / 2.0],
        }
    }
}
impl DockLayout {
    pub fn tabs(id: impl Into<String>, tabs: impl IntoIterator<Item = DockTab>) -> Self {
        Self::Tabs {
            id: id.into(),
            tabs: tabs.into_iter().collect(),
        }
    }
    pub fn split(
        id: impl Into<String>,
        orientation: DockOrientation,
        sizes: impl IntoIterator<Item = f32>,
        children: impl IntoIterator<Item = Self>,
    ) -> Self {
        Self::Split {
            id: id.into(),
            orientation,
            sizes: sizes.into_iter().collect(),
            children: children.into_iter().collect(),
        }
    }
    fn id(&self) -> &str {
        match self {
            Self::Tabs { id, .. } | Self::Split { id, .. } => id,
        }
    }
    fn group(&self, target: &str) -> Option<&Vec<DockTab>> {
        match self {
            Self::Tabs { id, tabs } => (id == target).then_some(tabs),
            Self::Split { children, .. } => children.iter().find_map(|c| c.group(target)),
        }
    }
    fn first_group(&self) -> Option<&str> {
        match self {
            Self::Tabs { id, .. } => Some(id),
            Self::Split { children, .. } => children.iter().find_map(Self::first_group),
        }
    }
    fn take(&mut self, group: &str, key: &str) -> Option<DockTab> {
        match self {
            Self::Tabs { id, tabs } if id == group => tabs
                .iter()
                .position(|t| t.key == key)
                .map(|i| tabs.remove(i)),
            Self::Split { children, .. } => children.iter_mut().find_map(|c| c.take(group, key)),
            _ => None,
        }
    }
    fn insert(
        &mut self,
        group: &str,
        tab: DockTab,
        index: usize,
        place: DockPlacement,
        new_id: &str,
    ) -> bool {
        match self {
            Self::Tabs { id, tabs } if id == group => {
                if place == DockPlacement::Center {
                    tabs.insert(index.min(tabs.len()), tab);
                } else {
                    let new = Self::tabs(new_id, [tab]);
                    let old = self.clone();
                    let axis = if matches!(place, DockPlacement::Left | DockPlacement::Right) {
                        DockOrientation::Horizontal
                    } else {
                        DockOrientation::Vertical
                    };
                    *self = Self::split(
                        format!("{new_id}__split"),
                        axis,
                        [50.0, 50.0],
                        if matches!(place, DockPlacement::Left | DockPlacement::Top) {
                            [new, old]
                        } else {
                            [old, new]
                        },
                    );
                }
                true
            }
            Self::Split { children, .. } => children
                .iter_mut()
                .any(|c| c.insert(group, tab.clone(), index, place, new_id)),
            _ => false,
        }
    }
    fn compact(&mut self) -> bool {
        match self {
            Self::Tabs { tabs, .. } => tabs.is_empty(),
            Self::Split {
                children, sizes, ..
            } => {
                let mut i = 0;
                while i < children.len() {
                    if children[i].compact() {
                        children.remove(i);
                        if i < sizes.len() {
                            sizes.remove(i);
                        }
                    } else {
                        i += 1;
                    }
                }
                if children.len() == 1 {
                    *self = children.remove(0);
                    false
                } else {
                    children.is_empty()
                }
            }
        }
    }
    pub fn into_element(self) -> Element {
        DockState::new(self.id().to_string(), self).into_element()
    }
}

#[derive(Clone, PartialEq)]
struct DockData {
    layout: DockLayout,
    selected: HashMap<String, String>,
    closed: Vec<(String, DockTab)>,
    maximized: Option<String>,
    sequence: u64,
    generation: u64,
}
impl DockData {
    fn selected(&self, group: &str, tabs: &[DockTab]) -> String {
        self.selected
            .get(group)
            .filter(|key| tabs.iter().any(|t| &t.key == *key))
            .cloned()
            .or_else(|| tabs.first().map(|t| t.key.clone()))
            .unwrap_or_default()
    }
    fn move_tab(
        &mut self,
        source: &str,
        key: &str,
        target: &str,
        index: usize,
        place: DockPlacement,
    ) -> bool {
        let Some(tabs) = self.layout.group(target) else {
            return false;
        };
        if source != target && tabs.iter().any(|t| t.key == key) {
            return false;
        }
        if source == target {
            if place != DockPlacement::Center && tabs.len() == 1 {
                return false;
            }
        }
        let Some(tab) = self.layout.take(source, key) else {
            return false;
        };
        self.sequence += 1;
        let new_id = format!("dock_group_{}", self.sequence);
        self.layout.insert(target, tab, index, place, &new_id);
        self.selected.insert(
            if place == DockPlacement::Center {
                target
            } else {
                &new_id
            }
            .to_string(),
            key.to_string(),
        );
        self.layout.compact();
        self.maximized = None;
        true
    }
}

/// Group IDs must be unique in a workspace and tab keys unique within a group.
#[derive(Clone)]
pub struct DockState {
    id: String,
    initial: DockLayout,
    data: Signal<DockData>,
    pointer: Signal<[f32; 2]>,
    center_drop_ratio: f32,
}
impl DockState {
    pub fn new(id: impl Into<String>, layout: DockLayout) -> Self {
        let id = id.into();
        let data = use_signal(&format!("{id}__dock_state"), || DockData {
            layout: layout.clone(),
            selected: HashMap::new(),
            closed: Vec::new(),
            maximized: None,
            sequence: 0,
            generation: 0,
        });
        let pointer = use_signal(&format!("{id}__dock_pointer"), || [0.0; 2]);
        Self {
            id,
            initial: layout,
            data,
            pointer,
            center_drop_ratio: 0.75,
        }
    }
    /// Fraction of each panel dimension reserved for merging, from 0 to 1.
    /// The default is 0.75; only the outer 12.5% on each side splits.
    pub fn center_drop_ratio(mut self, ratio: f32) -> Self {
        if ratio.is_finite() {
            self.center_drop_ratio = ratio.clamp(0.0, 1.0);
        }
        self
    }
    pub fn reset(&self) {
        self.data.update(|d| {
            d.layout = self.initial.clone();
            d.selected.clear();
            d.closed.clear();
            d.maximized = None;
            d.generation += 1;
        });
    }
    pub fn close(&self, group: &str, key: &str) {
        self.data.update(|d| {
            if d.layout
                .group(group)
                .is_some_and(|ts| ts.iter().any(|t| t.key == key && t.closable))
            {
                if let Some(tab) = d.layout.take(group, key) {
                    d.closed.push((group.to_string(), tab));
                }
                d.layout.compact();
                d.maximized = None;
            }
        });
    }
    pub fn restore_closed(&self) {
        self.data.update(|d| {
            if let Some((group, tab)) = d.closed.pop() {
                let target = if d.layout.group(&group).is_some() {
                    group
                } else {
                    d.layout.first_group().unwrap_or(&group).to_string()
                };
                let key = tab.key.clone();
                d.layout
                    .insert(&target, tab, usize::MAX, DockPlacement::Center, "");
                d.selected.insert(target, key);
            }
        });
    }
    pub fn move_tab(
        &self,
        source: &str,
        key: &str,
        target: &str,
        index: usize,
        place: DockPlacement,
    ) -> bool {
        let mut d = self.data.peek();
        if !d.move_tab(source, key, target, index, place) {
            return false;
        }
        self.data.set(d);
        true
    }
    pub fn into_element(self) -> Element {
        let state = self.clone();
        let end = self.clone();
        let pointer = self.pointer;
        let id = format!("{}__dock_body", self.id);
        let mut root = drag::drag_scope(format!("{}__dock_drag", self.id))
            .collision(CollisionAlg::PointerWithin)
            .threshold(6.0)
            .animation(true)
            .on_drag_move(move |ev| {
                pointer.try_set(ev.pointer);
            })
            .on_drag_end(move |ev| {
                if let (Some(payload), Some(target)) = (
                    ev.data.as_ref().and_then(|p| p.downcast_ref::<DockDrag>()),
                    ev.droppable_id.as_deref(),
                ) {
                    if let Some((group, index, place)) =
                        destination(&end.data.peek().layout, ev, target, end.center_drop_ratio)
                    {
                        end.move_tab(&payload.group, &payload.key, &group, index, place);
                    }
                }
            })
            .child(move || {
                vstack()
                    .w("100%")
                    .h("100%")
                    .min_w(0.0)
                    .min_h(0.0)
                    .gap(0.0)
                    .child(
                        reactive_view(id, move || {
                            let d = state.data.get();
                            render(&d.layout, &state, &d)
                        })
                        .w("100%")
                        .h("100%")
                        .min_w(0.0)
                        .min_h(0.0),
                    )
                    .child(Element::from(drag::drag_overlay().child_for(|item| {
                        Element::new()
                            .flex_row()
                            .items_center()
                            .gap(8.0)
                            .h(34.0)
                            .px(12.0)
                            .rounded(5.0)
                            .child(dock_icon("panel-top"))
                            .child(
                                Element::new()
                                    .text(
                                        item.data_as::<DockDrag>()
                                            .map(|d| d.title)
                                            .unwrap_or_default(),
                                    )
                                    .text_size(13.0),
                            )
                            .bg("background")
                            .border("border", 1.0)
                            .shadow([0.0, 0.0, 0.0, 0.22], 12.0)
                    })))
                    .into()
            });
        root.set_attr("data-slot", "dock-workspace");
        root
    }
}
#[derive(Clone)]
struct DockDrag {
    group: String,
    key: String,
    title: String,
}
fn destination(
    layout: &DockLayout,
    ev: &DragEvent,
    target: &str,
    center_ratio: f32,
) -> Option<(String, usize, DockPlacement)> {
    match layout {
        DockLayout::Tabs { id, tabs } => {
            for (i, tab) in tabs.iter().enumerate() {
                if target == format!("{id}__dock_tab_{}", tab.key) {
                    return Some((id.clone(), ev.to_index.unwrap_or(i), DockPlacement::Center));
                }
            }
            if target == format!("{id}__dock_header") {
                return Some((
                    id.clone(),
                    ev.to_index.unwrap_or(tabs.len()),
                    DockPlacement::Center,
                ));
            }
            if target == format!("{id}__dock_drop") {
                return Some((
                    id.clone(),
                    tabs.len(),
                    DockPlacement::at(
                        drag::last_hit_rect(&ev.scope_id, target)?,
                        ev.pointer,
                        center_ratio,
                    ),
                ));
            }
            None
        }
        DockLayout::Split { children, .. } => children
            .iter()
            .find_map(|c| destination(c, ev, target, center_ratio)),
    }
}
fn dock_icon(name: &str) -> Element {
    crate::utils::build_named_icon_text_node(
        name,
        crate::utils::lucide_glyph(name).unwrap_or_else(|| "×".into()),
    )
    .text_size(14.0)
    .text_color("muted-foreground")
    .pointer_events_none()
}
fn action(id: String, label: &str, glyph: &str, click: impl FnMut() + 'static) -> Element {
    let mut el: Element = interactive(
        Element::new()
            .w(26.0)
            .h(28.0)
            .flex_row()
            .items_center()
            .justify_center()
            .child(dock_icon(glyph))
            .text_color("muted-foreground")
            .rounded(3.0)
            .class("hover:bg-accent hover:text-foreground")
            .aria_label(label),
    )
    .focus_id(&id)
    .focusable(true)
    .focus_ring(true)
    .on_click(click)
    .into();
    el.template_id = Some(id);
    el
}
fn render(layout: &DockLayout, state: &DockState, d: &DockData) -> Element {
    match layout {
        DockLayout::Tabs { id, tabs } => render_group(id, tabs, state, d),
        DockLayout::Split {
            id,
            orientation,
            sizes,
            children,
        } => {
            if children.len() < 2 {
                return children
                    .first()
                    .map(|c| render(c, state, d))
                    .unwrap_or_else(Element::new);
            }
            let active = d
                .maximized
                .as_ref()
                .and_then(|g| children.iter().position(|c| c.group(g).is_some()));
            let split_id = format!("{id}__g{}", d.generation);
            let weights = use_signal(&format!("{split_id}__sizes"), || sizes.clone());
            let mut split = crate::components::resizable(split_id)
                .orientation(orientation.as_str())
                .default_sizes(sizes.clone())
                .sizes_signal(weights)
                .min_sizes(vec![5.0; children.len()])
                .with_handle(false)
                .fill();
            for c in children {
                let c = c.clone();
                let state = state.clone();
                let d = d.clone();
                split = split.panel(move || render(&c, &state, &d));
            }
            let mut root: Element = split.into();
            if let Some(active) = active {
                if let Some(row) = root.children.first_mut() {
                    for (i, c) in row.children.iter_mut().enumerate() {
                        if i != active * 2 {
                            c.style.display = taffy::style::Display::None;
                        }
                    }
                }
            }
            root.set_attr("jadix_dock_node", "split");
            root
        }
    }
}
fn render_tab(id: &str, index: usize, tab: &DockTab, state: &DockState, current: &str) -> Element {
    let selected = tab.key == current;
    let payload = DockDrag {
        group: id.to_string(),
        key: tab.key.clone(),
        title: tab.title.clone(),
    };
    let state = state.clone();
    let tab = tab.clone();
    let id = id.to_string();
    drag::draggable(format!("{id}__dock_tab_{}", tab.key))
        .list(format!("{id}__dock_tabs"))
        .index(index)
        .source_opacity(0.35)
        .item_type("dock-tab")
        .handle_only(true)
        .also_droppable(true)
        .accept("dock-tab")
        .data(Rc::new(payload))
        .child(move || {
            let key = tab.key.clone();
            let group = id.clone();
            let select = state.clone();
            let mut title: Element = interactive(
                Element::new()
                    .flex_row()
                    .items_center()
                    .child(
                        Element::new()
                            .text(&tab.title)
                            .text_size(13.0)
                            .pointer_events_none(),
                    )
                    .px(12.0)
                    .h(33.0)
                    .class(if selected {
                        "bg-background"
                    } else {
                        "bg-muted hover:bg-accent"
                    })
                    .aria_label(&tab.title),
            )
            .focus_id(format!("{id}__select_{}", tab.key))
            .focusable(true)
            .focus_ring(true)
            .on_click(move || {
                select.data.update(|d| {
                    d.selected.insert(group.clone(), key.clone());
                })
            })
            .into();
            title.set_attr("role", "tab");
            title.set_attr("aria-selected", if selected { "true" } else { "false" });
            let mut chip: Element = hstack()
                .h(35.0)
                .gap(0.0)
                .items("center")
                .flex_shrink(0.0)
                .bg(if selected { "background" } else { "muted" })
                .child(drag::drag_handle(title))
                .into();
            chip = chip
                .border_right("border", 1.0)
                .border_top(if selected { "primary" } else { "muted" }, 2.0);
            if tab.closable {
                let close = state.clone();
                let group = id.clone();
                let key = tab.key.clone();
                chip = chip.child(drag::drag_ignore(action(
                    format!("{id}__close_{}", tab.key),
                    &format!("关闭 {}", tab.title),
                    "x",
                    move || close.close(&group, &key),
                )));
            }
            chip
        })
        .flex_shrink(0.0)
}
fn render_group(id: &str, tabs: &[DockTab], state: &DockState, d: &DockData) -> Element {
    let current = d.selected(id, tabs);
    let group = id.to_string();
    let header_state = state.clone();
    let header_tabs = tabs.to_vec();
    let strip = drag::droppable(format!("{id}__dock_header"))
        .list(format!("{id}__dock_tabs"))
        .direction(drag::Axis::X)
        .accept("dock-tab")
        .child(move || {
            let current = header_state.data.peek().selected(&group, &header_tabs);
            let triggers = header_tabs
                .iter()
                .enumerate()
                .map(|(index, tab)| render_tab(&group, index, tab, &header_state, &current))
                .collect::<Vec<_>>();
            let mut strip: Element = hstack()
                .h(35.0)
                .min_w(0.0)
                .w("100%")
                .overflow("hidden")
                .gap(0.0)
                .children(triggers)
                .into();
            strip.set_attr("role", "tablist");
            let keyboard = header_state.clone();
            let group = group.clone();
            strip.on_key_down(move |ev| {
                use winit::{
                    event::ElementState,
                    keyboard::{Key, NamedKey},
                };
                if ev.state != ElementState::Pressed {
                    return false;
                }
                let d = keyboard.data.peek();
                let Some(ts) = d.layout.group(&group) else {
                    return false;
                };
                if ts.is_empty() {
                    return false;
                }
                let current = d.selected(&group, ts);
                let i = ts.iter().position(|t| t.key == current).unwrap_or(0);
                let next = match ev.logical_key {
                    Key::Named(NamedKey::ArrowRight) => (i + 1) % ts.len(),
                    Key::Named(NamedKey::ArrowLeft) => (i + ts.len() - 1) % ts.len(),
                    Key::Named(NamedKey::Home) => 0,
                    Key::Named(NamedKey::End) => ts.len() - 1,
                    _ => return false,
                };
                let key = ts[next].key.clone();
                keyboard.data.update(|d| {
                    d.selected.insert(group.clone(), key);
                });
                true
            })
        })
        .flex_grow(1.0)
        .flex_basis(0.0)
        .min_w(0.0)
        .overflow("hidden")
        .h(35.0);
    let split = state.clone();
    let group = id.to_string();
    let key = current.clone();
    let split = action(
        format!("{id}__split"),
        "向右拆分活动标签",
        "columns-2",
        move || {
            split.move_tab(&group, &key, &group, 0, DockPlacement::Right);
        },
    );
    let max = state.clone();
    let group = id.to_string();
    let max = action(
        format!("{id}__maximize"),
        "最大化 / 还原组",
        if d.maximized.is_some() {
            "minimize-2"
        } else {
            "maximize-2"
        },
        move || {
            max.data.update(|d| {
                d.maximized = if d.maximized.as_deref() == Some(&group) {
                    None
                } else {
                    Some(group.clone())
                };
            })
        },
    );
    let mut header: Element = hstack()
        .w("100%")
        .h(36.0)
        .flex_shrink(0.0)
        .gap(0.0)
        .overflow("hidden")
        .items("center")
        .bg("muted")
        .px(2.0)
        .child(strip)
        .child(split)
        .child(max)
        .into();
    header = header
        .border_bottom_color(crate::style::parse_color("border"))
        .border_bottom_width(1.0);
    let body_tabs = tabs.to_vec();
    let group = id.to_string();
    let body_state = state.clone();
    let body = drag::droppable(format!("{id}__dock_drop"))
        .sortable(false)
        .accept("dock-tab")
        .child(move || {
            let current = body_state.data.peek().selected(&group, &body_tabs);
            let panels = body_tabs
                .iter()
                .map(|tab| {
                    let mut panel: Element = vstack()
                        .w("100%")
                        .h("100%")
                        .min_w(0.0)
                        .min_h(0.0)
                        .overflow("hidden")
                        .child((tab.content)())
                        .into();
                    panel.template_id = Some(format!("{group}__dock_panel_{}", tab.key));
                    panel.set_attr("role", "tabpanel");
                    if tab.key != current {
                        panel.style.display = taffy::style::Display::None;
                    }
                    panel
                })
                .collect::<Vec<_>>();
            vstack()
                .w("100%")
                .h("100%")
                .min_w(0.0)
                .min_h(0.0)
                .overflow("hidden")
                .children(panels)
                .into()
        })
        .w("100%")
        .flex_grow(1.0)
        .flex_basis(0.0)
        .min_h(0.0)
        .min_w(0.0)
        .overflow("hidden");
    let mut root: Element = vstack()
        .w("100%")
        .h("100%")
        .min_w(0.0)
        .min_h(0.0)
        .gap(0.0)
        .bg("background")
        .overflow("hidden")
        .child(header)
        .child(body)
        .child(preview(id, state.pointer, state.center_drop_ratio))
        .into();
    root.set_attr("jadix_dock_node", "tabs");
    root.set_attr("jadix_dock_group", id);
    root
}
fn preview(id: &str, pointer: Signal<[f32; 2]>, center_ratio: f32) -> Element {
    let ctx = drag::use_drag().expect("Dock drag scope");
    let id = id.to_string();
    let target = format!("{id}__dock_drop");
    let preview_id = format!("{id}__dock_preview");
    crate::utils::popup::flow_hidden_portal_host(reactive_view(preview_id.clone(), move || {
        if !ctx.overlay_open.get() {
            return Element::new();
        }
        let active = ctx.active_droppable.get().unwrap_or_default();
        let header_target = format!("{id}__dock_header");
        let tab_prefix = format!("{id}__dock_tab_");
        let merge = active == header_target || active.starts_with(&tab_prefix);
        if active != target && !merge {
            return Element::new();
        }
        let Some(bounds) = drag::last_hit_rect(&ctx.id, &target) else {
            return Element::new();
        };
        let placement = if merge {
            DockPlacement::Center
        } else {
            // Ghost paint-follow uses silent signal writes. Subscribe to the move
            // event instead, using the same pointer as the eventual drop commit.
            DockPlacement::at(bounds, pointer.get(), center_ratio)
        };
        let [x, y, w, h] = placement.preview(bounds);
        let mut el = Element::new()
            .fixed()
            .left(x)
            .top(y)
            .w(w)
            .h(h)
            .bg([0.23, 0.51, 0.96, 0.16])
            .border([0.23, 0.51, 0.96, 0.75], 1.0)
            .rounded(4.0)
            .pointer_events_none()
            .z_index(902);
        el.set_attr("jadix_portal", "1");
        el.set_attr("jadix_portal_key", &preview_id);
        el
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::reactivity::create_root;
    fn tab(key: &str) -> DockTab {
        DockTab::new(key, key, Element::new)
    }
    fn mount_dock(dock: DockState) -> Rc<std::cell::RefCell<Element>> {
        use crate::app::{reactive, runtime};
        use crate::core::reactivity::{create_effect, isolate_reads};
        reactive::reset_registered_anchors();
        runtime::clear_test_reconcile_root();
        let live = Rc::new(std::cell::RefCell::new(Element::new()));
        runtime::install_test_reconcile_root(live.clone());
        let tree = live.clone();
        create_effect(move || {
            *tree.borrow_mut() = isolate_reads(|| dock.clone().into_element());
        });
        reactive::drain_deferred_view_remounts();
        live
    }
    #[test]
    fn central_three_quarters_merges_and_only_outer_edges_split() {
        let bounds = [100.0, 200.0, 800.0, 400.0];
        assert_eq!(
            DockPlacement::at(bounds, [220.0, 400.0], 0.75),
            DockPlacement::Center
        );
        assert_eq!(
            DockPlacement::at(bounds, [500.0, 260.0], 0.75),
            DockPlacement::Center
        );
        assert_eq!(
            DockPlacement::at(bounds, [180.0, 400.0], 0.75),
            DockPlacement::Left
        );
        assert_eq!(
            DockPlacement::at(bounds, [500.0, 560.0], 0.75),
            DockPlacement::Bottom
        );
        assert_eq!(
            DockPlacement::at(bounds, [220.0, 400.0], 0.5),
            DockPlacement::Left
        );
    }
    #[test]
    fn dragging_over_a_panel_body_does_not_reserve_a_sortable_row() {
        let (_owner, ()) = create_root(|| {
            let dock = DockState::new(
                "body-drop-regression",
                DockLayout::split(
                    "columns",
                    DockOrientation::Horizontal,
                    [50.0, 50.0],
                    [
                        DockLayout::tabs("source", [tab("a"), tab("b")]),
                        DockLayout::tabs("target", [tab("c")]),
                    ],
                ),
            );
            let live = mount_dock(dock);
            let mut layout = crate::layout::layout_tree::LayoutTree::new();
            layout.build_and_layout(&mut live.borrow_mut(), 1000.0, 700.0, [0.0; 2]);
            let hits = drag::collect_hits(&live.borrow());
            let source = hits
                .iter()
                .find(|hit| hit.id == "source__dock_tab_a")
                .unwrap();
            let target = hits
                .iter()
                .find(|hit| hit.id == "target__dock_drop")
                .unwrap();
            let before = target.rect.to_bounds();
            drag::begin(drag::BeginDrag {
                scope_id: "body-drop-regression__dock_drag".into(),
                draggable_id: source.id.clone(),
                pointer: [source.rect.x + 5.0, source.rect.y + 5.0],
                grab_offset: [5.0; 2],
                start_bounds: source.rect.to_bounds(),
                from_list: Some("source__dock_tabs".into()),
                from_index: Some(0),
                item_type: "dock-tab".into(),
                group: String::new(),
                capture_id: "capture".into(),
            });
            drag::tick(
                before[0] + before[2] / 2.0,
                before[1] + before[3] / 2.0,
                &hits,
                [1000.0, 700.0],
            );
            crate::app::reactive::drain_deferred_view_remounts();
            let mut root = live.borrow_mut();
            let mut impact = crate::core::tree::LayoutStatus::Clean;
            crate::core::tree::sync_reactive_elements_subtree(
                &mut root,
                &mut layout,
                &mut impact,
                1000.0,
                700.0,
                [0.0; 2],
            );
            fn body(element: &Element) -> Option<&Element> {
                if element
                    .get_attr(drag::ATTR_DRAG_DROPPABLE)
                    .map(String::as_str)
                    == Some("target__dock_drop")
                {
                    return Some(element);
                }
                element.children.iter().find_map(body)
            }
            let target = body(&root).unwrap();
            assert_eq!(
                crate::layout::lp_length_px(target.style.padding.bottom).unwrap_or(0.0),
                0.0
            );
            assert_eq!(
                crate::layout::lp_length_px(target.style.padding.right).unwrap_or(0.0),
                0.0
            );
            assert!(target.get_attr(drag::ATTR_DRAG_LIST).is_none());
            drop(root);
            let _ = drag::end(&hits);
            crate::app::runtime::clear_test_reconcile_root();
            crate::app::reactive::reset_registered_anchors();
        });
    }
    #[test]
    fn docking_keeps_nested_groups_filling_their_panels() {
        let (_owner, ()) = create_root(|| {
            let dock = DockState::new(
                "fill-regression",
                DockLayout::split(
                    "columns",
                    DockOrientation::Horizontal,
                    [20.0, 80.0],
                    [
                        DockLayout::tabs("files", [tab("files")]),
                        DockLayout::split(
                            "rows",
                            DockOrientation::Vertical,
                            [75.0, 25.0],
                            [
                                DockLayout::tabs("editor", [tab("web"), tab("notes")]),
                                DockLayout::tabs("tools", [tab("terminal"), tab("problems")]),
                            ],
                        ),
                    ],
                ),
            );
            dock.move_tab("tools", "problems", "editor", 2, DockPlacement::Center);
            dock.move_tab("editor", "notes", "tools", 1, DockPlacement::Right);
            let live = mount_dock(dock);
            let mut root = live.borrow_mut();
            let mut layout = crate::layout::layout_tree::LayoutTree::new();
            layout.build_and_layout(&mut root, 1000.0, 700.0, [0.0; 2]);
            fn check(element: &Element) -> usize {
                let mut panels = 0;
                if element.get_attr("data-slot").map(String::as_str) == Some("resizable-panel") {
                    panels += 1;
                    assert!(element.layout_bounds[3] > 0.0);
                    let content = &element.children[0];
                    assert!(
                        (element.layout_bounds[3] - content.layout_bounds[3]).abs() < 1.0,
                        "panel {:?} content {:?}",
                        element.layout_bounds,
                        content.layout_bounds
                    );
                }
                for child in &element.children {
                    panels += check(child);
                }
                panels
            }
            assert!(check(&root) >= 4);
            drop(root);
            crate::app::runtime::clear_test_reconcile_root();
            crate::app::reactive::reset_registered_anchors();
        });
    }
    #[test]
    fn docking_moves_splits_collapses_restores_and_resets() {
        let (_owner, ()) = create_root(|| {
            let dock = DockState::new(
                "test",
                DockLayout::split(
                    "root",
                    DockOrientation::Horizontal,
                    [20.0, 80.0],
                    [
                        DockLayout::tabs("left", [tab("files").closable(false)]),
                        DockLayout::tabs("editor", [tab("web"), tab("notes")]),
                    ],
                ),
            );
            assert!(dock.move_tab("editor", "notes", "left", 0, DockPlacement::Center));
            assert_eq!(
                dock.data.peek().layout.group("left").unwrap()[0].key,
                "notes"
            );
            assert!(dock.move_tab("left", "notes", "editor", 0, DockPlacement::Bottom));
            let data = dock.data.peek();
            let DockLayout::Split { children, .. } = &data.layout else {
                panic!()
            };
            let DockLayout::Split { children, .. } = &children[1] else {
                panic!()
            };
            let group = children[1].id().to_string();
            dock.close(&group, "notes");
            assert_eq!(dock.data.peek().layout.group("editor").unwrap().len(), 1);
            dock.restore_closed();
            assert!(dock.data.peek().closed.is_empty());
            dock.close("left", "files");
            assert!(dock
                .data
                .peek()
                .layout
                .group("left")
                .unwrap()
                .iter()
                .any(|t| t.key == "files"));
            assert!(!dock.move_tab("editor", "web", "missing", 0, DockPlacement::Right));
            dock.reset();
            assert_eq!(dock.data.peek().layout.group("editor").unwrap().len(), 2);
        });
    }
    #[test]
    fn moving_within_one_target_keeps_the_preview_pointer_live() {
        let (_owner, ()) = create_root(|| {
            let dock = DockState::new(
                "live-preview",
                DockLayout::tabs("editor", [tab("a"), tab("b")]),
            );
            let _workspace = dock.clone().into_element();
            let scope = "live-preview__dock_drag";
            let mut body = Element::new();
            body.layout_bounds = [0.0, 100.0, 800.0, 600.0];
            body.set_attr(drag::ATTR_DRAG_SCOPE, scope);
            body.set_attr(drag::ATTR_DRAG_DROPPABLE, "editor__dock_drop");
            body.set_attr(drag::ATTR_DRAG_ACCEPT, "dock-tab");
            let hits = drag::collect_hits(&body);
            drag::begin(drag::BeginDrag {
                scope_id: scope.into(),
                draggable_id: "a".into(),
                pointer: [30.0, 20.0],
                grab_offset: [10.0, 10.0],
                start_bounds: [20.0, 10.0, 100.0, 30.0],
                from_list: None,
                from_index: None,
                item_type: "dock-tab".into(),
                group: String::new(),
                capture_id: "capture".into(),
            });
            assert!(drag::tick(400.0, 300.0, &hits, [1000.0, 800.0]));
            assert_eq!(dock.pointer.peek(), [400.0, 300.0]);
            assert_eq!(
                DockPlacement::at(
                    hits[0].rect.to_bounds(),
                    dock.pointer.peek(),
                    dock.center_drop_ratio
                ),
                DockPlacement::Center
            );
            assert!(drag::tick(780.0, 300.0, &hits, [1000.0, 800.0]));
            assert_eq!(dock.pointer.peek(), [780.0, 300.0]);
            assert_eq!(
                DockPlacement::at(
                    hits[0].rect.to_bounds(),
                    dock.pointer.peek(),
                    dock.center_drop_ratio
                ),
                DockPlacement::Right
            );
            let _ = drag::end(&hits);
        });
    }
    #[test]
    fn forward_reorder_commits_the_same_slot_as_the_drag_preview() {
        let (_owner, ()) = create_root(|| {
            let dock = DockState::new(
                "reorder-forward",
                DockLayout::tabs("editor", [tab("a"), tab("b"), tab("c"), tab("d")]),
            );
            assert!(dock.move_tab("editor", "a", "editor", 1, DockPlacement::Center));
            assert_eq!(
                dock.data
                    .peek()
                    .layout
                    .group("editor")
                    .unwrap()
                    .iter()
                    .map(|t| t.key.as_str())
                    .collect::<Vec<_>>(),
                ["b", "a", "c", "d"]
            );
            assert!(dock.move_tab("editor", "b", "editor", 3, DockPlacement::Center));
            assert_eq!(
                dock.data
                    .peek()
                    .layout
                    .group("editor")
                    .unwrap()
                    .iter()
                    .map(|t| t.key.as_str())
                    .collect::<Vec<_>>(),
                ["a", "c", "d", "b"]
            );
        });
    }
    #[test]
    fn reordering_and_closing_last_tab_keep_a_restorable_group() {
        let (_owner, ()) = create_root(|| {
            let dock = DockState::new(
                "reorder",
                DockLayout::tabs("editor", [tab("a"), tab("b"), tab("c")]),
            );
            assert!(dock.move_tab("editor", "a", "editor", 3, DockPlacement::Center));
            assert_eq!(
                dock.data
                    .peek()
                    .layout
                    .group("editor")
                    .unwrap()
                    .iter()
                    .map(|t| t.key.as_str())
                    .collect::<Vec<_>>(),
                ["b", "c", "a"]
            );
            for key in ["a", "b", "c"] {
                dock.close("editor", key);
            }
            dock.restore_closed();
            assert_eq!(dock.data.peek().layout.group("editor").unwrap()[0].key, "c");
            assert!(!dock.move_tab("editor", "c", "editor", 0, DockPlacement::Left));
        });
    }
}

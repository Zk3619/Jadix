//! Portable retained menus: shared navigation and popovers, with no host-specific widget.
use super::{button, popover, popover_content, use_popover, Element, HostedMenuItem, PopoverContentOpts, Signal};
pub use jadix::portable::hosted::{MenuAction, MenuNavigation};
use jadix::portable::{core::reactivity::component_root, use_scoped_signal};
use jadix::presentation::{accessibility::{CheckedState, SemanticRole}, interaction::{InteractionKind, InteractionPhase}, keyboard::KeyPhase, NavigationKey};
use std::{cell::{Cell, RefCell}, rc::Rc};

type Selection = Rc<RefCell<Box<dyn FnMut(&str)>>>;

/// The supplied child remains the trigger; its existing handlers are retained.
pub fn dropdown_menu(id: impl Into<String>, items: Vec<HostedMenuItem>) -> MenuBuilder {
    MenuBuilder { id: id.into(), items, context: false, disabled: false, open: None, selection: None }
}

/// Long press and ContextMenu / Shift+F10 open the same retained menu.
pub fn context_menu(id: impl Into<String>, items: Vec<HostedMenuItem>) -> MenuBuilder {
    MenuBuilder { context: true, ..dropdown_menu(id, items) }
}

pub struct MenuBuilder {
    id: String,
    items: Vec<HostedMenuItem>,
    context: bool,
    disabled: bool,
    open: Option<Signal<bool>>,
    selection: Option<Selection>,
}

#[derive(Clone)]
struct Session {
    id: String,
    state: Signal<MenuNavigation>,
    open: Signal<bool>,
    restore: Signal<bool>,
    selection: Option<Selection>,
    focus: Rc<RefCell<Vec<(String, Signal<bool>)>>>,
    visible_rows: Rc<Cell<usize>>,
}

impl Session {
    fn show(&self) {
        let mut state = self.state.get(); state.reset(); self.state.set(state);
        for (id, requested) in self.focus.borrow().iter() {
            requested.set(self.state.get().highlighted() == Some(id.as_str()));
        }
        self.open.set(true);
    }
    fn apply(&self, action: MenuAction) {
        match action {
            MenuAction::Selected(id) => {
                self.open.set(false); self.restore.set(true);
                if let Some(callback) = &self.selection { (callback.borrow_mut())(&id); }
            }
            MenuAction::Close => { self.open.set(false); self.restore.set(true); }
            _ => {}
        }
    }
    fn activate(&self, id: &str) {
        let mut state = self.state.get();
        let action = state.activate(id); self.state.set(state); self.apply(action);
    }
    fn row_id(&self, item: &str) -> String { format!("{}__menu_item_{}", self.id, item) }
}

impl MenuBuilder {
    pub fn disabled(mut self, disabled: bool) -> Self { self.disabled = disabled; self }
    pub fn open_signal(mut self, open: Signal<bool>) -> Self { self.open = Some(open); self }
    pub fn on_select(mut self, callback: impl FnMut(&str) + 'static) -> Self {
        self.selection = Some(Rc::new(RefCell::new(Box::new(callback)))); self
    }
    pub fn child(self, child: impl Into<Element>) -> Element {
        let child = child.into();
        component_root(|| {
            let state = match MenuNavigation::new(self.items) {
                Ok(state) => state,
                Err(error) => {
                    let mut element = Element::new().id(self.id).child(child);
                    element.set_attr("jadix_native_unsupported", error); return element;
                }
            };
            let open = self.open.unwrap_or_else(|| use_scoped_signal(&format!("{}__menu_open", self.id), || false));
            let state = use_scoped_signal(&format!("{}__menu_navigation", self.id), || state);
            let restore = child.focus_requested.unwrap_or_else(|| use_scoped_signal(&format!("{}__menu_restore", self.id), || false));
            let session = Session { id: self.id.clone(), state, open, restore, selection: self.selection, focus: Default::default(), visible_rows: Rc::new(Cell::new(8)) };
            popover(self.id.clone()).open(open).child(|| {
                let context = use_popover().unwrap();
                let mut trigger = child;
                let trigger_id = trigger.template_id.clone().unwrap_or_else(|| format!("{}__menu_trigger", self.id));
                trigger.template_id = Some(trigger_id.clone());
                context.trigger_fid.set(trigger_id);
                trigger.focus_requested = Some(restore);
                trigger.descriptor.keyboard.focusable = !self.disabled;
                trigger.descriptor.keyboard.key_down = !self.disabled;
                let disabled = self.disabled;
                let s = session.clone();
                trigger = trigger.and_on_key(move |event| {
                    let m = event.modifiers;
                    if !disabled && event.phase == KeyPhase::Down && !event.repeat && !m.alt && !m.control && !m.meta
                        && ((event.key == "ContextMenu" && !m.shift) || (event.key == "F10" && m.shift)) { s.show(); }
                });
                if self.context {
                    trigger.descriptor.interaction.long_press_millis = (!disabled).then_some(500);
                    trigger.descriptor.interaction.context_menu = !disabled;
                    let s = session.clone();
                    trigger = trigger.and_on_interaction(move |event| {
                        if !disabled && event.phase == InteractionPhase::Started
                            && matches!(event.kind, InteractionKind::LongPress | InteractionKind::ContextMenu) { s.show(); }
                    });
                } else {
                    let previous = trigger.on_click.take();
                    let s = session.clone();
                    trigger = trigger.on_click(move || {
                        if let Some(previous) = &previous { (previous.borrow_mut())(); }
                        if !disabled { if s.open.get() { s.apply(MenuAction::Close); } else { s.show(); } }
                    });
                    if !matches!(trigger.descriptor.content, jadix::presentation::NodeContent::Button { .. }) {
                        trigger.descriptor.interaction.press = !disabled;
                        let click = trigger.on_click.clone();
                        trigger = trigger.and_on_interaction(move |event| {
                            if !disabled && event.kind == InteractionKind::Press && event.phase == InteractionPhase::Ended {
                                if let Some(click) = &click { (click.borrow_mut())(); }
                            }
                        });
                        let click = trigger.on_click.clone();
                        trigger = trigger.and_on_key(move |event| {
                            if !disabled && event.phase == KeyPhase::Down && !event.repeat
                                && matches!(event.key.as_str(), "Enter" | " ")
                                && !event.modifiers.alt && !event.modifiers.control && !event.modifiers.meta {
                                if let Some(click) = &click { (click.borrow_mut())(); }
                            }
                        });
                    }
                }
                let s = session.clone();
                let mut content = popover_content(PopoverContentOpts { align: "start".into(), ..Default::default() }, move || {
                    let s = s.clone();
                    let key = s.clone(); let render = s.clone();
                    let page = s.visible_rows.clone();
                    let mut viewport = jadix::portable::primitives::scroll().menu_scroll().on_viewport(move |_, height| {
                        page.set((height / 40.0).floor().max(1.0) as usize);
                    });
                    viewport.descriptor.style.max_height = Some(320.0);
                    viewport.child(Element::new().flex_col().flex_shrink(0.0).bind_children(
                        move || key.state.get().path().to_vec(),
                        move |_| menu_rows(&render),
                    ))
                });
                content.descriptor.accessibility.role = Some(SemanticRole::Menu);
                content.focus_restore_target = Some(context.trigger_fid.get());
                Element::new().id(self.id).child(trigger).child(content)
            })
        })
    }
}

fn menu_rows(session: &Session) -> Vec<Element> {
    session.focus.borrow_mut().clear();
    let state = session.state.get();
    let mut rows = Vec::new();
    if !state.path().is_empty() {
        let s = session.clone();
        rows.push(button(&format!("{}__menu_back", session.id)).text("‹").on_click(move || {
            let mut state = s.state.get(); let action = state.back(); s.state.set(state); s.apply(action);
        }).into());
    }
    for entry in state.entries() {
        let item = entry.item;
        if item.inline {
            rows.push(Element::new().text(&item.label).text_size(12.0)); continue;
        }
        let id = item.id.clone();
        let s = session.clone(); let action_id = id.clone();
        let caption = format!("{}{}{}{}", if item.checked { "✓ " } else { "" }, item.label,
            item.accelerator.as_ref().map(|v| format!("    {v}")).unwrap_or_default(),
            if item.children.is_empty() { "" } else { "    ›" });
        let mut row: Element = button(&s.row_id(&id)).text(caption).disabled(!entry.enabled)
            .on_click(move || s.activate(&action_id)).into();
        row = row.h(40.0).flex_shrink(0.0);
        row.descriptor.accessible_name = item.accessibility_label.clone().or_else(|| Some(item.label.clone()));
        row.descriptor.accessibility.role = Some(if item.checked { SemanticRole::Menuitemcheckbox } else { SemanticRole::Menuitem });
        row.descriptor.accessibility.checked = item.checked.then_some(CheckedState::True);
        row.descriptor.keyboard.focusable = entry.enabled;
        row.descriptor.keyboard.key_down = entry.enabled;
        row.descriptor.navigation_keys = vec![NavigationKey::Up, NavigationKey::Down, NavigationKey::Home, NavigationKey::End,
            NavigationKey::Left, NavigationKey::Right, NavigationKey::Enter, NavigationKey::Escape, NavigationKey::Tab,
            NavigationKey::PageUp, NavigationKey::PageDown];
        let requested = use_scoped_signal(&format!("{}__focus", session.row_id(&id)), || state.highlighted() == Some(id.as_str()));
        requested.set(state.highlighted() == Some(id.as_str()));
        row.focus_requested = Some(requested);
        session.focus.borrow_mut().push((id.clone(), requested));
        let s = session.clone(); let row_id = id.clone();
        row.on_navigation = Some(Rc::new(RefCell::new(Box::new(move |key| {
            let mut state = s.state.get(); state.highlight(&row_id);
            let action = state.navigate_page(key, s.visible_rows.get());
            let next = state.highlighted().map(|id| s.row_id(id));
            s.state.set(state); s.apply(action);
            s.open.get().then_some(next).flatten()
        }))));
        let s = session.clone();
        row = row.on_key(move |event| {
            let m = event.modifiers;
            if event.phase != KeyPhase::Down || m.control || m.alt || m.meta || event.key.chars().count() != 1 { return; }
            let mut state = s.state.get();
            if state.typeahead_input(&event.key) {
                for (id, requested) in s.focus.borrow().iter() { requested.set(state.highlighted() == Some(id.as_str())); }
                s.state.set(state);
            }
        });
        if item.destructive { row = row.text_color([190, 30, 45, 255]); }
        rows.push(row);
    }
    rows
}

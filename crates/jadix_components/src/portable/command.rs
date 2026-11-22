//! Command palette backed by the same live registry as menus and shortcuts.
use super::{button, input, Element, Signal};
use jadix::portable::{commands::CommandRegistry, core::reactivity::component_root, use_scoped_signal};
use jadix::presentation::{NavigationKey, accessibility::{LiveRegion, SemanticRole}};
use std::{cell::RefCell, rc::Rc};

pub struct CommandPaletteBuilder {
    id: String,
    registry: CommandRegistry,
    open: Option<Signal<bool>>,
    query: Option<Signal<String>>,
    error: Option<Signal<String>>,
}
pub fn command_palette(id: impl Into<String>, registry: CommandRegistry) -> CommandPaletteBuilder {
    CommandPaletteBuilder { id: id.into(), registry, open: None, query: None, error: None }
}
impl CommandPaletteBuilder {
    pub fn open_signal(mut self, open: Signal<bool>) -> Self { self.open = Some(open); self }
    pub fn query_signal(mut self, query: Signal<String>) -> Self { self.query = Some(query); self }
    pub fn error_signal(mut self, error: Signal<String>) -> Self { self.error = Some(error); self }
    pub fn build(self) -> Element {
        component_root(|| {
            let Self { id, registry, open, query, error } = self;
            let open = open.unwrap_or_else(|| use_scoped_signal(&format!("{id}__open"), || true));
            let query = query.unwrap_or_else(|| use_scoped_signal(&format!("{id}__query"), String::new));
            let error = error.unwrap_or_else(|| use_scoped_signal(&format!("{id}__error"), String::new));
            let selected = use_scoped_signal(&format!("{id}__selected"), String::new);
            let invoke = {
                let registry = registry.clone();
                Rc::new(move |key: &str| {
                    match registry.invoke(key, jadix::portable::schema::Value::Null) {
                        Ok(true) => { error.set(String::new()); open.set(false); }
                        Ok(false) => {}
                        Err(message) => error.set(message),
                    }
                })
            };
            let mut search: Element = input(&format!("{id}__search")).value_signal(query).max_length(1024).placeholder("Search commands").into();
            search.descriptor.accessibility.role = Some(SemanticRole::Combobox);
            let prefix = id.clone();
            search.accessibility_binding = Some(Rc::new(move || {
                let active = selected.get();
                ((!active.is_empty()).then(|| format!("{prefix}__command_{active}")), vec![format!("{prefix}__list")])
            }));
            let commands = registry.clone(); let execute = invoke.clone();
            search.on_submit = Some(Rc::new(RefCell::new(Box::new(move || {
                let entries = commands.search(&query.peek());
                let enabled: Vec<_> = entries.iter().filter(|entry| entry.accepts_null && entry.state.enabled).collect();
                if let Some(entry) = enabled.iter().find(|entry| entry.id == selected.peek()).or(enabled.first()) { execute(&entry.id); }
            }))));
            let commands = registry.clone(); let prefix = id.clone();
            search.descriptor.navigation_keys = vec![NavigationKey::Down, NavigationKey::Up, NavigationKey::Escape];
            search.on_navigation = Some(Rc::new(RefCell::new(Box::new(move |key| {
                if key == NavigationKey::Escape {
                    if query.peek().is_empty() { open.set(false); } else { query.set(String::new()); }
                    return None;
                }
                let entries: Vec<_> = commands.search(&query.peek()).into_iter().filter(|entry| entry.accepts_null && entry.state.enabled).collect();
                let entry = if key == NavigationKey::Up { entries.last() } else { entries.first() }?;
                selected.set(entry.id.clone()); Some(format!("{prefix}__command_{}", entry.id))
            }))));
            let render_id = id.clone();
            let rows = Element::new().flex_col().bind_children(
                move || registry.search(&query.get()).into_iter().filter(|entry| entry.accepts_null).collect::<Vec<_>>(),
                move |entries| {
                    let enabled = Rc::new(entries.iter().filter(|entry| entry.state.enabled).map(|entry| entry.id.clone()).collect::<Vec<_>>());
                    if !enabled.contains(&selected.peek()) { selected.set(enabled.first().cloned().unwrap_or_default()); }
                    if entries.is_empty() { return vec![Element::new().text("No commands")]; }
                    entries.iter().map(|entry| {
                        let key = entry.id.clone(); let action = invoke.clone(); let click = key.clone();
                        let mut row: Element = button(&format!("{render_id}__command_{key}")).text(&entry.label)
                            .disabled(!entry.state.enabled).on_click(move || action(&click)).into();
                        row.descriptor.accessibility.role = Some(SemanticRole::Option);
                        let highlight = key.clone();
                        row.descriptor_binding = Some(Rc::new(move |descriptor| descriptor.accessible_selected = Some(selected.get() == highlight)));
                        let choices = enabled.clone(); let prefix = render_id.clone(); let current = key.clone(); let execute = invoke.clone();
                        row.descriptor.navigation_keys = vec![NavigationKey::Up, NavigationKey::Down, NavigationKey::Home, NavigationKey::End, NavigationKey::Enter, NavigationKey::Escape];
                        row.on_navigation = Some(Rc::new(RefCell::new(Box::new(move |key| {
                            match key {
                                NavigationKey::Escape => {
                                    if query.peek().is_empty() { open.set(false); return None; }
                                    query.set(String::new()); return Some(format!("{prefix}__search"));
                                }
                                NavigationKey::Enter => { execute(&current); return None; }
                                _ => {}
                            }
                            let index = choices.iter().position(|id| *id == current).unwrap_or(0);
                            let next = match key {
                                NavigationKey::Up => jadix::portable::roving::roving_move(choices.len(), index, -1),
                                NavigationKey::Down => jadix::portable::roving::roving_move(choices.len(), index, 1),
                                NavigationKey::Home => jadix::portable::roving::roving_edge(choices.len(), false),
                                NavigationKey::End => jadix::portable::roving::roving_edge(choices.len(), true), _ => None,
                            }?;
                            selected.set(choices[next].clone()); Some(format!("{prefix}__command_{}", choices[next]))
                        }))));
                        row.h(40.).flex_shrink(0.)
                    }).collect()
                },
            );
            let mut list = jadix::portable::primitives::scroll().id(format!("{id}__list")).child(rows);
            list.descriptor.style.max_height = Some(320.);
            list.descriptor.accessibility.role = Some(SemanticRole::Listbox);
            let prefix = id.clone();
            list.accessibility_binding = Some(Rc::new(move || {
                let active = selected.get();
                ((!active.is_empty()).then(|| format!("{prefix}__command_{active}")), vec![])
            }));
            let mut status = Element::new();
            status.text_binding = Some(Rc::new(move || error.get()));
            status.descriptor.accessibility.role = Some(SemanticRole::Alert);
            status.descriptor.accessibility.live = LiveRegion::Polite;
            let mut root = Element::new().id(id).flex_col().child(search).child(list).child(status);
            root.visibility = Some(Rc::new(move || open.get()));
            root
        })
    }
}
impl From<CommandPaletteBuilder> for Element { fn from(builder: CommandPaletteBuilder) -> Self { builder.build() } }

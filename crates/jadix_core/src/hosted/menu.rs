//! Shared menu navigation. Hosts choose presentation, never selection semantics.
use super::{HostedContent, HostedMenuItem};
use crate::presentation::NavigationKey;
use std::rc::Rc;
use web_time::{Duration, Instant};

#[derive(Clone, Debug, PartialEq)]
pub struct MenuNavigation {
    items: Rc<[HostedMenuItem]>,
    path: Vec<String>,
    highlighted: Option<String>,
    search: String,
    search_at: Option<Instant>,
}

#[derive(Clone, Copy)]
pub struct MenuEntry<'a> {
    pub item: &'a HostedMenuItem,
    pub enabled: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MenuAction { None, Changed, Selected(String), Close }

impl MenuNavigation {
    pub fn new(items: Vec<HostedMenuItem>) -> Result<Self, &'static str> {
        HostedContent::MenuButton { label: String::new(), enabled: true, items: items.clone() }.validate()?;
        let mut menu = Self { items: items.into(), path: Vec::new(), highlighted: None, search: String::new(), search_at: None };
        menu.first(false);
        Ok(menu)
    }

    pub fn path(&self) -> &[String] { &self.path }
    pub fn highlighted(&self) -> Option<&str> { self.highlighted.as_deref() }

    /// Inline groups retain their labels; disabling a group disables descendants.
    pub fn entries(&self) -> Vec<MenuEntry<'_>> {
        fn flatten<'a>(items: &'a [HostedMenuItem], enabled: bool, out: &mut Vec<MenuEntry<'a>>) {
            for item in items {
                let enabled = enabled && item.enabled;
                out.push(MenuEntry { item, enabled: enabled && !item.inline });
                if item.inline { flatten(&item.children, enabled, out); }
            }
        }
        let mut items: &[HostedMenuItem] = &self.items;
        for id in &self.path {
            let mut level = Vec::new();
            flatten(items, true, &mut level);
            let Some(entry) = level.into_iter().find(|entry| entry.enabled && entry.item.id == *id) else { return Vec::new() };
            items = &entry.item.children;
        }
        let mut entries = Vec::new();
        flatten(items, true, &mut entries);
        entries
    }

    pub fn reset(&mut self) { self.path.clear(); self.clear_search(); self.first(false); }

    fn clear_search(&mut self) { self.search.clear(); self.search_at = None; }

    fn first(&mut self, last: bool) {
        let entries = self.entries();
        let mut enabled = entries.iter().filter(|entry| entry.enabled);
        self.highlighted = (if last { enabled.next_back() } else { enabled.next() })
            .map(|entry| entry.item.id.clone());
    }

    pub fn highlight(&mut self, id: &str) -> bool {
        if !self.entries().iter().any(|entry| entry.enabled && entry.item.id == id) { return false; }
        self.highlighted = Some(id.into()); true
    }

    pub fn activate(&mut self, id: &str) -> MenuAction {
        self.clear_search();
        let entries = self.entries();
        let Some(entry) = entries.iter().find(|entry| entry.enabled && entry.item.id == id) else { return MenuAction::None };
        if entry.item.children.is_empty() { return MenuAction::Selected(id.into()); }
        self.path.push(id.into()); self.first(false); MenuAction::Changed
    }

    pub fn back(&mut self) -> MenuAction {
        self.clear_search();
        if let Some(parent) = self.path.pop() {
            self.highlighted = Some(parent); MenuAction::Changed
        } else { MenuAction::Close }
    }

    pub fn navigate(&mut self, key: NavigationKey) -> MenuAction {
        self.navigate_page(key, 10)
    }

    /// A host may supply the number of visible rows; page movement never wraps.
    pub fn navigate_page(&mut self, key: NavigationKey, visible_rows: usize) -> MenuAction {
        self.clear_search();
        use NavigationKey::*;
        match key {
            Escape | Left => return self.back(),
            Tab => return MenuAction::Close,
            Right | Enter => {
                if let Some(id) = self.highlighted.clone() {
                    if key == Right && self.entries().iter().any(|entry| entry.item.id == id && entry.item.children.is_empty()) {
                        return MenuAction::None;
                    }
                    return self.activate(&id);
                }
            }
            Home => self.first(false), End => self.first(true),
            Up | Down | PageUp | PageDown => {
                let entries = self.entries();
                let enabled: Vec<_> = entries.iter().filter(|entry| entry.enabled).collect();
                if enabled.is_empty() { return MenuAction::None; }
                let current = enabled.iter().position(|entry| Some(entry.item.id.as_str()) == self.highlighted());
                let index = match (current, key) {
                    (Some(index), Up) => (index + enabled.len() - 1) % enabled.len(),
                    (Some(index), PageUp) => index.saturating_sub(visible_rows.max(1)),
                    (Some(index), PageDown) => index.saturating_add(visible_rows.max(1)).min(enabled.len() - 1),
                    (Some(index), _) => (index + 1) % enabled.len(),
                    (None, Up | PageUp) => enabled.len() - 1,
                    _ => 0,
                };
                self.highlighted = Some(enabled[index].item.id.clone());
            }
        }
        MenuAction::None
    }

    /// Search after the current entry, wrapping once. Caller owns timeout/input buffering.
    pub fn typeahead(&mut self, prefix: &str) -> bool {
        if prefix.is_empty() || prefix.len() > 256 || prefix.chars().any(char::is_control) { return false; }
        let prefix = prefix.to_lowercase();
        let entries = self.entries();
        if entries.is_empty() { return false; }
        let start = entries.iter().position(|entry| Some(entry.item.id.as_str()) == self.highlighted())
            .map_or(0, |index| index + 1);
        let found = (0..entries.len()).map(|offset| &entries[(start + offset) % entries.len()])
            .find(|entry| entry.enabled && entry.item.label.to_lowercase().starts_with(&prefix))
            .map(|entry| entry.item.id.clone());
        if let Some(id) = found { self.highlighted = Some(id); true } else { false }
    }

    /// Buffer printable input for 700 ms. Repeating one character cycles matches;
    /// extending a prefix retains the current row when its label still matches.
    pub fn typeahead_input(&mut self, input: &str) -> bool {
        self.typeahead_input_at(input, Instant::now())
    }

    pub fn typeahead_input_at(&mut self, input: &str, now: Instant) -> bool {
        if input.is_empty() || input.len() > 256 || input.chars().any(char::is_control) { return false; }
        if self.search_at.is_none_or(|last| now.saturating_duration_since(last) >= Duration::from_millis(700)) {
            self.search.clear();
        }
        let input = input.to_lowercase();
        let cycle = self.search == input && input.chars().count() == 1;
        let extending = !self.search.is_empty() && !cycle;
        if cycle || self.search.len() + input.len() > 256 { self.search.clear(); }
        self.search.push_str(&input);
        self.search_at = Some(now);
        if extending && self.entries().iter().any(|entry| entry.enabled
            && Some(entry.item.id.as_str()) == self.highlighted()
            && entry.item.label.to_lowercase().starts_with(&self.search)) { return true; }
        if self.typeahead(&self.search.clone()) { return true; }
        // An unmatched prefix starts a fresh search from the latest input.
        self.search = input;
        self.typeahead(&self.search.clone())
    }
}

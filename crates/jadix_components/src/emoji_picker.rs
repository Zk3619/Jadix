//! EmojiPicker composition shared by retained native and GPU controls.
use jadix::portable::{create_computed, create_memo, untrack, presentation::{AccessibleRole, NavigationKey}, use_scoped_signal, Signal};
use std::{cell::RefCell, rc::Rc};
use jadix::virtualization::ListScrollAlign;
use jadix::portable::presentation::style::ScrollBehavior;

pub use jadix::portable::emoji::{catalog, EmojiQuery, EmojiAnnotation, EmojiGrid, EmojiMatch, EmojiPage, EmojiRow, EmojiSearch};
pub const GROUPS: [&str; 11] = ["Smileys & emotion", "People & body", "Components", "Animals & nature", "Food & drink", "Travel & places", "Activities", "Objects", "Symbols", "Flags", "Recently used"];
pub const TONES: [&str; 6] = ["Default skin tone", "Light skin tone", "Medium-light skin tone", "Medium skin tone", "Medium-dark skin tone", "Dark skin tone"];
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EmojiGroup {
    pub id: u8,
    pub label: String,
    pub first: bool,
}

type Callback<T> = Rc<RefCell<Box<dyn FnMut(T)>>>;
type Navigation = Rc<RefCell<Box<dyn FnMut(NavigationKey)>>>;

#[derive(Clone, Default)]
pub struct EmojiPickerController(Rc<RefCell<Option<PickerState>>>);
#[derive(Clone)]
struct PickerState {
    disabled: bool,
    grid: Rc<RefCell<Rc<EmojiGrid>>>,
    query: Rc<dyn Fn() -> EmojiQuery>,
    highlight: Signal<Option<usize>>,
    reveal: Signal<Option<(usize, ListScrollAlign, ScrollBehavior)>>,
    revision: Signal<usize>,
    focus: Signal<bool>,
    on_highlight: Option<Callback<Option<EmojiMatch>>>,
    navigation: Navigation,
}
impl EmojiPickerController {
    pub fn focus(&self) { if let Some(state) = self.0.borrow().as_ref().filter(|s| !s.disabled) { state.focus.set(true); } }
    pub fn matched_count(&self) -> usize { self.0.borrow().as_ref().map_or(0, |s| s.grid.borrow().items.len()) }
    pub fn entry(&self, text: &str) -> Option<EmojiMatch> { self.0.borrow().as_ref().and_then(|s| (s.query)().entry(text).ok().flatten()) }
    pub fn navigate(&self, key: NavigationKey) {
        let state = self.0.borrow().clone();
        if let Some(state) = state { (state.navigation.borrow_mut())(key); }
    }
    pub fn highlight(&self, text: &str) -> bool {
        let Some(state) = self.0.borrow().clone() else { return false; };
        if state.disabled { return false; }
        let grid = state.grid.borrow().clone();
        let Some(index) = grid.index_of(text) else { return false; };
        state.revision.set(state.revision.get().wrapping_add(1));
        state.reveal.set(grid.row_for(index).map(|row| (row, ListScrollAlign::Nearest, ScrollBehavior::Smooth)));
        if state.highlight.get() != Some(index) {
            state.highlight.set(Some(index));
            if let Some(callback) = state.on_highlight { (callback.borrow_mut())(Some(grid.items[index].clone())); }
        }
        true
    }
    pub fn scroll_to_group(&self, group: u8) -> bool { self.scroll_to_group_with_animation(group, true) }
    /// Reveal a section using the shared list's immediate or animated scrolling.
    pub fn scroll_to_group_with_animation(&self, group: u8, animated: bool) -> bool {
        let Some(state) = self.0.borrow().clone() else { return false; };
        if state.disabled { return false; }
        let row = state.grid.borrow().section_row(group);
        if let Some(row) = row { state.revision.set(state.revision.get().wrapping_add(1)); state.reveal.set(Some((row, ListScrollAlign::Start, if animated { ScrollBehavior::Smooth } else { ScrollBehavior::Instant }))); true } else { false }
    }
}

/// Search fields always retain Home/End for caret movement. Empty fields can
/// delegate horizontal arrows to the grid; a panel without a search field owns
/// every navigation key.
pub fn navigation_keys(query: &str) -> Vec<NavigationKey> {
    use NavigationKey::*;
    let mut keys = vec![Up, Down, PageUp, PageDown, Enter, Escape];
    if query.is_empty() { keys.extend([Left, Right]); }
    keys
}

pub fn panel_navigation_keys() -> Vec<NavigationKey> {
    let mut keys = navigation_keys("");
    keys.extend([NavigationKey::Home, NavigationKey::End]);
    keys
}


/// Backend adapters use existing controls and virtualization; filtering,
/// section layout, navigation and selection remain shared.
#[doc(hidden)]
pub trait PickerRenderer: 'static {
    type Element: 'static;
    type ListState: Default + Clone + 'static;
    fn container(id: &str, row: bool, children: Vec<Self::Element>) -> Self::Element;
    fn flexible(element: Self::Element) -> Self::Element;
    fn placeholder(item_size: f32) -> Self::Element;
    fn semantics(element: Self::Element, role: Option<AccessibleRole>, label: &str) -> Self::Element;
    fn input(id: &str, value: Signal<String>, placeholder: &str, disabled: bool,
        navigation: Box<dyn FnMut(NavigationKey)>) -> Self::Element;
    fn navigation_surface(id: &str, disabled: bool, navigation: Box<dyn FnMut(NavigationKey)>) -> Self::Element;
    fn focus_target(element: Self::Element, id: &str, request: Signal<bool>, label: &str, grid: Rc<RefCell<Rc<EmojiGrid>>>, highlight: Signal<Option<usize>>) -> Self::Element;
    fn select(id: &str, value: Signal<String>, options: Vec<(String, String)>, disabled: bool) -> Self::Element;
    fn button(id: &str, emoji: &EmojiMatch, item_size: f32, disabled: bool, selected: Box<dyn Fn() -> bool>, child: Option<Self::Element>, pointer_move: Box<dyn FnMut([f64; 2])>, action: Box<dyn FnMut()>) -> Self::Element;
    fn text(text: String) -> Self::Element;
    fn virtual_rows(state: Self::ListState, id: &str, count: usize, row_height: f32, height: Option<f32>, visible: Box<dyn Fn(std::ops::Range<usize>)>, focus: Box<dyn Fn() -> Option<(usize, usize, ListScrollAlign, ScrollBehavior)>>, render: Box<dyn Fn(usize) -> Self::Element>) -> Self::Element;
    fn region(id: &str, key: Box<dyn Fn() -> EmojiQuery>, render: Box<dyn FnMut(EmojiQuery) -> Self::Element>) -> Self::Element;
}

/// Applies the same part customization contract to native and GPU elements.
/// Each hook receives the composed control, including its events and semantics.
pub struct EmojiPickerParts<E> {
    pub search_bar: Option<Rc<dyn Fn(E) -> E>>,
    pub search: Option<Rc<dyn Fn(E) -> E>>,
    pub list: Option<Rc<dyn Fn(E) -> E>>,
    pub row: Option<Rc<dyn Fn(E) -> E>>,
    pub item: Option<Rc<dyn Fn(E) -> E>>,
    pub group: Option<Rc<dyn Fn(E) -> E>>,
}
impl<E> Default for EmojiPickerParts<E> {
    fn default() -> Self { Self { search_bar: None, search: None, list: None, row: None, item: None, group: None } }
}
fn customize<E>(hook: &Option<Rc<dyn Fn(E) -> E>>, element: E) -> E {
    match hook { Some(hook) => hook(element), None => element }
}

pub struct EmojiPickerBuilder<R: PickerRenderer> {
    id: String,
    query: EmojiQuery,
    annotations: Option<Rc<dyn Fn() -> Vec<EmojiAnnotation>>>,
    groups: Option<Rc<dyn Fn() -> Option<Vec<u8>>>>,
    recent: Option<Rc<dyn Fn() -> Vec<String>>>,
    emoji_version: Option<Rc<dyn Fn() -> Option<String>>>,
    disabled: bool,
    searchable: bool,
    show_filters: bool,
    placeholder: String,
    columns: usize,
    row_height: f32,
    item_size: f32,
    height: Option<f32>,
    rtl: bool,
    value: Option<Signal<String>>,
    query_signal: Option<Signal<String>>,
    query_source: Option<Rc<dyn Fn() -> String>>,
    skin_tone_signal: Option<Signal<Option<u8>>>,
    on_select: Option<Callback<String>>,
    on_select_entry: Option<Callback<EmojiMatch>>,
    on_highlight: Option<Callback<Option<EmojiMatch>>>,
    on_cancel: Option<Callback<()>>,
    group_labels: Vec<String>,
    render_item: Option<Rc<dyn Fn(&EmojiMatch) -> R::Element>>,
    render_group: Option<Rc<dyn Fn(EmojiGroup) -> R::Element>>,
    header: Option<R::Element>,
    footer: Option<R::Element>,
    empty: Option<Rc<dyn Fn() -> R::Element>>,
    controller: Option<EmojiPickerController>,
    auto_focus: bool,
    accessibility_label: String,
    search_accessibility_label: Option<String>,
    search_prefix: Option<R::Element>,
    search_suffix: Option<R::Element>,
    on_query: Option<Callback<String>>,
    on_group: Option<Callback<Option<u8>>>,
    tone_labels: Vec<String>,
    parts: EmojiPickerParts<R::Element>,
}
impl<R: PickerRenderer> EmojiPickerBuilder<R> {
    pub fn new(id: impl Into<String>) -> Self {
        Self { id: id.into(), query: EmojiQuery::default(), annotations: None, groups: None, recent: None, emoji_version: None, disabled: false, searchable: true, show_filters: false,
            placeholder: "Search emoji".into(), columns: 8, row_height: 40., item_size: 36., height: None, rtl: false, value: None, query_signal: None, query_source: None, skin_tone_signal: None,
            on_select: None, on_select_entry: None, on_highlight: None, on_cancel: None,
            group_labels: GROUPS.iter().map(|s| (*s).into()).collect(), render_item: None,
            render_group: None, header: None, footer: None, empty: None,
            controller: None, auto_focus: false, accessibility_label: "Emoji".into(), search_accessibility_label: None,
            search_prefix: None, search_suffix: None, on_query: None, on_group: None,
            tone_labels: TONES.iter().map(|s| (*s).into()).collect(), parts: EmojiPickerParts::default() }
    }
    pub fn parts(mut self, parts: EmojiPickerParts<R::Element>) -> Self { self.parts = parts; self }
    pub fn query(mut self, query: EmojiQuery) -> Self { self.query = query; self }
    /// Reactive catalog metadata is resolved by the shared model on every change.
    pub fn annotations_fn(mut self, value: impl Fn() -> Vec<EmojiAnnotation> + 'static) -> Self { self.annotations = Some(Rc::new(value)); self }
    pub fn groups_fn(mut self, value: impl Fn() -> Option<Vec<u8>> + 'static) -> Self { self.groups = Some(Rc::new(value)); self }
    pub fn recent_fn(mut self, value: impl Fn() -> Vec<String> + 'static) -> Self { self.recent = Some(Rc::new(value)); self }
    pub fn emoji_version_fn(mut self, value: impl Fn() -> Option<String> + 'static) -> Self { self.emoji_version = Some(Rc::new(value)); self }
    pub fn controller(mut self, controller: EmojiPickerController) -> Self { self.controller = Some(controller); self }
    pub fn auto_focus(mut self, value: bool) -> Self { self.auto_focus = value; self }
    pub fn accessibility_label(mut self, label: impl Into<String>) -> Self { self.accessibility_label = label.into(); self }
    pub fn search_accessibility_label(mut self, label: impl Into<String>) -> Self { self.search_accessibility_label = Some(label.into()); self }
    pub fn search_prefix(mut self, element: R::Element) -> Self { self.search_prefix = Some(element); self }
    pub fn search_suffix(mut self, element: R::Element) -> Self { self.search_suffix = Some(element); self }
    pub fn on_query_change(mut self, callback: impl FnMut(String) + 'static) -> Self { self.on_query = Some(Rc::new(RefCell::new(Box::new(callback)))); self }
    pub fn on_group_change(mut self, callback: impl FnMut(Option<u8>) + 'static) -> Self { self.on_group = Some(Rc::new(RefCell::new(Box::new(callback)))); self }
    pub fn tone_labels(mut self, labels: Vec<String>) -> Self { assert_eq!(labels.len(), TONES.len()); self.tone_labels = labels; self }
    pub fn query_signal(mut self, query: Signal<String>) -> Self { self.query_signal = Some(query); self }
    /// Authoritative query owned by the caller. Edits notify on_query_change;
    /// results and the field change only when the caller accepts that proposal.
    pub fn query_fn(mut self, query: impl Fn() -> String + 'static) -> Self { self.query_source = Some(Rc::new(query)); self }
    pub fn default_query(mut self, query: impl Into<String>) -> Self { self.query.query = query.into(); self }
    /// Two-way skin tone state: None is the default, Some(1..=5) selects a variant.
    pub fn skin_tone_signal(mut self, tone: Signal<Option<u8>>) -> Self { self.skin_tone_signal = Some(tone); self }
    pub fn disabled(mut self, disabled: bool) -> Self { self.disabled = disabled; self }
    pub fn show_filters(mut self, show: bool) -> Self { self.show_filters = show; self }
    pub fn searchable(mut self, searchable: bool) -> Self { self.searchable = searchable; self }
    pub fn placeholder(mut self, text: impl Into<String>) -> Self { self.placeholder = text.into(); self }
    pub fn columns(mut self, columns: usize) -> Self { assert!((1..=64).contains(&columns)); self.columns = columns; self }
    /// Logical row extent used by virtualization and keyboard reveal on every backend.
    pub fn row_height(mut self, height: f32) -> Self { assert!(height.is_finite() && height > 0.); self.row_height = height; self }
    /// Option height; cells divide the available row width equally.
    pub fn item_size(mut self, size: f32) -> Self { assert!(size.is_finite() && size > 0.); self.item_size = size; self }
    /// Override the list viewport; by default it fills the picker's remaining height.
    pub fn height(mut self, height: f32) -> Self { assert!(height.is_finite() && height > 0.); self.height = Some(height); self }
    pub fn rtl(mut self, rtl: bool) -> Self { self.rtl = rtl; self }
    pub fn value_signal(mut self, value: Signal<String>) -> Self { self.value = Some(value); self }
    pub fn on_select(mut self, callback: impl FnMut(String) + 'static) -> Self { self.on_select = Some(Rc::new(RefCell::new(Box::new(callback)))); self }
    pub fn on_select_entry(mut self, callback: impl FnMut(EmojiMatch) + 'static) -> Self { self.on_select_entry = Some(Rc::new(RefCell::new(Box::new(callback)))); self }
    pub fn on_highlight_change(mut self, callback: impl FnMut(Option<EmojiMatch>) + 'static) -> Self { self.on_highlight = Some(Rc::new(RefCell::new(Box::new(callback)))); self }
    pub fn on_cancel(mut self, mut callback: impl FnMut() + 'static) -> Self { self.on_cancel = Some(Rc::new(RefCell::new(Box::new(move |()| callback())))); self }
    pub fn group_labels(mut self, mut labels: Vec<String>) -> Self { assert!(labels.len() <= GROUPS.len()); labels.resize(GROUPS.len(), String::new()); self.group_labels = labels; self }
    pub fn render_item(mut self, render: impl Fn(&EmojiMatch) -> R::Element + 'static) -> Self { self.render_item = Some(Rc::new(render)); self }
    pub fn render_group(mut self, render: impl Fn(EmojiGroup) -> R::Element + 'static) -> Self { self.render_group = Some(Rc::new(render)); self }
    pub fn header(mut self, element: impl Into<R::Element>) -> Self { self.header = Some(element.into()); self }
    pub fn footer(mut self, element: impl Into<R::Element>) -> Self { self.footer = Some(element.into()); self }
    pub fn empty(mut self, render: impl Fn() -> R::Element + 'static) -> Self { self.empty = Some(Rc::new(render)); self }
    pub fn build(self) -> R::Element {
        self.query.validate().expect("invalid EmojiPicker query");
        assert!(self.item_size <= self.row_height, "EmojiPicker item size must fit its row height");
        assert!(!self.id.is_empty(), "EmojiPicker requires a stable instance id");
        let id = self.id;
        assert!(self.query_source.is_none() || self.query_signal.is_none(), "query_fn and query_signal are mutually exclusive");
        let controlled = self.query_source.is_some();
        let query_source = self.query_source;
        let initial_query = query_source.as_ref().map_or_else(|| self.query.query.clone(), |source| source());
        let query = self.query_signal.unwrap_or_else(|| use_scoped_signal(&format!("{id}__query"), || initial_query));
        if let Some(source) = query_source.clone() {
            let external = source.clone();
            create_computed(move || {
                let next = external();
                untrack(|| { if query.get() != next { query.set(next); } });
            });
            let on_query = self.on_query.clone();
            create_computed(move || {
                let proposal = query.get();
                untrack(|| {
                    if proposal != source() {
                        if let Some(callback) = &on_query { (callback.borrow_mut())(proposal); }
                        // A parent can accept, transform or reject synchronously.
                        let accepted = source();
                        if query.get() != accepted { query.set(accepted); }
                    }
                });
            });
        }
        let group = use_scoped_signal(&format!("{id}__group"), || self.query.group.map_or("all".into(), |g| g.to_string()));
        let tone = use_scoped_signal(&format!("{id}__tone"), || self.query.tone.map_or("default".into(), |t| t.to_string()));
        if let Some(value) = self.skin_tone_signal {
            create_computed(move || {
                let next = value.get().map_or("default".into(), |tone| tone.to_string());
                untrack(|| { if tone.get() != next { tone.set(next); } });
            });
            create_computed(move || {
                let next = tone.get().parse().ok();
                untrack(|| { if value.get() != next { value.set(next); } });
            });
        }
        let highlight = use_scoped_signal(&format!("{id}__highlight"), || None::<usize>);
        let reveal = use_scoped_signal(&format!("{id}__reveal"), || None::<(usize, ListScrollAlign, ScrollBehavior)>);
        let revision = use_scoped_signal(&format!("{id}__navigation_revision"), || 0usize);
        let focus = use_scoped_signal(&format!("{id}__focus"), || self.auto_focus && !self.disabled);
        let grid = Rc::new(RefCell::new(Rc::new(EmojiGrid::new(Vec::new(), self.columns))));
        let disabled = self.disabled;
        let selected = self.value;
        let on_select = self.on_select;
        let on_select_entry = self.on_select_entry;
        let choose: Rc<dyn Fn(&EmojiMatch)> = Rc::new(move |entry| {
            if disabled { return; }
            if let Some(value) = selected { value.set(entry.text.clone()); }
            if let Some(callback) = &on_select { (callback.borrow_mut())(entry.text.clone()); }
            if let Some(callback) = &on_select_entry { (callback.borrow_mut())(entry.clone()); }
        });
        let page_rows = Rc::new(std::cell::Cell::new((self.height.unwrap_or(self.row_height) / self.row_height).ceil() as usize));
        let navigation_page_rows = page_rows.clone();
        let nav_grid = grid.clone();
        let nav_choose = choose.clone();
        let on_highlight = self.on_highlight.clone();
        let on_cancel = self.on_cancel;
        let navigation: Navigation = Rc::new(RefCell::new(Box::new(move |key| {
            if disabled { return; }
            revision.set(revision.get().wrapping_add(1));
            reveal.set(None);
            if key == NavigationKey::Escape {
                if !query.get().is_empty() { query.set(String::new()); }
                else if let Some(callback) = &on_cancel { (callback.borrow_mut())(()); }
                return;
            }
            let grid = nav_grid.borrow().clone();
            if key == NavigationKey::Enter {
                if let Some(entry) = highlight.get().and_then(|i| grid.items.get(i)) { nav_choose(entry); }
                return;
            }
            let next = grid.navigate(highlight.get(), key, self.rtl, navigation_page_rows.get());
            reveal.set(next.and_then(|index| grid.row_for(index)).map(|row| (row, ListScrollAlign::Nearest, ScrollBehavior::Smooth)));
            if next != highlight.get() {
                highlight.set(next);
                if let Some(callback) = &on_highlight { (callback.borrow_mut())(next.and_then(|i| grid.items.get(i).cloned())); }
            }
        })));
        let configuration = self.query.clone();
        let request: Rc<dyn Fn() -> EmojiQuery> = Rc::new(move || {
            let mut request = EmojiQuery {
                query: query_source.as_ref().map_or_else(|| query.get(), |source| source()), group: group.get().parse().ok(), tone: tone.get().parse().ok(), offset: 0, limit: 128, ..configuration.clone()
            };
            if let Some(value) = &self.annotations { request.annotations = value(); }
            if let Some(value) = &self.groups { request.groups = value(); }
            if let Some(value) = &self.recent { request.recent = value(); }
            if let Some(value) = &self.emoji_version { request.emoji_version = value(); }
            request
        });
        // Update the shared model before any adapter projects controls. Search
        // fields precede results in the tree and must announce the current page.
        let model_request = request.clone();
        let current_grid = grid.clone();
        let on_query = self.on_query.clone();
        let on_highlight = self.on_highlight.clone();
        let columns = self.columns;
        let headings: Vec<_> = self.group_labels.iter().enumerate().filter_map(|(id, label)|
            (self.render_group.is_some() || !label.is_empty()).then_some(id as u8)).collect();
        let mut search = EmojiSearch::default();
        let mut previous_query = None::<String>;
        create_computed(move || {
            let request = model_request();
            untrack(|| {
                let had_query = previous_query.is_some();
                let query_changed = previous_query.as_ref() != Some(&request.query);
                if query_changed {
                    if previous_query.is_some() && !controlled {
                        if let Some(callback) = &on_query { (callback.borrow_mut())(request.query.clone()); }
                    }
                    previous_query = Some(request.query.clone());
                }
                let items = if search.search(&request).is_ok() { search.matches().cloned().collect() } else { Vec::new() };
                let model = Rc::new(EmojiGrid::with_headers(items, columns, &request.query, |group| headings.contains(&group)));
                let previous = highlight.get().and_then(|i| current_grid.borrow().items.get(i).cloned());
                let next = if disabled { None } else if query_changed {
                    (!request.query.trim().is_empty() && !model.items.is_empty()).then_some(0)
                } else {
                    previous.as_ref().and_then(|entry| model.index_of(&entry.text).or_else(|| model.index_of(&entry.base)))
                };
                let entry = next.and_then(|i| model.items.get(i).cloned());
                let destination = next.and_then(|index| model.row_for(index));
                *current_grid.borrow_mut() = model;
                highlight.set(next);
                revision.set(revision.get().wrapping_add(1));
                reveal.set(if query_changed && had_query { (!current_grid.borrow().rows.is_empty()).then_some((0, ListScrollAlign::Start, ScrollBehavior::Instant)) } else if query_changed { None } else { destination.map(|row| (row, ListScrollAlign::Nearest, ScrollBehavior::Instant)) });
                if previous != entry {
                    if let Some(callback) = &on_highlight { (callback.borrow_mut())(entry); }
                }
            });
        });
        if let Some(controller) = &self.controller { *controller.0.borrow_mut() = Some(PickerState {
            disabled, grid: grid.clone(), query: request.clone(), highlight, reveal, revision, focus,
            on_highlight: self.on_highlight.clone(), navigation: navigation.clone(),
        });
            let controller = Rc::downgrade(&controller.0);
            let binding = Rc::downgrade(&navigation);
            jadix::portable::on_cleanup(move || {
                if let (Some(controller), Some(binding)) = (controller.upgrade(), binding.upgrade()) {
                    let mut current = controller.borrow_mut();
                    if current.as_ref().is_some_and(|state| Rc::ptr_eq(&state.navigation, &binding)) { *current = None; }
                }
            });
        }
        let mut children = Vec::new();
        if let Some(header) = self.header { children.push(header); }
        if self.searchable {
            let target = format!("{id}__search");
            let mut bar = Vec::new();
            if let Some(prefix) = self.search_prefix { bar.push(prefix); }
            let navigation = navigation.clone();
            let input = R::input(&target, query, &self.placeholder, disabled, Box::new(move |key| (navigation.borrow_mut())(key)));
            bar.push(customize(&self.parts.search, R::focus_target(input, &target, focus, self.search_accessibility_label.as_deref().unwrap_or(&self.accessibility_label), grid.clone(), highlight)));
            if let Some(suffix) = self.search_suffix { bar.push(suffix); }
            children.push(customize(&self.parts.search_bar, R::container(&format!("{id}__search_bar"), true, bar)));
        } else {
            let target = format!("{id}__keyboard");
            let surface = R::navigation_surface(&target, disabled, Box::new(move |key| (navigation.borrow_mut())(key)));
            children.push(R::focus_target(surface, &target, focus, &self.accessibility_label, grid.clone(), highlight));
        }
        if self.show_filters {
        let filter_request = request.clone();
        let labels = self.group_labels.clone();
        let tones: Vec<_> = self.tone_labels.into_iter().enumerate().map(|(i, name)| (if i == 0 { "default".into() } else { i.to_string() }, name)).collect();
        let filter_id = id.clone();
        children.push(R::region(&format!("{id}__filter_options"), Box::new(move || EmojiQuery {
            groups: filter_request().groups, ..EmojiQuery::default()
        }), Box::new(move |request| {
            let order = request.groups.unwrap_or_else(|| vec![10, 0, 1, 3, 4, 5, 6, 7, 8, 9]);
            let mut groups = vec![("all".into(), "All categories".into())];
            groups.extend(order.into_iter().filter_map(|g| labels.get(g as usize).map(|label| (g.to_string(), label.clone()))));
            R::container(&format!("{filter_id}__filters"), true, vec![
                R::select(&format!("{filter_id}__categories"), group, groups, disabled),
                R::select(&format!("{filter_id}__tones"), tone, tones.clone(), disabled),
            ])
        })));
        }
        let list_state = R::ListState::default();
        let results_id = id.clone();
        let results_label = self.accessibility_label.clone();
        let previous_group = Rc::new(std::cell::Cell::new(None::<u8>));
        // Keep this across result/virtual-row replacement so scrolling under a
        // stationary pointer cannot override keyboard navigation.
        let pointer_position = Rc::new(std::cell::Cell::new(None::<[f64; 2]>));
        children.push(R::flexible(R::region(&format!("{id}__results"), Box::new(move || request()), Box::new(move |request| {
            // Full data stays in the shared model; only the viewport owns controls.
            let model = grid.borrow().clone();
            if model.items.is_empty() { return self.empty.as_ref().map_or_else(|| R::text("No emoji found".into()), |render| render()); }
            let count = model.rows.len();
            let choose = choose.clone();
            let render_item = self.render_item.clone();
            let on_item_highlight = self.on_highlight.clone();
            let render_group = self.render_group.clone();
            let labels = self.group_labels.clone();
            let instance = results_id.clone();
            let group_grid = model.clone();
            let on_group = self.on_group.clone();
            let previous_group = previous_group.clone();
            let searching = !request.query.trim().is_empty();
            let page_rows = page_rows.clone();
            let row_part = self.parts.row.clone();
            let item_part = self.parts.item.clone();
            let group_part = self.parts.group.clone();
            let pointer_position = pointer_position.clone();
            customize(&self.parts.list, R::semantics(R::virtual_rows(list_state.clone(), &format!("{results_id}__scroll"), count, self.row_height, self.height, Box::new(move |range| {
                page_rows.set(range.len().max(1));
                if searching { return; }
                let group = group_grid.section_for_row(range.start);
                if previous_group.replace(group) != group {
                    if let Some(callback) = &on_group { (callback.borrow_mut())(group); }
                }
            }), Box::new(move || reveal.get().map(|(row, align, behavior)| (row, revision.get(), align, behavior))), Box::new(move |row| {
                match &model.rows[row] {
                    EmojiRow::Group(group) => customize(&group_part, R::semantics(render_group.as_ref().map_or_else(|| R::text(labels[*group as usize].clone()), |render| render(EmojiGroup { id: *group, label: labels[*group as usize].clone(), first: row == 0 })), Some(AccessibleRole::Heading), &labels[*group as usize])),
                    EmojiRow::Items(range) => customize(&row_part, R::container(&format!("{instance}__row_{row}"), true, (0..self.columns).map(|column| {
                        if column >= range.len() { return R::placeholder(self.item_size); }
                        let index = range.start + column;
                        let entry = model.items[index].clone();
                        let chosen = entry.clone();
                        let choose = choose.clone();
                        let child = render_item.as_ref().map(|render| render(&entry));
                        let hovered = entry.clone();
                        let pointer_position = pointer_position.clone();
                        let on_highlight = on_item_highlight.clone();
                        // Only the old and new option invalidate their projection.
                        let active = create_memo(move || highlight.get() == Some(index));
                        customize(&item_part, R::button(&format!("{instance}__emoji_{index}"), &entry, self.item_size, disabled, Box::new(move || active.get()), child,
                            Box::new(move |position| {
                                if !position.iter().all(|value| value.is_finite()) || disabled { return; }
                                if pointer_position.replace(Some(position)) == Some(position) { return; }
                                if highlight.get() != Some(index) {
                                    highlight.set(Some(index));
                                    if let Some(callback) = &on_highlight { (callback.borrow_mut())(Some(hovered.clone())); }
                                }
                            }), Box::new(move || choose(&chosen))))
                    }).collect())),
                }
            })), Some(AccessibleRole::ListBox), &results_label))
        }))));
        if let Some(footer) = self.footer { children.push(footer); }
        R::semantics(R::container(&id, false, children), None, &self.accessibility_label)
    }
}

//! Native and Shell presentation of the shared tree model.
use jadix::{
    portable::tree_model::{LoadRequest, Tree, TreeCommand, TreeError, TreeLimits, TreeNode},
    portable::{create_signal, virtual_list::virtual_list_indexed, Element, Signal},
    presentation::{AccessibleRole, NavigationKey, NodeContent},
};
use std::{cell::RefCell, rc::Rc};

type Loader = Rc<dyn Fn(LoadRequest<String>)>;

#[derive(Clone)]
pub struct TreeViewState {
    model: Rc<RefCell<Tree<String>>>,
    count: Signal<usize>,
    revision: Signal<u64>,
}

impl TreeViewState {
    pub fn new(nodes: Vec<TreeNode<String>>, limits: TreeLimits) -> Result<Self, TreeError> {
        let model = Tree::new(nodes, limits)?;
        Ok(Self {
            count: create_signal(model.visible().len()),
            model: Rc::new(RefCell::new(model)),
            revision: create_signal(0),
        })
    }

    pub fn read<R>(&self, read: impl FnOnce(&Tree<String>) -> R) -> R {
        self.revision.get();
        read(&self.model.borrow())
    }

    /// Publish model changes after releasing its borrow, so reactive readers and
    /// synchronous lazy loaders can safely reenter the component.
    pub fn update<R>(&self, update: impl FnOnce(&mut Tree<String>) -> R) -> R {
        let result = update(&mut self.model.borrow_mut());
        let count = self.model.borrow().visible().len();
        if self.count.get() != count { self.count.set(count); }
        self.revision.set(self.revision.get().wrapping_add(1));
        result
    }

    pub fn complete_load(&self, request: &LoadRequest<String>, nodes: Vec<TreeNode<String>>) -> Result<(), TreeError> {
        self.update(|tree| tree.complete_load(request, nodes))
    }
}

pub struct TreeViewBuilder {
    id: String,
    state: TreeViewState,
    height: f32,
    row_height: f32,
    loader: Option<Loader>,
}

pub fn tree_view(id: impl Into<String>, state: TreeViewState) -> TreeViewBuilder {
    TreeViewBuilder { id: id.into(), state, height: 320., row_height: 32., loader: None }
}

impl TreeViewBuilder {
    pub fn h(mut self, height: f32) -> Self { self.height = height; self }
    pub fn row_height(mut self, height: f32) -> Self { self.row_height = height; self }
    pub fn on_load(mut self, loader: impl Fn(LoadRequest<String>) + 'static) -> Self {
        self.loader = Some(Rc::new(loader)); self
    }
    pub fn build(self) -> Element {
        let Self { id, state, height, row_height, loader } = self;
        let render_state = state.clone();
        let row_id = id.clone();
        let keys = state.clone();
        let focus = state.clone();
        let mut element = virtual_list_indexed(state.count.get(), row_height as f64, move |index| {
            let key = render_state.read(|tree| tree.visible()[index.get()].key.clone());
            let select_state = render_state.clone();
            let select_key = key.clone();
            let mut item: Element = super::button(&item_id(&row_id, &key))
                .on_click(move || { let _ = select_state.update(|tree| tree.select(&select_key, false, false)); })
                .into();
            item.descriptor.accessible_role = Some(AccessibleRole::TreeItem);
            item.descriptor.navigation_keys = vec![NavigationKey::Up, NavigationKey::Down,
                NavigationKey::Left, NavigationKey::Right, NavigationKey::Home, NavigationKey::End];
            let binding_state = render_state.clone();
            let binding_key = key.clone();
            let caption_state = render_state.clone();
            let caption_key = key.clone();
            let mut caption = Element::new().text("");
            caption.text_binding = Some(Rc::new(move || caption_state.read(|tree| tree.label(&caption_key).unwrap_or_default().into())));
            item.children.push(caption);
            item.descriptor_binding = Some(Rc::new(move |descriptor| {
                binding_state.read(|tree| {
                    descriptor.content = NodeContent::Button {
                        label: tree.label(&binding_key).unwrap_or_default().into(),
                        enabled: tree.enabled(&binding_key),
                    };
                    descriptor.accessible_selected = Some(tree.selected().contains(&binding_key));
                    descriptor.accessible_expanded = tree.branch(&binding_key).then(|| tree.expanded(&binding_key));
                    descriptor.accessibility.busy = tree.loading(&binding_key);
                });
            }));
            let navigation_state = render_state.clone();
            let navigation_key = key.clone();
            let navigation_id = row_id.clone();
            let navigation_loader = loader.clone();
            item.on_navigation = Some(Rc::new(RefCell::new(Box::new(move |key| {
                let command = match key {
                    NavigationKey::Up => TreeCommand::Previous,
                    NavigationKey::Down => TreeCommand::Next,
                    NavigationKey::Left => TreeCommand::Parent,
                    NavigationKey::Right => TreeCommand::Child,
                    NavigationKey::Home => TreeCommand::First,
                    NavigationKey::End => TreeCommand::Last,
                    _ => return None,
                };
                let request = navigation_state.update(|tree| {
                    tree.focus(&navigation_key)?;
                    tree.command(command)
                }).ok().flatten();
                if let (Some(loader), Some(request)) = (&navigation_loader, request) { loader(request); }
                navigation_state.read(|tree| tree.focused().map(|key| item_id(&navigation_id, key)))
            }))));
            let toggle_state = render_state.clone();
            let toggle_key = key.clone();
            let toggle_loader = loader.clone();
            let mut disclosure: Element = super::button(&format!("{row_id}__toggle_{key}"))
                .on_click(move || {
                    let request = toggle_state.update(|tree| tree.set_expanded(&toggle_key, !tree.expanded(&toggle_key))).ok().flatten();
                    if let (Some(loader), Some(request)) = (&toggle_loader, request) { loader(request); }
                }).into();
            let disclosure_state = render_state.clone();
            let disclosure_key = key.clone();
            let caption_state = render_state.clone();
            let caption_key = key.clone();
            let mut caption = Element::new().text("");
            caption.text_binding = Some(Rc::new(move || caption_state.read(|tree| if tree.expanded(&caption_key) { "▾" } else { "▸" }.into())));
            disclosure.children.push(caption);
            disclosure.descriptor_binding = Some(Rc::new(move |descriptor| {
                disclosure_state.read(|tree| {
                    let expanded = tree.expanded(&disclosure_key);
                    descriptor.content = NodeContent::Button {
                        label: if expanded { "▾" } else { "▸" }.into(),
                        enabled: tree.enabled(&disclosure_key) && tree.branch(&disclosure_key),
                    };
                    descriptor.accessible_name = Some(format!("{} {}", if expanded { "Collapse" } else { "Expand" }, tree.label(&disclosure_key).unwrap_or_default()));
                });
            }));
            let depth_state = render_state.clone();
            let mut indent = Element::new().h(1.);
            indent.descriptor_binding = Some(Rc::new(move |descriptor| {
                let depth = depth_state.read(|tree| tree.visible()[index.get()].depth);
                descriptor.style.width = Some(depth as f32 * 16.);
            }));
            Element::new().flex_row().h(row_height).children(vec![indent, disclosure.w(28.).h(row_height), item.flex_grow(1.).h(row_height)])
        }).id(id).h(height).count(state.count)
            .key(move |index| keys.read(|tree| tree.visible()[index].key.clone()))
            .focused_index_request_fn(move || focus.read(|tree| {
                tree.focused().and_then(|key| tree.visible_index(key))
                    .map(|index| (index, focus.revision.get() as usize))
            })).build();
        element.descriptor.accessible_role = Some(AccessibleRole::Tree);
        element
    }
}

fn item_id(id: &str, key: &str) -> String { format!("{id}__item_{key}") }

impl From<TreeViewBuilder> for Element { fn from(builder: TreeViewBuilder) -> Self { builder.build() } }
impl jadix::portable::core::tree::IntoElementList for TreeViewBuilder {
    fn into_element_list(self) -> Vec<Element> { vec![self.build()] }
}

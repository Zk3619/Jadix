use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Children<Key> {
    Loaded(Vec<TreeNode<Key>>),
    Unloaded,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TreeNode<Key> {
    pub key: Key,
    pub label: String,
    pub enabled: bool,
    pub children: Children<Key>,
}

impl<Key> TreeNode<Key> {
    pub fn leaf(key: Key, label: impl Into<String>) -> Self {
        Self {
            key,
            label: label.into(),
            enabled: true,
            children: Children::Loaded(Vec::new()),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TreeLimits {
    pub nodes: usize,
    pub depth: usize,
    pub label_bytes: usize,
}

impl Default for TreeLimits {
    fn default() -> Self {
        Self {
            nodes: 100_000,
            depth: 64,
            label_bytes: 16 * 1024 * 1024,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TreeError {
    DuplicateKey,
    UnknownKey,
    HiddenKey,
    Disabled,
    NotLazy,
    StaleLoad,
    Budget,
    IdentityExhausted,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LoadRequest<Key> {
    tree: u64,
    key: Key,
    generation: u64,
}

impl<Key> LoadRequest<Key> {
    pub fn key(&self) -> &Key {
        &self.key
    }
    pub fn generation(&self) -> u64 {
        self.generation
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TreeCommand {
    Previous,
    Next,
    First,
    Last,
    Parent,
    Child,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TreeRow<Key> {
    pub key: Key,
    pub depth: usize,
    pub position: usize,
    pub siblings: usize,
}

#[derive(Clone)]
struct Entry<Key> {
    label: String,
    enabled: bool,
    parent: Option<Key>,
    children: Option<Vec<Key>>,
    depth: usize,
    expanded: bool,
    loading: Option<u64>,
}

type PreparedNodes<Key> = (BTreeMap<Key, Entry<Key>>, Vec<Key>, usize);

pub struct Tree<Key> {
    identity: u64,
    visibility_revision: u64,
    entries: BTreeMap<Key, Entry<Key>>,
    roots: Vec<Key>,
    visible: Vec<TreeRow<Key>>,
    positions: BTreeMap<Key, usize>,
    focused: Option<Key>,
    anchor: Option<Key>,
    selected: BTreeSet<Key>,
    limits: TreeLimits,
    label_bytes: usize,
    generation: u64,
}

impl<Key: Clone + Ord> Tree<Key> {
    pub fn new(nodes: Vec<TreeNode<Key>>, limits: TreeLimits) -> Result<Self, TreeError> {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        let identity = NEXT
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
                value.checked_add(1)
            })
            .map_err(|_| TreeError::IdentityExhausted)?;
        let mut tree = Self {
            identity,
            visibility_revision: 0,
            entries: BTreeMap::new(),
            roots: Vec::new(),
            visible: Vec::new(),
            positions: BTreeMap::new(),
            focused: None,
            anchor: None,
            selected: BTreeSet::new(),
            limits,
            label_bytes: 0,
            generation: 0,
        };
        let (entries, roots, bytes) = tree.prepare(nodes, None, 0)?;
        tree.entries = entries;
        tree.roots = roots;
        tree.label_bytes = bytes;
        tree.refresh();
        Ok(tree)
    }

    fn prepare(
        &self,
        nodes: Vec<TreeNode<Key>>,
        parent: Option<Key>,
        depth: usize,
    ) -> Result<PreparedNodes<Key>, TreeError> {
        let roots = nodes.iter().map(|node| node.key.clone()).collect();
        let mut pending: Vec<_> = nodes
            .into_iter()
            .map(|node| (node, parent.clone(), depth))
            .collect();
        let mut entries = BTreeMap::new();
        let mut bytes = self.label_bytes;
        while let Some((node, parent, depth)) = pending.pop() {
            if self.entries.contains_key(&node.key) || entries.contains_key(&node.key) {
                return Err(TreeError::DuplicateKey);
            }
            bytes = bytes
                .checked_add(node.label.len())
                .ok_or(TreeError::Budget)?;
            if depth > self.limits.depth
                || bytes > self.limits.label_bytes
                || self.entries.len().saturating_add(entries.len()) >= self.limits.nodes
            {
                return Err(TreeError::Budget);
            }
            let children = match node.children {
                Children::Loaded(children) => {
                    let keys = children.iter().map(|child| child.key.clone()).collect();
                    pending.extend(
                        children
                            .into_iter()
                            .map(|child| (child, Some(node.key.clone()), depth + 1)),
                    );
                    Some(keys)
                }
                Children::Unloaded => None,
            };
            entries.insert(
                node.key,
                Entry {
                    label: node.label,
                    enabled: node.enabled,
                    parent,
                    children,
                    depth,
                    expanded: false,
                    loading: None,
                },
            );
        }
        Ok((entries, roots, bytes))
    }

    fn refresh(&mut self) {
        self.visibility_revision = self.visibility_revision.saturating_add(1);
        let mut visible = Vec::new();
        let mut pending: Vec<_> = self
            .roots
            .iter()
            .enumerate()
            .rev()
            .map(|(index, key)| (key.clone(), index + 1, self.roots.len()))
            .collect();
        while let Some((key, position, siblings)) = pending.pop() {
            let entry = &self.entries[&key];
            visible.push(TreeRow {
                key: key.clone(),
                depth: entry.depth,
                position,
                siblings,
            });
            if entry.expanded {
                if let Some(children) = &entry.children {
                    pending.extend(
                        children
                            .iter()
                            .enumerate()
                            .rev()
                            .map(|(index, key)| (key.clone(), index + 1, children.len())),
                    );
                }
            }
        }
        self.positions = visible
            .iter()
            .enumerate()
            .map(|(index, row)| (row.key.clone(), index))
            .collect();
        self.visible = visible;
        if let Some(focused) = self.focused.clone() {
            let mut candidate = Some(focused);
            while let Some(key) = candidate {
                if self.positions.contains_key(&key) && self.entries[&key].enabled {
                    self.focused = Some(key);
                    return;
                }
                candidate = self.entries[&key].parent.clone();
            }
        }
        self.focused = self
            .visible
            .iter()
            .find(|row| self.entries[&row.key].enabled)
            .map(|row| row.key.clone());
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }
    pub fn visibility_revision(&self) -> u64 {
        self.visibility_revision
    }
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
    pub fn visible(&self) -> &[TreeRow<Key>] {
        &self.visible
    }
    pub fn focused(&self) -> Option<&Key> {
        self.focused.as_ref()
    }
    pub fn visible_index(&self, key: &Key) -> Option<usize> {
        self.positions.get(key).copied()
    }
    pub fn selected(&self) -> &BTreeSet<Key> {
        &self.selected
    }
    pub fn label(&self, key: &Key) -> Option<&str> {
        self.entries.get(key).map(|entry| entry.label.as_str())
    }
    pub fn enabled(&self, key: &Key) -> bool {
        self.entries.get(key).is_some_and(|entry| entry.enabled)
    }
    pub fn expanded(&self, key: &Key) -> bool {
        self.entries.get(key).is_some_and(|entry| entry.expanded)
    }
    pub fn loading(&self, key: &Key) -> bool {
        self.entries
            .get(key)
            .is_some_and(|entry| entry.loading.is_some())
    }
    pub fn branch(&self, key: &Key) -> bool {
        self.entries.get(key).is_some_and(|entry| {
            entry
                .children
                .as_ref()
                .is_none_or(|children| !children.is_empty())
        })
    }

    pub fn focus(&mut self, key: &Key) -> Result<(), TreeError> {
        let entry = self.entries.get(key).ok_or(TreeError::UnknownKey)?;
        if !entry.enabled {
            return Err(TreeError::Disabled);
        }
        if !self.positions.contains_key(key) {
            return Err(TreeError::HiddenKey);
        }
        self.focused = Some(key.clone());
        Ok(())
    }

    pub fn select(&mut self, key: &Key, extend: bool, additive: bool) -> Result<(), TreeError> {
        let previous = self.focused.clone();
        self.focus(key)?;
        let current = self.positions[key];
        let anchor = self
            .anchor
            .as_ref()
            .or(previous.as_ref())
            .and_then(|key| self.positions.get(key))
            .copied();
        if !additive {
            self.selected.clear();
        }
        if extend {
            let anchor = anchor.unwrap_or(current);
            if self.anchor.is_none() {
                self.anchor = Some(self.visible[anchor].key.clone());
            }
            for row in &self.visible[anchor.min(current)..=anchor.max(current)] {
                if self.entries[&row.key].enabled {
                    self.selected.insert(row.key.clone());
                }
            }
        } else {
            self.anchor = Some(key.clone());
            if additive && self.selected.contains(key) {
                self.selected.remove(key);
            } else {
                self.selected.insert(key.clone());
            }
        }
        Ok(())
    }

    pub fn set_expanded(
        &mut self,
        key: &Key,
        expanded: bool,
    ) -> Result<Option<LoadRequest<Key>>, TreeError> {
        let entry = self.entries.get(key).ok_or(TreeError::UnknownKey)?;
        if !entry.enabled {
            return Err(TreeError::Disabled);
        }
        let request = if expanded && entry.children.is_none() && entry.loading.is_none() {
            let generation = self
                .generation
                .checked_add(1)
                .ok_or(TreeError::IdentityExhausted)?;
            self.generation = generation;
            Some(LoadRequest {
                tree: self.identity,
                key: key.clone(),
                generation,
            })
        } else {
            None
        };
        let entry = self.entries.get_mut(key).unwrap();
        entry.expanded = expanded
            && entry
                .children
                .as_ref()
                .is_none_or(|children| !children.is_empty());
        if !expanded {
            entry.loading = None;
        } else if let Some(request) = &request {
            entry.loading = Some(request.generation);
        }
        if !expanded {
            let mut pending = entry.children.clone().unwrap_or_default();
            while let Some(key) = pending.pop() {
                let descendant = self.entries.get_mut(&key).unwrap();
                descendant.loading = None;
                if let Some(children) = &descendant.children {
                    pending.extend(children.iter().cloned());
                }
            }
        }
        self.refresh();
        Ok(request)
    }

    pub fn cancel_load(&mut self, request: &LoadRequest<Key>) -> Result<(), TreeError> {
        if request.tree != self.identity {
            return Err(TreeError::StaleLoad);
        }
        let entry = self
            .entries
            .get_mut(&request.key)
            .ok_or(TreeError::StaleLoad)?;
        if entry.loading != Some(request.generation) {
            return Err(TreeError::StaleLoad);
        }
        entry.loading = None;
        Ok(())
    }

    pub fn request_pending(&self, request: &LoadRequest<Key>) -> bool {
        request.tree == self.identity
            && self
                .entries
                .get(&request.key)
                .is_some_and(|entry| entry.loading == Some(request.generation))
    }

    pub fn complete_load(
        &mut self,
        request: &LoadRequest<Key>,
        nodes: Vec<TreeNode<Key>>,
    ) -> Result<(), TreeError> {
        if request.tree != self.identity {
            return Err(TreeError::StaleLoad);
        }
        let entry = self.entries.get(&request.key).ok_or(TreeError::StaleLoad)?;
        if entry.loading != Some(request.generation) {
            return Err(TreeError::StaleLoad);
        }
        if entry.children.is_some() {
            return Err(TreeError::NotLazy);
        }
        let (entries, children, bytes) =
            self.prepare(nodes, Some(request.key.clone()), entry.depth + 1)?;
        let entry = self.entries.get_mut(&request.key).unwrap();
        entry.expanded = !children.is_empty() && entry.expanded;
        entry.children = Some(children);
        entry.loading = None;
        self.entries.extend(entries);
        self.label_bytes = bytes;
        self.refresh();
        Ok(())
    }

    pub fn command(&mut self, command: TreeCommand) -> Result<Option<LoadRequest<Key>>, TreeError> {
        let Some(key) = self.focused.clone() else {
            return Ok(None);
        };
        let current = self.positions[&key];
        let target = match command {
            TreeCommand::Previous => self.visible[..current]
                .iter()
                .rev()
                .find(|row| self.enabled(&row.key))
                .map(|row| row.key.clone()),
            TreeCommand::Next => self.visible[current + 1..]
                .iter()
                .find(|row| self.enabled(&row.key))
                .map(|row| row.key.clone()),
            TreeCommand::First => self
                .visible
                .iter()
                .find(|row| self.enabled(&row.key))
                .map(|row| row.key.clone()),
            TreeCommand::Last => self
                .visible
                .iter()
                .rev()
                .find(|row| self.enabled(&row.key))
                .map(|row| row.key.clone()),
            TreeCommand::Parent => {
                if self.expanded(&key) {
                    return self.set_expanded(&key, false);
                }
                let mut parent = self.entries[&key].parent.clone();
                while parent.as_ref().is_some_and(|key| !self.enabled(key)) {
                    parent = self.entries[parent.as_ref().unwrap()].parent.clone();
                }
                parent
            }
            TreeCommand::Child => {
                if !self.expanded(&key) {
                    return self.set_expanded(&key, true);
                }
                self.entries[&key]
                    .children
                    .as_ref()
                    .and_then(|children| children.iter().find(|key| self.enabled(key)).cloned())
            }
        };
        if let Some(target) = target {
            self.focus(&target)?;
        }
        Ok(None)
    }
}

//! # TreeView
//!
//! 树形列表：DFS 展开扁平化 + VirtualList；点击展开/折叠。

use crate::app::reactive::reactive_view;
use crate::components::virtual_list::virtual_list;
use crate::core::reactivity::{use_signal, Signal};
use crate::core::tree::Element;
use crate::primitives::r#box::r#box;
use crate::primitives::text::text;
use crate::utils::tree_model::{self, TreeNodeLike};
use std::collections::HashSet;

#[derive(Clone, Debug)]
pub struct TreeNode {
    pub id: String,
    pub label: String,
    pub children: Vec<TreeNode>,
}

impl TreeNodeLike for TreeNode {
    fn id(&self) -> &str {
        &self.id
    }
    fn children(&self) -> &[Self] {
        &self.children
    }
}

#[derive(Clone)]
struct FlatRow {
    id: String,
    label: String,
    depth: usize,
    has_children: bool,
}

fn flatten(nodes: &[TreeNode], expanded: &HashSet<String>, depth: usize, out: &mut Vec<FlatRow>) {
    *out = tree_model::flatten_expanded(nodes, expanded, depth, |n, d, has| FlatRow {
        id: n.id.clone(),
        label: n.label.clone(),
        depth: d,
        has_children: has,
    });
}

pub struct TreeViewBuilder {
    id: String,
    roots: Vec<TreeNode>,
    row_height: f32,
    height: f32,
    expanded: Option<Signal<HashSet<String>>>,
}

pub fn tree_view(id: impl Into<String>) -> TreeViewBuilder {
    TreeViewBuilder {
        id: id.into(),
        roots: Vec::new(),
        row_height: 28.0,
        height: 240.0,
        expanded: None,
    }
}

impl TreeViewBuilder {
    pub fn roots(mut self, roots: impl IntoIterator<Item = TreeNode>) -> Self {
        self.roots = roots.into_iter().collect();
        self
    }

    pub fn row_height(mut self, h: f32) -> Self {
        self.row_height = h;
        self
    }

    pub fn height(mut self, h: f32) -> Self {
        self.height = h;
        self
    }

    pub fn expanded_signal(mut self, s: Signal<HashSet<String>>) -> Self {
        self.expanded = Some(s);
        self
    }
}

impl From<TreeViewBuilder> for Element {
    fn from(builder: TreeViewBuilder) -> Self {
        let TreeViewBuilder {
            id,
            roots,
            row_height,
            height,
            expanded,
        } = builder;
        let exp_key = format!("{id}__tree_exp");
        let expanded =
            expanded.unwrap_or_else(|| use_signal(exp_key.as_str(), HashSet::<String>::new));
        let view_id = format!("{id}__tree_view");

        reactive_view(view_id, move || {
            let exp = expanded.get();
            let mut flat = Vec::new();
            flatten(&roots, &exp, 0, &mut flat);
            let count = flat.len();
            let exp_sig = expanded;

            virtual_list(format!("{id}__rows"))
                .item_count(count)
                .item_height(row_height)
                .height(height)
                .render_item(move |i| {
                    let row = flat.get(i).cloned().unwrap_or(FlatRow {
                        id: String::new(),
                        label: String::new(),
                        depth: 0,
                        has_children: false,
                    });
                    let pad = 8.0 + row.depth as f32 * 16.0;
                    let marker = if row.has_children {
                        if exp_sig.peek().contains(&row.id) {
                            "▼ "
                        } else {
                            "▶ "
                        }
                    } else {
                        "  "
                    };
                    let label = format!("{marker}{}", row.label);
                    let nid = row.id.clone();
                    let has = row.has_children;
                    let s = exp_sig;
                    let mut el: Element = r#box()
                        .w("100%")
                        .h(row_height)
                        .pl(pad)
                        .child(text(label).size(13.0))
                        .into();
                    el = el.on_click(move || {
                        if !has {
                            return;
                        }
                        let mut set = s.get();
                        tree_model::toggle_expanded(&mut set, &nid);
                        s.set(set);
                    });
                    el
                })
                .into()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flatten_respects_expanded() {
        let roots = vec![TreeNode {
            id: "a".into(),
            label: "A".into(),
            children: vec![TreeNode {
                id: "b".into(),
                label: "B".into(),
                children: vec![],
            }],
        }];
        let mut out = Vec::new();
        flatten(&roots, &HashSet::new(), 0, &mut out);
        assert_eq!(out.len(), 1);
        let mut exp = HashSet::new();
        exp.insert("a".into());
        out.clear();
        flatten(&roots, &exp, 0, &mut out);
        assert_eq!(out.len(), 2);
    }
}

//! Tree 展开扁平化（WP-SI.8）。

use std::collections::HashSet;

/// 可展节点最小面：id + 子节点。
pub trait TreeNodeLike {
    fn id(&self) -> &str;
    fn children(&self) -> &[Self]
    where
        Self: Sized;
}

/// 深度优先：仅展开 `expanded` 中的节点。
pub fn flatten_expanded<T, R, F>(
    nodes: &[T],
    expanded: &HashSet<String>,
    depth: usize,
    map: F,
) -> Vec<R>
where
    T: TreeNodeLike,
    F: Fn(&T, usize, bool) -> R + Copy,
{
    let mut out = Vec::new();
    flatten_into(nodes, expanded, depth, map, &mut out);
    out
}

fn flatten_into<T, R, F>(
    nodes: &[T],
    expanded: &HashSet<String>,
    depth: usize,
    map: F,
    out: &mut Vec<R>,
) where
    T: TreeNodeLike,
    F: Fn(&T, usize, bool) -> R + Copy,
{
    for n in nodes {
        let has_children = !n.children().is_empty();
        out.push(map(n, depth, has_children));
        if has_children && expanded.contains(n.id()) {
            flatten_into(n.children(), expanded, depth + 1, map, out);
        }
    }
}

pub fn toggle_expanded(set: &mut HashSet<String>, id: &str) {
    if !set.remove(id) {
        set.insert(id.to_string());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct N {
        id: String,
        kids: Vec<N>,
    }

    impl TreeNodeLike for N {
        fn id(&self) -> &str {
            &self.id
        }
        fn children(&self) -> &[Self] {
            &self.kids
        }
    }

    #[test]
    fn flatten_and_toggle() {
        let root = N {
            id: "a".into(),
            kids: vec![N {
                id: "b".into(),
                kids: vec![],
            }],
        };
        let mut exp = HashSet::new();
        let closed: Vec<_> = flatten_expanded(&[root], &exp, 0, |n, d, _| (n.id().to_string(), d));
        assert_eq!(closed, vec![("a".into(), 0)]);
        toggle_expanded(&mut exp, "a");
        let root = N {
            id: "a".into(),
            kids: vec![N {
                id: "b".into(),
                kids: vec![],
            }],
        };
        let open: Vec<_> = flatten_expanded(&[root], &exp, 0, |n, d, _| (n.id().to_string(), d));
        assert_eq!(open, vec![("a".into(), 0), ("b".into(), 1)]);
    }
}

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

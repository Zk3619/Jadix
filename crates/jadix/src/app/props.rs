//! Solid-style `mergeProps` / `splitProps` / `children()` — Rust semantics, not JS Proxy.
//!
//! - [`merge_props`]: The later source `Some` overwrites the previous source; `None` / empty `id` does not delete the existing value.
//! - [`split_props!`](crate::split_props): `mem::take` lists the fields, and the remaining fields can be spread through `..rest`.
//! - [`children`]: 把 `Option<Element>` / fragment 展平；闭包版在 render effect 里求值以便订 signal。

use crate::components::base::BaseComponentBuilder;
use crate::core::reactivity::{create_render_effect, create_signal, Signal};
use crate::core::tree::Element;
use std::cell::RefCell;
use std::rc::Rc;

/// Field-wise overlay for `#[component]` generated `*Props` bags.
pub trait MergeProps {
    /// `overlay` wins on `Some` / non-empty `id`; `None` does not clobber.
    fn merge_with(self, overlay: Self) -> Self;
}

/// Two-source merge. For three or more, use [`merge_props!`](crate::merge_props).
#[inline]
pub fn merge_props<P: MergeProps>(base: P, overlay: P) -> P {
    base.merge_with(overlay)
}

/// Compose `base` / `base_builder` closures: earlier runs first, later last (caller wins).
pub fn merge_base_prop(
    earlier: Option<Box<dyn FnOnce(BaseComponentBuilder) -> BaseComponentBuilder>>,
    later: Option<Box<dyn FnOnce(BaseComponentBuilder) -> BaseComponentBuilder>>,
) -> Option<Box<dyn FnOnce(BaseComponentBuilder) -> BaseComponentBuilder>> {
    match (earlier, later) {
        (Some(a), Some(b)) => Some(Box::new(move |x| b(a(x)))),
        (Some(a), None) => Some(a),
        (None, Some(b)) => Some(b),
        (None, None) => None,
    }
}

/// Flattened children list (Solid `children().toArray()`).
#[derive(Default)]
pub struct Children {
    items: Vec<Element>,
}

impl Children {
    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn as_slice(&self) -> &[Element] {
        &self.items
    }

    pub fn into_vec(self) -> Vec<Element> {
        self.items
    }

    /// Pack back to a single node (0 → `None`, 1 → that node, N → `jadix_fragment`).
    pub fn into_element(self) -> Option<Element> {
        pack_children(self.items)
    }
}

impl IntoIterator for Children {
    type Item = Element;
    type IntoIter = std::vec::IntoIter<Element>;

    fn into_iter(self) -> Self::IntoIter {
        self.items.into_iter()
    }
}

impl From<Children> for Vec<Element> {
    fn from(value: Children) -> Self {
        value.items
    }
}

impl From<Children> for Option<Element> {
    fn from(value: Children) -> Self {
        value.into_element()
    }
}

/// Eager flatten. Use when `children` is already an [`Element`] tree.
pub fn resolve_children(children: Option<Element>) -> Children {
    Children {
        items: Element::unpack_fragment_children(children),
    }
}

/// Solid `children(() => props.children)`: `source` runs in a render effect so `signal.get()`
/// inside it does not subscribe the wrapper's component boundary.
pub struct ChildrenAccessor {
    items: Rc<RefCell<Vec<Element>>>,
    gen: Signal<u32>,
}

impl ChildrenAccessor {
    fn track(&self) {
        let _ = self.gen.get();
    }

    /// Take the current flattened list (empties the slot until the source re-runs).
    pub fn take(&self) -> Vec<Element> {
        self.track();
        std::mem::take(&mut *self.items.borrow_mut())
    }

    pub fn len(&self) -> usize {
        self.track();
        self.items.borrow().len()
    }

    pub fn is_empty(&self) -> bool {
        self.track();
        self.items.borrow().is_empty()
    }

    pub fn to_element(&self) -> Option<Element> {
        pack_children(self.take())
    }
}

/// Accessor form of [`resolve_children`]. Call from an owner (`create_root` / component body).
pub fn children(mut source: impl FnMut() -> Option<Element> + 'static) -> ChildrenAccessor {
    let items = Rc::new(RefCell::new(Vec::<Element>::new()));
    let gen = create_signal(0u32);
    let items_e = Rc::clone(&items);
    create_render_effect(move || {
        *items_e.borrow_mut() = Element::unpack_fragment_children(source());
        let n = gen.peek();
        gen.set(n.wrapping_add(1));
    });
    ChildrenAccessor { items, gen }
}

fn pack_children(mut items: Vec<Element>) -> Option<Element> {
    match items.len() {
        0 => None,
        1 => items.pop(),
        _ => {
            let mut frag = Element::new();
            frag.set_attr("jadix_fragment", "1");
            Some(frag.children(items))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_children_unpacks_fragment() {
        let mut frag = Element::new();
        frag.set_attr("jadix_fragment", "1");
        let frag = frag
            .child(Element::new().text("a"))
            .child(Element::new().text("b"));
        let c = resolve_children(Some(frag));
        assert_eq!(c.len(), 2);
        assert_eq!(c.as_slice()[0].text.as_deref(), Some("a"));
        let packed = c.into_element().expect("frag");
        assert_eq!(
            packed.get_attr("jadix_fragment").map(String::as_str),
            Some("1")
        );
        assert_eq!(packed.children.len(), 2);
    }

    #[test]
    fn merge_base_prop_applies_earlier_then_later() {
        let earlier = Some(Box::new(|mut b: BaseComponentBuilder| {
            b.el.set_attr("a", "1");
            b
        })
            as Box<dyn FnOnce(BaseComponentBuilder) -> BaseComponentBuilder>);
        let later = Some(Box::new(|mut b: BaseComponentBuilder| {
            b.el.set_attr("b", "1");
            b
        })
            as Box<dyn FnOnce(BaseComponentBuilder) -> BaseComponentBuilder>);
        let merged = merge_base_prop(earlier, later).expect("both");
        let out = merged(BaseComponentBuilder::new(Element::new())).el;
        assert_eq!(out.get_attr("a").map(String::as_str), Some("1"));
        assert_eq!(out.get_attr("b").map(String::as_str), Some("1"));
    }
}

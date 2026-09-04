//! Windows PE default stack is ~1 MiB. Layout used to fit; collect/build frames
//! grew until calculator/gallery needed an 8 MiB grow. This file is the red/green
//! loop: measure type sizes + stack watermarks, then assert a calculator-shaped
//! tree stays under the default budget. Deep chains must layout fully on a heap
//! work queue — they are a memory/time cost, not a stack overflow.

use crate::components::button::button;
use crate::core::tree::Element;
use crate::layout::layout_tree::{stack_probe, LayoutTree, TextLayoutInfo};
use crate::primitives::{hstack, vstack};
use crate::renderer::InstanceInput;
use crate::style::{AnimationSnapshot, Style, TextStyle, TransitionSnapshot};
use std::mem::size_of;

/// Leave headroom for winit / the event loop on the same 1 MiB thread.
const LAYOUT_STACK_BUDGET: usize = 768 * 1024;

/// Deep enough to have overflowed the old recursive collect; shallow enough that
/// Taffy's own `compute_layout` recursion still fits the default stack.
const DEEP_CHAIN_WRAPPERS: usize = 128;

fn chain(depth: usize) -> Element {
    nest_chain_iterative(depth)
}

/// Build a parent-chain without mapping depth onto the call stack (`Element` is ~3.6 KiB).
fn nest_chain_iterative(wrappers: usize) -> Element {
    let mut el = Element::new().text("leaf").w(40.0).h(20.0);
    for _ in 0..wrappers {
        el = Element::new()
            .w("100%")
            .h("100%")
            .bg([0.95, 0.95, 0.95, 1.0])
            .p(1.0)
            .child(el);
    }
    el
}

fn max_depth(el: &Element) -> usize {
    let mut max = 0usize;
    let mut stack: Vec<(&Element, usize)> = vec![(el, 1)];
    while let Some((el, d)) = stack.pop() {
        max = max.max(d);
        for c in &el.children {
            stack.push((c, d + 1));
        }
    }
    max
}

fn leaf_has_layout_node(el: &Element) -> bool {
    let mut cur = el;
    while let Some(child) = cur.children.first() {
        cur = child;
    }
    cur.layout_node.is_some()
}

fn calculator_pad() -> Element {
    let mut pad = vstack().w("100%").h("100%").gap("0.15rem");
    for r in 0..6 {
        let mut row = hstack().w("100%").gap("0.15rem");
        for c in 0..5 {
            row = row.child(Element::from(
                button(format!("k{r}{c}")).w("100%").h("100%").child(
                    Element::new()
                        .text(format!("{}", (r * 5 + c) % 10))
                        .w("100%")
                        .h("100%"),
                ),
            ));
        }
        pad = pad.child(Element::from(row));
    }
    Element::new()
        .w(380.0)
        .h(620.0)
        .flex_col()
        .p(8.0)
        .child(Element::new().text("123").w("100%").h(48.0))
        .child(Element::from(pad))
}

fn layout_and_probe(root: &mut Element, w: f32, h: f32) -> (usize, usize, usize, usize) {
    stack_probe::reset();
    let mut tree = LayoutTree::new();
    let _ = tree.build_and_layout(root, w, h, [0.0, 0.0]);
    (
        stack_probe::used(),
        stack_probe::collect_max_frame(),
        stack_probe::build_max_frame(),
        stack_probe::cascade_max_frame(),
    )
}

#[test]
fn layout_type_sizes_are_documented() {
    eprintln!("size_of Element            = {}", size_of::<Element>());
    eprintln!("size_of Style              = {}", size_of::<Style>());
    eprintln!(
        "size_of taffy::Style       = {}",
        size_of::<taffy::style::Style>()
    );
    eprintln!("size_of TextStyle          = {}", size_of::<TextStyle>());
    eprintln!(
        "size_of TextLayoutInfo     = {}",
        size_of::<TextLayoutInfo>()
    );
    eprintln!(
        "size_of InstanceInput      = {}",
        size_of::<InstanceInput>()
    );
    eprintln!(
        "size_of TransitionSnapshot = {}",
        size_of::<TransitionSnapshot>()
    );
    eprintln!(
        "size_of AnimationSnapshot  = {}",
        size_of::<AnimationSnapshot>()
    );
}

#[test]
fn nested_chain_layout_stack_stays_under_default_budget() {
    let depth = 24;
    let mut root = chain(depth);
    let (used, collect_f, build_f, cascade_f) = layout_and_probe(&mut root, 320.0, 240.0);
    eprintln!(
        "chain depth={} tree_depth={} used={} collect_frame={} build_frame={} cascade_frame={} marks={}",
        depth,
        max_depth(&root),
        used,
        collect_f,
        build_f,
        cascade_f,
        stack_probe::collect_marks(),
    );
    assert!(
        used < LAYOUT_STACK_BUDGET,
        "nested chain used {used} stack bytes (collect_frame={collect_f}, build_frame={build_f}, cascade_frame={cascade_f}); Windows default is 1 MiB"
    );
    assert!(
        collect_f < 12 * 1024,
        "collect frame {collect_f} grew back toward the old ~21 KiB MSVC-debug paint frame"
    );
}

#[test]
fn calculator_shaped_pad_layout_stack_stays_under_default_budget() {
    let mut root = calculator_pad();
    let (used, collect_f, build_f, cascade_f) = layout_and_probe(&mut root, 380.0, 620.0);
    eprintln!(
        "calc-pad tree_depth={} used={} collect_frame={} build_frame={} cascade_frame={} marks={}",
        max_depth(&root),
        used,
        collect_f,
        build_f,
        cascade_f,
        stack_probe::collect_marks(),
    );
    assert!(
        used < LAYOUT_STACK_BUDGET,
        "calculator-shaped pad used {used} stack bytes (collect_frame={collect_f}, build_frame={build_f}, cascade_frame={cascade_f}); Windows default is 1 MiB"
    );
    assert!(
        collect_f < 12 * 1024,
        "collect frame {collect_f} grew back toward the old ~21 KiB MSVC-debug paint frame"
    );
}

#[test]
fn layout_deep_chain_stays_under_default_budget() {
    let wrappers = DEEP_CHAIN_WRAPPERS;
    let mut root = nest_chain_iterative(wrappers);
    let (used, collect_f, build_f, cascade_f) = layout_and_probe(&mut root, 320.0, 240.0);
    eprintln!(
        "deep chain wrappers={} tree_depth={} used={} collect_frame={} build_frame={} cascade_frame={}",
        wrappers,
        max_depth(&root),
        used,
        collect_f,
        build_f,
        cascade_f,
    );
    assert!(
        used < LAYOUT_STACK_BUDGET,
        "deep chain used {used} stack bytes (collect_frame={collect_f}, build_frame={build_f}, cascade_frame={cascade_f}); heap walk must not grow the OS stack with tree depth"
    );
    assert!(
        leaf_has_layout_node(&root),
        "deep chain leaf must be laid out (no depth cap skipping descendants)"
    );
    assert_eq!(
        max_depth(&root),
        wrappers + 1,
        "fixture depth should be wrappers + leaf"
    );
}

#[test]
fn element_drop_is_iterative_on_tall_tree() {
    let mut root = Element::new().text("leaf");
    for _ in 0..4_000 {
        root = Element::new().child(root);
    }
    drop(root);
}

#[test]
fn reconcile_deep_chain_stays_on_heap() {
    use crate::app::test_support::reconcile_elements;
    use crate::core::tree::LayoutStatus;

    const WRAPPERS: usize = 2_048;
    let mut old = nest_chain_iterative(WRAPPERS);
    let mut new = nest_chain_iterative(WRAPPERS);
    // Same structure, different leaf text — positional zip must not recurse on the OS stack.
    {
        let mut leaf = &mut new;
        while let Some(child) = leaf.children.first_mut() {
            leaf = child;
        }
        leaf.text = Some("leaf-b".into());
    }
    let status = reconcile_elements(&mut new, &mut old);
    assert_ne!(status, LayoutStatus::StructureChanged);
    let mut leaf = &new;
    while let Some(child) = leaf.children.first() {
        leaf = child;
    }
    assert_eq!(leaf.text.as_deref(), Some("leaf-b"));
}

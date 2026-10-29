use super::*;
use crate::layout::layout_tree::LayoutTree;
use crate::primitives::r#box::r#box;
use crate::renderer::{take_capture_jobs, take_effect_jobs, EffectKind};

#[test]
fn wrap_queues_capture_and_effect() {
    let _ = take_capture_jobs();
    let _ = take_effect_jobs();
    let mut root = effect_wrap("blur")
        .strength(0.7)
        .w(120.0)
        .h(80.0)
        .child(r#box().w(60.0).h(40.0).bg([0.2, 0.5, 1.0, 1.0]))
        .into();
    root = Element::new().w(200.0).h(120.0).child(root);
    let mut tree = LayoutTree::new();
    let _ = tree.build_and_layout(&mut root, 200.0, 120.0, [-1.0, -1.0]);
    let caps = take_capture_jobs();
    assert_eq!(caps.len(), 1);
    assert!(!caps[0].instances.is_empty() || !caps[0].textured_draws.is_empty());
    let fxs = take_effect_jobs();
    assert_eq!(fxs.len(), 1);
    assert_eq!(fxs[0].kind, EffectKind::Blur);
    assert!((fxs[0].strength - 0.7).abs() < 1e-3);
}

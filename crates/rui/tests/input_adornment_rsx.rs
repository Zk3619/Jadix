//! Gallery-shaped RSX: `clearable="true"` / `password_toggle="true"` must mount and paint.

use rui::components::input::{input_value_signal_key, INPUT_FOCUS_PREFIX};
// `ui!` resolves component tags via these imports in the caller's scope.
use rui::components::{Input, InputProps};
use rui::core::reactivity::use_signal;
use rui::layout::layout_tree::LayoutTree;
use rui::renderer::font_loader;
use rui::text::install_shared_font_system;
use rui::ui;

#[test]
fn gallery_style_password_input_shows_adornment_glyphs() {
    let mut fs = font_loader::create_font_system();
    font_loader::apply_default_families(fs.db_mut());
    install_shared_font_system(&mut fs);

    let id = "gal-s2-input-pw";
    use_signal(&input_value_signal_key(id), String::new).set("secret1".into());
    use_signal("__rui_button_focus_id", String::new).set(format!("{INPUT_FOCUS_PREFIX}{id}"));

    let el = ui! {
        <Input
            id="gal-s2-input-pw"
            type="password"
            password_toggle="true"
            clearable="true"
            flex_grow="1"
        />
    };

    let clear_tid = format!("__rui_input:{id}:clear");
    let reveal_tid = format!("__rui_input:{id}:reveal");
    let tids: Vec<_> = el
        .children
        .iter()
        .map(|c| c.template_id.clone())
        .collect();
    assert!(
        el.children
            .iter()
            .any(|c| c.template_id.as_deref() == Some(clear_tid.as_str())),
        "missing clear btn; children tids={tids:?}"
    );
    assert!(
        el.children
            .iter()
            .any(|c| c.template_id.as_deref() == Some(reveal_tid.as_str())),
        "missing reveal btn; children tids={tids:?}"
    );

    let clear_op = el
        .children
        .iter()
        .find(|c| c.template_id.as_deref() == Some(clear_tid.as_str()))
        .map(|c| c.style.opacity);
    let eye_op = el
        .children
        .iter()
        .find(|c| c.template_id.as_deref() == Some(reveal_tid.as_str()))
        .map(|c| c.style.opacity);
    assert_eq!(clear_op, Some(1.0), "focused+nonempty clear opacity");
    assert_eq!(eye_op, Some(1.0), "focused eye opacity");

    let mut tree = LayoutTree::new();
    let mut root = rui::core::tree::Element::new().w(400.0).h(80.0).child(el);
    let result = tree.build_and_layout(&mut root, 400.0, 80.0, [-1.0, -1.0]);
    let lucide: Vec<_> = result
        .text_nodes
        .iter()
        .filter(|n| n.style.font_family.as_deref() == Some("lucide"))
        .collect();
    assert!(
        lucide.len() >= 2,
        "expected ≥2 lucide adornments, got {} (text_nodes={})",
        lucide.len(),
        result.text_nodes.len()
    );
    for n in &lucide {
        assert!(
            n.style.color[3] > 0,
            "invisible lucide alpha: {:?}",
            n.style.color
        );
        assert!(
            n.width > 0.5 && n.height > 0.5,
            "zero-size lucide w={} h={}",
            n.width,
            n.height
        );
    }
}

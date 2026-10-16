//! Gallery-shaped RSX: `clearable="true"` / `password_toggle="true"` must mount and paint.

use jadix::components::input::{input_value_signal_key, INPUT_FOCUS_PREFIX};
// `ui!` resolves component tags via these imports in the caller's scope.
use jadix::components::{Input, InputProps};
use jadix::core::reactivity::use_signal;
use jadix::layout::layout_tree::LayoutTree;
use jadix::renderer::font_loader;
use jadix::text::install_shared_font_system;
use jadix::ui;

#[test]
fn gallery_style_password_input_shows_adornment_glyphs() {
    let mut fs = font_loader::create_font_system();
    font_loader::apply_default_families(fs.db_mut());
    install_shared_font_system(&mut fs);

    let id = "gal-s2-input-pw";
    use_signal(&input_value_signal_key(id), String::new).set("secret1".into());
    use_signal("__jadix_button_focus_id", String::new).set(format!("{INPUT_FOCUS_PREFIX}{id}"));

    let el = ui! {
        <Input
            id="gal-s2-input-pw"
            type="password"
            password_toggle="true"
            clearable="true"
            flex_grow="1"
        />
    };

    let clear_tid = format!("__jadix_input:{id}:clear");
    let reveal_tid = format!("__jadix_input:{id}:reveal");
    let tids: Vec<_> = el.children.iter().map(|c| c.template_id.clone()).collect();
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
    let mut root = jadix::core::tree::Element::new().w(400.0).h(80.0).child(el);
    let result = tree.build_and_layout(&mut root, 400.0, 80.0, [-1.0, -1.0]);
    for tid in [&clear_tid, &reveal_tid] {
        let button = root.children[0]
            .children
            .iter()
            .find(|c| c.template_id.as_deref() == Some(tid.as_str()))
            .unwrap();
        let [x, y, w, h] = button.layout_bounds;
        let inside = |rect: [f32; 4]| {
            rect[2] > 0.5
                && rect[3] > 0.5
                && rect[0] >= x - 0.5
                && rect[1] >= y - 0.5
                && rect[0] + rect[2] <= x + w + 0.5
                && rect[1] + rect[3] <= y + h + 0.5
        };
        // SVG-enabled builds paint named Lucide icons as textures; glyphs
        // remain the fallback. Require visible paint in each button.
        let glyph = result.text_nodes.iter().any(|n| {
            n.style.font_family.as_deref() == Some("lucide")
                && n.style.color_resolved()[3] > 0
                && inside([n.x, n.y, n.width, n.height])
        });
        let texture = result.textured_draws.iter().any(|n| {
            n.texture_key.starts_with("jadix_lucide_") && n.opacity > 0.01 && inside(n.rect)
        });
        assert!(glyph || texture, "missing visible adornment paint: {tid}");
    }
}

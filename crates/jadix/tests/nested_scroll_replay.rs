use jadix::core::reactivity::use_signal;
use jadix::core::scroll_viewport::{scroll_offset_y_signal_key, SCROLL_VIEWPORT_FOCUS_PREFIX};
use jadix::core::tree::Element;
use jadix::layout::{layout_tree::RenderResult, LayoutTree};

fn viewport(id: &str, height: f32, child: Element) -> Element {
    let mut el = Element::new()
        .w(300.0)
        .h(height)
        .flex_shrink(0.0)
        .overflow_y("scroll")
        .overflow_x("hidden")
        .child(child);
    el.focus_id = Some(format!("{SCROLL_VIEWPORT_FOCUS_PREFIX}{id}"));
    use_signal(&scroll_offset_y_signal_key(id), || 0.0_f32).set_silent(0.0);
    el
}

fn fixture() -> Element {
    let scene = Element::new()
        .w(300.0)
        .h(1200.0)
        .children((0..40).map(|row| {
            Element::new()
                .absolute()
                .left(8.0)
                .top(row as f32 * 30.0)
                .w(280.0)
                .h(24.0)
                .text(format!("Absolute text row {row}"))
                .white_space("nowrap")
                .text_size(15.0)
                .text_line_height(24.0)
                .bg(if row % 2 == 0 { "#f6f3e9" } else { "#234f42" })
        }));
    viewport(
        "nested-replay-outer",
        240.0,
        Element::new()
            .w(300.0)
            .flex_col()
            .child(Element::new().w(300.0).h(100.0).flex_shrink(0.0))
            .child(viewport("nested-replay-inner", 180.0, scene))
            .child(Element::new().w(300.0).h(600.0).flex_shrink(0.0)),
    )
}

fn text_output(r: &RenderResult) -> Vec<(String, [f32; 4], Option<[f32; 4]>)> {
    r.text_nodes
        .iter()
        .map(|n| (n.text.clone(), [n.x, n.y, n.width, n.height], n.clip_bounds))
        .collect()
}

#[test]
fn nested_scroll_replay_matches_fresh_collect() {
    let mut root = fixture();
    let mut tree = LayoutTree::new();
    tree.build_and_layout(&mut root, 300.0, 240.0, [-1.0; 2]);
    tree.set_collect_skip_allowed(true);
    tree.build_and_layout_fast(&mut root, 300.0, 240.0, [-1.0; 2], true, false);
    for (frame, (outer, inner)) in [
        (0.0, 0.0),
        (8.0, 0.0),
        (16.0, 0.0),
        (45.0, 0.0),
        (45.0, 25.0),
        (60.0, 25.0),
        (120.0, 25.0),
        (175.0, 25.0),
        (200.0, 300.0),
        (180.0, 300.0),
        (130.0, 300.0),
        (10.0, 300.0),
        (0.0, 0.0),
    ]
    .into_iter()
    .enumerate()
    {
        root.scroll_offset[1] = outer;
        root.children[0].children[1].scroll_offset[1] = inner;
        use_signal(&scroll_offset_y_signal_key("nested-replay-outer"), || {
            0.0_f32
        })
        .set_silent(outer);
        use_signal(&scroll_offset_y_signal_key("nested-replay-inner"), || {
            0.0_f32
        })
        .set_silent(inner);
        let retained = tree.build_and_layout_fast(&mut root, 300.0, 240.0, [-1.0; 2], true, false);
        let mut fresh_root = fixture();
        fresh_root.scroll_offset[1] = outer;
        fresh_root.children[0].children[1].scroll_offset[1] = inner;
        use_signal(&scroll_offset_y_signal_key("nested-replay-outer"), || {
            0.0_f32
        })
        .set_silent(outer);
        use_signal(&scroll_offset_y_signal_key("nested-replay-inner"), || {
            0.0_f32
        })
        .set_silent(inner);
        let fresh = LayoutTree::new().build_and_layout(&mut fresh_root, 300.0, 240.0, [-1.0; 2]);
        assert_eq!(
            text_output(&retained),
            text_output(&fresh),
            "text/clip frame {frame}: outer={outer}, inner={inner}"
        );
        assert_eq!(
            bytemuck::cast_slice::<_, u8>(&retained.instances),
            bytemuck::cast_slice::<_, u8>(&fresh.instances),
            "rect frame {frame}"
        );
    }
}

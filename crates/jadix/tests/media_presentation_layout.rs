use jadix::{core::reactivity::create_root, prelude::*};

fn viewport(root: &Element) -> Option<&Element> {
    if root.attrs.get("data-slot").map(String::as_str) == Some("media-viewport") {
        return Some(root);
    }
    root.children.iter().find_map(viewport)
}

fn describe(root: &Element, depth: usize, output: &mut String) {
    output.push_str(&format!(
        "{}{:?} {:?} grow={} basis={:?} wrap={:?}\n",
        " ".repeat(depth),
        root.attrs.get("data-slot"),
        root.layout_bounds,
        root.style.flex_grow,
        root.style.flex_basis,
        root.style.flex_wrap
    ));
    for child in &root.children {
        describe(child, depth + 1, output);
    }
}

#[test]
fn expanded_default_layout_fills_available_height_with_or_without_sidebar() {
    let (_owner, ()) = create_root(|| {
        for (sidebar, portal) in [(false, false), (true, false), (false, true), (true, true)] {
            let presentation = MediaPresentationController::default();
            presentation.set_layout(MediaLayoutMode::WindowFullscreen);
            let slots = if sidebar {
                MediaPlayerSlots::new()
                    .replace(MediaPlayerSlot::Sidebar, || Element::new().text("Sidebar"))
            } else {
                MediaPlayerSlots::default()
            };
            let slots = slots
                .style(MediaPlayerSlot::Viewport, |slot| {
                    slot.class("hover:bg-card")
                })
                .style(MediaPlayerSlot::Sidebar, |slot| slot.class("hover:bg-card"));
            let mut player: Element = video_player()
                .presentation(presentation)
                .slots(slots)
                .style(|base| base.h(600.0))
                .into();
            if portal {
                let content: Element = vstack()
                    .flex_grow(1.0)
                    .flex_basis(0.0)
                    .class("w-full min-w-0 min-h-0 gap-3")
                    .children(std::mem::take(&mut player.children))
                    .into();
                let close: Element = button("")
                    .size("sm")
                    .child(Element::new().text("Exit fullscreen"))
                    .into();
                let mut panel: Element = vstack().class("absolute inset-0 w-full h-full min-w-0 min-h-0 gap-2 bg-card p-3 text-card-foreground overflow-hidden").child(close).child(content).into();
                panel.set_attr("jadix_portal", "1");
                player = vstack().child(panel).into();
            }
            let mut layout = jadix::core::tree::LayoutTree::new();
            let mut impact = jadix::core::tree::LayoutStatus::Clean;
            jadix::style::sync_class_variants_subtree(
                &mut player,
                [-1.0; 2],
                "",
                false,
                900.0,
                600.0,
                &mut layout,
                &mut impact,
            );
            layout.build_and_layout(&mut player, 900.0, 600.0, [-1.0; 2]);
            let surface = viewport(&player).unwrap();
            let mut detail = String::new();
            describe(&player, 0, &mut detail);
            assert!(surface.layout_bounds[2] > 800.0, "{detail}");
            assert!(
                surface.layout_bounds[3] > 100.0 && surface.layout_bounds[3] < 600.0,
                "{detail}"
            );
        }
    });
}

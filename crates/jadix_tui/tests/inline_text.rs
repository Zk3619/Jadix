use jadix_core::presentation::{
    style::{InlineStyle, NodeStyle},
    *,
};
use jadix_tui::{
    ratatui::{
        buffer::Buffer,
        layout::Rect as Area,
        style::{Color, Modifier},
        widgets::Widget,
    },
    CellSurface,
};
#[test]
fn mixed_unicode_styles_wrap_and_invalid_ranges_preserve_active_surface() {
    assert!(serde_json::to_value(NodeStyle::default())
        .unwrap()
        .get("inline_styles")
        .is_none());
    let life = RootLifetime::new(RootId::new());
    let id = NodeId::new(life.id());
    let mut surface = CellSurface::new(life, [1., 1.]).unwrap();
    let mut node = PresentationNode {
        id,
        descriptor: NodeDescriptor {
            content: NodeContent::Text("A中文🦀Z".into()),
            frame: Rect {
                x: 0.,
                y: 0.,
                width: 6.,
                height: 3.,
            },
            accessible_description: None,
            accessible_name: None,
            accessible_role: None,
            accessible_expanded: None,
            interaction: Default::default(),
            keyboard: Default::default(),
            navigation_keys: Vec::new(),
            accessibility: Default::default(),
            accessibility_relations: Default::default(),
            accessible_selected: None,
            style: NodeStyle {
                foreground: [255; 4],
                inline_styles: vec![
                    InlineStyle {
                        range: [1, 7],
                        foreground: Some([255, 0, 0, 255]),
                        italic: Some(true),
                        underline: Some(true),
                        ..Default::default()
                    },
                    InlineStyle {
                        range: [7, 11],
                        bold: Some(true),
                        strikethrough: Some(true),
                        ..Default::default()
                    },
                ],
                ..Default::default()
            },
        },
        children: vec![],
    };
    surface.present(0, vec![node.clone()]).unwrap();
    let mut buffer = Buffer::empty(Area::new(0, 0, 6, 3));
    (&mut surface).render(buffer.area, &mut buffer);
    assert_eq!(buffer[(1, 0)].fg, Color::Rgb(255, 0, 0));
    assert!(buffer[(1, 0)]
        .modifier
        .contains(Modifier::ITALIC | Modifier::UNDERLINED));
    assert_eq!(buffer[(0, 1)].symbol(), "🦀");
    assert!(buffer[(0, 1)]
        .modifier
        .contains(Modifier::BOLD | Modifier::CROSSED_OUT));
    node.descriptor.style.inline_styles[0].range[0] = 2;
    assert!(surface.present(surface.revision(), vec![node]).is_err());
    assert_eq!(surface.nodes().count(), 1);
}

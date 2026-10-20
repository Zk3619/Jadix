// Preserve the former vendor regression suite while using upstream Taffy.
use taffy::geometry::Point;
use taffy::prelude::*;
use taffy::style::Overflow;

fn layout(tree: &mut TaffyTree<()>, root: NodeId) -> usize {
    let mut calls = 0;
    tree.compute_layout_with_measure(
        root,
        Size {
            width: AvailableSpace::Definite(400.0),
            height: AvailableSpace::Definite(300.0),
        },
        |input, _, _, style| {
            taffy::compute_leaf_layout(
                input,
                style,
                |_, _| 0.0,
                |known, available| {
                    calls += 1;
                    let width = known.width.unwrap_or(match available.width {
                        AvailableSpace::MinContent => 12.0,
                        AvailableSpace::Definite(w) => w.min(83.0),
                        AvailableSpace::MaxContent => 83.0,
                    });
                    Size {
                        width,
                        height: known
                            .height
                            .unwrap_or(if width < 60.0 { 34.0 } else { 17.0 }),
                    }
                },
            )
        },
    )
    .unwrap();
    calls
}

#[test]
fn leaf_percentage_and_explicit_constraints_survive_parent_resize() {
    for display in [Display::Flex, Display::Grid, Display::Block] {
        for variant in 0..14 {
            let mut tree = TaffyTree::<()>::new();
            tree.disable_rounding();
            let mut style = relative_style();
            match variant {
                1 => style.size.width = Dimension::percent(0.5),
                2 => style.size.height = Dimension::percent(0.5),
                3 => style.min_size.width = LengthPercentageAuto::percent(0.8),
                4 => style.min_size.height = LengthPercentageAuto::percent(0.8),
                5 => style.max_size.width = LengthPercentageAuto::percent(0.1),
                6 => style.max_size.height = LengthPercentageAuto::percent(0.1),
                7 => style.padding.top = LengthPercentage::percent(0.1),
                8 => style.padding.left = LengthPercentage::percent(0.1),
                9 => style.margin.bottom = LengthPercentageAuto::percent(0.1),
                10 => style.border.right = LengthPercentage::percent(0.1),
                11 => style.aspect_ratio = Some(1.5),
                12 => style.size.width = Dimension::length(90.0),
                13 => style.min_size.height = LengthPercentageAuto::length(50.0),
                _ => {}
            }
            let leaf = tree.new_leaf(style).unwrap();
            let mut parent_style = Style {
                display,
                size: Size {
                    width: Dimension::length(400.0),
                    height: Dimension::length(300.0),
                },
                overflow: Point {
                    x: Overflow::Hidden,
                    y: Overflow::Hidden,
                },
                ..relative_style()
            };
            let root = tree
                .new_with_children(parent_style.clone(), &[leaf])
                .unwrap();
            layout(&mut tree, root);
            parent_style.size = Size {
                width: Dimension::length(160.0),
                height: Dimension::length(100.0),
            };
            tree.set_style(root, parent_style).unwrap();
            layout(&mut tree, root);
            let reused = *tree.layout(leaf).unwrap();
            tree.mark_dirty(leaf).unwrap();
            layout(&mut tree, root);
            assert_eq!(
                tree.layout(leaf).unwrap(),
                &reused,
                "{display:?}, variant {variant}"
            );
        }
    }
}

#[test]
fn intrinsic_flex_container_keeps_percentage_descendants_correct() {
    for variant in 0..4 {
        let mut tree = TaffyTree::<()>::new();
        tree.disable_rounding();
        let mut child_style = relative_style();
        match variant {
            0 => child_style.size.width = Dimension::percent(0.5),
            1 => child_style.padding.top = LengthPercentage::percent(0.2),
            2 => child_style.min_size.height = LengthPercentageAuto::percent(0.5),
            _ => child_style.margin.right = LengthPercentageAuto::percent(0.1),
        }
        let leaf = tree.new_leaf(child_style).unwrap();
        let intrinsic = tree.new_with_children(relative_style(), &[leaf]).unwrap();
        let mut style = Style {
            size: Size {
                width: Dimension::length(400.0),
                height: Dimension::length(300.0),
            },
            ..relative_style()
        };
        let root = tree.new_with_children(style.clone(), &[intrinsic]).unwrap();
        layout(&mut tree, root);
        style.size.width = Dimension::length(160.0);
        tree.set_style(root, style).unwrap();
        layout(&mut tree, root);
        let reused = (
            *tree.layout(leaf).unwrap(),
            *tree.layout(intrinsic).unwrap(),
        );
        tree.mark_dirty(leaf).unwrap();
        layout(&mut tree, root);
        assert_eq!(
            (
                *tree.layout(leaf).unwrap(),
                *tree.layout(intrinsic).unwrap()
            ),
            reused,
            "percentage descendant variant {variant}"
        );
    }
}

#[test]
fn moving_absolute_child_keeps_flow_measurements() {
    let zero = false;
    let fixed = true;
    let mut t: TaffyTree<()> = TaffyTree::new();
    t.disable_rounding();
    let mut children = Vec::new();
    for _ in 0..100 {
        let leaf = t
            .new_leaf(Style {
                size: Size {
                    width: Dimension::percent(1.0),
                    height: Dimension::auto(),
                },
                min_size: Size {
                    width: LengthPercentageAuto::length(0.0),
                    height: LengthPercentageAuto::length(16.0),
                },
                ..relative_style()
            })
            .unwrap();
        let row = t
            .new_with_children(
                Style {
                    min_size: Size {
                        width: LengthPercentageAuto::auto(),
                        height: LengthPercentageAuto::length(20.0),
                    },
                    flex_shrink: 0.0,
                    ..relative_style()
                },
                &[leaf],
            )
            .unwrap();
        children.push(row);
    }
    let marker = t
        .new_leaf(Style {
            position: Position::Absolute,
            size: Size {
                width: Dimension::length(20.0),
                height: Dimension::length(20.0),
            },
            ..relative_style()
        })
        .unwrap();
    children.push(marker);
    let content = t
        .new_with_children(
            Style {
                flex_direction: FlexDirection::Column,
                flex_shrink: 0.0,
                ..relative_style()
            },
            &children,
        )
        .unwrap();
    let vp = t
        .new_with_children(
            Style {
                size: Size {
                    width: Dimension::percent(1.0),
                    height: if zero {
                        Dimension::percent(1.0)
                    } else {
                        Dimension::auto()
                    },
                },
                min_size: Size {
                    width: LengthPercentageAuto::length(0.0),
                    height: LengthPercentageAuto::length(0.0),
                },
                flex_direction: FlexDirection::Column,
                flex_grow: 1.0,
                flex_basis: if zero {
                    Dimension::percent(0.0)
                } else {
                    Dimension::auto()
                },
                overflow: Point {
                    x: Overflow::Hidden,
                    y: Overflow::Scroll,
                },
                ..relative_style()
            },
            &[content],
        )
        .unwrap();
    let outer = t
        .new_with_children(
            Style {
                size: Size {
                    width: Dimension::percent(1.0),
                    height: if fixed {
                        Dimension::percent(1.0)
                    } else {
                        Dimension::auto()
                    },
                },
                flex_direction: FlexDirection::Column,
                flex_grow: 1.0,
                min_size: Size {
                    width: LengthPercentageAuto::length(0.0),
                    height: LengthPercentageAuto::length(0.0),
                },
                ..relative_style()
            },
            &[vp],
        )
        .unwrap();
    let root = t
        .new_with_children(
            Style {
                size: Size {
                    width: Dimension::length(400.0),
                    height: if fixed {
                        Dimension::length(300.0)
                    } else {
                        Dimension::auto()
                    },
                },
                flex_direction: FlexDirection::Column,
                ..relative_style()
            },
            &[outer],
        )
        .unwrap();
    let available = Size {
        width: AvailableSpace::Definite(400.0),
        height: if fixed {
            AvailableSpace::Definite(300.0)
        } else {
            AvailableSpace::MaxContent
        },
    };
    let mut calls = 0;
    t.compute_layout_with_measure(root, available, |input, _, _, style| {
        taffy::compute_leaf_layout(
            input,
            style,
            |_, _| 0.0,
            |k, _| {
                calls += 1;
                Size {
                    width: k.width.unwrap_or(80.0),
                    height: k.height.unwrap_or(20.0),
                }
            },
        )
    })
    .unwrap();
    let first = calls;
    let mut style = t.style(marker).unwrap().clone();
    style.inset.left = LengthPercentageAuto::length(30.0);
    t.set_style(marker, style).unwrap();
    calls = 0;

    t.compute_layout_with_measure(root, available, |input, _, _, style| {
        taffy::compute_leaf_layout(
            input,
            style,
            |_, _| 0.0,
            |k, _| {
                calls += 1;
                Size {
                    width: k.width.unwrap_or(80.0),
                    height: k.height.unwrap_or(20.0),
                }
            },
        )
    })
    .unwrap();
    assert!(calls <= 1, "absolute relocation remeasured flow: {calls}");
    println!(
        "zero={zero} fixed={fixed} first={first} again={calls} vp={:?} content={:?}",
        t.layout(vp).unwrap().size,
        t.layout(content).unwrap().size
    );
}

#[test]
fn outflow_dirty_matches_fresh_layout_across_flow_and_overflow_changes() {
    for display in [Display::Flex, Display::Grid, Display::Block] {
        for rounding in [false, true] {
            let mut tree = TaffyTree::<()>::new();
            if !rounding {
                tree.disable_rounding();
            }
            let flow = tree
                .new_leaf(Style {
                    size: Size {
                        width: Dimension::percent(0.6),
                        height: Dimension::length(35.0),
                    },
                    ..relative_style()
                })
                .unwrap();
            let text = tree.new_leaf(relative_style()).unwrap();
            let overlay = tree
                .new_with_children(
                    Style {
                        position: Position::Absolute,
                        size: Size {
                            width: Dimension::length(90.0),
                            height: Dimension::auto(),
                        },
                        inset: Rect {
                            left: LengthPercentageAuto::length(10.0),
                            top: LengthPercentageAuto::length(8.0),
                            ..Rect::auto()
                        },
                        ..relative_style()
                    },
                    &[text],
                )
                .unwrap();
            let parent = tree
                .new_with_children(
                    Style {
                        display,
                        size: Size {
                            width: Dimension::percent(1.0),
                            height: Dimension::auto(),
                        },
                        ..relative_style()
                    },
                    &[flow, overlay],
                )
                .unwrap();
            let footer = tree
                .new_leaf(Style {
                    size: Size {
                        width: Dimension::length(30.0),
                        height: Dimension::length(20.0),
                    },
                    ..relative_style()
                })
                .unwrap();
            let root = tree
                .new_with_children(
                    Style {
                        flex_direction: FlexDirection::Column,
                        size: Size {
                            width: Dimension::length(400.0),
                            height: Dimension::auto(),
                        },
                        ..relative_style()
                    },
                    &[parent, footer],
                )
                .unwrap();
            let nodes = [root, parent, flow, overlay, text, footer];
            layout(&mut tree, root);
            for step in 0..12 {
                let mut style = tree.style(overlay).unwrap().clone();
                match step {
                    0 => style.inset.left = LengthPercentageAuto::length(500.0),
                    1 => style.size.width = Dimension::percent(0.3),
                    2 => style.position = Position::Relative,
                    3 => style.position = Position::Absolute,
                    4 => style.display = Display::None,
                    5 => style.display = Display::Flex,
                    6 => style.padding.top = LengthPercentage::percent(0.1),
                    7 => style.inset.left = LengthPercentageAuto::length(-80.0),
                    8 => style.size.height = Dimension::length(300.0),
                    9 => style.position = Position::Fixed,
                    10 => style.inset.left = LengthPercentageAuto::length(80.0),
                    _ => style.position = Position::Relative,
                }
                tree.set_style(overlay, style).unwrap();
                if step == 6 {
                    let mut style = tree.style(flow).unwrap().clone();
                    style.size.height = Dimension::length(62.0);
                    tree.set_style(flow, style).unwrap();
                }
                if step == 8 {
                    tree.set_style(
                        text,
                        Style {
                            size: Size {
                                width: Dimension::length(120.0),
                                height: Dimension::length(90.0),
                            },
                            ..relative_style()
                        },
                    )
                    .unwrap();
                }
                assert!(
                    tree.dirty(root).unwrap(),
                    "root must still recompute placement/overflow"
                );
                let mut fresh = tree.clone();
                for node in nodes {
                    fresh.mark_dirty(node).unwrap();
                }
                layout(&mut tree, root);
                layout(&mut fresh, root);
                for node in nodes {
                    assert_eq!(
                        tree.layout(node).unwrap(),
                        fresh.layout(node).unwrap(),
                        "{display:?}, rounding={rounding}, step={step}, node={node:?}"
                    );
                }
            }
        }
    }
}

fn relative_style() -> Style {
    Style {
        position: Position::Relative,
        ..Style::default()
    }
}

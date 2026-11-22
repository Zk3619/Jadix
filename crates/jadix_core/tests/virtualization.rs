use jadix_core::virtualization::*;

#[test]
fn load_edges_use_logical_thresholds_and_rearm_only_on_approach_or_data_changes() {
    for total in [0.0_f64, 40.0, 100.0, 2000.0, 1e30] {
        for viewport in [0.0_f64, 20.0, 100.0, 4000.0] {
            for requested in [0.0_f64, 5.0, 25.0, 50.0, 51.0, 1000.0, 1e31] {
                for order in [ListOrder::Forward, ListOrder::Reversed] {
                    for thresholds in [[0.0, 0.0], [0.25, 0.5], [2.0, 0.0], [f64::MAX; 2]] {
                        let near = ListLoadEdges::new(
                            requested,
                            viewport,
                            total,
                            thresholds,
                            total > 0.0,
                            order,
                        )
                        .unwrap();
                        let offset = requested.min((total - viewport).max(0.0));
                        let physical_start = offset;
                        let physical_end = (total - viewport - offset).max(0.0);
                        let (start, end) = if order == ListOrder::Forward {
                            (physical_start, physical_end)
                        } else {
                            (physical_end, physical_start)
                        };
                        assert_eq!(
                            near.start,
                            viewport > 0.0 && total > 0.0 && start <= viewport * thresholds[0]
                        );
                        assert_eq!(near.end, viewport > 0.0 && end <= viewport * thresholds[1]);
                        assert_eq!(near.entered(ListLoadEdges::default(), false), near);
                        assert_eq!(near.entered(near, false), ListLoadEdges::default());
                        assert_eq!(near.entered(near, true), near);
                    }
                }
            }
        }
    }
    let both = ListLoadEdges {
        start: true,
        end: true,
    };
    let end = ListLoadEdges {
        start: false,
        end: true,
    };
    assert_eq!(
        both.entered(end, false),
        ListLoadEdges {
            start: true,
            end: false
        }
    );
    assert_eq!(
        ListLoadEdges::new(0.0, 100.0, 0.0, [1.0; 2], false, ListOrder::Forward),
        Ok(end)
    );
    for invalid in [-1.0, f64::NAN, f64::INFINITY] {
        for field in 0..5 {
            let mut values = [0.0, 100.0, 2000.0, 0.25, 0.5];
            values[field] = invalid;
            assert_eq!(
                ListLoadEdges::new(
                    values[0],
                    values[1],
                    values[2],
                    [values[3], values[4]],
                    true,
                    ListOrder::Forward
                ),
                Err(VirtualError::InvalidExtent)
            );
        }
    }
}

#[test]
fn surrounding_content_projects_fixed_and_measured_rows_without_changing_row_coordinates() {
    let fixed = UniformList::new(4, 20.0, 5.0).unwrap();
    let mut measured = VirtualAxis::new(0..4, 20.0).unwrap();
    measured.set_gap(5.0).unwrap();
    let content = ListContent::new(
        fixed.total(),
        [120.0, 130.0],
        100.0,
        ListContentAlign::Start,
    )
    .unwrap();
    assert_eq!(content.total(), 345.0);
    assert_eq!(content.max_offset(100.0), Ok(245.0));
    for (offset, visible) in [
        (0.0, 0..0),
        (30.0, 0..1),
        (120.0, 0..4),
        (150.0, 1..4),
        (220.0, 4..4),
        (999.0, 4..4),
    ] {
        let a = content
            .viewport(offset, 100.0, |o, v| fixed.viewport(o, v))
            .unwrap();
        let b = content
            .viewport(offset, 100.0, |o, v| measured.viewport_geometry(o, v))
            .unwrap();
        assert_eq!(a, b);
        assert_eq!(a.visible, visible);
        assert_eq!(a.total_extent, 345.0);
        assert_eq!(a.offset, offset.min(245.0));
        assert_eq!(a.viewport_extent, 100.0);
    }
    for (offset, anchor) in [
        (0.0, None),
        (119.0, None),
        (120.0, Some(0.0)),
        (214.0, Some(94.0)),
        (215.0, None),
    ] {
        assert_eq!(content.anchor_offset(offset), Ok(anchor));
    }
    for (align, expected) in [
        (ListScrollAlign::Start, 170.0),
        (ListScrollAlign::Center, 130.0),
        (ListScrollAlign::End, 90.0),
        (ListScrollAlign::Nearest, 90.0),
    ] {
        assert_eq!(
            content.scroll_to(50.0, 20.0, 0.0, 100.0, align, 0.0),
            Ok(expected)
        );
    }
    assert_eq!(content.restore(50.0, 75.0, 7.0, 100.0), Ok(177.0));
    assert!(content.restore(50.0, 75.0, 30.0, 100.0).unwrap() < 195.0);
    let partial = content
        .viewport(30.0, 100.0, |o, v| fixed.viewport(o, v))
        .unwrap();
    assert_eq!(
        content.viewable_items(&partial, ListViewability::ItemFraction(0.5), |i| (
            i as f64 * 25.0,
            i as f64 * 25.0 + 20.0
        )),
        Ok(0..1)
    );
    assert!(
        content
            .viewable_items(&partial, ListViewability::ViewportFraction(0.5), |i| (
                i as f64 * 25.0,
                i as f64 * 25.0 + 20.0
            ))
            .unwrap()
            .is_empty()
    );
    let headers = ListStickyHeaders::new([0, 2]);
    assert_eq!(
        content.sticky_header(&partial, &headers, 0.0, |i| (i as f64 * 25.0, 20.0)),
        Ok(None)
    );
    let footer = content
        .viewport(245.0, 100.0, |o, v| fixed.viewport(o, v))
        .unwrap();
    assert_eq!(
        content.sticky_header(&footer, &headers, 0.0, |i| (i as f64 * 25.0, 20.0)),
        Ok(Some(ListStickyHeader {
            index: 2,
            translation: 25.0
        }))
    );
    for (align, insets) in [
        (ListContentAlign::Start, [10.0, 20.0]),
        (ListContentAlign::Center, [35.0, 45.0]),
        (ListContentAlign::End, [60.0, 20.0]),
    ] {
        let short = ListContent::new(20.0, [10.0, 20.0], 100.0, align).unwrap();
        assert_eq!(short.insets(), insets);
        assert_eq!(short.max_offset(100.0), Ok(0.0));
    }
    assert!(
        content
            .viewport(120.0, 0.0, |o, v| fixed.viewport(o, v))
            .unwrap()
            .visible
            .is_empty()
    );
    for invalid in [f64::NAN, f64::INFINITY, -1.0] {
        assert!(ListContent::new(20.0, [invalid, 0.0], 100.0, ListContentAlign::Start).is_err());
        assert!(
            content
                .viewport(invalid, 100.0, |o, v| fixed.viewport(o, v))
                .is_err()
        );
        assert!(content.max_offset(invalid).is_err());
    }
    assert!(ListContent::new(f64::MAX, [f64::MAX, 0.0], 100.0, ListContentAlign::Start).is_err());
    assert!(
        content
            .scroll_to(90.0, 20.0, 0.0, 100.0, ListScrollAlign::Start, 0.0)
            .is_err()
    );
}

#[test]
fn grid_prediction_preserves_visibility_and_budget_including_partial_wrap_rows() {
    let grid = UniformGrid::new(10_000, 10_000, [20.0; 2], [5.0; 2]).unwrap();
    let base = grid.window([1050.0; 2], [100.0; 2], [1; 2], 36).unwrap();
    assert_eq!((base.rows.clone(), base.columns.clone()), (41..47, 41..47));
    for (ahead, rows, columns) in [
        ([100.0; 2], 41..50, 41..50),
        ([-100.0; 2], 38..47, 38..47),
        ([100.0, -100.0], 38..47, 41..50),
        ([f64::MAX; 2], 41..50, 41..50),
        ([-f64::MAX; 2], 38..47, 38..47),
        ([0.0; 2], 41..47, 41..47),
    ] {
        let next = grid
            .window_with_scroll_ahead([1050.0; 2], [100.0; 2], [1; 2], 81, ahead)
            .unwrap();
        assert_eq!(next.viewport, base.viewport);
        assert_eq!((next.rows, next.columns), (rows, columns));
        assert_eq!(
            grid.window_with_scroll_ahead([1050.0; 2], [100.0; 2], [1; 2], 36, ahead)
                .unwrap(),
            base
        );
    }
    assert_eq!(
        grid.window_with_scroll_ahead([1050.0; 2], [100.0; 2], [1; 2], 35, [100.0; 2]),
        Err(VirtualError::Capacity)
    );
    let wrap = UniformWrap::new(43, 95.0, 20.0, 20.0, [5.0; 2]).unwrap();
    let base = wrap.window(175.0, 50.0, 1, 16).unwrap();
    assert_eq!(base.rows, 6..10);
    let predicted = wrap
        .window_with_scroll_ahead(175.0, 50.0, 1, 19, 50.0)
        .unwrap();
    assert_eq!(predicted.viewport, base.viewport);
    assert_eq!((predicted.rows, predicted.columns), (6..11, 0..4));
    assert_eq!(
        wrap.window_with_scroll_ahead(175.0, 50.0, 1, 18, 50.0)
            .unwrap(),
        base
    );
    assert_eq!(
        wrap.window_with_scroll_ahead(175.0, 50.0, 1, 15, 50.0),
        Err(VirtualError::Capacity)
    );

    // A short or hidden viewport must never grow behind the actual content or
    // turn a zero-dimensional viewport into mounted cells.
    let short = UniformGrid::new(2, 2, [10.0; 2], [0.0; 2]).unwrap();
    for size in [[f64::MAX; 2], [0.0, 100.0], [100.0, 0.0]] {
        assert_eq!(
            short
                .window_with_scroll_ahead([f64::MAX; 2], size, [0; 2], 4, [f64::MAX; 2])
                .unwrap(),
            short.window([f64::MAX; 2], size, [0; 2], 4).unwrap()
        );
    }
    assert!(
        wrap.window_with_scroll_ahead(100.0, 0.0, 1, 0, 50.0)
            .unwrap()
            .rows
            .is_empty()
    );
    for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        for ahead in [[bad, 0.0], [0.0, bad]] {
            assert_eq!(
                grid.window_with_scroll_ahead([0.0; 2], [100.0; 2], [0; 2], 100, ahead),
                Err(VirtualError::InvalidExtent)
            );
        }
        assert_eq!(
            wrap.window_with_scroll_ahead(0.0, 50.0, 0, 100, bad),
            Err(VirtualError::InvalidExtent)
        );
    }
    // The rectangular product can overflow usize without any per-cell allocation.
    let huge = UniformGrid::new(usize::MAX, usize::MAX, [1.0; 2], [0.0; 2]).unwrap();
    let near = (usize::MAX as f64).sqrt() * 0.75;
    let base = huge
        .window([0.0; 2], [near; 2], [0; 2], usize::MAX)
        .unwrap();
    assert_eq!(
        huge.window_with_scroll_ahead([0.0; 2], [near; 2], [0; 2], usize::MAX, [near; 2])
            .unwrap(),
        base
    );
}

#[test]
fn scroll_ahead_is_directional_bounded_and_rebases_without_idle_churn() {
    let mut ahead = ListScrollAhead::default();
    for (offset, viewport, lead) in [
        (100.0, 100.0, 0.0),
        (130.0, 100.0, 60.0),
        (135.0, 100.0, 45.0),
        (135.0, 100.0, 45.0),
        (185.0, 100.0, 100.0),
        (165.0, 100.0, -40.0),
        (10_000.0, 100.0, 0.0),
        (10_010.0, 100.0, 20.0),
        (10_015.0, 50.0, 0.0),
        (10_025.0, 50.0, 20.0),
        (10_025.0, 0.0, 0.0),
        (10_025.0, 100.0, 0.0),
    ] {
        assert_eq!(ahead.update(offset, viewport), Ok(lead));
    }
    ahead.reset();
    assert_eq!(ahead.update(50.0, 100.0), Ok(0.0));
    for invalid in [-1.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert_eq!(
            ahead.update(invalid, 100.0),
            Err(VirtualError::InvalidExtent)
        );
        assert_eq!(ahead.update(0.0, invalid), Err(VirtualError::InvalidExtent));
    }
    assert_eq!(ahead.update(60.0, 100.0), Ok(20.0));
    ahead.reset();
    assert_eq!(ahead.update(0.0, f64::MAX), Ok(0.0));
    assert_eq!(ahead.update(f64::MAX, f64::MAX), Ok(f64::MAX));
    assert_eq!(ahead.update(0.0, f64::MAX), Ok(-f64::MAX));
}

#[test]
fn list_order_projects_indices_and_half_open_ranges_without_storage_or_overflow() {
    for count in [0, 1, 7, usize::MAX] {
        for order in [ListOrder::Forward, ListOrder::Reversed] {
            assert_eq!(order.index(count, count), None);
            for index in [0, count / 2, count.saturating_sub(1)] {
                if index < count {
                    let visual = order.index(index, count).unwrap();
                    assert_eq!(order.index(visual, count), Some(index));
                    assert_eq!(
                        order.range(index..index + 1, count),
                        Some(visual..visual + 1)
                    );
                }
            }
            for range in [0..0, 0..count, count / 2..count, count..count] {
                let projected = order.range(range.clone(), count).unwrap();
                assert_eq!(projected.len(), range.len());
                assert_eq!(order.range(projected, count), Some(range));
            }
            assert_eq!(order.range(1..0, count), None);
        }
    }
    assert_eq!(ListOrder::Reversed.range(2..5, 7), Some(2..5));
    assert_eq!(ListOrder::Reversed.range(0..2, 7), Some(5..7));
    assert_eq!(ListOrder::Reversed.range(0..8, 7), None);
}

#[test]
fn short_content_alignment_validates_extents_and_only_distributes_unused_space() {
    assert_eq!(ListContentAlign::default(), ListContentAlign::Start);
    for (align, short, empty) in [
        (ListContentAlign::Start, [0.0, 0.0], [0.0, 0.0]),
        (ListContentAlign::Center, [27.5, 27.5], [50.0, 50.0]),
        (ListContentAlign::End, [55.0, 0.0], [100.0, 0.0]),
    ] {
        assert_eq!(align.padding(45.0, 100.0), Ok(short));
        assert_eq!(align.padding(0.0, 100.0), Ok(empty));
        for (content, viewport) in [(100.0, 100.0), (101.0, 100.0), (0.0, 0.0), (f64::MAX, 0.0)] {
            assert_eq!(align.padding(content, viewport), Ok([0.0, 0.0]));
        }
        assert!(
            align
                .padding(0.0, f64::MAX)
                .unwrap()
                .iter()
                .all(|x| x.is_finite())
        );
        for invalid in [-1.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert_eq!(
                align.padding(invalid, 100.0),
                Err(VirtualError::InvalidExtent)
            );
            assert_eq!(
                align.padding(45.0, invalid),
                Err(VirtualError::InvalidExtent)
            );
        }
    }
}

#[test]
fn keyed_positions_survive_mutations_and_restore_on_both_geometry_models() {
    let previous = [10, 11, 12, 13, 14];
    let saved = ListScrollPosition {
        index: 2,
        offset: 22.5,
    };
    for (next, expected) in [
        (vec![8, 9, 10, 11, 12, 13, 14], Some((4, 22.5))),
        (vec![14, 12, 11, 10, 13], Some((1, 22.5))),
        (vec![10, 11, 14], Some((2, 0.0))),
        (vec![11, 10], Some((0, 0.0))),
        (vec![30, 31], None),
        (vec![], None),
    ] {
        let remapped = saved
            .remap_keys(&previous, |key| next.iter().position(|k| k == key))
            .unwrap();
        assert_eq!(remapped.map(|p| (p.index, p.offset)), expected);
        if let Some(position) = remapped {
            let uniform = UniformList::new(next.len(), 20.0, 5.0).unwrap();
            let mut measured = VirtualAxis::new(next, 20.0).unwrap();
            measured.set_gap(5.0).unwrap();
            for viewport in [0.0, 10.0, 500.0] {
                assert_eq!(
                    uniform.restore_position(position, viewport),
                    measured.restore_position(position, viewport)
                );
            }
        }
    }
    for gap in [0.0, 5.0, 100.0] {
        let uniform = UniformList::new(5, 20.0, gap).unwrap();
        let mut measured = VirtualAxis::new(0..5, 20.0).unwrap();
        measured.set_gap(gap).unwrap();
        for offset in (0..700).map(|n| n as f64 * 0.5) {
            let position = uniform.anchor(offset).unwrap().unwrap();
            let anchor = measured.anchor(offset).unwrap().unwrap();
            assert_eq!(
                (position.index, position.offset),
                (anchor.key, anchor.within)
            );
            assert_eq!(
                uniform.restore_position(position, 10.0),
                measured.restore_anchor(&anchor, 10.0)
            );
        }
    }
    let uniform = UniformList::new(5, 3.0, 0.0).unwrap();
    assert!(uniform.restore_position(saved, 0.0).unwrap() < 9.0);
    assert_eq!(
        UniformList::new(0, 20.0, 5.0).unwrap().anchor(2.0),
        Ok(None)
    );
    assert_eq!(
        uniform.restore_position(
            ListScrollPosition {
                index: 5,
                offset: 0.0
            },
            0.0
        ),
        Err(VirtualError::UnknownKey)
    );
    for invalid in [f64::NAN, f64::INFINITY, -1.0] {
        assert_eq!(uniform.anchor(invalid), Err(VirtualError::InvalidExtent));
        assert_eq!(
            ListScrollPosition {
                offset: invalid,
                ..saved
            }
            .remap_keys(&previous, |_| Some(0)),
            Err(VirtualError::InvalidExtent)
        );
        assert_eq!(
            uniform.restore_position(saved, invalid),
            Err(VirtualError::InvalidExtent)
        );
    }
}

#[test]
fn spaced_geometry_matches_linear_row_overlap_and_alignment_oracles() {
    for count in [0, 1, 31] {
        for gap in [0.0, 8.0, 100.0] {
            let uniform = UniformList::new(count, 20.0, gap).unwrap();
            let mut axis = VirtualAxis::new(0..count, 20.0).unwrap();
            axis.set_gap(gap).unwrap();
            let total = count as f64 * 20.0 + count.saturating_sub(1) as f64 * gap;
            assert_eq!(uniform.total(), total);
            assert_eq!(axis.total(), total);
            for viewport in [0.0, 1.0, 5.0, 80.0, 10_000.0] {
                for offset in (0..400).map(|n| n as f64 * 2.5) {
                    let top = offset.min((total - viewport).max(0.0));
                    let overlap = |i: usize| {
                        let start = i as f64 * (20.0 + gap);
                        ((start + 20.0).min(top + viewport) - start.max(top)).max(0.0)
                    };
                    let expected: Vec<_> = (0..count).filter(|&i| overlap(i) > 0.0).collect();
                    let state = uniform.viewport(offset, viewport).unwrap();
                    assert_eq!(state.visible.clone().collect::<Vec<_>>(), expected);
                    assert_eq!(axis.viewport_geometry(offset, viewport).unwrap(), state);
                    assert_eq!(
                        axis.window(offset, viewport, 0.0, &[], count)
                            .unwrap()
                            .visible,
                        state.visible
                    );
                    assert_eq!(
                        uniform.scroll_position(offset, viewport),
                        axis.scroll_position(offset, viewport)
                    );
                    for policy in [
                        ListViewability::AnyVisible,
                        ListViewability::ItemFraction(0.5),
                        ListViewability::ViewportFraction(0.7),
                    ] {
                        let expected: Vec<_> = expected
                            .iter()
                            .copied()
                            .filter(|&i| {
                                let amount = overlap(i);
                                amount == 20.0
                                    || match policy {
                                        ListViewability::AnyVisible => true,
                                        ListViewability::ItemFraction(f) => amount / 20.0 >= f,
                                        ListViewability::ViewportFraction(f) => {
                                            amount / viewport >= f
                                        }
                                    }
                            })
                            .collect();
                        assert_eq!(
                            uniform
                                .viewable_items(offset, viewport, policy)
                                .unwrap()
                                .collect::<Vec<_>>(),
                            expected
                        );
                        assert_eq!(
                            axis.viewable_items(offset, viewport, policy)
                                .unwrap()
                                .collect::<Vec<_>>(),
                            expected
                        );
                    }
                }
                for index in 0..count {
                    for fraction in [0.0, 0.25, 0.5, 1.0] {
                        let expected = if viewport == 0.0 {
                            0.0
                        } else {
                            (index as f64 * (20.0 + gap) - (viewport - 20.0) * fraction - 3.0)
                                .clamp(0.0, (total - viewport).max(0.0))
                        };
                        let align = ListScrollAlign::Fraction(fraction);
                        assert_eq!(
                            uniform.scroll_to_index(index, 0.0, viewport, align, 3.0),
                            Ok(expected)
                        );
                        assert_eq!(
                            axis.scroll_to_index(index, 0.0, viewport, align, 3.0),
                            Ok(expected)
                        );
                    }
                }
            }
        }
    }
    let huge = UniformList::new(usize::MAX, 1.0, 2.0).unwrap();
    assert_eq!(huge.viewport(20.0, 5.0).unwrap().visible, 7..9);
}

#[test]
fn measured_spacing_preserves_keys_anchors_and_atomic_validation() {
    let mut axis = VirtualAxis::new(0..4, 20.0).unwrap();
    assert_eq!(axis.set_gap(10.0), Ok(true));
    assert_eq!(axis.set_gap(10.0), Ok(false));
    let anchor = axis.anchor(25.0).unwrap().unwrap();
    assert_eq!((anchor.key, anchor.within), (0, 25.0));
    assert_eq!(axis.viewport_geometry(25.0, 3.0).unwrap().visible, 1..1);
    assert_eq!(axis.scroll_position(25.0, 3.0), Ok(None));
    assert_eq!(
        axis.scroll_position(25.0, 10.0),
        Ok(Some(ListScrollPosition {
            index: 1,
            offset: 0.0
        }))
    );
    axis.measure(&1, 40.0).unwrap();
    assert_eq!(axis.position(2), Some(80.0));
    assert_eq!(axis.total(), 130.0);
    axis.append([(4, 30.0)]).unwrap();
    assert_eq!(axis.total(), 170.0);
    let mut replacement = axis.replacement([4, 0, 1], 99.0).unwrap();
    assert_eq!(replacement.total(), 110.0);
    assert_eq!(replacement.restore_anchor(&anchor, 5.0), Ok(65.0));
    assert_eq!(
        replacement.measure_preserving_scroll(&4, 50.0, 65.0, 5.0, false),
        Ok(85.0)
    );
    assert_eq!(
        replacement.measure_preserving_scroll(&1, 60.0, 125.0, 5.0, true),
        Ok(145.0)
    );
    for bad in [-1.0, f64::NAN, f64::INFINITY, f64::MAX] {
        assert_eq!(axis.set_gap(bad), Err(VirtualError::InvalidExtent));
        assert!(UniformList::new(4, 20.0, bad).is_err());
        assert_eq!(axis.total(), 170.0);
    }
    let mut overflow = VirtualAxis::new([0, 1], 1.0).unwrap();
    overflow.set_gap(f64::MAX * 0.6).unwrap();
    assert_eq!(
        overflow.append([(2, 1.0)]),
        Err(VirtualError::InvalidExtent)
    );
    assert_eq!(overflow.len(), 2);
    assert!(overflow.replacement([0, 1, 2], 1.0).is_err());
}

#[test]
fn row_navigation_matches_reference_positions_and_measured_prefixes() {
    let mut axis = VirtualAxis::new(0..100, 50.0).unwrap();
    for (index, align, extra, expected) in [
        (10, ListScrollAlign::Start, 0.0, 500.0),
        (10, ListScrollAlign::Center, 0.0, 325.0),
        (10, ListScrollAlign::End, 0.0, 150.0),
        (10, ListScrollAlign::Fraction(0.25), 12.5, 400.0),
        (99, ListScrollAlign::Start, 0.0, 4600.0),
        (0, ListScrollAlign::End, 10.0, 0.0),
    ] {
        assert_eq!(
            axis.scroll_to_index(index, 0.0, 400.0, align, extra),
            Ok(expected)
        );
        assert_eq!(
            ListViewport::uniform_scroll_to(100, 50.0, index, 0.0, 400.0, align, extra),
            Ok(expected)
        );
    }
    axis.measure(&0, 100.0).unwrap();
    axis.measure(&10, 200.0).unwrap();
    assert_eq!(
        axis.scroll_to_index(10, 0.0, 400.0, ListScrollAlign::Center, 10.0),
        Ok(440.0)
    );
    assert_eq!(
        axis.scroll_to_index(10, 560.0, 100.0, ListScrollAlign::Nearest, 0.0),
        Ok(560.0)
    );
    assert_eq!(
        axis.scroll_to_index(10, 0.0, 100.0, ListScrollAlign::Nearest, 0.0),
        Ok(550.0)
    );
    assert_eq!(
        axis.scroll_to_index(10, 120.0, 0.0, ListScrollAlign::Start, 0.0),
        Ok(120.0)
    );
    for invalid in [f64::NAN, f64::INFINITY, -0.1, 1.1] {
        assert_eq!(
            axis.scroll_to_index(10, 0.0, 400.0, ListScrollAlign::Fraction(invalid), 0.0),
            Err(VirtualError::InvalidExtent)
        );
    }
    assert_eq!(
        axis.scroll_to_index(100, 0.0, 400.0, ListScrollAlign::Start, 0.0),
        Err(VirtualError::UnknownKey)
    );
    assert_eq!(
        ListViewport::uniform_scroll_to(0, 50.0, 0, 0.0, 400.0, ListScrollAlign::Start, 0.0),
        Err(VirtualError::UnknownKey)
    );
    assert_eq!(
        axis.scroll_to_index(0, 0.0, 400.0, ListScrollAlign::Start, f64::NAN),
        Err(VirtualError::InvalidExtent)
    );
    for index in 0..100 {
        for fraction in [0.0, 0.2, 0.5, 1.0] {
            let heights: Vec<f64> = (0..100)
                .map(|i| {
                    if i == 0 {
                        100.0
                    } else if i == 10 {
                        200.0
                    } else {
                        50.0
                    }
                })
                .collect();
            let expected =
                (heights[..index].iter().sum::<f64>() - (400.0 - heights[index]) * fraction - 7.0)
                    .clamp(0.0, heights.iter().sum::<f64>() - 400.0);
            assert_eq!(
                axis.scroll_to_index(index, 0.0, 400.0, ListScrollAlign::Fraction(fraction), 7.0),
                Ok(expected)
            );
        }
    }
}

#[test]
fn viewport_geometry_matches_uniform_and_measured_half_open_windows_without_rows() {
    for count in [0, 1, 100] {
        let axis = VirtualAxis::new(0..count, 20.0).unwrap();
        for viewport in [0.0, 1.0, 80.0, 3000.0] {
            for offset in [0.0, 20.0, 205.0, 10_000.0] {
                let geometry = axis.viewport_geometry(offset, viewport).unwrap();
                assert_eq!(
                    geometry,
                    ListViewport::uniform(count, 20.0, offset, viewport).unwrap()
                );
                assert_eq!(
                    geometry.visible,
                    axis.window(offset, viewport, 0.0, &[], count)
                        .unwrap()
                        .visible
                );
            }
        }
    }
    let mut axis = VirtualAxis::new(0..5, 20.0).unwrap();
    axis.measure(&0, 60.0).unwrap();
    assert_eq!(axis.viewport_geometry(60.0, 40.0).unwrap().visible, 1..3);
    assert_eq!(axis.viewport_geometry(61.0, 40.0).unwrap().visible, 1..4);
    let geometry = axis.viewport_geometry(999.0, 40.0).unwrap();
    assert_eq!(geometry.offset, 100.0);
    assert_eq!(geometry.total_extent, 140.0);
    assert_eq!(geometry.visible, 3..5);
    for invalid in [-1.0, f64::NAN, f64::INFINITY] {
        assert_eq!(
            axis.viewport_geometry(invalid, 80.0),
            Err(VirtualError::InvalidExtent)
        );
        assert_eq!(
            axis.viewport_geometry(0.0, invalid),
            Err(VirtualError::InvalidExtent)
        );
        assert_eq!(
            ListViewport::uniform(5, 20.0, invalid, 80.0),
            Err(VirtualError::InvalidExtent)
        );
        assert_eq!(
            ListViewport::uniform(5, invalid, 0.0, 80.0),
            Err(VirtualError::InvalidExtent)
        );
    }
    assert_eq!(
        ListViewport::uniform(5, 0.0, 0.0, 80.0),
        Err(VirtualError::InvalidExtent)
    );
    assert_eq!(
        ListViewport::uniform(5, f64::MAX, 0.0, 80.0),
        Err(VirtualError::InvalidExtent)
    );
}

#[test]
fn viewport_position_and_measurement_keep_the_reading_anchor_through_shrinkage() {
    let mut axis = VirtualAxis::new(0..100, 50.0).unwrap();
    assert_eq!(
        axis.scroll_position(105.0, 100.0).unwrap(),
        Some(ListScrollPosition {
            index: 2,
            offset: 5.0
        })
    );
    assert_eq!(
        axis.window(105.0, 100.0, 50.0, &[], 8)
            .unwrap()
            .visible
            .start,
        1
    );
    let offset = axis
        .measure_preserving_scroll(&0, 20.0, 105.0, 100.0, false)
        .unwrap();
    assert_eq!(offset, 75.0);
    assert_eq!(
        axis.scroll_position(offset, 100.0).unwrap(),
        Some(ListScrollPosition {
            index: 2,
            offset: 5.0
        })
    );
    let offset = axis
        .measure_preserving_scroll(&1, 80.0, offset, 100.0, false)
        .unwrap();
    assert_eq!(offset, 105.0);
    let end = axis
        .measure_preserving_scroll(&99, 10.0, axis.total() - 100.0, 100.0, true)
        .unwrap();
    assert_eq!(end, axis.total() - 100.0);
    let total = axis.total();
    assert_eq!(
        axis.measure_preserving_scroll(&0, 10.0, f64::NAN, 100.0, false),
        Err(VirtualError::InvalidExtent)
    );
    assert_eq!(axis.total(), total);
    assert_eq!(axis.scroll_position(0.0, 0.0).unwrap(), None);
    assert_eq!(
        VirtualAxis::new(Vec::<u32>::new(), 10.0)
            .unwrap()
            .scroll_position(0.0, 100.0)
            .unwrap(),
        None
    );
}

#[test]
fn shrinking_the_anchor_row_keeps_its_key_in_the_viewport() {
    for extent in [10.0, 0.125] {
        let mut axis = VirtualAxis::new(0..100, 50.0).unwrap();
        let offset = axis
            .measure_preserving_scroll(&2, extent, 145.0, 100.0, false)
            .unwrap();
        let anchor = axis.anchor(offset).unwrap().unwrap();
        assert_eq!(anchor.key, 2);
        assert!(anchor.within >= 0.0 && anchor.within < extent);
        assert_eq!(
            axis.scroll_position(offset, 100.0).unwrap().unwrap().index,
            2
        );
        assert_eq!(
            axis.window(offset, 100.0, 0.0, &[], 8)
                .unwrap()
                .visible
                .start,
            2
        );
    }
}

#[test]
fn hundred_thousand_rows_materialize_a_bounded_window_and_pin_an_editor() {
    let axis = VirtualAxis::new(0u64..100_000, 32.0).unwrap();
    let window = axis.window(1_600_000.0, 640.0, 64.0, &[7], 32).unwrap();
    assert_eq!(window.visible, 49_998..50_022);
    assert_eq!(window.mounted.len(), 25);
    assert_eq!(window.mounted[0], 7);
    assert_eq!(window.total, 3_200_000.0);
    assert_eq!(axis.key(window.visible.start), Some(&49_998));
    assert_eq!(
        axis.window(0.0, 0.0, 0.0, &[], 0).unwrap().mounted,
        Vec::<usize>::new()
    );
    assert_eq!(
        axis.window(0.0, 640.0, 0.0, &[], 19),
        Err(VirtualError::Capacity)
    );
}

#[test]
fn measured_heights_and_reordered_keys_preserve_a_scroll_anchor() {
    let mut axis = VirtualAxis::new(["a", "b", "c", "d"], 30.0).unwrap();
    let anchor = axis.anchor(65.0).unwrap().unwrap();
    assert_eq!(
        anchor,
        ScrollAnchor {
            key: "c",
            within: 5.0
        }
    );
    axis.measure(&"a", 80.0).unwrap();
    assert_eq!(axis.restore_anchor(&anchor, 30.0).unwrap(), 115.0);
    axis.replace(["b", "a", "c", "d"], 30.0).unwrap();
    assert_eq!(axis.extent(&"a"), Some(80.0));
    assert_eq!(axis.restore_anchor(&anchor, 30.0).unwrap(), 115.0);
    assert_eq!(axis.reveal(&"d", 0.0, 30.0).unwrap(), 140.0);
    assert_eq!(axis.window(110.0, 30.0, 0.0, &[], 1).unwrap().visible, 2..3);
    assert_eq!(axis.window(111.0, 30.0, 0.0, &[], 2).unwrap().visible, 2..4);
}

#[test]
fn focused_window_keeps_adjacent_rows_mounted_for_sequential_navigation() {
    let axis = VirtualAxis::new(["a", "b", "c", "d", "e"], 20.0).unwrap();
    let window = axis
        .window_with_focus(0.0, 20.0, 0.0, &[], Some(&"c"), 4)
        .unwrap();
    assert_eq!(window.visible, 0..1);
    assert_eq!(window.mounted, vec![0, 1, 2, 3]);

    let last = axis
        .window_with_focus(80.0, 20.0, 0.0, &[], Some(&"e"), 2)
        .unwrap();
    assert_eq!(last.visible, 4..5);
    assert_eq!(last.mounted, vec![3, 4]);
    assert_eq!(
        axis.window_with_focus(0.0, 20.0, 0.0, &[], None, 1),
        axis.window(0.0, 20.0, 0.0, &[], 1)
    );
    assert_eq!(
        axis.window_with_focus(0.0, 20.0, 0.0, &[], Some(&"missing"), 4),
        Err(VirtualError::UnknownKey)
    );
    assert_eq!(
        axis.window_with_focus(0.0, 20.0, 0.0, &[], Some(&"c"), 3),
        Err(VirtualError::Capacity)
    );
}

#[test]
fn parent_measured_windows_match_physical_intersections_and_distinct_mount_budgets() {
    for gap in [0.0, 5.0] {
        let mut axis = VirtualAxis::new(0..5, 20.0).unwrap();
        for (index, extent) in [7.0, 31.0, 13.0, 59.0, 23.0].into_iter().enumerate() {
            axis.measure(&index, extent).unwrap();
        }
        axis.set_gap(gap).unwrap();
        let total = axis.total();
        for origin in [0.0, 100.0] {
            for offset in (0..340).step_by(7) {
                for viewport in [0.0, 1.0, 40.0, 500.0] {
                    let parent = ParentViewport::new(offset as f64, viewport, origin).unwrap();
                    let start = parent.offset().clamp(0.0, total);
                    let end = (parent.offset() + viewport).clamp(0.0, total);
                    for overscan in [0.0, 9.0, f64::MAX] {
                        let expected: Vec<_> = (0..axis.len())
                            .filter(|&index| {
                                let top = axis.position(index).unwrap();
                                end > start
                                    && top < (end + overscan).min(total)
                                    && top + axis.extent(&index).unwrap()
                                        > (start - overscan).max(0.0)
                            })
                            .collect();
                        let window = axis.window_in_parent(parent, overscan, &[], 5).unwrap();
                        assert_eq!(window.mounted, expected);
                        assert_eq!(window.offset, parent.offset());
                        assert_eq!(window.total, total);
                        assert_eq!(window.visible.len(), expected.len());
                        if let (Some(&first), Some(&last)) = (expected.first(), expected.last()) {
                            assert_eq!(window.visible, first..last + 1);
                        }
                        let mut pinned = expected.clone();
                        pinned.extend([0, 0, 4, 4]);
                        pinned.sort_unstable();
                        pinned.dedup();
                        assert_eq!(
                            axis.window_in_parent(parent, overscan, &[0, 0, 4, 4], pinned.len())
                                .unwrap()
                                .mounted,
                            pinned
                        );
                        assert_eq!(
                            axis.window_in_parent(
                                parent,
                                overscan,
                                &[0, 0, 4, 4],
                                pinned.len() - 1
                            ),
                            Err(VirtualError::Capacity)
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn focus_windows_merge_visible_pinned_and_adjacent_rows_without_counting_duplicates() {
    let axis = VirtualAxis::<usize>::new(0..5, 20.0).unwrap();
    for offset in [0.0, 20.0, 60.0, 80.0, 200.0] {
        for viewport in [0.0, 20.0, 100.0] {
            for focused in [None::<usize>, Some(0), Some(1), Some(2), Some(3), Some(4)] {
                let parent = ParentViewport::new(offset, viewport, 40.0).unwrap();
                for in_parent in [false, true] {
                    let base = if in_parent {
                        axis.window_in_parent(parent, 0.0, &[], 5)
                    } else {
                        axis.window(offset, viewport, 0.0, &[], 5)
                    }
                    .unwrap();
                    let mut expected = base.mounted;
                    expected.extend([0, 4]);
                    if let Some(index) = focused {
                        expected.extend(index.saturating_sub(1)..(index + 2).min(axis.len()));
                    }
                    expected.sort_unstable();
                    expected.dedup();
                    for capacity in 0..=5 {
                        let window = if in_parent {
                            axis.window_with_focus_in_parent(
                                parent,
                                0.0,
                                &[4, 0, 4, 0, 0, 4],
                                focused.as_ref(),
                                capacity,
                            )
                        } else {
                            axis.window_with_focus(
                                offset,
                                viewport,
                                0.0,
                                &[4, 0, 4, 0, 0, 4],
                                focused.as_ref(),
                                capacity,
                            )
                        };
                        if capacity < expected.len() {
                            assert_eq!(window, Err(VirtualError::Capacity));
                        } else {
                            let window = window.unwrap();
                            assert_eq!(window.mounted, expected);
                            assert_eq!(window.visible, base.visible);
                            assert_eq!(window.offset, base.offset);
                        }
                    }
                }
            }
        }
    }
    let parent = ParentViewport::new(0.0, 20.0, 100.0).unwrap();
    for overscan in [-1.0, f64::NAN, f64::INFINITY] {
        assert_eq!(
            axis.window_in_parent(parent, overscan, &[], 5),
            Err(VirtualError::InvalidExtent)
        );
        assert_eq!(
            axis.window_with_focus_in_parent(parent, overscan, &[], Some(&2), 5),
            Err(VirtualError::InvalidExtent)
        );
    }
    assert_eq!(
        axis.window_in_parent(parent, 0.0, &[9], 5),
        Err(VirtualError::UnknownKey)
    );
    assert_eq!(
        axis.window_with_focus_in_parent(parent, 0.0, &[], Some(&9), 5),
        Err(VirtualError::UnknownKey)
    );
    let empty = VirtualAxis::<usize>::new([], 20.0).unwrap();
    assert!(
        empty
            .window_in_parent(parent, 100.0, &[], 0)
            .unwrap()
            .mounted
            .is_empty()
    );
    assert_eq!(
        empty.window_with_focus_in_parent(parent, 0.0, &[], Some(&0), 1),
        Err(VirtualError::UnknownKey)
    );
}

#[test]
fn invalid_or_duplicate_updates_leave_the_index_unchanged() {
    let mut axis = VirtualAxis::new([1, 2, 3], 24.0).unwrap();
    assert_eq!(axis.replace([1, 1], 24.0), Err(VirtualError::DuplicateKey));
    assert_eq!(axis.measure(&2, f64::NAN), Err(VirtualError::InvalidExtent));
    assert_eq!(axis.measure(&9, 20.0), Err(VirtualError::UnknownKey));
    assert_eq!(axis.total(), 72.0);
    assert_eq!(axis.position(3), Some(72.0));
    assert!(axis.window(f64::INFINITY, 20.0, 0.0, &[], 4).is_err());
    assert!(axis.window(0.0, 20.0, 0.0, &[8], 4).is_err());
}

#[test]
fn two_dimensional_windows_bound_the_cross_product() {
    let grid = VirtualGrid {
        rows: VirtualAxis::new(0..100_000, 32.0).unwrap(),
        columns: VirtualAxis::new(0..10_000, 100.0).unwrap(),
    };
    let [columns, rows] = grid
        .window(
            [500_000.0, 1_600_000.0],
            [500.0, 320.0],
            [0.0; 2],
            &[],
            &[0],
            60,
        )
        .unwrap();
    assert_eq!(columns.visible, 5000..5005);
    assert_eq!(rows.visible, 50_000..50_010);
    assert_eq!(columns.mounted.len() * rows.mounted.len(), 60);
    assert!(
        grid.window([0.0; 2], [500.0, 320.0], [0.0; 2], &[], &[9999], 59)
            .is_err()
    );
}

#[test]
fn measured_grid_parent_windows_match_rectangles_and_distinct_pinned_cell_budgets() {
    let mut grid = VirtualGrid {
        rows: VirtualAxis::new(0..5, 20.0).unwrap(),
        columns: VirtualAxis::new(0..4, 20.0).unwrap(),
    };
    for (key, extent) in [7.0, 31.0, 13.0, 59.0, 23.0].into_iter().enumerate() {
        grid.rows.measure(&key, extent).unwrap();
    }
    for (key, extent) in [11.0, 43.0, 17.0, 29.0].into_iter().enumerate() {
        grid.columns.measure(&key, extent).unwrap();
    }
    for gap in [0.0, 5.0] {
        grid.rows.set_gap(gap).unwrap();
        grid.columns.set_gap(gap).unwrap();
        for offset in [[0.0; 2], [55.0, 85.0], [115.0, 137.0], [500.0; 2]] {
            for size in [[0.0, 40.0], [40.0, 0.0], [1.0; 2], [40.0; 2], [500.0; 2]] {
                for overscan in [[0.0; 2], [9.0, 7.0], [f64::MAX; 2]] {
                    for in_parent in [false, true] {
                        let parent = [
                            ParentViewport::new(offset[0], size[0], 50.0).unwrap(),
                            ParentViewport::new(offset[1], size[1], 80.0).unwrap(),
                        ];
                        let axes = [&grid.columns, &grid.rows];
                        let intervals: [_; 2] = std::array::from_fn(|i| {
                            let total = axes[i].total();
                            if in_parent {
                                (
                                    parent[i].offset().clamp(0.0, total),
                                    (parent[i].offset() + size[i]).clamp(0.0, total),
                                )
                            } else {
                                let start = offset[i].min((total - size[i]).max(0.0));
                                (start, (start + size[i]).min(total))
                            }
                        });
                        let area = intervals.iter().all(|(start, end)| end > start);
                        for pins in [false, true] {
                            let expected: [Vec<_>; 2] = std::array::from_fn(|i| {
                                let (start, end) = intervals[i];
                                (0..axes[i].len())
                                    .filter(|&index| {
                                        let origin = axes[i].position(index).unwrap();
                                        (pins && (index == 0 || index == axes[i].len() - 1))
                                            || (area
                                                && origin
                                                    < (end + overscan[i]).min(axes[i].total())
                                                && origin + axes[i].extent(&index).unwrap()
                                                    > (start - overscan[i]).max(0.0))
                                    })
                                    .collect()
                            });
                            let budget = (expected[0].len() * expected[1].len())
                                .max(expected[0].len())
                                .max(expected[1].len());
                            let row_pins: &[usize] = if pins { &[4, 0, 4, 0] } else { &[] };
                            let column_pins: &[usize] = if pins { &[3, 0, 3, 0] } else { &[] };
                            for capacity in [budget, budget.saturating_sub(1)] {
                                let window = if in_parent {
                                    grid.window_in_parent(
                                        parent,
                                        overscan,
                                        row_pins,
                                        column_pins,
                                        capacity,
                                    )
                                } else {
                                    grid.window(
                                        offset,
                                        size,
                                        overscan,
                                        row_pins,
                                        column_pins,
                                        capacity,
                                    )
                                };
                                if capacity < budget {
                                    assert_eq!(window, Err(VirtualError::Capacity));
                                } else {
                                    let window = window.unwrap();
                                    for i in 0..2 {
                                        assert_eq!(window[i].mounted, expected[i]);
                                        assert_eq!(window[i].total, axes[i].total());
                                        assert_eq!(
                                            window[i].offset,
                                            if in_parent {
                                                parent[i].offset()
                                            } else {
                                                intervals[i].0
                                            }
                                        );
                                        if !area {
                                            assert!(window[i].visible.is_empty());
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn measured_grid_hidden_axes_validate_geometry_and_keep_only_budgeted_pins() {
    let grid = VirtualGrid {
        rows: VirtualAxis::new(0..100_000, 10.0).unwrap(),
        columns: VirtualAxis::new(0..10_000, 20.0).unwrap(),
    };
    for viewport in [[0.0, f64::MAX], [f64::MAX, 0.0], [0.0; 2]] {
        let windows = grid
            .window([0.0; 2], viewport, [f64::MAX; 2], &[], &[], 0)
            .unwrap();
        assert!(
            windows
                .iter()
                .all(|w| w.mounted.is_empty() && w.visible.is_empty())
        );
        let windows = grid
            .window(
                [0.0; 2],
                viewport,
                [f64::MAX; 2],
                &[99_999, 0, 0],
                &[9999, 0, 0],
                4,
            )
            .unwrap();
        assert_eq!(windows[0].mounted, [0, 9999]);
        assert_eq!(windows[1].mounted, [0, 99_999]);
        assert_eq!(
            grid.window([0.0; 2], viewport, [0.0; 2], &[0, 99_999], &[0, 9999], 3),
            Err(VirtualError::Capacity)
        );
        assert_eq!(
            grid.window([0.0; 2], viewport, [0.0; 2], &[100_000], &[], 1),
            Err(VirtualError::UnknownKey)
        );
    }
    // Each axis fits, but the billion-cell product fails before mount vectors
    // are materialized. Existing axis state remains available after rejection.
    assert_eq!(
        grid.window([0.0; 2], [f64::MAX; 2], [0.0; 2], &[], &[], 100_000),
        Err(VirtualError::Capacity)
    );
    assert_eq!(grid.rows.position(99_999), Some(999_990.0));
    for invalid in [-1.0, f64::NAN, f64::INFINITY] {
        for i in 0..2 {
            let mut size = [0.0; 2];
            size[i] = invalid;
            assert_eq!(
                grid.window([0.0; 2], size, [0.0; 2], &[], &[], 0),
                Err(VirtualError::InvalidExtent)
            );
            let mut offset = [0.0; 2];
            offset[i] = invalid;
            assert_eq!(
                grid.window(offset, [0.0; 2], [0.0; 2], &[], &[], 0),
                Err(VirtualError::InvalidExtent)
            );
            let mut overscan = [0.0; 2];
            overscan[i] = invalid;
            let parent = [ParentViewport::new(0.0, 0.0, 100.0).unwrap(); 2];
            assert_eq!(
                grid.window_in_parent(parent, overscan, &[], &[], 0),
                Err(VirtualError::InvalidExtent)
            );
        }
    }
    let empty = VirtualGrid {
        rows: VirtualAxis::new(0..0, 10.0).unwrap(),
        columns: VirtualAxis::new(0..100, 20.0).unwrap(),
    };
    assert!(
        empty
            .window([0.0; 2], [100.0; 2], [f64::MAX; 2], &[], &[], 0)
            .unwrap()
            .iter()
            .all(|w| w.mounted.is_empty())
    );
    assert_eq!(
        empty
            .window([0.0; 2], [100.0; 2], [0.0; 2], &[], &[0, 0], 1)
            .unwrap()[0]
            .mounted,
        [0]
    );
    assert_eq!(
        empty.window([0.0; 2], [100.0; 2], [0.0; 2], &[0], &[], 1),
        Err(VirtualError::UnknownKey)
    );
}

#[test]
fn prefix_index_matches_linear_layout_after_variable_measurements() {
    let mut axis = VirtualAxis::new(0..257, 12.0).unwrap();
    let heights: Vec<_> = (0..257)
        .map(|index| (index % 11 + 1) as f64 * 3.0)
        .collect();
    for (index, height) in heights.iter().copied().enumerate() {
        axis.measure(&index, height).unwrap();
    }
    let mut position = 0.0;
    for (index, height) in heights.iter().copied().enumerate() {
        assert_eq!(axis.position(index), Some(position));
        assert_eq!(
            axis.window(position, height, 0.0, &[], 1).unwrap().visible,
            index..index + 1
        );
        position += height;
    }
    assert_eq!(axis.total(), position);
    assert_eq!(axis.position(257), Some(position));
}

#[test]
fn revealing_a_partly_visible_oversized_row_preserves_the_current_window() {
    let axis = VirtualAxis::new(["before", "large", "after"], 600.0).unwrap();
    for offset in [250.0, 600.0, 850.0, 1100.0] {
        assert_eq!(axis.reveal(&"large", offset, 400.0).unwrap(), offset);
    }
    assert_eq!(axis.reveal(&"large", 0.0, 400.0).unwrap(), 600.0);
    assert_eq!(axis.reveal(&"large", 1300.0, 400.0).unwrap(), 600.0);
}

#[test]
fn append_matches_rebuild_across_prefix_boundaries_and_later_measurements() {
    let mut axis = VirtualAxis::new(0..0, 12.0).unwrap();
    let mut count = 0;
    for batch in [1, 2, 5, 8, 17, 32, 64, 128, 257] {
        let end = count + batch;
        assert_eq!(axis.append((count..end).map(|key| (key, 12.0))), Ok(batch));
        let measured = end / 2;
        axis.measure(&measured, (end + 1) as f64).unwrap();
        let rebuilt = axis.replacement(0..end, 12.0).unwrap();
        for index in 0..=end {
            assert_eq!(axis.position(index), rebuilt.position(index));
        }
        for offset in (0..axis.total() as usize).step_by(7) {
            assert_eq!(
                axis.window(offset as f64, 23.0, 0.0, &[], 8).unwrap(),
                rebuilt.window(offset as f64, 23.0, 0.0, &[], 8).unwrap()
            );
        }
        assert_eq!(axis.index(&(end - 1)), Some(end - 1));
        count = end;
    }
}

#[test]
fn append_rejects_entire_batch_and_preserves_existing_anchor() {
    let mut axis = VirtualAxis::new(0..3, 20.0).unwrap();
    let anchor = axis.anchor(25.0).unwrap().unwrap();
    for (batch, error) in [
        (vec![(3, 10.0), (1, 10.0)], VirtualError::DuplicateKey),
        (vec![(3, 10.0), (3, 10.0)], VirtualError::DuplicateKey),
        (vec![(3, 10.0), (4, f64::NAN)], VirtualError::InvalidExtent),
        (vec![(3, 10.0), (4, 0.0)], VirtualError::InvalidExtent),
        (
            vec![(3, f64::MAX), (4, f64::MAX)],
            VirtualError::InvalidExtent,
        ),
    ] {
        assert_eq!(axis.append(batch), Err(error));
        assert_eq!(axis.len(), 3);
        assert_eq!(axis.total(), 60.0);
        assert_eq!(axis.index(&3), None);
        assert_eq!(axis.position(3), Some(60.0));
    }
    assert_eq!(axis.append([]), Ok(0));
    assert_eq!(axis.append([(3, 45.0)]), Ok(1));
    assert_eq!(axis.restore_anchor(&anchor, 20.0).unwrap(), 25.0);
    assert_eq!(axis.reveal(&3, 0.0, 20.0).unwrap(), 60.0);
    axis.measure(&0, 35.0).unwrap();
    assert_eq!(axis.position(4), Some(120.0));
}

#[test]
fn append_capacity_error_does_not_publish_a_partial_batch() {
    let mut axis = VirtualAxis::new(0..999_999, 1.0).unwrap();
    assert_eq!(
        axis.append([(999_999, 1.0), (1_000_000, 1.0)]),
        Err(VirtualError::Capacity)
    );
    assert_eq!(axis.len(), 999_999);
    assert_eq!(axis.index(&999_999), None);
    assert_eq!(axis.append([(999_999, 1.0)]), Ok(1));
    assert_eq!(axis.append([]), Ok(0));
    assert_eq!(axis.position(1_000_000), Some(1_000_000.0));
}

#[test]
fn appended_small_rows_do_not_subtract_large_unrelated_prefixes() {
    let mut axis = VirtualAxis::new(0..5, 1.0).unwrap();
    axis.measure(&0, 1e16).unwrap();
    axis.append([(5, 3.0)]).unwrap();
    axis.measure(&0, 1.0).unwrap();
    // The first four rows have their own floating-point measurement rounding;
    // the new Fenwick range [4, 6) must still retain both small row extents.
    assert_eq!(axis.position(6).unwrap() - axis.position(4).unwrap(), 4.0);
}

#[test]
fn viewability_thresholds_match_overlap_oracle_for_uniform_and_measured_rows() {
    for measured in [false, true] {
        let mut axis = VirtualAxis::new(0..30, 20.0).unwrap();
        if measured {
            for index in 0..30 {
                axis.measure(&index, [5.0, 17.0, 200.0][index % 3]).unwrap();
            }
        }
        for viewport in [0.0, 3.0, 40.0, 90.0, 5000.0] {
            for offset in [0.0, 5.0, 19.0, 63.0, 102.0, 9999.0] {
                let geometry = axis.viewport_geometry(offset, viewport).unwrap();
                for policy in [
                    ListViewability::AnyVisible,
                    ListViewability::ItemFraction(0.0),
                    ListViewability::ItemFraction(0.5),
                    ListViewability::ItemFraction(1.0),
                    ListViewability::ViewportFraction(0.0),
                    ListViewability::ViewportFraction(0.75),
                    ListViewability::ViewportFraction(1.0),
                ] {
                    let expected: Vec<_> = (0..30)
                        .filter(|&index| {
                            let start = axis.position(index).unwrap();
                            let end = axis.position(index + 1).unwrap();
                            let overlap = (end.min(geometry.offset + viewport)
                                - start.max(geometry.offset))
                            .max(0.0);
                            overlap > 0.0
                                && ((start >= geometry.offset && end <= geometry.offset + viewport)
                                    || match policy {
                                        ListViewability::AnyVisible => true,
                                        ListViewability::ItemFraction(f) => {
                                            overlap / (end - start) >= f
                                        }
                                        ListViewability::ViewportFraction(f) => {
                                            overlap / viewport >= f
                                        }
                                    })
                        })
                        .collect();
                    let actual = axis.viewable_items(offset, viewport, policy).unwrap();
                    assert_eq!(actual.collect::<Vec<_>>(), expected);
                    if !measured {
                        assert_eq!(
                            ListViewport::uniform_viewable_items(
                                30, 20.0, offset, viewport, policy
                            )
                            .unwrap()
                            .collect::<Vec<_>>(),
                            expected
                        );
                    }
                }
            }
        }
    }
    for invalid in [-0.1, 1.1, f64::NAN, f64::INFINITY] {
        for policy in [
            ListViewability::ItemFraction(invalid),
            ListViewability::ViewportFraction(invalid),
        ] {
            assert_eq!(
                ListViewport::uniform_viewable_items(0, 20.0, 0.0, 0.0, policy),
                Err(VirtualError::InvalidExtent)
            );
            assert_eq!(
                VirtualAxis::new(0..0, 20.0)
                    .unwrap()
                    .viewable_items(0.0, 0.0, policy),
                Err(VirtualError::InvalidExtent)
            );
        }
    }
    assert_eq!(
        ListViewport::uniform_viewable_items(
            100_000,
            20.0,
            1_000_005.0,
            40.0,
            ListViewability::ItemFraction(0.5)
        )
        .unwrap(),
        50_000..50_002
    );
}

#[test]
fn viewability_dwell_is_continuous_per_row_and_restarts_on_revision_or_resume() {
    use std::time::Duration;
    let ms = Duration::from_millis;
    let mut tracker = ListViewabilityTracker::new(ms(100));
    assert!(tracker.update(0..3, ms(0)).is_none());
    assert_eq!(tracker.next_deadline(), Some(ms(100)));
    assert!(tracker.update(1..4, ms(50)).is_none());
    let change = tracker.update(1..4, ms(100)).unwrap();
    assert_eq!(change.viewable, [1, 2]);
    assert_eq!(change.changed().collect::<Vec<_>>(), [(1, true), (2, true)]);
    assert_eq!(tracker.next_deadline(), Some(ms(150)));
    assert_eq!(tracker.update(1..4, ms(150)).unwrap().viewable, [1, 2, 3]);
    assert_eq!(tracker.next_deadline(), None);
    assert!(tracker.update(1..4, ms(160)).is_none());
    let change = tracker.update(2..5, ms(170)).unwrap();
    assert_eq!(change.changed().collect::<Vec<_>>(), [(1, false)]);
    assert!(tracker.update(1..5, ms(180)).is_none());
    let change = tracker.update(1..5, ms(270)).unwrap();
    assert_eq!(change.viewable, [2, 3, 4]);
    assert_eq!(tracker.next_deadline(), Some(ms(280)));
    assert_eq!(
        tracker.update(1..5, ms(280)).unwrap().viewable,
        [1, 2, 3, 4]
    );
    tracker.restart();
    assert!(tracker.update(1..5, ms(500)).unwrap().viewable.is_empty());
    assert!(tracker.update(1..5, ms(550)).is_none());
    assert_eq!(
        tracker.update(1..5, ms(600)).unwrap().viewable,
        [1, 2, 3, 4]
    );
    assert!(tracker.update(0..0, ms(610)).unwrap().viewable.is_empty());
    assert_eq!(tracker.next_deadline(), None);
    let mut immediate = ListViewabilityTracker::default();
    assert!(immediate.update(0..0, ms(0)).is_none());
    assert_eq!(immediate.update(0..2, ms(0)).unwrap().viewable, [0, 1]);
    let change = immediate.update(3..5, ms(1)).unwrap();
    assert_eq!(
        change.changed().collect::<Vec<_>>(),
        [(3, true), (4, true), (0, false), (1, false)]
    );
    assert!(immediate.update(3..5, ms(2)).is_none());
    assert_eq!(immediate.next_deadline(), None);
}

#[test]
fn uniform_grid_matches_rectangular_overlap_and_bounds_mount_product() {
    for (rows, columns) in [(0, 7), (9, 0), (1, 1), (9, 7)] {
        for gap in [[0.0, 0.0], [8.0, 13.0]] {
            let grid = UniformGrid::new(rows, columns, [20.0, 10.0], gap).unwrap();
            let empty = rows == 0 || columns == 0;
            let total = if empty {
                [0.0; 2]
            } else {
                [
                    columns as f64 * 20.0 + columns.saturating_sub(1) as f64 * gap[0],
                    rows as f64 * 10.0 + rows.saturating_sub(1) as f64 * gap[1],
                ]
            };
            assert_eq!(grid.total(), total);
            for offset in [[0.0, 0.0], [21.0, 11.0], [49.0, 69.0], [10_000.0; 2]] {
                for size in [[0.0, 30.0], [40.0, 0.0], [2.0, 3.0], [71.0, 51.0]] {
                    let window = grid.window(offset, size, [0, 0], 63).unwrap();
                    let current = [
                        offset[0].min((total[0] - size[0]).max(0.0)),
                        offset[1].min((total[1] - size[1]).max(0.0)),
                    ];
                    let mut expected = Vec::new();
                    for row in 0..rows {
                        for column in 0..columns {
                            let origin = [
                                column as f64 * (20.0 + gap[0]),
                                row as f64 * (10.0 + gap[1]),
                            ];
                            assert_eq!(grid.cell_origin(row, column), Some(origin));
                            if size[0] > 0.0
                                && size[1] > 0.0
                                && origin[0] < current[0] + size[0]
                                && origin[0] + 20.0 > current[0]
                                && origin[1] < current[1] + size[1]
                                && origin[1] + 10.0 > current[1]
                            {
                                expected.push((row, column));
                            }
                        }
                    }
                    let actual: Vec<_> = window
                        .rows
                        .clone()
                        .flat_map(|row| window.columns.clone().map(move |column| (row, column)))
                        .collect();
                    assert_eq!(actual, expected);
                    assert_eq!(
                        [window.viewport.columns.offset, window.viewport.rows.offset],
                        current
                    );
                }
            }
            assert_eq!(grid.cell_origin(rows, 0), None);
            assert_eq!(grid.cell_origin(0, columns), None);
        }
    }
    let grid = UniformGrid::new(100, 200, [20.0, 10.0], [5.0, 3.0]).unwrap();
    let window = grid
        .window([250.0, 130.0], [70.0, 30.0], [1, 2], 35)
        .unwrap();
    assert_eq!((window.columns, window.rows), (9..14, 8..15));
    assert_eq!(
        grid.window([250.0, 130.0], [70.0, 30.0], [1, 2], 34),
        Err(VirtualError::Capacity)
    );
    for (align, fraction) in [
        (ListScrollAlign::Start, 0.0),
        (ListScrollAlign::Center, 0.5),
        (ListScrollAlign::End, 1.0),
    ] {
        assert_eq!(
            grid.scroll_to_cell(10, 20, [0.0; 2], [70.0, 30.0], [align; 2], [3.0, 4.0])
                .unwrap(),
            [500.0 - 50.0 * fraction - 3.0, 130.0 - 20.0 * fraction - 4.0]
        );
    }
    assert_eq!(
        grid.scroll_to_cell(
            100,
            0,
            [0.0; 2],
            [70.0, 30.0],
            [ListScrollAlign::Start; 2],
            [0.0; 2]
        ),
        Err(VirtualError::UnknownKey)
    );
    assert_eq!(
        grid.window([f64::NAN, 0.0], [70.0, 30.0], [0; 2], 100),
        Err(VirtualError::InvalidExtent)
    );
    assert!(UniformGrid::new(1, 1, [0.0, 10.0], [0.0; 2]).is_err());
    let huge = UniformGrid::new(usize::MAX, usize::MAX, [1.0; 2], [0.0; 2]).unwrap();
    assert_eq!(
        huge.window([0.0; 2], [f64::MAX; 2], [0; 2], usize::MAX),
        Err(VirtualError::Capacity)
    );
    assert_eq!(
        huge.window([0.0; 2], [10.0; 2], [0; 2], 100).unwrap().rows,
        0..10
    );
}

#[test]
fn uniform_wrap_matches_flat_item_overlap_and_retains_a_reflow_anchor() {
    for count in [0, 1, 17, 83] {
        for width in [5.0, 80.0, 175.0, 260.0, 1000.0] {
            let wrap = UniformWrap::new(count, width, 80.0, 10.0, [7.0, 3.0]).unwrap();
            let mut columns = 1;
            while columns < count && (columns + 1) as f64 * 80.0 + columns as f64 * 7.0 <= width {
                columns += 1;
            }
            assert_eq!(wrap.columns(), columns);
            let rows = count.div_ceil(columns);
            let total = rows as f64 * 10.0 + rows.saturating_sub(1) as f64 * 3.0;
            assert_eq!(wrap.total()[1], total);
            assert_eq!(wrap.is_empty(), count == 0);
            for viewport in [0.0, 2.0, 25.0, 10000.0] {
                for offset in [0.0_f64, 5.0, 13.0, 40.0, 999.0] {
                    let top = offset.min((total - viewport).max(0.0));
                    let expected: Vec<_> = (0..count)
                        .filter(|i| {
                            let start = (i / columns) as f64 * 13.0;
                            viewport > 0.0 && start < top + viewport && start + 10.0 > top
                        })
                        .collect();
                    let window = wrap.window(offset, viewport, 0, count).unwrap();
                    let actual: Vec<_> = window
                        .rows
                        .clone()
                        .flat_map(|r| window.columns.clone().filter_map(move |c| wrap.index(r, c)))
                        .collect();
                    assert_eq!(actual, expected);
                    let overscan = wrap.window(offset, viewport, 1, count).unwrap();
                    let mounted = overscan
                        .rows
                        .clone()
                        .flat_map(|r| {
                            overscan
                                .columns
                                .clone()
                                .filter_map(move |c| wrap.index(r, c))
                        })
                        .count();
                    if mounted > 0 {
                        assert_eq!(
                            wrap.window(offset, viewport, 1, mounted - 1),
                            Err(VirtualError::Capacity)
                        );
                    }
                }
            }
            assert_eq!(wrap.cell(count), None);
            assert_eq!(wrap.index(rows, 0), None);
            assert_eq!(wrap.index(usize::MAX, columns - 1), None);
            for i in 0..count {
                assert_eq!(wrap.cell(i), Some([i / columns, i % columns]));
                let origin = wrap.item_origin(i).unwrap();
                assert!(
                    (origin[0] - (i % columns) as f64 * (wrap.cell_size()[0] + 7.0)).abs() < 1e-9
                );
                assert_eq!(origin[1], (i / columns) as f64 * 13.0);
            }
        }
    }
    let make = |width| UniformWrap::new(100, width, 80.0, 10.0, [7.0, 3.0]).unwrap();
    let three = make(260.0);
    let anchor = three.anchor(68.125, 25.0).unwrap().unwrap();
    assert_eq!(
        anchor,
        ListScrollPosition {
            index: 15,
            offset: 3.125
        }
    );
    assert_eq!(make(350.0).restore_anchor(anchor, 25.0), Ok(42.125));
    assert_eq!(make(175.0).restore_anchor(anchor, 25.0), Ok(94.125));
    assert_eq!(three.restore_anchor(anchor, 25.0), Ok(68.125));
    assert_eq!(
        make(175.0).scroll_to_item(15, 0.0, 25.0, ListScrollAlign::Start, 2.0),
        Ok(89.0)
    );
    assert_eq!(
        make(175.0).restore_anchor(
            ListScrollPosition {
                index: 100,
                offset: 0.0
            },
            25.0
        ),
        Err(VirtualError::UnknownKey)
    );
    assert_eq!(
        UniformWrap::new(1, 5.0, 80.0, 10.0, [7.0, 3.0])
            .unwrap()
            .restore_anchor(
                ListScrollPosition {
                    index: 0,
                    offset: 50.0
                },
                20.0
            ),
        Ok(0.0)
    );
    for width in [0.0, -1.0, f64::NAN, f64::INFINITY] {
        assert!(UniformWrap::new(10, width, 80.0, 10.0, [0.0; 2]).is_err());
    }
    assert!(UniformWrap::new(10, 260.0, 0.0, 10.0, [0.0; 2]).is_err());
    assert!(UniformWrap::new(10, 260.0, 80.0, 0.0, [0.0; 2]).is_err());
    assert!(UniformWrap::new(10, 260.0, 80.0, 10.0, [-1.0, 0.0]).is_err());
    let huge = UniformWrap::new(usize::MAX, 2.0, 1.0, 1.0, [0.0; 2]).unwrap();
    assert_eq!(
        huge.window(0.0, f64::MAX, 0, usize::MAX)
            .unwrap()
            .rows
            .len(),
        usize::MAX.div_ceil(2)
    );
    assert_eq!(huge.cell(usize::MAX - 1).unwrap()[1], 0);
}

#[test]
fn sticky_headers_match_linear_push_off_oracle_for_uniform_and_measured_rows() {
    let headers = ListStickyHeaders::new([9, 3, 0, 3, 27, usize::MAX]);
    assert_eq!(headers.indices(), &[0, 3, 9, 27, usize::MAX]);
    for count in [0, 1, 11, 31] {
        let uniform = UniformList::new(count, 20.0, 7.0).unwrap();
        let mut axis = VirtualAxis::new(0..count, 20.0).unwrap();
        axis.set_gap(7.0).unwrap();
        for measured in [false, true] {
            if measured {
                for i in 0..count {
                    axis.measure(&i, 5.0 + (i % 5) as f64 * 13.0).unwrap();
                }
            }
            let total = if measured {
                axis.total()
            } else {
                uniform.total()
            };
            for viewport in [0.0, 1.0, 50.0, 10000.0] {
                for offset in (0..160).map(|i| i as f64 * 6.25) {
                    for inset in [-100.0, 0.0, 7.5, 150.0, f64::MAX] {
                        let at = offset.min((total - viewport).max(0.0)) + inset;
                        let start = |i| {
                            if measured {
                                axis.position(i).unwrap()
                            } else {
                                i as f64 * 27.0
                            }
                        };
                        let eligible: Vec<_> = headers
                            .indices()
                            .iter()
                            .copied()
                            .filter(|&i| i < count && start(i) <= at)
                            .collect();
                        let expected =
                            eligible
                                .last()
                                .copied()
                                .filter(|_| viewport > 0.0)
                                .map(|index| {
                                    let next = headers
                                        .indices()
                                        .iter()
                                        .copied()
                                        .find(|&i| i > index && i < count)
                                        .map_or(total, start);
                                    let extent = if measured {
                                        5.0 + (index % 5) as f64 * 13.0
                                    } else {
                                        20.0
                                    };
                                    ListStickyHeader {
                                        index,
                                        translation: (at.min(next - extent) - start(index))
                                            .max(0.0),
                                    }
                                });
                        let actual = if measured {
                            axis.sticky_header(&headers, offset, viewport, inset)
                        } else {
                            uniform.sticky_header(&headers, offset, viewport, inset)
                        };
                        assert_eq!(actual.unwrap(), expected);
                        if !measured {
                            assert_eq!(
                                axis.sticky_header(&headers, offset, viewport, inset),
                                Ok(expected)
                            );
                        }
                    }
                }
            }
        }
    }
    let list = UniformList::new(usize::MAX, 1.0, 0.0).unwrap();
    assert_eq!(
        list.sticky_header(&headers, 1_000_000.5, 40.0, 0.0)
            .unwrap(),
        Some(ListStickyHeader {
            index: 27,
            translation: 999_973.5
        })
    );
    for invalid in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert!(list.sticky_header(&headers, 0.0, 40.0, invalid).is_err());
        assert!(list.sticky_header(&headers, invalid, 40.0, 0.0).is_err());
    }
    let mut axis = VirtualAxis::new([0, 1, 2, 3], 20.0).unwrap();
    axis.measure(&0, 40.0).unwrap();
    let replaced = axis.replacement([3, 0, 1], 20.0).unwrap();
    assert_eq!(
        replaced
            .sticky_header(&ListStickyHeaders::new([0, 1]), 18.0, 10.0, 0.0)
            .unwrap(),
        Some(ListStickyHeader {
            index: 0,
            translation: 0.0
        })
    );
}

#[test]
fn wrap_viewability_matches_flat_item_overlap_without_padding_or_dataset_scans() {
    let policies = [
        ListViewability::AnyVisible,
        ListViewability::ItemFraction(0.0),
        ListViewability::ItemFraction(0.5),
        ListViewability::ItemFraction(1.0),
        ListViewability::ViewportFraction(0.0),
        ListViewability::ViewportFraction(0.5),
        ListViewability::ViewportFraction(1.0),
    ];
    for count in 0_usize..18 {
        for width in [90.0, 180.0, 270.0] {
            let wrap = UniformWrap::new(count, width, 80.0, 20.0, [10.0, 5.0]).unwrap();
            let columns = ((width + 10.0) / 90.0).floor() as usize;
            let columns = columns.min(count.max(1));
            let rows = count.div_ceil(columns);
            let total = if rows == 0 {
                0.0
            } else {
                rows as f64 * 25.0 - 5.0
            };
            for viewport in [0.0, 5.0, 20.0, 40.0, 100.0] {
                for offset in [0.0_f64, 5.0, 19.0, 20.0, 24.0, 25.0, 1000.0] {
                    let top = offset.min((total - viewport).max(0.0));
                    let bottom = top + viewport;
                    for policy in policies {
                        let expected: Vec<_> = (0..count)
                            .filter(|i| {
                                let start = (i / columns) as f64 * 25.0;
                                let end = start + 20.0;
                                let overlap = (end.min(bottom) - start.max(top)).max(0.0);
                                overlap > 0.0
                                    && ((start >= top && end <= bottom)
                                        || match policy {
                                            ListViewability::AnyVisible => true,
                                            ListViewability::ItemFraction(f) => overlap / 20.0 >= f,
                                            ListViewability::ViewportFraction(f) => {
                                                overlap / viewport >= f
                                            }
                                        })
                            })
                            .collect();
                        let actual: Vec<_> = wrap
                            .viewable_items(offset, viewport, policy)
                            .unwrap()
                            .collect();
                        assert_eq!(
                            actual, expected,
                            "count={count}, width={width}, offset={offset}, viewport={viewport}, policy={policy:?}"
                        );
                    }
                }
            }
        }
    }
    let huge = UniformWrap::new(usize::MAX, 270.0, 80.0, 20.0, [10.0, 5.0]).unwrap();
    assert_eq!(huge.viewable_items(0.0, 40.0, policies[0]), Ok(0..6));
    for policy in [
        ListViewability::ItemFraction(f64::NAN),
        ListViewability::ViewportFraction(1.1),
    ] {
        assert_eq!(
            huge.viewable_items(0.0, 40.0, policy),
            Err(VirtualError::InvalidExtent)
        );
    }
    assert_eq!(
        huge.viewable_items(-1.0, 40.0, policies[0]),
        Err(VirtualError::InvalidExtent)
    );
}

#[test]
fn aligned_wrap_geometry_matches_physical_item_and_prediction_oracles() {
    for count in [0usize, 1, 3, 9, 101] {
        let natural = UniformWrap::new(count, 180.0, 80.0, 20.0, [10.0, 5.0]).unwrap();
        for align in [
            ListContentAlign::Start,
            ListContentAlign::Center,
            ListContentAlign::End,
        ] {
            let free = (100.0 - natural.total()[1]).max(0.0);
            let pad = match align {
                ListContentAlign::Start => 0.0,
                ListContentAlign::Center => free * 0.5,
                ListContentAlign::End => free,
            };
            let wrap = natural.with_content_align(100.0, align).unwrap();
            let total = if align == ListContentAlign::Start {
                natural.total()[1]
            } else {
                natural.total()[1].max(100.0)
            };
            assert_eq!(wrap.total(), [natural.total()[0], total]);
            for index in 0..count {
                let origin = natural.item_origin(index).unwrap();
                assert_eq!(wrap.item_origin(index), Some([origin[0], origin[1] + pad]));
                assert_eq!(
                    wrap.scroll_to_item(index, 0.0, 100.0, ListScrollAlign::Start, 0.0)
                        .unwrap(),
                    (origin[1] + pad).min((total - 100.0).max(0.0))
                );
            }
            for viewport in [0.0_f64, 10.0, 100.0, 150.0] {
                for requested in [0.0_f64, 25.0, 75.0, 10000.0] {
                    let offset = requested.min((total - viewport).max(0.0));
                    let items = |top: f64, bottom: f64| {
                        (0..count)
                            .filter(|&i| {
                                let start = (i / natural.columns()) as f64 * 25.0 + pad;
                                (start + 20.0).min(bottom) > start.max(top)
                            })
                            .collect::<Vec<_>>()
                    };
                    let expected = items(offset, offset + viewport);
                    let window = wrap.window(requested, viewport, 0, count).unwrap();
                    assert_eq!(window.viewport.rows.total_extent, total);
                    assert_eq!(window.viewport.rows.offset, offset);
                    let mounted = (window.rows.start.saturating_mul(wrap.columns()).min(count)
                        ..window.rows.end.saturating_mul(wrap.columns()).min(count))
                        .collect::<Vec<_>>();
                    assert_eq!(mounted, expected);
                    assert_eq!(
                        wrap.viewable_items(requested, viewport, ListViewability::AnyVisible)
                            .unwrap()
                            .collect::<Vec<_>>(),
                        expected
                    );
                    for ahead in [-100.0_f64, 100.0] {
                        let predicted = wrap
                            .window_with_scroll_ahead(requested, viewport, 0, count, ahead)
                            .unwrap();
                        assert_eq!(predicted.viewport, window.viewport);
                        if viewport > 0.0 {
                            let lead = ahead.clamp(-viewport, viewport);
                            let top = (offset + lead.min(0.0)).max(0.0);
                            let end = (offset + viewport).min(total);
                            let bottom = (end + lead.max(0.0)).min(total);
                            let predicted_items = (predicted
                                .rows
                                .start
                                .saturating_mul(wrap.columns())
                                .min(count)
                                ..predicted.rows.end.saturating_mul(wrap.columns()).min(count))
                                .collect::<Vec<_>>();
                            assert_eq!(predicted_items, items(top, bottom));
                            assert_eq!(predicted.columns.is_empty(), predicted_items.is_empty());
                            assert_eq!(
                                wrap.window_with_scroll_ahead(
                                    requested,
                                    viewport,
                                    0,
                                    expected.len(),
                                    ahead
                                )
                                .unwrap(),
                                window
                            );
                        }
                    }
                    let anchor = wrap.anchor(requested, viewport).unwrap();
                    if pad > offset || expected.is_empty() {
                        assert_eq!(anchor, None);
                    } else if let Some(anchor) = anchor {
                        assert!(expected.contains(&anchor.index));
                        // A viewport beginning in a gap anchors the next row at its start.
                        let start = (anchor.index / natural.columns()) as f64 * 25.0 + pad;
                        assert_eq!(
                            wrap.restore_anchor(anchor, viewport).unwrap(),
                            offset.max(start).min((total - viewport).max(0.0))
                        );
                    }
                }
            }
            for invalid in [-1.0, f64::NAN, f64::INFINITY] {
                assert!(natural.with_content_align(invalid, align).is_err());
            }
            assert_eq!(
                wrap.with_content_align(100.0, ListContentAlign::Start)
                    .unwrap()
                    .total(),
                natural.total()
            );
        }
    }
    let huge = UniformWrap::new(usize::MAX, 180.0, 80.0, 20.0, [10.0, 5.0])
        .unwrap()
        .with_content_align(100.0, ListContentAlign::End)
        .unwrap();
    assert_eq!(huge.window(0.0, 100.0, 0, 8).unwrap().rows, 0..4);
}

#[test]
fn wrap_surrounding_content_uses_physical_geometry_without_creating_data_items() {
    for count in [0usize, 3, 31] {
        let rows = UniformWrap::new(count, 180.0, 80.0, 20.0, [10.0, 5.0]).unwrap();
        for insets in [[0.0, 0.0], [60.0, 90.0], [10.0, 5.0]] {
            for align in [
                ListContentAlign::Start,
                ListContentAlign::Center,
                ListContentAlign::End,
            ] {
                let natural = rows.total()[1] + insets[0] + insets[1];
                let free = (100.0 - natural).max(0.0);
                let pad = match align {
                    ListContentAlign::Start => 0.0,
                    ListContentAlign::Center => free * 0.5,
                    ListContentAlign::End => free,
                };
                let total = if align == ListContentAlign::Start {
                    natural
                } else {
                    natural.max(100.0)
                };
                let leading = pad + insets[0];
                let wrap = rows.with_content(insets, 100.0, align).unwrap();
                assert_eq!(wrap.total(), [180.0, total]);
                assert_eq!(wrap.content().insets()[0], leading);
                assert_eq!(wrap.grid().total(), rows.grid().total());
                for viewport in [0.0_f64, 10.0, 30.0, 100.0] {
                    for requested in [0.0_f64, 25.0, 65.0, 10000.0] {
                        let offset = requested.min((total - viewport).max(0.0));
                        let expected = |top: f64, bottom: f64| {
                            (0..count)
                                .filter(|&i| {
                                    let start = leading + (i / wrap.columns()) as f64 * 25.0;
                                    (start + 20.0).min(bottom) > start.max(top)
                                })
                                .collect::<Vec<_>>()
                        };
                        let window = wrap.window(requested, viewport, 0, count).unwrap();
                        let flat = |window: &UniformGridWindow| {
                            (window.rows.start.saturating_mul(wrap.columns()).min(count)
                                ..window.rows.end.saturating_mul(wrap.columns()).min(count))
                                .collect::<Vec<_>>()
                        };
                        assert_eq!(flat(&window), expected(offset, offset + viewport));
                        assert_eq!(window.viewport.rows.total_extent, total);
                        assert_eq!(
                            wrap.viewable_items(requested, viewport, ListViewability::AnyVisible)
                                .unwrap()
                                .collect::<Vec<_>>(),
                            flat(&window)
                        );
                        if offset < leading
                            || offset >= leading + rows.total()[1]
                            || viewport == 0.0
                        {
                            assert_eq!(wrap.anchor(requested, viewport).unwrap(), None);
                        }
                        for ahead in [-30.0_f64, 30.0] {
                            let predicted = wrap
                                .window_with_scroll_ahead(requested, viewport, 0, count, ahead)
                                .unwrap();
                            assert_eq!(predicted.viewport, window.viewport);
                            if viewport > 0.0 {
                                let lead = ahead.clamp(-viewport, viewport);
                                assert_eq!(
                                    flat(&predicted),
                                    expected(
                                        (offset + lead.min(0.0)).max(0.0),
                                        (offset + viewport + lead.max(0.0)).min(total)
                                    )
                                );
                            }
                        }
                        for index in 0..count {
                            let start = leading + (index / wrap.columns()) as f64 * 25.0;
                            assert_eq!(wrap.item_origin(index).unwrap()[1], start);
                            assert_eq!(
                                wrap.scroll_to_item(
                                    index,
                                    offset,
                                    viewport,
                                    ListScrollAlign::Start,
                                    0.0
                                )
                                .unwrap(),
                                if viewport == 0.0 {
                                    offset
                                } else {
                                    start.min((total - viewport).max(0.0))
                                }
                            );
                        }
                    }
                }
            }
        }
        for bad in [-1.0, f64::NAN, f64::INFINITY] {
            for insets in [[bad, 0.0], [0.0, bad]] {
                assert_eq!(
                    rows.with_content(insets, 100.0, ListContentAlign::Start)
                        .unwrap_err(),
                    VirtualError::InvalidExtent
                );
            }
        }
    }
    let huge = UniformWrap::new(usize::MAX, 180.0, 80.0, 20.0, [10.0, 5.0])
        .unwrap()
        .with_content([200.0, 100.0], 100.0, ListContentAlign::Start)
        .unwrap();
    assert!(huge.window(0.0, 100.0, 0, 0).unwrap().rows.is_empty());
    assert_eq!(huge.window(200.0, 100.0, 0, 8).unwrap().rows, 0..4);
}

#[test]
fn parent_viewports_match_absolute_overlap_for_fixed_measured_and_slotted_content() {
    let fixed = UniformList::new(4, 20.0, 5.0).unwrap();
    let mut measured = VirtualAxis::new(0..4, 20.0).unwrap();
    measured.set_gap(5.0).unwrap();
    let headers = ListStickyHeaders::new([0, 2]);
    for insets in [[0.0, 0.0], [60.0, 90.0]] {
        let content =
            ListContent::new(fixed.total(), insets, 0.0, ListContentAlign::Start).unwrap();
        for parent_offset in [
            0.0, 50.0, 100.0, 105.0, 120.0, 124.0, 125.0, 190.0, 195.0, 245.0, 400.0,
        ] {
            for extent in [0.0, 1.0, 4.0, 20.0, 100.0, 400.0] {
                let parent = ParentViewport::new(parent_offset, extent, 100.0).unwrap();
                let top = parent_offset - 100.0;
                let bottom = top + extent;
                let state = content
                    .viewport_in_parent(parent, |o, v| fixed.viewport(o, v))
                    .unwrap();
                assert_eq!(
                    state,
                    content
                        .viewport_in_parent(parent, |o, v| measured.viewport_geometry(o, v))
                        .unwrap()
                );
                assert_eq!(
                    (state.offset, state.viewport_extent, state.total_extent),
                    (top, extent, content.total())
                );
                let expected: Vec<_> = (0..4)
                    .filter(|&i| {
                        let start = insets[0] + i as f64 * 25.0;
                        (start + 20.0).min(bottom) > start.max(top)
                    })
                    .collect();
                assert_eq!(state.visible.clone().collect::<Vec<_>>(), expected);
                for policy in [
                    ListViewability::AnyVisible,
                    ListViewability::ItemFraction(0.75),
                    ListViewability::ViewportFraction(0.5),
                ] {
                    let expected: Vec<_> = expected
                        .iter()
                        .copied()
                        .filter(|&i| {
                            let start = insets[0] + i as f64 * 25.0;
                            let overlap = (start + 20.0).min(bottom) - start.max(top);
                            let threshold = match policy {
                                ListViewability::AnyVisible => 0.0,
                                ListViewability::ItemFraction(f) => 20.0 * f,
                                ListViewability::ViewportFraction(f) => extent * f,
                            };
                            overlap >= 20.0 || overlap >= threshold
                        })
                        .collect();
                    assert_eq!(
                        content
                            .viewable_items(&state, policy, |i| (
                                i as f64 * 25.0,
                                i as f64 * 25.0 + 20.0
                            ))
                            .unwrap()
                            .collect::<Vec<_>>(),
                        expected
                    );
                }
                if insets == [0.0, 0.0] {
                    assert_eq!(fixed.viewport_in_parent(parent).unwrap(), state);
                    assert_eq!(measured.viewport_in_parent(parent).unwrap(), state);
                }
                let pin = content
                    .sticky_header(&state, &headers, 0.0, |i| (i as f64 * 25.0, 20.0))
                    .unwrap();
                if extent == 0.0 || bottom <= 0.0 || top >= content.total() || top < insets[0] {
                    assert_eq!(pin, None);
                } else {
                    let index = if top >= insets[0] + 50.0 { 2 } else { 0 };
                    let start = insets[0] + index as f64 * 25.0;
                    let next = if index == 0 {
                        insets[0] + 50.0
                    } else {
                        insets[0] + fixed.total()
                    };
                    assert_eq!(
                        pin,
                        Some(ListStickyHeader {
                            index,
                            translation: top.min(next - 20.0).max(start) - start
                        })
                    );
                }
            }
        }
    }
    // A measured change affects visibility through the same prefix index.
    measured.measure(&1, 60.0).unwrap();
    assert_eq!(
        measured
            .viewport_in_parent(ParentViewport::new(180.0, 10.0, 100.0).unwrap())
            .unwrap()
            .visible,
        1..2
    );
    let parent = ParentViewport::new(0.0, 100.0, 300.0).unwrap();
    assert_eq!(
        parent.scroll_to(25.0, 20.0, 1000.0, ListScrollAlign::Center, 0.0),
        Ok(285.0)
    );
    assert_eq!(
        parent.scroll_to(25.0, 20.0, 1000.0, ListScrollAlign::Nearest, 0.0),
        Ok(245.0)
    );
    assert_eq!(
        ParentViewport::new(900.0, 100.0, 300.0).unwrap().scroll_to(
            25.0,
            20.0,
            1000.0,
            ListScrollAlign::Nearest,
            0.0
        ),
        Ok(325.0)
    );
    assert_eq!(
        parent.scroll_to(25.0, 20.0, 340.0, ListScrollAlign::Start, 0.0),
        Err(VirtualError::InvalidExtent)
    );
    // The edge approached before a region arrives is logical end; passing it
    // entirely rearms it. Reversed order swaps physical edges and thresholds.
    for (offset, start, end) in [
        (0.0, true, false),
        (40.0, true, true),
        (80.0, true, true),
        (120.0, true, true),
        (151.0, false, false),
    ] {
        let parent = ParentViewport::new(offset, 100.0, 100.0).unwrap();
        let edges = parent
            .load_edges(40.0, [0.25, 0.0], true, ListOrder::Forward)
            .unwrap();
        assert_eq!(edges, ListLoadEdges { start, end });
        let reversed = parent
            .load_edges(40.0, [0.0, 0.25], true, ListOrder::Reversed)
            .unwrap();
        assert_eq!(
            reversed,
            ListLoadEdges {
                start: end,
                end: start
            }
        );
    }
    assert_eq!(
        ParentViewport::new(100.0, 100.0, 100.0)
            .unwrap()
            .load_edges(0.0, [0.0; 2], false, ListOrder::Forward),
        Ok(ListLoadEdges {
            start: false,
            end: true
        })
    );
    for bad in [-1.0, f64::NAN, f64::INFINITY] {
        for field in 0..3 {
            let mut values = [0.0, 100.0, 100.0];
            values[field] = bad;
            assert_eq!(
                ParentViewport::new(values[0], values[1], values[2]),
                Err(VirtualError::InvalidExtent)
            );
        }
        assert!(parent.viewport(bad, |o, v| fixed.viewport(o, v)).is_err());
        assert!(
            parent
                .load_edges(40.0, [bad, 0.0], true, ListOrder::Forward)
                .is_err()
        );
        assert!(
            parent
                .scroll_to(0.0, 20.0, 1000.0, ListScrollAlign::Fraction(bad), 0.0)
                .is_err()
        );
    }
    assert_eq!(
        fixed
            .viewport_in_parent(ParentViewport::new(f64::MAX, f64::MAX, f64::MAX).unwrap())
            .unwrap()
            .visible,
        0..4
    );
}

#[test]
fn parent_grid_and_wrap_windows_keep_real_cells_and_parent_thresholds() {
    let grid = UniformGrid::new(4, 3, [20.0, 10.0], [5.0, 5.0]).unwrap();
    let wrap = UniformWrap::new(11, 70.0, 20.0, 10.0, [5.0, 5.0])
        .unwrap()
        .with_content([20.0, 30.0], 0.0, ListContentAlign::Start)
        .unwrap();
    for offset in [
        0.0, 50.0, 90.0, 100.0, 110.0, 120.0, 125.0, 150.0, 200.0, 400.0,
    ] {
        for extent in [0.0, 1.0, 10.0, 30.0, 100.0, 400.0] {
            let parent = ParentViewport::new(offset, extent, 100.0).unwrap();
            let top = offset - 100.0;
            let bottom = top + extent;
            let state = grid.window_in_parent([parent; 2], [0, 0], 12).unwrap();
            let expected: Vec<_> = (0..4)
                .flat_map(|row| (0..3).map(move |col| (row, col)))
                .filter(|&(row, col)| {
                    (col as f64 * 25.0 + 20.0).min(bottom) > (col as f64 * 25.0).max(top)
                        && (row as f64 * 15.0 + 10.0).min(bottom) > (row as f64 * 15.0).max(top)
                })
                .collect();
            let actual: Vec<_> = state
                .rows
                .clone()
                .flat_map(|row| state.columns.clone().map(move |col| (row, col)))
                .collect();
            assert_eq!(actual, expected);
            assert_eq!(
                (state.viewport.rows.offset, state.viewport.columns.offset),
                (top, top)
            );
            assert_eq!(
                (
                    state.viewport.rows.viewport_extent,
                    state.viewport.columns.viewport_extent
                ),
                (extent, extent)
            );
            let state = wrap.window_in_parent(parent, 0, 11).unwrap();
            let expected: Vec<_> = (0..11)
                .filter(|&i| {
                    let y = 20.0 + (i / 3) as f64 * 15.0;
                    (y + 10.0).min(bottom) > y.max(top)
                })
                .collect();
            let actual: Vec<_> = (state.rows.start.saturating_mul(3).min(11)
                ..state.rows.end.saturating_mul(3).min(11))
                .collect();
            assert_eq!(actual, expected);
            assert_eq!(
                wrap.viewable_items_in_parent(parent, ListViewability::AnyVisible)
                    .unwrap()
                    .collect::<Vec<_>>(),
                expected
            );
            let qualified: Vec<_> = expected
                .iter()
                .copied()
                .filter(|&i| {
                    let y = 20.0 + (i / 3) as f64 * 15.0;
                    let overlap = (y + 10.0).min(bottom) - y.max(top);
                    overlap == 10.0 || overlap >= extent * 0.5
                })
                .collect();
            assert_eq!(
                wrap.viewable_items_in_parent(parent, ListViewability::ViewportFraction(0.5))
                    .unwrap()
                    .collect::<Vec<_>>(),
                qualified
            );
        }
    }
    // Overscan must not mount an entire offscreen region or charge its cells.
    for offset in [0.0, 1000.0] {
        let parent = ParentViewport::new(offset, 20.0, 100.0).unwrap();
        assert!(
            grid.window_in_parent([parent; 2], [usize::MAX; 2], 0)
                .unwrap()
                .rows
                .is_empty()
        );
        assert!(
            wrap.window_in_parent(parent, usize::MAX, 0)
                .unwrap()
                .rows
                .is_empty()
        );
    }
    let last = ParentViewport::new(165.0, 10.0, 100.0).unwrap();
    assert_eq!(wrap.window_in_parent(last, 0, 2).unwrap().rows, 3..4);
    assert_eq!(
        wrap.window_in_parent(last, 0, 1),
        Err(VirtualError::Capacity)
    );
    let huge = UniformWrap::new(usize::MAX, 70.0, 20.0, 10.0, [5.0, 5.0]).unwrap();
    assert_eq!(
        huge.window_in_parent(ParentViewport::new(100.0, 10.0, 100.0).unwrap(), 0, 3)
            .unwrap()
            .rows,
        0..1
    );
    let empty = UniformWrap::new(0, 70.0, 20.0, 10.0, [5.0, 5.0])
        .unwrap()
        .with_content([20.0, 30.0], 0.0, ListContentAlign::Start)
        .unwrap();
    let state = empty.window_in_parent(last, usize::MAX, 0).unwrap();
    assert!(state.rows.is_empty());
    assert_eq!(state.viewport.rows.total_extent, 50.0);
}

#[test]
fn parent_measurements_compensate_only_complete_old_intervals_and_accumulate() {
    for gap in [0.0, 5.0] {
        for offset in [
            0.0_f64, 80.0, 100.0, 110.0, 120.0, 125.0, 145.0, 160.0, 300.0,
        ] {
            for viewport in [0.0_f64, 40.0, 500.0] {
                for index in 0..4 {
                    for extent in [5.0, 20.0, 80.0] {
                        let mut axis = VirtualAxis::new(0..4, 20.0).unwrap();
                        axis.set_gap(gap).unwrap();
                        let parent = ParentViewport::new(offset, viewport, 100.0).unwrap();
                        let start = index as f64 * (20.0 + gap);
                        let delta = extent - 20.0;
                        let total = 500.0 + delta;
                        let keeps =
                            viewport > 0.0 && offset > 100.0 && 100.0 + start + 20.0 <= offset;
                        let expected = (offset + if keeps { delta } else { 0.0 })
                            .clamp(0.0, (total - viewport).max(0.0));
                        let next = axis
                            .measure_in_parent(&index, extent, parent, total)
                            .unwrap();
                        assert_eq!(next.parent_offset(), expected);
                        assert_eq!(next.offset(), expected - 100.0);
                        assert_eq!(next.extent(), viewport);
                        assert_eq!(axis.extent(&index), Some(extent));
                        if keeps && expected == offset + delta && index < 3 {
                            assert_eq!(
                                100.0 + axis.position(index + 1).unwrap() - expected,
                                100.0 + (index + 1) as f64 * (20.0 + gap) - offset
                            );
                        }
                    }
                }
            }
        }
    }
    // Earlier compensation must be visible to later measurements in one layout.
    let mut axis = VirtualAxis::new(0..4, 20.0).unwrap();
    axis.set_gap(5.0).unwrap();
    let parent = ParentViewport::new(160.0, 40.0, 100.0).unwrap();
    let parent = axis.measure_in_parent(&0, 60.0, parent, 540.0).unwrap();
    let parent = axis.measure_in_parent(&1, 60.0, parent, 580.0).unwrap();
    assert_eq!(parent.parent_offset(), 240.0);
    assert_eq!(
        100.0 + axis.position(2).unwrap() - parent.parent_offset(),
        -10.0
    );
    // The same primitive handles a measured header/slot or an authored spacer.
    let parent = ParentViewport::new(180.0, 40.0, 100.0).unwrap();
    assert_eq!(
        parent
            .preserve_change(0.0..60.0, 100.0, 540.0)
            .unwrap()
            .parent_offset(),
        220.0
    );
    assert_eq!(
        parent
            .preserve_change(70.0..100.0, 80.0, 550.0)
            .unwrap()
            .parent_offset(),
        180.0
    );
    assert_eq!(
        parent
            .preserve_change(80.0..80.0, 30.0, 530.0)
            .unwrap()
            .parent_offset(),
        210.0
    );
    assert_eq!(
        parent
            .preserve_change(90.0..100.0, 10.0, 200.0)
            .unwrap()
            .parent_offset(),
        160.0
    );
    for (range, extent, total) in [
        (10.0..0.0, 10.0, 500.0),
        (-1.0..0.0, 0.0, 500.0),
        (0.0..f64::INFINITY, 10.0, 500.0),
        (0.0..10.0, f64::NAN, 500.0),
        (0.0..10.0, 10.0, -1.0),
        (0.0..10.0, 401.0, 500.0),
    ] {
        assert!(parent.preserve_change(range, extent, total).is_err());
    }
    let before = (axis.total(), axis.extent(&0), axis.position(4));
    for (key, extent, total) in [
        (0, 0.0, 580.0),
        (0, f64::INFINITY, 580.0),
        (9, 20.0, 580.0),
        (0, 60.0, f64::NAN),
        (0, 60.0, 50.0),
    ] {
        assert!(axis.measure_in_parent(&key, extent, parent, total).is_err());
        assert_eq!((axis.total(), axis.extent(&0), axis.position(4)), before);
    }
}

#[test]
fn parent_splices_keep_page_content_stable_and_fail_atomically() {
    for gap in [0.0, 5.0] {
        for start in 0..=4 {
            for end in start..=4 {
                for inserted in 0..=3 {
                    let inserted_rows: Vec<_> = [7.0, 13.0, 29.0]
                        .into_iter()
                        .take(inserted)
                        .enumerate()
                        .map(|(i, extent)| (100 + i, extent))
                        .collect();
                    let mut expected_rows: Vec<_> = (0..4).map(|i| (i, 20.0)).collect();
                    expected_rows.splice(start..end, inserted_rows.clone());
                    let total = expected_rows.iter().map(|(_, e)| e).sum::<f64>()
                        + expected_rows.len().saturating_sub(1) as f64 * gap;
                    let delta = total - (80.0 + 3.0 * gap);
                    let old_end = end as f64 * 20.0 + end.min(3) as f64 * gap;
                    for offset in [
                        0.0_f64, 100.0, 110.0, 120.0, 125.0, 145.0, 160.0, 195.0, 300.0,
                    ] {
                        for viewport in [0.0_f64, 40.0, 500.0] {
                            let mut axis = VirtualAxis::new(0..4, 20.0).unwrap();
                            axis.set_gap(gap).unwrap();
                            let parent = ParentViewport::new(offset, viewport, 100.0).unwrap();
                            let keeps =
                                viewport > 0.0 && offset > 100.0 && 100.0 + old_end <= offset;
                            let expected_offset = (offset + if keeps { delta } else { 0.0 })
                                .clamp(0.0, (500.0 + delta - viewport).max(0.0));
                            let parent = axis
                                .splice_in_parent(
                                    start..end,
                                    inserted_rows.clone(),
                                    parent,
                                    500.0 + delta,
                                )
                                .unwrap();
                            assert_eq!(
                                parent.parent_offset(),
                                expected_offset,
                                "gap={gap}, range={start}..{end}, inserted={inserted}, offset={offset}, viewport={viewport}"
                            );
                            assert_eq!(axis.total(), total);
                            assert_eq!(axis.len(), expected_rows.len());
                            let mut position = 0.0;
                            for (i, (key, extent)) in expected_rows.iter().enumerate() {
                                assert_eq!(axis.key(i), Some(key));
                                assert_eq!(axis.index(key), Some(i));
                                assert_eq!(axis.extent(key), Some(*extent));
                                assert_eq!(axis.position(i), Some(position));
                                position += extent + gap;
                            }
                            let state = axis.viewport_in_parent(parent).unwrap();
                            let visible: Vec<_> = expected_rows
                                .iter()
                                .enumerate()
                                .filter_map(|(i, (_, e))| {
                                    let top = 100.0 + axis.position(i).unwrap();
                                    (viewport > 0.0
                                        && top < expected_offset + viewport
                                        && top + e > expected_offset)
                                        .then_some(i)
                                })
                                .collect();
                            assert_eq!(state.visible.collect::<Vec<_>>(), visible);
                        }
                    }
                }
            }
        }
    }
    // Removed keys may be reused with new sizes; unrelated measurements survive.
    let mut axis = VirtualAxis::new(0..4, 20.0).unwrap();
    axis.set_gap(5.0).unwrap();
    axis.measure(&3, 37.0).unwrap();
    axis.splice(1..3, [(2, 8.0), (1, 9.0)]).unwrap();
    assert_eq!(
        (axis.extent(&3), axis.key(1), axis.extent(&2)),
        (Some(37.0), Some(&2), Some(8.0))
    );
    let parent = ParentViewport::new(200.0, 40.0, 100.0).unwrap();
    let snapshot = |a: &VirtualAxis<usize>| {
        (
            a.total(),
            (0..a.len())
                .map(|i| {
                    let key = *a.key(i).unwrap();
                    (key, a.extent(&key).unwrap(), a.position(i).unwrap())
                })
                .collect::<Vec<_>>(),
        )
    };
    let before = snapshot(&axis);
    for (range, items, total) in [
        (2..1, vec![], 500.0),
        (0..5, vec![], 500.0),
        (1..2, vec![(0, 10.0)], 500.0),
        (1..2, vec![(100, 8.0), (100, 9.0)], 500.0),
        (1..2, vec![(100, 0.0)], 500.0),
        (1..2, vec![(100, f64::NAN)], 500.0),
        (1..2, vec![(100, 8.0)], f64::NAN),
        (1..2, vec![(100, 8.0)], 50.0),
    ] {
        assert!(axis.splice_in_parent(range, items, parent, total).is_err());
        assert_eq!(snapshot(&axis), before);
    }
    let mut empty = VirtualAxis::new([], 20.0).unwrap();
    let parent = empty
        .splice_in_parent(0..0, [(1, 20.0)], parent, 520.0)
        .unwrap();
    assert_eq!((empty.total(), parent.parent_offset()), (20.0, 220.0));
}

#[test]
fn parent_grid_prediction_keeps_visibility_budget_and_parent_navigation_coordinates() {
    for count in [0, 1, 7, 1_000_000] {
        let grid = UniformGrid::new(count, count, [20.0, 10.0], [5.0, 3.0]).unwrap();
        let wrap = UniformWrap::new(count, 70.0, 20.0, 10.0, [5.0, 3.0])
            .unwrap()
            .with_content([15.0, 25.0], 0.0, ListContentAlign::Start)
            .unwrap();
        for requested in [0.0, 125.0, 150.0, 180.0, 900.0, 1e12] {
            let parent = ParentViewport::new(requested, 50.0, 150.0).unwrap();
            for capacity in [0, 4, 16, 128] {
                let base = grid.window_in_parent([parent; 2], [1; 2], capacity);
                assert_eq!(
                    grid.window_in_parent_with_scroll_ahead(
                        [parent; 2],
                        [1; 2],
                        capacity,
                        [0.0; 2]
                    ),
                    base
                );
                let wrap_base = wrap.window_in_parent(parent, 1, capacity);
                assert_eq!(
                    wrap.window_in_parent_with_scroll_ahead(parent, 1, capacity, 0.0),
                    wrap_base
                );
                for ahead in [-1e6, -20.0, 20.0, 1e6] {
                    if let Ok(base) = &base {
                        let predicted = grid
                            .window_in_parent_with_scroll_ahead(
                                [parent; 2],
                                [1; 2],
                                capacity,
                                [ahead; 2],
                            )
                            .unwrap();
                        assert_eq!(predicted.viewport, base.viewport);
                        assert!(predicted.columns.len() * predicted.rows.len() <= capacity);
                        assert!(
                            predicted.columns.start <= base.columns.start
                                && predicted.columns.end >= base.columns.end
                        );
                        if base.rows.is_empty() || base.columns.is_empty() {
                            assert!(predicted.rows.is_empty() && predicted.columns.is_empty());
                        }
                    }
                    if let Ok(base) = &wrap_base {
                        let predicted = wrap
                            .window_in_parent_with_scroll_ahead(parent, 1, capacity, ahead)
                            .unwrap();
                        assert_eq!(predicted.viewport, base.viewport);
                        let real = (predicted.rows.end * wrap.columns()).min(count)
                            - (predicted.rows.start * wrap.columns()).min(count);
                        assert!(real <= capacity);
                        if parent.offset() >= wrap.total()[1]
                            || parent.offset() + parent.extent() <= 0.0
                        {
                            assert!(predicted.rows.is_empty());
                        }
                    }
                }
            }
            assert_eq!(
                grid.window_in_parent_with_scroll_ahead([parent; 2], [0; 2], 128, [f64::NAN; 2]),
                Err(VirtualError::InvalidExtent)
            );
            assert_eq!(
                wrap.window_in_parent_with_scroll_ahead(parent, 0, 128, f64::NAN),
                Err(VirtualError::InvalidExtent)
            );
        }
    }
    let grid = UniformGrid::new(20, 20, [20.0, 10.0], [5.0, 3.0]).unwrap();
    for parents in [
        [
            ParentViewport::new(0.0, 50.0, 150.0).unwrap(),
            ParentViewport::new(180.0, 50.0, 150.0).unwrap(),
        ],
        [
            ParentViewport::new(180.0, 50.0, 150.0).unwrap(),
            ParentViewport::new(180.0, 0.0, 150.0).unwrap(),
        ],
    ] {
        let base = grid.window_in_parent(parents, [1; 2], 128).unwrap();
        assert!(base.rows.is_empty() || base.columns.is_empty());
        assert_eq!(
            grid.window_in_parent_with_scroll_ahead(parents, [1; 2], 128, [1e6; 2]),
            Ok(base)
        );
    }
    let parent = [ParentViewport::new(10.0, 100.0, 150.0).unwrap(); 2];
    assert_eq!(
        grid.scroll_to_cell_in_parent(
            3,
            4,
            parent,
            [1000.0; 2],
            [ListScrollAlign::Start; 2],
            [0.0; 2]
        )
        .unwrap(),
        [250.0, 189.0]
    );
    assert!(
        grid.scroll_to_cell_in_parent(
            30,
            4,
            parent,
            [1000.0; 2],
            [ListScrollAlign::Start; 2],
            [0.0; 2]
        )
        .is_err()
    );
    let wrap = UniformWrap::new(7, 70.0, 20.0, 10.0, [5.0, 3.0])
        .unwrap()
        .with_content([15.0, 25.0], 0.0, ListContentAlign::Start)
        .unwrap();
    assert_eq!(
        wrap.scroll_to_item_in_parent(6, parent[1], 1000.0, ListScrollAlign::Start, 5.0)
            .unwrap(),
        186.0
    );
    assert!(
        wrap.scroll_to_item_in_parent(7, parent[1], 1000.0, ListScrollAlign::Start, 0.0)
            .is_err()
    );
}

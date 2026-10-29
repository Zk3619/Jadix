#[test]
fn resized_parent_is_dirty_when_nested_menu_closes() {
    let previous = [
        (1, [100.0, 100.0, 280.0, 340.0]),
        (2, [110.0, 330.0, 144.0, 200.0]),
    ];
    let current = [(1, [100.0, 100.0, 280.0, 368.0])];
    assert_eq!(
        overlay_membership_paint_stamps(&previous, &current),
        current.to_vec()
    );
}

#[cfg(test)]
#[test]
fn distant_culled_edges_must_not_repaint_the_middle_of_a_4k_viewport() {
    let viewport = [0.0, 0.0, 3840.0, 2160.0];
    let previous = [
        (1, [0.0, 0.0, 3840.0, 20.0]),
        (2, [0.0, 2140.0, 3840.0, 20.0]),
    ];
    let keys = HashSet::from([1_u64, 2_u64]);
    let pins = clip_culled_scroll_blit_pins(&previous, &keys, &[viewport], &[[0.0, 24.0]]);
    assert!(
        pins.iter().all(|r| !(1920.0 >= r[0]
            && 1920.0 < r[0] + r[2]
            && 1080.0 >= r[1]
            && 1080.0 < r[1] + r[3])),
        "middle was already copied; pins={pins:?}"
    );
    assert!(pins.iter().all(|r| r[3] <= 44.0));
}

#[cfg(test)]
#[test]
fn newly_collected_overflow_ink_repairs_the_retained_scroll_overlap() {
    let viewport = [0.0, 0.0, 300.0, 240.0];
    let previous = [(1, [0.0, 200.0, 300.0, 40.0])];
    let current = [
        (1, [0.0, 170.0, 300.0, 40.0]),
        (2, [60.0, 192.0, 180.0, 80.0]),
        (3, [0.0, 300.0, 300.0, 20.0]),
    ];
    assert_eq!(
        changed_scroll_blit_pins(
            &previous,
            &current,
            &HashSet::from([1]),
            &[viewport],
            &[[0.0, 30.0]]
        ),
        vec![[60.0, 192.0, 180.0, 48.0]],
    );
}

#[cfg(test)]
#[test]
fn resized_clipped_ink_repairs_both_old_and_new_coverage() {
    let viewport = [0.0, 0.0, 300.0, 240.0];
    let previous = [
        (1, [60.0, 210.0, 180.0, 40.0]),
        (2, [20.0, 100.0, 30.0, 30.0]),
    ];
    let current = [
        (1, [60.0, 180.0, 180.0, 60.0]),
        (2, [20.0, 70.0, 30.0, 60.0]),
    ];
    assert_eq!(
        changed_scroll_blit_pins(
            &previous,
            &current,
            &HashSet::from([1]),
            &[viewport],
            &[[0.0, 30.0]]
        ),
        vec![[60.0, 180.0, 180.0, 60.0]],
    );
    assert_eq!(
        changed_scroll_blit_pins(
            &current,
            &previous,
            &HashSet::from([1]),
            &[viewport],
            &[[0.0, -30.0]]
        ),
        vec![[60.0, 210.0, 180.0, 30.0]],
    );
}

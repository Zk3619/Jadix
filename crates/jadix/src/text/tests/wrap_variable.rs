use super::*;
use crate::text::measure::prepare;

#[test]
fn first_n_lines_use_narrow_width() {
    // Latin words that wrap at soft spaces.
    let prepared = prepare(
        "alpha bravo charlie delta echo foxtrot golf hotel",
        14.0,
        400,
        21.0,
    );
    let wide = layout_lines(&prepared, 10_000.0);
    assert_eq!(wide.len(), 1, "unwrapped should be one line");

    let narrow = layout_lines(&prepared, 80.0);
    assert!(
        narrow.len() >= 3,
        "narrow wrap should produce several lines, got {}",
        narrow.len()
    );

    // Obstacle: first 2 lines narrower, rest wider → fewer total lines than all-narrow.
    let mixed = layout_lines_variable_width(&prepared, 80.0, 10_000.0, 2);
    assert!(
        mixed.len() >= 2,
        "expected at least the obstacle lines, got {}",
        mixed.len()
    );
    assert!(
        mixed.len() < narrow.len() || mixed.len() == narrow.len(),
        "wider rest should not produce more lines than all-narrow ({} vs {})",
        mixed.len(),
        narrow.len()
    );
    // After first_lines, remaining text should fit on one wide line when width_b is huge.
    if mixed.len() > 2 {
        assert_eq!(
            mixed.len(),
            3,
            "two narrow + one wide rest expected, got {}",
            mixed.len()
        );
    }
}

#[test]
fn zero_first_lines_uses_width_b_only() {
    let prepared = prepare("one two three four five six", 14.0, 400, 21.0);
    let a = layout_lines(&prepared, 60.0);
    let b = layout_lines_variable_width(&prepared, 60.0, 10_000.0, 0);
    let wide = layout_lines(&prepared, 10_000.0);
    assert_eq!(b.len(), wide.len());
    assert!(a.len() >= b.len());
}

#[test]
fn cursor_consumes_hard_breaks_once() {
    for source in [
        "alpha\n\nbravo\n",
        "\nalpha",
        "中文测试\n第二段",
        "alpha bravo charlie",
    ] {
        let prepared = prepare(source, 14.0, 400, 21.0);
        let expected = layout_lines(&prepared, 90.0);
        let mut cursor = LayoutCursor::default();
        let mut actual = Vec::new();
        while let Some((line, next)) = layout_next_line(&prepared, cursor, 90.0) {
            assert!(next.segment_index > cursor.segment_index);
            actual.push(line);
            cursor = next;
        }
        assert_eq!(
            actual
                .iter()
                .map(|line| (&line.start_seg, &line.end_seg, &line.text))
                .collect::<Vec<_>>(),
            expected
                .iter()
                .map(|line| (&line.start_seg, &line.end_seg, &line.text))
                .collect::<Vec<_>>(),
            "{source}"
        );
        assert_eq!(
            layout_lines_variable_width(&prepared, 90.0, 90.0, 2)
                .iter()
                .map(|line| (&line.start_seg, &line.end_seg, &line.text))
                .collect::<Vec<_>>(),
            expected
                .iter()
                .map(|line| (&line.start_seg, &line.end_seg, &line.text))
                .collect::<Vec<_>>()
        );
    }
}

#[test]
fn flow_resumes_across_columns_without_losing_text_or_crossing_shapes() {
    let source = "alpha bravo charlie delta echo foxtrot 中文排版绕过图形。\n\n第二段 text continues across columns. ".repeat(8);
    let prepared = prepare(&source, 14.0, 400, 21.0);
    let exclusions = [
        TextExclusion::Circle {
            center: [150.0, 84.0],
            radius: 52.0,
        },
        TextExclusion::Rect(TextFlowRect {
            x: 20.0,
            y: 160.0,
            width: 90.0,
            height: 45.0,
        }),
        TextExclusion::Rect(TextFlowRect {
            x: 40.0,
            y: 180.0,
            width: 90.0,
            height: 45.0,
        }),
    ];
    let mut cursor = LayoutCursor::default();
    let mut actual = String::new();
    let mut saw_split_row = false;
    let mut previous_y = None;
    for column in 0..20 {
        let region = TextFlowRect {
            x: 0.0,
            y: 0.0,
            width: 300.0,
            height: 252.0,
        };
        let flow = layout_text_flow(&prepared, cursor, region, &exclusions);
        assert!(flow.cursor.segment_index > cursor.segment_index);
        for fragment in &flow.lines {
            assert!(fragment.line.width <= fragment.available_width);
            assert!(fragment.x >= 0.0 && fragment.x + fragment.line.width <= 300.0);
            assert!(fragment.y >= 0.0 && fragment.y + 21.0 <= 252.0);
            for exclusion in exclusions {
                if let Some((left, right)) = exclusion.interval(fragment.y, fragment.y + 21.0) {
                    assert!(fragment.x + fragment.line.width <= left || fragment.x >= right);
                }
            }
            saw_split_row |= previous_y == Some((column, fragment.y));
            previous_y = Some((column, fragment.y));
            actual.push_str(&fragment.line.text);
        }
        cursor = flow.cursor;
        if cursor.segment_index == prepared.segment_count() {
            break;
        }
    }
    assert_eq!(cursor.segment_index, prepared.segment_count());
    assert_eq!(actual, source.replace('\n', ""));
    assert!(saw_split_row);
}

#[test]
fn blocked_rows_and_unbreakable_words_do_not_consume_text() {
    let prepared = prepare("unbreakableword", 14.0, 400, 21.0);
    let region = TextFlowRect {
        x: 0.0,
        y: 0.0,
        width: 400.0,
        height: 84.0,
    };
    let blocked = layout_text_flow(
        &prepared,
        LayoutCursor::default(),
        region,
        &[TextExclusion::Rect(TextFlowRect {
            height: 42.0,
            ..region
        })],
    );
    assert_eq!(blocked.lines.len(), 1);
    assert_eq!(blocked.lines[0].y, 42.0);
    assert_eq!(blocked.lines[0].line.text, "unbreakableword");
    for region in [
        TextFlowRect {
            width: 1.0,
            ..region
        },
        TextFlowRect {
            width: f32::NAN,
            ..region
        },
        TextFlowRect {
            height: 20.0,
            ..region
        },
    ] {
        let flow = layout_text_flow(&prepared, LayoutCursor::default(), region, &[]);
        assert!(flow.lines.is_empty());
        assert_eq!(flow.cursor, LayoutCursor::default());
    }
}

#[test]
fn circular_exclusion_uses_the_whole_line_band() {
    let circle = TextExclusion::Circle {
        center: [100.0, 100.0],
        radius: 50.0,
    };
    assert_eq!(circle.interval(80.0, 110.0), Some((50.0, 150.0)));
    let (left, right) = circle.interval(50.0, 70.0).unwrap();
    assert!((left - 60.0).abs() < 0.001);
    assert!((right - 140.0).abs() < 0.001);
    assert_eq!(circle.interval(150.0, 171.0), None);
}

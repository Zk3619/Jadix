use super::*;

#[test]
fn parse_hms_accepts_seconds_and_legacy_hm() {
    assert_eq!(parse_hms("09:30:45"), Some((9, 30, 45)));
    assert_eq!(parse_hms("09:30"), Some((9, 30, 0)));
    assert_eq!(parse_hms("2026-08-14T09:30:05"), Some((9, 30, 5)));
    assert_eq!(parse_hms("24:00"), None);
    assert_eq!(parse_hms("12:60"), None);
}

#[test]
fn parse_time_unit_clamps() {
    assert_eq!(parse_time_unit("09", 23), 9);
    assert_eq!(parse_time_unit("99", 23), 23);
    assert_eq!(parse_time_unit("ab", 59), 0);
    assert_eq!(format_hms_parts("9", "5", ""), "09:05:00");
}

#[test]
fn datetime_hms_sync_writes_iso_local() {
    use crate::core::reactivity::create_root;
    let (_h, out) = create_root(|| {
        let val = use_signal("t-dt-val", || "2026-08-13T09:00:00".to_string());
        let hour = use_signal("t-dt-h", || "09".to_string());
        let minute = use_signal("t-dt-mi", || "00".to_string());
        let second = use_signal("t-dt-s", || "06".to_string());
        sync_datetime_value_from_hms(val, hour, minute, second);
        val.peek()
    });
    assert_eq!(out, "2026-08-13T09:00:06");
}

#[test]
fn datetime_panel_effect_ignores_hms_and_val() {
    use crate::core::reactivity::{create_effect, create_root};
    use std::cell::Cell;
    use std::rc::Rc;
    create_root(|| {
        let open = use_signal("t-p-open", || true);
        let val = use_signal("t-p-val", || "2026-08-13T09:00:00".to_string());
        let year = use_signal("t-p-y", || 2026i32);
        let month = use_signal("t-p-m", || 8u32);
        let hour = use_signal("t-p-h", || "09".to_string());
        let minute = use_signal("t-p-mi", || "00".to_string());
        let second = use_signal("t-p-s", || "00".to_string());
        let runs = Rc::new(Cell::new(0u32));
        create_effect({
            let runs = runs.clone();
            move || {
                runs.set(runs.get() + 1);
                let _ = open.get();
                isolate_reads(|| {
                    let _ = build_date_picker_panel_content(
                        "t-p",
                        DatePickerKind::DateTimeLocal,
                        CalendarCaption::Label,
                        open,
                        val,
                        year,
                        month,
                        hour,
                        minute,
                        second,
                    );
                });
            }
        });
        assert_eq!(runs.get(), 1);
        hour.set("10".into());
        second.set("06".into());
        val.set("2026-08-13T10:00:06".into());
        assert_eq!(runs.get(), 1, "改 HMS/val 不得重建 DatePicker portal");
        open.set(false);
        assert!(runs.get() >= 2);
    });
}

#[test]
fn month_picker_panel_keeps_caption_select_layout() {
    use crate::core::reactivity::create_root;
    use crate::core::tree::LayoutTree;

    let (_owner, mut panel) = create_root(|| {
        let open = use_signal("t-mp-open", || true);
        let val = use_signal("t-mp-val", || "2026-08".to_string());
        let year = use_signal("t-mp-y", || 2026i32);
        let month = use_signal("t-mp-m", || 8u32);
        let hour = use_signal("t-mp-h", || "09".to_string());
        let minute = use_signal("t-mp-mi", || "00".to_string());
        let second = use_signal("t-mp-s", || "00".to_string());
        build_date_picker_panel_content(
            "t-mp",
            DatePickerKind::Month,
            CalendarCaption::Label,
            open,
            val,
            year,
            month,
            hour,
            minute,
            second,
        )
    });

    let mut layout = LayoutTree::new();
    layout.build_and_layout(&mut panel, 400.0, 400.0, [-1.0, -1.0]);

    fn find_tid<'a>(el: &'a Element, want: &str) -> Option<&'a Element> {
        if el.template_id.as_deref() == Some(want) {
            return Some(el);
        }
        el.children.iter().find_map(|c| find_tid(c, want))
    }
    let month = find_tid(&panel, "t-mp:dp_m__select").expect("month select");
    let year = find_tid(&panel, "t-mp:dp_y__select").expect("year select");
    assert!(
            month.layout_bounds[2] > 40.0 && year.layout_bounds[2] > 40.0,
            "Month picker selects must not collapse (flex-basis 0 needs a definite panel width); month={:?} year={:?} panel={:?}",
            month.layout_bounds,
            year.layout_bounds,
            panel.layout_bounds
        );
    assert!(
        panel.layout_bounds[2] > 200.0,
        "Month picker panel must keep calendar width, got {:?}",
        panel.layout_bounds
    );
    let pad = crate::layout::dimension::css_root_font_size_px() * 0.75;
    let top = month.layout_bounds[1] - panel.layout_bounds[1];
    assert!(
            top > pad - 2.0 && top < pad + 6.0,
            "month row must sit at panel padding (not padding+gap); top={top} pad={pad} panel={:?} month={:?}",
            panel.layout_bounds,
            month.layout_bounds
        );
}

#[test]
fn datetime_time_row_sits_near_separator() {
    use crate::core::reactivity::create_root;
    use crate::core::tree::LayoutTree;

    let (_owner, mut panel) = create_root(|| {
        let open = use_signal("t-dt-gap-open", || true);
        let val = use_signal("t-dt-gap-val", || "2026-09-15T09:00:00".to_string());
        let year = use_signal("t-dt-gap-y", || 2026i32);
        let month = use_signal("t-dt-gap-m", || 9u32);
        let hour = use_signal("t-dt-gap-h", || "09".to_string());
        let minute = use_signal("t-dt-gap-mi", || "00".to_string());
        let second = use_signal("t-dt-gap-s", || "00".to_string());
        build_date_picker_panel_content(
            "t-dt-gap",
            DatePickerKind::DateTimeLocal,
            CalendarCaption::Label,
            open,
            val,
            year,
            month,
            hour,
            minute,
            second,
        )
    });

    let mut layout = LayoutTree::new();
    layout.build_and_layout(&mut panel, 400.0, 640.0, [-1.0, -1.0]);

    fn find_tid<'a>(el: &'a Element, want: &str) -> Option<&'a Element> {
        if el.template_id.as_deref() == Some(want) {
            return Some(el);
        }
        el.children.iter().find_map(|c| find_tid(c, want))
    }
    fn find_hairline<'a>(el: &'a Element) -> Option<&'a Element> {
        let h = el.layout_bounds[3];
        let w = el.layout_bounds[2];
        if h > 0.0 && h <= 2.0 && w > 80.0 {
            return Some(el);
        }
        el.children.iter().find_map(|c| find_hairline(c))
    }

    let sep = find_hairline(&panel).expect("datetime separator");
    let hour = find_tid(&panel, "t-dt-gap__time__h__combobox").expect("hour combobox");
    let gap = hour.layout_bounds[1] - (sep.layout_bounds[1] + sep.layout_bounds[3]);
    let pad = crate::layout::dimension::css_root_font_size_px() * 0.75;
    assert!(
            gap > pad - 2.0 && gap < pad + 6.0,
            "time row must sit one padding below the hairline (not wrapper pt+gap+row py); gap={gap} pad={pad} sep={:?} hour={:?}",
            sep.layout_bounds,
            hour.layout_bounds
        );
}

#[test]
fn time_kind_parses_and_formats_trigger() {
    assert_eq!(DatePickerKind::parse("time"), DatePickerKind::Time);
    assert_eq!(DatePickerKind::Time.as_str(), "time");
    assert_eq!(
        format_trigger_value(DatePickerKind::Time, "09:30"),
        "09:30:00"
    );
    assert_eq!(
        format_trigger_value(DatePickerKind::Time, "18:05:07"),
        "18:05:07"
    );
}

#[test]
fn time_hms_sync_writes_clock() {
    use crate::core::reactivity::create_root;
    let (_h, out) = create_root(|| {
        let val = use_signal("t-tm-val", || String::new());
        let hour = use_signal("t-tm-h", || "09".to_string());
        let minute = use_signal("t-tm-mi", || "30".to_string());
        let second = use_signal("t-tm-s", || "05".to_string());
        sync_time_value_from_hms(val, hour, minute, second);
        val.peek()
    });
    assert_eq!(out, "09:30:05");
}

#[test]
fn time_picker_uses_date_picker_overlay_shell() {
    use crate::core::reactivity::create_root;

    let (_owner, el) = create_root(|| {
        let open = use_signal("t-tp-ov__open", || false);
        let val = use_signal("t-tp-ov__value", || String::new());
        let year = use_signal("t-tp-ov__y", || 2026i32);
        let month = use_signal("t-tp-ov__m", || 8u32);
        let hour = use_signal("t-tp-ov__h", || String::new());
        let minute = use_signal("t-tp-ov__mi", || String::new());
        let second = use_signal("t-tp-ov__s", || String::new());
        build_date_picker_view(
            DatePickerRenderProps {
                id: "t-tp-ov".into(),
                kind: DatePickerKind::Time,
                caption: CalendarCaption::Label,
                disabled: false,
            },
            open,
            val,
            year,
            month,
            hour,
            minute,
            second,
            SlotChrome::default(),
            SlotChrome::default(),
        )
    });

    fn find_tid<'a>(el: &'a Element, want: &str) -> Option<&'a Element> {
        if el.template_id.as_deref() == Some(want) {
            return Some(el);
        }
        el.children.iter().find_map(|c| find_tid(c, want))
    }

    assert!(
        find_tid(&el, "t-tp-ov__trigger_view").is_some(),
        "TimePicker must use the DatePicker trigger island"
    );
    assert!(
        find_tid(&el, "t-tp-ov__view").is_some(),
        "TimePicker must host HMS in anchored_overlay_content, not an inline row"
    );
    assert!(
        find_tid(&el, "t-tp-ov__time__h__combobox").is_none(),
        "HMS comboboxes belong in the overlay, not the closed trigger row"
    );

    let (_from_owner, live) = create_root(|| time_picker("t-tp-from").into());
    assert!(
        find_tid(&live, "t-tp-from__dp_view").is_some(),
        "TimePicker builder must reuse DatePicker live view identity"
    );
}

#[test]
fn time_picker_panel_hosts_hms_at_calendar_width() {
    use crate::core::reactivity::create_root;
    use crate::core::tree::LayoutTree;

    let (_owner, mut panel) = create_root(|| {
        let open = use_signal("t-tm-p-open", || true);
        let val = use_signal("t-tm-p-val", || "09:30:00".to_string());
        let year = use_signal("t-tm-p-y", || 2026i32);
        let month = use_signal("t-tm-p-m", || 8u32);
        let hour = use_signal("t-tm-p-h", || "09".to_string());
        let minute = use_signal("t-tm-p-mi", || "30".to_string());
        let second = use_signal("t-tm-p-s", || "00".to_string());
        build_date_picker_panel_content(
            "t-tm-p",
            DatePickerKind::Time,
            CalendarCaption::Label,
            open,
            val,
            year,
            month,
            hour,
            minute,
            second,
        )
    });

    let mut layout = LayoutTree::new();
    layout.build_and_layout(&mut panel, 400.0, 400.0, [-1.0, -1.0]);

    fn find_tid<'a>(el: &'a Element, want: &str) -> Option<&'a Element> {
        if el.template_id.as_deref() == Some(want) {
            return Some(el);
        }
        el.children.iter().find_map(|c| find_tid(c, want))
    }

    let hour = find_tid(&panel, "t-tm-p__time__h__combobox").expect("hour combobox");
    let minute = find_tid(&panel, "t-tm-p__time__m__combobox").expect("minute combobox");
    let second = find_tid(&panel, "t-tm-p__time__s__combobox").expect("second combobox");
    assert!(
        hour.layout_bounds[2] > 40.0
            && minute.layout_bounds[2] > 40.0
            && second.layout_bounds[2] > 40.0,
        "Time picker HMS comboboxes must not collapse; hour={:?} minute={:?} second={:?}",
        hour.layout_bounds,
        minute.layout_bounds,
        second.layout_bounds
    );
    assert!(
        panel.layout_bounds[2] > 200.0,
        "Time picker panel must keep calendar width, got {:?}",
        panel.layout_bounds
    );
}

#[test]
fn time_panel_effect_ignores_hms_and_val() {
    use crate::core::reactivity::{create_effect, create_root};
    use std::cell::Cell;
    use std::rc::Rc;
    create_root(|| {
        let open = use_signal("t-tm-fx-open", || true);
        let val = use_signal("t-tm-fx-val", || "09:00:00".to_string());
        let year = use_signal("t-tm-fx-y", || 2026i32);
        let month = use_signal("t-tm-fx-m", || 8u32);
        let hour = use_signal("t-tm-fx-h", || "09".to_string());
        let minute = use_signal("t-tm-fx-mi", || "00".to_string());
        let second = use_signal("t-tm-fx-s", || "00".to_string());
        let runs = Rc::new(Cell::new(0u32));
        create_effect({
            let runs = runs.clone();
            move || {
                runs.set(runs.get() + 1);
                let _ = open.get();
                isolate_reads(|| {
                    let _ = build_date_picker_panel_content(
                        "t-tm-fx",
                        DatePickerKind::Time,
                        CalendarCaption::Label,
                        open,
                        val,
                        year,
                        month,
                        hour,
                        minute,
                        second,
                    );
                });
            }
        });
        assert_eq!(runs.get(), 1);
        hour.set("10".into());
        second.set("06".into());
        val.set("10:00:06".into());
        assert_eq!(runs.get(), 1, "改 HMS/val 不得重建 TimePicker portal");
        open.set(false);
        assert!(runs.get() >= 2);
    });
}

use super::*;

#[test]
fn weekday_known_dates() {
    assert_eq!(weekday(2026, 8, 14), 5, "Fri");
    assert_eq!(weekday(1970, 1, 1), 4, "Thu");
}

#[test]
fn monday_week_leading_cells() {
    // 2026-09-01 is Tuesday (2). Monday start → one leading cell (Mon empty, Tue = 1).
    assert_eq!(week_grid_leading(2, 1), 1);
    // Sunday start → two leading cells.
    assert_eq!(week_grid_leading(2, 0), 2);
}

#[test]
fn add_days_crosses_month() {
    assert_eq!(add_days(2026, 1, 31, 1), (2026, 2, 1));
    assert_eq!(add_days(2026, 3, 1, -1), (2026, 2, 28));
}

#[test]
fn parse_and_format_iso() {
    assert_eq!(parse_ymd("2026-08-14"), Some((2026, 8, 14)));
    assert_eq!(parse_ymd("2026-08-14T09:30"), Some((2026, 8, 14)));
    assert_eq!(format_ymd(2026, 8, 1), "2026-08-01");
}

#[test]
fn long_date_format() {
    assert_eq!(format_long_date(2026, 8, 14), "August 14, 2026");
    assert_eq!(format_month_year(2026, 8), "August 2026");
}

#[test]
fn day_grid_min_h_covers_weekday_plus_six_weeks() {
    // 7 × CELL(2rem) + 6 × gap(0.5rem)
    assert_eq!(CALENDAR_DAY_GRID_MIN_H, "17rem");
}

#[test]
fn caption_parse() {
    assert_eq!(
        CalendarCaption::parse("dropdown"),
        CalendarCaption::Dropdown
    );
    assert_eq!(CalendarCaption::parse("label"), CalendarCaption::Label);
}

#[test]
fn outside_month_days_use_muted_foreground() {
    let val = use_signal("t-cal-out-val", || "2026-08-14".to_string());
    let year = use_signal("t-cal-out-y", || 2026i32);
    let month = use_signal("t-cal-out-m", || 8u32);
    let focus = use_signal("t-cal-out-f", || 14u32);
    let msel = use_signal("t-cal-out-msel", || "8".to_string());
    let ysel = use_signal("t-cal-out-ysel", || "2026".to_string());
    let grid = build_calendar_day_grid(
        "t-cal-out",
        false,
        val,
        year,
        month,
        focus,
        msel,
        ysel,
        None,
        SlotMap::default(),
    );
    fn find_fid<'a>(el: &'a Element, fid: &str) -> Option<&'a Element> {
        if el.focus_id.as_deref() == Some(fid) {
            return Some(el);
        }
        el.children.iter().find_map(|c| find_fid(c, fid))
    }
    fn text_fg(el: &Element) -> Option<[u8; 4]> {
        el.children
            .iter()
            .find_map(|c| c.text.as_ref().map(|_| c.text_style.color_resolved()))
    }
    let muted = parse_text_color("muted-foreground");
    // Aug 2026 starts Saturday → d0-0 is July 26 (outside).
    let outside = find_fid(&grid, "t-cal-out:d0-0").expect("July 26 cell");
    assert_eq!(text_fg(outside), Some(muted));
    assert_eq!(
        outside
            .authoring_state
            .get::<crate::components::button::ButtonHoverPaint>()
            .and_then(|p| p.text_color),
        Some(muted),
        "hover paint must keep muted, not ghost foreground"
    );
    let inside = find_fid(&grid, "t-cal-out:d0-6").expect("Aug 1 cell");
    assert_ne!(
        text_fg(inside),
        Some(muted),
        "in-month day stays variant fg"
    );
}

#[test]
fn calendar_emits_selected_iso() {
    let val = use_signal("t-cal-val", || "2026-08-14".to_string());
    let year = use_signal("t-cal-y", || 2026i32);
    let month = use_signal("t-cal-m", || 8u32);
    let focus = use_signal("t-cal-f", || 14u32);
    let msel = use_signal("t-cal-msel", || "8".to_string());
    let ysel = use_signal("t-cal-ysel", || "2026".to_string());
    let el = build_calendar_view(
        CalendarRenderProps {
            id: "t-cal".into(),
            caption: CalendarCaption::Label,
            disabled: false,
            bordered: true,
        },
        val,
        year,
        month,
        focus,
        msel,
        ysel,
        None,
        SlotMap::default(),
    );
    fn has_text(el: &Element, needle: &str) -> bool {
        el.text.as_deref().is_some_and(|t| t.contains(needle))
            || el.children.iter().any(|c| has_text(c, needle))
    }
    assert!(
        has_text(&el, "August") || has_text(&el, "14"),
        "month grid should render"
    );
}

#[test]
fn calendar_sync_placeholder_is_out_of_flow() {
    let val = use_signal("t-cal-gap-val", || "2026-08-14".to_string());
    let year = use_signal("t-cal-gap-y", || 2026i32);
    let month = use_signal("t-cal-gap-m", || 8u32);
    let focus = use_signal("t-cal-gap-f", || 14u32);
    let msel = use_signal("t-cal-gap-msel", || "8".to_string());
    let ysel = use_signal("t-cal-gap-ysel", || "2026".to_string());
    let el = build_calendar_view(
        CalendarRenderProps {
            id: "t-cal-gap".into(),
            caption: CalendarCaption::Label,
            disabled: false,
            bordered: true,
        },
        val,
        year,
        month,
        focus,
        msel,
        ysel,
        None,
        SlotMap::default(),
    );
    fn in_flow_zero_flex_item(el: &Element) -> bool {
        el.children.iter().any(|c| {
            let zero = crate::layout::dim_length_px(c.style.size.width).is_some_and(|w| w == 0.0)
                && crate::layout::dim_length_px(c.style.size.height).is_some_and(|h| h == 0.0);
            zero && c.style.position != taffy::style::Position::Absolute
        })
    }
    assert!(
        !in_flow_zero_flex_item(&el),
        "0×0 month-sync placeholder must not occupy the caption column gap"
    );
}

#[test]
fn calendar_dropdown_caption_uses_select_triggers() {
    let val = use_signal("t-cal-dd-val", || "2026-08-14".to_string());
    let year = use_signal("t-cal-dd-y", || 2026i32);
    let month = use_signal("t-cal-dd-m", || 8u32);
    let focus = use_signal("t-cal-dd-f", || 14u32);
    let msel = use_signal("t-cal-dd-msel", || "8".to_string());
    let ysel = use_signal("t-cal-dd-ysel", || "2026".to_string());
    let el = build_calendar_view(
        CalendarRenderProps {
            id: "t-cal-dd".into(),
            caption: CalendarCaption::Dropdown,
            disabled: false,
            bordered: true,
        },
        val,
        year,
        month,
        focus,
        msel,
        ysel,
        None,
        SlotMap::default(),
    );
    fn has_focus_id(el: &Element, id: &str) -> bool {
        el.focus_id.as_deref() == Some(id) || el.children.iter().any(|c| has_focus_id(c, id))
    }
    assert!(
        has_focus_id(&el, "t-cal-dd:cap_m:trigger"),
        "month caption must reuse Select trigger"
    );
    assert!(
        has_focus_id(&el, "t-cal-dd:cap_y:trigger"),
        "year caption must reuse Select trigger"
    );
}

#[test]
fn calendar_dropdown_caption_selects_flex_instead_of_fixed_chip() {
    let val = use_signal("t-cal-flex-val", || "2026-09-14".to_string());
    let year = use_signal("t-cal-flex-y", || 2026i32);
    let month = use_signal("t-cal-flex-m", || 9u32);
    let focus = use_signal("t-cal-flex-f", || 14u32);
    let msel = use_signal("t-cal-flex-msel", || "9".to_string());
    let ysel = use_signal("t-cal-flex-ysel", || "2026".to_string());
    let el = build_calendar_view(
        CalendarRenderProps {
            id: "t-cal-flex".into(),
            caption: CalendarCaption::Dropdown,
            disabled: false,
            bordered: true,
        },
        val,
        year,
        month,
        focus,
        msel,
        ysel,
        None,
        SlotMap::default(),
    );
    fn find_tid<'a>(el: &'a Element, want: &str) -> Option<&'a Element> {
        if el.template_id.as_deref() == Some(want) {
            return Some(el);
        }
        el.children.iter().find_map(|c| find_tid(c, want))
    }
    let month = find_tid(&el, "t-cal-flex:cap_m__select").expect("month select");
    let year = find_tid(&el, "t-cal-flex:cap_y__select").expect("year select");
    use crate::layout::{dim_kind, DimKind};
    assert!(
        matches!(dim_kind(month.style.min_size.width), DimKind::Length(v) if v < 0.5)
            || matches!(dim_kind(month.style.min_size.width), DimKind::Auto),
        "month select min-width must allow shrink, got {:?}",
        dim_kind(month.style.min_size.width)
    );
    assert!(
        month.style.flex_grow > 0.5,
        "month select must fill leftover caption width, grow={}",
        month.style.flex_grow
    );
    assert!(
        month.style.flex_shrink > 0.5,
        "month select must shrink instead of overlapping nav"
    );
    assert!(
        !matches!(
            dim_kind(month.style.size.width),
            DimKind::Length(px) if (px - 116.0).abs() < 1.0
        ),
        "must not pin a September chip width"
    );
    assert!(
        year.style.flex_shrink > 0.5,
        "year select must shrink on a narrow caption"
    );
    assert!(
        matches!(dim_kind(month.style.size.width), DimKind::Auto),
        "month select must not keep select()'s width:100% in a row, got {:?}",
        dim_kind(month.style.size.width)
    );
    assert!(
        matches!(dim_kind(year.style.size.width), DimKind::Auto),
        "year select must not keep select()'s width:100% in a row, got {:?}",
        dim_kind(year.style.size.width)
    );
    assert!(
        matches!(dim_kind(month.style.flex_basis), DimKind::Length(v) if v < 0.5),
        "month flex-basis 0 so the slot is the leftover width, got {:?}",
        dim_kind(month.style.flex_basis)
    );
    assert!(
        matches!(dim_kind(year.style.flex_basis), DimKind::Length(v) if v < 0.5),
        "year flex-basis 0 so month:year grow 2:1, got {:?}",
        dim_kind(year.style.flex_basis)
    );
    assert!(
        (month.style.flex_grow - CAPTION_MONTH_GROW).abs() < 0.01
            && (year.style.flex_grow - CAPTION_YEAR_GROW).abs() < 0.01,
        "caption selects must grow 2:1, month={} year={}",
        month.style.flex_grow,
        year.style.flex_grow
    );
    assert!(
        !matches!(
            dim_kind(year.style.size.width),
            DimKind::Length(px) if (px - 76.0).abs() < 1.0
        ),
        "must not pin a 4.75rem year chip"
    );
    assert!(
        matches!(dim_kind(year.style.min_size.width), DimKind::Length(v) if v < 0.5)
            || matches!(dim_kind(year.style.min_size.width), DimKind::Auto),
        "year select min-width must follow the 2:1 slot, got {:?}",
        dim_kind(year.style.min_size.width)
    );
}

fn xywh_overlap(a: [f32; 4], b: [f32; 4]) -> bool {
    let ax2 = a[0] + a[2];
    let ay2 = a[1] + a[3];
    let bx2 = b[0] + b[2];
    let by2 = b[1] + b[3];
    a[0] < bx2 - 0.5 && b[0] < ax2 - 0.5 && a[1] < by2 - 0.5 && b[1] < ay2 - 0.5
}

#[test]
fn calendar_dropdown_caption_selects_do_not_overlap_in_layout() {
    let val = use_signal("t-cal-aabb-val", || "2026-09-14".to_string());
    let year = use_signal("t-cal-aabb-y", || 2026i32);
    let month = use_signal("t-cal-aabb-m", || 9u32);
    let focus = use_signal("t-cal-aabb-f", || 14u32);
    let msel = use_signal("t-cal-aabb-msel", || "9".to_string());
    let ysel = use_signal("t-cal-aabb-ysel", || "2026".to_string());
    let mut el = build_calendar_view(
        CalendarRenderProps {
            id: "t-cal-aabb".into(),
            caption: CalendarCaption::Dropdown,
            disabled: false,
            bordered: true,
        },
        val,
        year,
        month,
        focus,
        msel,
        ysel,
        None,
        SlotMap::default(),
    );
    let mut layout = crate::core::tree::LayoutTree::new();
    layout.build_and_layout(&mut el, 400.0, 400.0, [-1.0, -1.0]);

    fn find_tid<'a>(el: &'a Element, want: &str) -> Option<&'a Element> {
        if el.template_id.as_deref() == Some(want) {
            return Some(el);
        }
        el.children.iter().find_map(|c| find_tid(c, want))
    }
    fn find_fid<'a>(el: &'a Element, want: &str) -> Option<&'a Element> {
        if el.focus_id.as_deref() == Some(want) {
            return Some(el);
        }
        el.children.iter().find_map(|c| find_fid(c, want))
    }
    let month = find_tid(&el, "t-cal-aabb:cap_m__select").expect("month select");
    let year = find_tid(&el, "t-cal-aabb:cap_y__select").expect("year select");
    let prev = find_fid(&el, "t-cal-aabb:prev").expect("prev nav");
    let next = find_fid(&el, "t-cal-aabb:next").expect("next nav");
    assert!(
        month.layout_bounds[2] > 1.0 && year.layout_bounds[2] > 1.0,
        "caption selects must receive a flex slot, month={:?} year={:?}",
        month.layout_bounds,
        year.layout_bounds
    );
    assert!(
        !xywh_overlap(month.layout_bounds, year.layout_bounds),
        "month/year selects must not occupy the same pixels: month={:?} year={:?}",
        month.layout_bounds,
        year.layout_bounds
    );
    assert!(
        month.layout_bounds[0] + month.layout_bounds[2] <= year.layout_bounds[0] + 0.5,
        "month select must sit to the left of year: month={:?} year={:?}",
        month.layout_bounds,
        year.layout_bounds
    );
    assert!(
        !xywh_overlap(month.layout_bounds, prev.layout_bounds),
        "month select must not overlap prev nav: month={:?} prev={:?}",
        month.layout_bounds,
        prev.layout_bounds
    );
    assert!(
        (month.layout_bounds[2] - year.layout_bounds[2]).abs() < 2.0,
        "month/year laid-out width must match (equal flex grow), month={:?} year={:?}",
        month.layout_bounds,
        year.layout_bounds
    );
    assert!(
        !xywh_overlap(year.layout_bounds, next.layout_bounds),
        "year select must not overlap next nav: year={:?} next={:?}",
        year.layout_bounds,
        next.layout_bounds
    );
}

#[test]
fn builder_style_reaches_calendar_view() {
    let val = use_signal("t-cal-style-val", || "2026-08-14".to_string());
    let year = use_signal("t-cal-style-y", || 2026i32);
    let month = use_signal("t-cal-style-m", || 8u32);
    let focus = use_signal("t-cal-style-f", || 14u32);
    let msel = use_signal("t-cal-style-msel", || "8".to_string());
    let ysel = use_signal("t-cal-style-ysel", || "2026".to_string());
    let view = build_calendar_view(
        CalendarRenderProps {
            id: "t-cal-style".into(),
            caption: CalendarCaption::Label,
            disabled: false,
            bordered: true,
        },
        val,
        year,
        month,
        focus,
        msel,
        ysel,
        None,
        SlotMap::default(),
    );
    let view = SlotChrome::default()
        .style(|b| b.text_size(20.0))
        .apply(view);
    fn find_size(el: &Element, want: f32) -> bool {
        el.text_style.size.is_some_and(|s| (s - want).abs() < 0.01)
            || el.children.iter().any(|c| find_size(c, want))
    }
    assert!(
        find_size(&view, 20.0),
        "calendar .style must land on the visible view root, not the placeholder"
    );
}

#[test]
fn builder_day_style_reaches_day_slot() {
    let val = use_signal("t-cal-day-val", || "2026-08-14".to_string());
    let year = use_signal("t-cal-day-y", || 2026i32);
    let month = use_signal("t-cal-day-m", || 8u32);
    let focus = use_signal("t-cal-day-f", || 14u32);
    let msel = use_signal("t-cal-day-msel", || "8".to_string());
    let ysel = use_signal("t-cal-day-ysel", || "2026".to_string());
    let slots = SlotMap::default().slot(SLOT_DAY, |c| c.style(|b| b.text_size(20.0)));
    let grid = build_calendar_day_grid(
        "t-cal-day",
        false,
        val,
        year,
        month,
        focus,
        msel,
        ysel,
        None,
        slots,
    );
    fn find_day_size(el: &Element, want: f32) -> bool {
        let slot = el.attrs.get(ATTR_SLOT).map(String::as_str);
        let is_day = slot == Some(SLOT_DAY) || slot == Some(SLOT_DAY_TODAY);
        (is_day && el.text_style.size.is_some_and(|s| (s - want).abs() < 0.01))
            || el.children.iter().any(|c| find_day_size(c, want))
    }
    assert!(
        find_day_size(&grid, 20.0),
        "calendar .slot(calendar-day) style must land on data-slot=calendar-day"
    );
}

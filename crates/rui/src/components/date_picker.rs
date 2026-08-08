//! # DatePicker / TimePicker
//!
//! 日期与时间选择 MVP。对应 HTML `date` / `datetime-local` / `month` / `week` / `time`。
//!
//! - **DatePicker**：点击触发器打开月份网格；点日写入 `YYYY-MM-DD`；Esc / 遮罩关闭。
//! - **TimePicker**：小时 / 分钟按钮列表，写入 `HH:MM`。
//!
//! ## 用法
//! ```ignore
//! <DatePicker id="due" kind="date" />
//! <TimePicker id="alarm" value="09:30" />
//! ```

use crate::app::portal_overlay::{portal_sync, OverlayKind, OverlayMode};
use crate::app::reactive::reactive_component_live;
use crate::components::button::button;
use crate::components::picker_stub::{lucide, stub_label, stub_trigger, stub_trigger_focus_id};
use crate::core::reactivity::{use_signal, Signal};
use crate::core::tree::Element;
use crate::primitives::r#box::{hstack, r#box, vstack};
use crate::tokens;
use crate::utils::popup::{
    apply_stored_floating_place, mark_floating, panel_block_clicks, PopupPlacement,
};
use crate::utils::text_node::{build_text_node, TextNodeConfig};
use rui_macros::component;

/// DatePicker 的 HTML 对齐模式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DatePickerKind {
    #[default]
    Date,
    DateTimeLocal,
    Month,
    Week,
}

impl DatePickerKind {
    pub fn parse(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "datetime-local" | "datetime_local" | "datetime" => Self::DateTimeLocal,
            "month" => Self::Month,
            "week" => Self::Week,
            _ => Self::Date,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Date => "date",
            Self::DateTimeLocal => "datetime-local",
            Self::Month => "month",
            Self::Week => "week",
        }
    }

    fn placeholder(self) -> &'static str {
        match self {
            Self::Date => "Pick a date…",
            Self::DateTimeLocal => "Pick date & time…",
            Self::Month => "Pick a month…",
            Self::Week => "Pick a week…",
        }
    }
}

fn is_leap(y: i32) -> bool {
    (y % 4 == 0 && y % 100 != 0) || (y % 400 == 0)
}

fn days_in_month(y: i32, m: u32) -> u32 {
    match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if is_leap(y) {
                29
            } else {
                28
            }
        }
        _ => 30,
    }
}

/// Sakamoto：0 = Sunday … 6 = Saturday。
fn weekday(y: i32, m: u32, d: u32) -> u32 {
    let t = [0, 3, 2, 5, 0, 3, 5, 1, 4, 6, 2, 4];
    let mut yy = y;
    if m < 3 {
        yy -= 1;
    }
    let w = (yy + yy / 4 - yy / 100 + yy / 400 + t[(m as usize) - 1] + d as i32) % 7;
    w.rem_euclid(7) as u32
}

fn parse_ymd(s: &str) -> Option<(i32, u32, u32)> {
    let parts: Vec<_> = s.trim().split('-').collect();
    if parts.len() < 2 {
        return None;
    }
    let y: i32 = parts[0].parse().ok()?;
    let m: u32 = parts[1].parse().ok()?;
    let d: u32 = if parts.len() >= 3 {
        parts[2].parse().ok()?
    } else {
        1
    };
    if (1..=12).contains(&m) && (1..=31).contains(&d) {
        Some((y, m, d))
    } else {
        None
    }
}

fn parse_hm(s: &str) -> Option<(u32, u32)> {
    let parts: Vec<_> = s.trim().split(':').collect();
    if parts.len() < 2 {
        return None;
    }
    let h: u32 = parts[0].parse().ok()?;
    let m: u32 = parts[1].parse().ok()?;
    if h < 24 && m < 60 {
        Some((h, m))
    } else {
        None
    }
}

fn month_title(y: i32, m: u32) -> String {
    format!("{y:04}-{m:02}")
}

/// HTML `date` / `datetime-local` / `month` / `week` 选择器。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn DatePicker(
    id: String,
    kind: Option<String>,
    value: Option<String>,
    disabled: bool,
    children: Option<Element>,
) -> Element {
    let _ = children;
    let kind = kind
        .as_deref()
        .map(DatePickerKind::parse)
        .unwrap_or_default();
    date_picker(&id)
        .kind(kind)
        .default_value(value.unwrap_or_default())
        .disabled(disabled)
        .into()
}

/// HTML `input type="time"` 选择器。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn TimePicker(
    id: String,
    value: Option<String>,
    disabled: bool,
    children: Option<Element>,
) -> Element {
    let _ = children;
    time_picker(&id)
        .default_value(value.unwrap_or_default())
        .disabled(disabled)
        .into()
}

pub struct DatePickerBuilder {
    id: String,
    kind: DatePickerKind,
    value: Option<Signal<String>>,
    default_value: String,
    disabled: bool,
}

pub fn date_picker(id: impl Into<String>) -> DatePickerBuilder {
    DatePickerBuilder {
        id: id.into(),
        kind: DatePickerKind::Date,
        value: None,
        default_value: String::new(),
        disabled: false,
    }
}

impl DatePickerBuilder {
    pub fn kind(mut self, k: DatePickerKind) -> Self {
        self.kind = k;
        self
    }
    pub fn value_signal(mut self, s: Signal<String>) -> Self {
        self.value = Some(s);
        self
    }
    pub fn default_value(mut self, v: impl Into<String>) -> Self {
        self.default_value = v.into();
        self
    }
    pub fn disabled(mut self, d: bool) -> Self {
        self.disabled = d;
        self
    }
}

#[derive(Clone, PartialEq)]
struct DatePickerRenderProps {
    id: String,
    kind: DatePickerKind,
    disabled: bool,
}

fn build_date_picker_view(
    props: DatePickerRenderProps,
    open: Signal<bool>,
    val: Signal<String>,
    year: Signal<i32>,
    month: Signal<u32>,
) -> Element {
    let DatePickerRenderProps {
        id,
        kind,
        disabled,
    } = props;

    let _ = open.get();
    let current = val.get();
    let y = year.get();
    let m = month.get();

    let display = if current.is_empty() {
        kind.placeholder().to_string()
    } else {
        current.clone()
    };
    let muted = current.is_empty();
    let icon = lucide(0xe063);
    let trigger_shell = stub_trigger(&id, icon, stub_label(&display, muted), disabled);
    let open_t = open;
    let year_t = year;
    let month_t = month;
    let val_peek = val;
    let mut trigger = trigger_shell;
    if !disabled {
        trigger = trigger.on_click(move || {
            if !open_t.peek() {
                if let Some((yy, mm, _)) = parse_ymd(&val_peek.peek()) {
                    year_t.set(yy);
                    month_t.set(mm);
                }
            }
            open_t.set(!open_t.peek());
        });
    }

    let mut view: Element = vstack().gap("0").into();
    view = view.relative().child(trigger);

    if disabled {
        return view;
    }

    if let Some(oz) = portal_sync(&id, open, OverlayKind::Popover, OverlayMode::Stacked) {
        if open.peek() {
            let mut panel: Element = vstack()
                .gap("0.5rem")
                .p("0.75rem")
                .w("17.5rem")
                .into();
            panel = panel
                .bg(tokens::background())
                .rounded("0.375rem")
                .border_style("solid")
                .border_width(1.0)
                .border_color(tokens::border())
                .shadow([0.0, 0.0, 0.0, 0.12], 8.0);

            let y_sig = year;
            let m_sig = month;
            let prev_id = format!("{id}:prev");
            let prev: Element = button(&prev_id)
                .variant("ghost")
                .size("sm")
                .on_click(move || {
                    let mut mm = m_sig.peek();
                    let mut yy = y_sig.peek();
                    if mm <= 1 {
                        mm = 12;
                        yy -= 1;
                    } else {
                        mm -= 1;
                    }
                    m_sig.set(mm);
                    y_sig.set(yy);
                })
                .text("‹")
                .into();
            let next_id = format!("{id}:next");
            let next: Element = button(&next_id)
                .variant("ghost")
                .size("sm")
                .on_click(move || {
                    let mut mm = m_sig.peek();
                    let mut yy = y_sig.peek();
                    if mm >= 12 {
                        mm = 1;
                        yy += 1;
                    } else {
                        mm += 1;
                    }
                    m_sig.set(mm);
                    y_sig.set(yy);
                })
                .text("›")
                .into();
            let title = build_text_node(
                month_title(y, m),
                TextNodeConfig {
                    size_px: 14.0,
                    weight: 600,
                    ..Default::default()
                },
            );
            let header: Element = hstack()
                .w("100%")
                .items_center()
                .justify("between")
                .gap("0.5rem")
                .child(prev)
                .child(title)
                .child(next)
                .into();
            panel = panel.child(header);

            match kind {
                DatePickerKind::Month => {
                    let mut grid: Element = hstack().w("100%").gap("0.25rem").wrap("wrap").into();
                    for mm in 1u32..=12 {
                        let v_sig = val;
                        let o_sig = open;
                        let y_sig = year;
                        let bid = format!("{id}:m{mm}");
                        let label = format!("{mm:02}");
                        let cell: Element = button(&bid)
                            .variant(if m == mm { "default" } else { "ghost" })
                            .size("sm")
                            .on_click(move || {
                                let yy = y_sig.peek();
                                v_sig.set(format!("{yy:04}-{mm:02}"));
                                o_sig.set(false);
                            })
                            .text(label)
                            .into();
                        grid = grid.child(cell);
                    }
                    panel = panel.child(grid);
                }
                DatePickerKind::Week => {
                    // MVP：选周 → 写入该周周一的 YYYY-MM-DD（简化）
                    let first_wd = weekday(y, m, 1);
                    let dim = days_in_month(y, m);
                    let mut week_start = 1u32;
                    if first_wd != 1 {
                        // Monday-based: if first isn't Monday, still list weeks from day 1
                        week_start = 1;
                    }
                    let mut week_idx = 0u32;
                    let mut d = week_start;
                    let mut weeks = vstack().w("100%").gap("0.25rem");
                    while d <= dim {
                        week_idx += 1;
                        let v_sig = val;
                        let o_sig = open;
                        let y_c = y;
                        let m_c = m;
                        let day = d;
                        let bid = format!("{id}:w{week_idx}");
                        let label = format!("W{week_idx} · {y_c:04}-{m_c:02}-{day:02}");
                        let row: Element = button(&bid)
                            .variant("ghost")
                            .size("sm")
                            .w("100%")
                            .on_click(move || {
                                v_sig.set(format!("{y_c:04}-{m_c:02}-{day:02}"));
                                o_sig.set(false);
                            })
                            .text(label)
                            .into();
                        weeks = weeks.child(row);
                        d = d.saturating_add(7);
                    }
                    panel = panel.child(weeks);
                }
                DatePickerKind::Date | DatePickerKind::DateTimeLocal => {
                    let dow = ["Su", "Mo", "Tu", "We", "Th", "Fr", "Sa"];
                    let mut head: Element = hstack().w("100%").gap("0.125rem").into();
                    for name in dow {
                        let mut cell: Element = r#box().w("2rem").h("1.25rem").into();
                        cell = cell.child(build_text_node(
                            name,
                            TextNodeConfig {
                                size_px: 11.0,
                                weight: 500,
                                ..Default::default()
                            },
                        ));
                        head = head.child(cell);
                    }
                    panel = panel.child(head);

                    let first = weekday(y, m, 1);
                    let dim = days_in_month(y, m);
                    let mut grid: Element = vstack().w("100%").gap("0.125rem").into();
                    let mut day = 1u32;
                    let mut started = false;
                    for _week in 0..6 {
                        if day > dim {
                            break;
                        }
                        let mut row: Element = hstack().w("100%").gap("0.125rem").into();
                        for col in 0..7 {
                            if !started {
                                if col as u32 == first {
                                    started = true;
                                } else {
                                    row = row.child(Element::from(r#box().w("2rem").h("2rem")));
                                    continue;
                                }
                            }
                            if day > dim {
                                row = row.child(Element::from(r#box().w("2rem").h("2rem")));
                                continue;
                            }
                            let d = day;
                            day += 1;
                            let v_sig = val;
                            let o_sig = open;
                            let y_c = y;
                            let m_c = m;
                            let selected = parse_ymd(&current)
                                .map(|(yy, mm, dd)| yy == y_c && mm == m_c && dd == d)
                                .unwrap_or(false);
                            let bid = format!("{id}:d{d}");
                            let cell: Element = button(&bid)
                                .variant(if selected { "default" } else { "ghost" })
                                .size("sm")
                                .w("2rem")
                                .on_click(move || {
                                    let base = format!("{y_c:04}-{m_c:02}-{d:02}");
                                    v_sig.set(base);
                                    o_sig.set(false);
                                })
                                .text(format!("{d}"))
                                .into();
                            row = row.child(cell);
                        }
                        grid = grid.child(row);
                    }
                    panel = panel.child(grid);
                }
            }

            let trigger_fid = stub_trigger_focus_id(&id);
            let mut floating = panel;
            floating = floating.absolute();
            mark_floating(
                &mut floating,
                &id,
                &trigger_fid,
                PopupPlacement::BottomStart,
            );
            floating.set_attr("rui_floating_match_trigger", "0");
            floating.set_attr("rui_floating_w", "17.5rem");
            apply_stored_floating_place(&mut floating, &id);
            floating = panel_block_clicks(floating, oz.panel);
            view = view.child(floating);
        }
    }

    view
}

impl From<DatePickerBuilder> for Element {
    fn from(builder: DatePickerBuilder) -> Self {
        let DatePickerBuilder {
            id,
            kind,
            value,
            default_value,
            disabled,
        } = builder;
        let open = use_signal(format!("{id}__dp_open").as_str(), || false);
        let initial = default_value.clone();
        let val = value.unwrap_or_else(|| {
            use_signal(format!("{id}__dp_value").as_str(), move || initial.clone())
        });
        let (iy, im, _) = parse_ymd(&default_value).unwrap_or((2026, 8, 1));
        let year = use_signal(format!("{id}__dp_year").as_str(), move || iy);
        let month = use_signal(format!("{id}__dp_month").as_str(), move || im);
        let view_id = format!("{id}__dp_view");
        let props = DatePickerRenderProps {
            id,
            kind,
            disabled,
        };
        reactive_component_live(view_id, props, move |p| {
            build_date_picker_view(p, open, val, year, month)
        })
    }
}

pub struct TimePickerBuilder {
    id: String,
    value: Option<Signal<String>>,
    default_value: String,
    disabled: bool,
}

pub fn time_picker(id: impl Into<String>) -> TimePickerBuilder {
    TimePickerBuilder {
        id: id.into(),
        value: None,
        default_value: String::new(),
        disabled: false,
    }
}

impl TimePickerBuilder {
    pub fn value_signal(mut self, s: Signal<String>) -> Self {
        self.value = Some(s);
        self
    }
    pub fn default_value(mut self, v: impl Into<String>) -> Self {
        self.default_value = v.into();
        self
    }
    pub fn disabled(mut self, d: bool) -> Self {
        self.disabled = d;
        self
    }
}

#[derive(Clone, PartialEq)]
struct TimePickerRenderProps {
    id: String,
    disabled: bool,
}

fn build_time_picker_view(
    props: TimePickerRenderProps,
    open: Signal<bool>,
    val: Signal<String>,
    hour: Signal<u32>,
    minute: Signal<u32>,
) -> Element {
    let TimePickerRenderProps { id, disabled } = props;
    let _ = open.get();
    let current = val.get();
    let h = hour.get();
    let mi = minute.get();

    let display = if current.is_empty() {
        "Pick a time…".into()
    } else {
        current
    };
    let muted = display == "Pick a time…";
    let icon = lucide(0xe087);
    let mut trigger = stub_trigger(&id, icon, stub_label(&display, muted), disabled);
    if !disabled {
        let open_t = open;
        let hour_t = hour;
        let minute_t = minute;
        let val_peek = val;
        trigger = trigger.on_click(move || {
            if !open_t.peek() {
                if let Some((hh, mm)) = parse_hm(&val_peek.peek()) {
                    hour_t.set(hh);
                    minute_t.set(mm);
                }
            }
            open_t.set(!open_t.peek());
        });
    }

    let mut view: Element = vstack().gap("0").relative(true).child(trigger).into();
    if disabled {
        return view;
    }

    if let Some(oz) = portal_sync(&id, open, OverlayKind::Popover, OverlayMode::Stacked) {
        if open.peek() {
            let mut hours_col: Element = vstack().gap("0.125rem").max_h("10rem").into();
            for hh in 0u32..24 {
                let h_sig = hour;
                let bid = format!("{id}:h{hh}");
                let row: Element = button(&bid)
                    .variant(if h == hh { "default" } else { "ghost" })
                    .size("sm")
                    .on_click(move || h_sig.set(hh))
                    .text(format!("{hh:02}"))
                    .into();
                hours_col = hours_col.child(row);
            }
            let mut mins_col: Element = vstack().gap("0.125rem").max_h("10rem").into();
            for mm in (0u32..60).step_by(5) {
                let m_sig = minute;
                let bid = format!("{id}:m{mm}");
                let row: Element = button(&bid)
                    .variant(if mi == mm { "default" } else { "ghost" })
                    .size("sm")
                    .on_click(move || m_sig.set(mm))
                    .text(format!("{mm:02}"))
                    .into();
                mins_col = mins_col.child(row);
            }

            let v_sig = val;
            let o_sig = open;
            let h_sig = hour;
            let m_sig = minute;
            let apply_id = format!("{id}:apply");
            let apply: Element = button(&apply_id)
                .variant("default")
                .size("sm")
                .w("100%")
                .on_click(move || {
                    v_sig.set(format!("{:02}:{:02}", h_sig.peek(), m_sig.peek()));
                    o_sig.set(false);
                })
                .text("Set time")
                .into();

            let mut panel: Element = vstack()
                .gap("0.5rem")
                .p("0.75rem")
                .w("12rem")
                .child(
                    hstack()
                        .gap("0.5rem")
                        .child(hours_col)
                        .child(mins_col),
                )
                .child(apply)
                .into();
            panel = panel
                .bg(tokens::background())
                .rounded("0.375rem")
                .border_style("solid")
                .border_width(1.0)
                .border_color(tokens::border())
                .shadow([0.0, 0.0, 0.0, 0.12], 8.0)
                .absolute();
            mark_floating(
                &mut panel,
                &id,
                &stub_trigger_focus_id(&id),
                PopupPlacement::BottomStart,
            );
            panel.set_attr("rui_floating_match_trigger", "0");
            panel.set_attr("rui_floating_w", "12rem");
            apply_stored_floating_place(&mut panel, &id);
            panel = panel_block_clicks(panel, oz.panel);
            view = view.child(panel);
        }
    }
    view
}

impl From<TimePickerBuilder> for Element {
    fn from(builder: TimePickerBuilder) -> Self {
        let TimePickerBuilder {
            id,
            value,
            default_value,
            disabled,
        } = builder;
        let open = use_signal(format!("{id}__tp_open").as_str(), || false);
        let initial = default_value.clone();
        let val = value.unwrap_or_else(|| {
            use_signal(format!("{id}__tp_value").as_str(), move || initial.clone())
        });
        let (ih, im) = parse_hm(&default_value).unwrap_or((9, 0));
        let hour = use_signal(format!("{id}__tp_hour").as_str(), move || ih);
        let minute = use_signal(format!("{id}__tp_minute").as_str(), move || im);
        let view_id = format!("{id}__tp_view");
        let props = TimePickerRenderProps { id, disabled };
        reactive_component_live(view_id, props, move |p| {
            build_time_picker_view(p, open, val, hour, minute)
        })
    }
}

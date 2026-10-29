//! # DatePicker / TimePicker
//!
//! 对照 ShadCN Aria `date-picker`（sidebar-12 内联 [`Calendar`] + 触发器浮层）：
//! HTML `date` / `datetime-local` / `month` / `week` / `time`。
//!
//! **Format (local, no time zone)**
//! - `date`：`YYYY-MM-DD`
//! - `datetime-local`：`YYYY-MM-DDTHH:MM:SS`（24h；兼容无秒的 `THH:MM`）
//! - `month`：`YYYY-MM`
//! - `week`: Monday of the week `YYYY-MM-DD`
//! - `time`：`HH:MM:SS`（24h；三个 Combobox，兼容 `HH:MM`）
//!
//! **键盘**：DatePicker 触发器 Enter/Space 开合、Esc 关浮层；打开后方向键改日（见 [`Calendar`]）。
//! TimePicker 共用同一套触发器浮层；面板内三个 Combobox（输入过滤 + ↑↓ / Esc）。
//!
//! ## Usage
//! ```ignore
//! ui! {
//!     <DatePicker id="due" kind="date" captionLayout="dropdown" />
//!     <DatePicker id="meet" kind="datetime-local" valueSignal={meet_sig} />
//!     <TimePicker id="alarm" value="09:30:00" />
//!     <Calendar id="inline" captionLayout="dropdown" />
//! }
//! date_picker("due").kind(DatePickerKind::Date).value_signal(sig)
//! ```
//!
//! Builder `.class()` / `.style()` land on the **trigger**. Panel card:
//! `.slot("date-picker-content", …)` (aliases `.content_class()` / `.content_style()`).

use crate::app::portal_overlay::{OverlayKind, OverlayMode};
use crate::app::reactive::{reactive_anchor, reactive_component_live, reactive_placeholder_sized};
use crate::components::base::{paint_slot, SlotChrome, SlotMap};
use crate::components::button::button;
use crate::components::calendar::{
    calendar, calendar_month_select, calendar_year_select, days_in_month, format_long_date,
    format_month_year, parse_ymd, weekday, CalendarCaption, CALENDAR_W,
};
use crate::components::combobox::{combobox, ComboboxItemData};
use crate::components::separator::separator;
use crate::core::reactivity::{isolate_reads, use_signal, with_child_owner, Signal};
use crate::core::tree::{parse_text_color, Element};
use crate::primitives::r#box::{hstack, vstack};
use crate::tokens;
use crate::utils::anchored_overlay::{
    anchored_overlay_content, provide_anchored_overlay_context, AnchoredOverlayContentOpts,
    FloatingMotionKind, FloatingStrategy, Placement,
};
use crate::utils::controlled::{resolve_open_signal, resolve_string_signal};
use crate::utils::lucide_glyph;
use crate::utils::popup::{
    flow_hidden_portal_host, PopupPlacement, ATTR_FLOATING_MATCH_TRIGGER, ATTR_FLOATING_MIN_W,
    FLOATING_TRIGGER_GAP,
};
use crate::utils::roving::{focus_by_id, focus_id_only};
use crate::utils::text_field::{commit_value_sig, input_value_signal_key};
use crate::utils::text_node::{build_icon_text_node, build_text_node, TextNodeConfig};
use winit::keyboard::{Key, NamedKey};

const SLOT_CONTENT: &str = "date-picker-content";

fn trigger_focus_id(id: &str) -> String {
    format!("{id}:trigger")
}

fn close_picker(open: Signal<bool>, trigger_fid: impl Into<String>) {
    open.set(false);
    focus_id_only(trigger_fid.into());
}

fn close_picker_keyboard(open: Signal<bool>, trigger_fid: impl Into<String>) {
    open.set(false);
    focus_by_id(trigger_fid.into());
}

fn chevron_down_icon() -> Element {
    let glyph = lucide_glyph("chevron-down").unwrap_or_else(|| "▾".into());
    build_icon_text_node(glyph)
        .text_size(16.0)
        .text_color(parse_text_color("muted-foreground"))
}

fn chevron_nav_icon(name: &str) -> Element {
    let glyph = lucide_glyph(name).unwrap_or_else(|| "?".into());
    build_icon_text_node(glyph)
        .text_size(16.0)
        .text_color(parse_text_color("foreground"))
}

/// ShadCN Aria DatePicker 触发器：`Button variant="outline"` + chevron-down，`justify-between`。
fn picker_trigger(id: &str, label: &str, empty: bool, disabled: bool, min_w: &str) -> Element {
    let mut color = if empty {
        parse_text_color("muted-foreground")
    } else {
        parse_text_color("foreground")
    };
    if disabled {
        color[3] = (color[3] as f32 * 0.5) as u8;
    }
    let text = build_text_node(
        label,
        TextNodeConfig {
            size_px: 14.0,
            weight: 400,
            color,
            ..Default::default()
        },
    );
    let mut el: Element = button(trigger_focus_id(id))
        .variant("outline")
        .size("default")
        .disabled(disabled)
        .child(text)
        .child(chevron_down_icon())
        .into();
    el = el
        .justify("between")
        .items_center()
        .w("100%")
        .min_w(min_w)
        .px("0.75rem");
    el
}

/// DatePicker 的 HTML 对齐模式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DatePickerKind {
    #[default]
    Date,
    DateTimeLocal,
    Time,
    Month,
    Week,
}

impl DatePickerKind {
    pub fn parse(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "datetime-local" | "datetime_local" | "datetime" => Self::DateTimeLocal,
            "time" => Self::Time,
            "month" => Self::Month,
            "week" => Self::Week,
            _ => Self::Date,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Date => "date",
            Self::DateTimeLocal => "datetime-local",
            Self::Time => "time",
            Self::Month => "month",
            Self::Week => "week",
        }
    }

    fn placeholder(self) -> String {
        match self {
            Self::Date => crate::tr_default_zh("jadix.date_picker.date", "Pick a date", "选择日期"),
            Self::DateTimeLocal => crate::tr_default_zh(
                "jadix.date_picker.datetime",
                "Pick date & time",
                "选择日期和时间",
            ),
            Self::Time => crate::tr_default_zh("jadix.date_picker.time", "Pick a time", "选择时间"),
            Self::Month => {
                crate::tr_default_zh("jadix.date_picker.month", "Pick a month", "选择月份")
            }
            Self::Week => crate::tr_default_zh("jadix.date_picker.week", "Pick a week", "选择周"),
        }
    }
}

fn format_trigger_value(kind: DatePickerKind, raw: &str) -> String {
    if raw.is_empty() {
        return String::new();
    }
    match kind {
        DatePickerKind::Date => parse_ymd(raw)
            .map(|(y, m, d)| format_long_date(y, m, d))
            .unwrap_or_else(|| raw.to_string()),
        DatePickerKind::DateTimeLocal => match (parse_ymd(raw), parse_hms(raw)) {
            (Some((y, m, d)), Some((h, mi, s))) => crate::format_datetime(y, m, d, h, mi, s),
            (Some((y, m, d)), None) => format_long_date(y, m, d),
            _ => raw.to_string(),
        },
        DatePickerKind::Month => parse_ymd(&format!("{raw}-01"))
            .or_else(|| parse_ymd(raw))
            .map(|(y, m, _)| format_month_year(y, m))
            .unwrap_or_else(|| raw.to_string()),
        DatePickerKind::Week => parse_ymd(raw)
            .map(|(y, m, d)| {
                crate::tr_with_default_zh(
                    "jadix.date_picker.week_of",
                    "Week of {date}",
                    "{date} 所在周",
                    &[("date", &format_long_date(y, m, d))],
                )
            })
            .unwrap_or_else(|| raw.to_string()),
        DatePickerKind::Time => parse_hms(raw)
            .map(|(h, mi, s)| format!("{h:02}:{mi:02}:{s:02}"))
            .unwrap_or_else(|| raw.to_string()),
    }
}

fn parse_hms(s: &str) -> Option<(u32, u32, u32)> {
    let t = s.split_once('T').map(|(_, t)| t).unwrap_or(s);
    let parts: Vec<_> = t.trim().split(':').collect();
    if parts.len() < 2 {
        return None;
    }
    let h: u32 = parts[0].parse().ok()?;
    let m: u32 = parts[1].parse().ok()?;
    let sec: u32 = if parts.len() >= 3 {
        parts[2].parse().ok()?
    } else {
        0
    };
    if h < 24 && m < 60 && sec < 60 {
        Some((h, m, sec))
    } else {
        None
    }
}

fn parse_time_unit(raw: &str, max: u32) -> u32 {
    let digits: String = raw.chars().filter(|c| c.is_ascii_digit()).collect();
    digits.parse::<u32>().unwrap_or(0).min(max)
}

fn format_hms_parts(h: &str, m: &str, s: &str) -> String {
    format!(
        "{:02}:{:02}:{:02}",
        parse_time_unit(h, 23),
        parse_time_unit(m, 59),
        parse_time_unit(s, 59)
    )
}

fn sanitize_time_unit_input(combo_id: &str, committed: Signal<String>, max: u32) {
    let store = use_signal(input_value_signal_key(combo_id).as_str(), String::new);
    let raw = store.get();
    let digits: String = raw.chars().filter(|c| c.is_ascii_digit()).take(2).collect();
    if digits != raw {
        commit_value_sig(combo_id, store, digits.clone());
    }
    if digits.is_empty() {
        return;
    }
    let Ok(n) = digits.parse::<u32>() else {
        return;
    };
    if n > max || digits.len() == 2 {
        let c = n.min(max);
        let s = format!("{c:02}");
        if store.peek() != s {
            commit_value_sig(combo_id, store, s.clone());
        }
        if committed.peek() != s {
            committed.set(s);
        }
    }
}

fn time_colon() -> Element {
    build_text_node(
        ":",
        TextNodeConfig {
            size_px: 14.0,
            weight: 500,
            ..Default::default()
        },
    )
    .flex_shrink(0.0)
}

fn time_unit_combobox(id: &str, value: Signal<String>, max: u32, placeholder: &str) -> Element {
    let combo_id = id.to_string();
    reactive_anchor(format!("{id}__clamp"), {
        let combo_id = combo_id.clone();
        move || {
            sanitize_time_unit_input(&combo_id, value, max);
            Element::new().w(0.0).h(0.0)
        }
    });
    let mut b = combobox(id)
        .value_signal(value)
        .placeholder(placeholder)
        .clearable(false)
        .class("w-full min-w-0")
        .scroll_selected("center");
    for i in 0..=max {
        b = b.item(ComboboxItemData::new(format!("{i:02}"), format!("{i:02}")));
    }
    let el: Element = b.into();
    el.min_w(0.0).flex_grow(1.0).flex_shrink(1.0)
}

fn time_hms_row(
    id: &str,
    hour: Signal<String>,
    minute: Signal<String>,
    second: Signal<String>,
    disabled: bool,
) -> Element {
    if disabled {
        let label = format_hms_parts(&hour.peek(), &minute.peek(), &second.peek());
        return build_text_node(
            label,
            TextNodeConfig {
                size_px: 14.0,
                ..Default::default()
            },
        );
    }
    Element::from(
        hstack()
            .w("100%")
            .gap("0.25rem")
            .items_center()
            .px("0.75rem")
            .py("0.5rem")
            .max_w("100%")
            .overflow("hidden")
            .child(time_unit_combobox(&format!("{id}__h"), hour, 23, "HH"))
            .child(time_colon())
            .child(time_unit_combobox(&format!("{id}__m"), minute, 59, "MM"))
            .child(time_colon())
            .child(time_unit_combobox(&format!("{id}__s"), second, 59, "SS")),
    )
}

fn sync_datetime_value_from_hms(
    val: Signal<String>,
    hour: Signal<String>,
    minute: Signal<String>,
    second: Signal<String>,
) {
    // Only go to HMS → val; do not write back HMS from val first, otherwise the newly selected seconds will be erased back to the old value.
    let hs = hour.get();
    let ms = minute.get();
    let ss = second.get();
    if hs.is_empty() && ms.is_empty() && ss.is_empty() {
        return;
    }
    if let Some((y, mo, d)) = parse_ymd(&val.peek()) {
        let next = format!("{y:04}-{mo:02}-{d:02}T{}", format_hms_parts(&hs, &ms, &ss));
        if val.peek() != next {
            val.set(next);
        }
    }
}

fn sync_time_value_from_hms(
    val: Signal<String>,
    hour: Signal<String>,
    minute: Signal<String>,
    second: Signal<String>,
) {
    let hs = hour.get();
    let ms = minute.get();
    let ss = second.get();
    if hs.is_empty() && ms.is_empty() && ss.is_empty() {
        return;
    }
    let next = format_hms_parts(&hs, &ms, &ss);
    if val.peek() != next {
        val.set(next);
    }
}

fn picker_open_focus_id(id: &str, kind: DatePickerKind) -> String {
    match kind {
        DatePickerKind::Time => format!("input:{id}__time__h"),
        _ => format!("{id}__grid__cal_focus"),
    }
}

fn sync_hms_signals(
    raw: &str,
    hour: Signal<String>,
    minute: Signal<String>,
    second: Signal<String>,
) {
    let Some((hh, mm, ss)) = parse_hms(raw) else {
        return;
    };
    let hs = format!("{hh:02}");
    let ms = format!("{mm:02}");
    let ss_s = format!("{ss:02}");
    if hour.peek().len() != 1 && hour.peek() != hs {
        hour.set(hs);
    }
    if minute.peek().len() != 1 && minute.peek() != ms {
        minute.set(ms);
    }
    if second.peek().len() != 1 && second.peek() != ss_s {
        second.set(ss_s);
    }
}

fn month_nav_buttons(id: &str, year: Signal<i32>, month: Signal<u32>) -> (Element, Element) {
    let y_sig = year;
    let m_sig = month;
    let prev: Element = button(format!("{id}:prev"))
        .variant("ghost")
        .size("icon-sm")
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
        .child(chevron_nav_icon(crate::chrome_nav_chevron(true)))
        .into();
    let y_sig = year;
    let m_sig = month;
    let next: Element = button(format!("{id}:next"))
        .variant("ghost")
        .size("icon-sm")
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
        .child(chevron_nav_icon(crate::chrome_nav_chevron(false)))
        .into();
    (prev, next)
}

crate::__component_contract! {
DatePickerProps [];
/// HTML `date` / `datetime-local` / `time` / `month` / `week` selectors.
#[allow(non_snake_case)]
pub fn DatePicker(
identity [] id: String,
plain [] kind: Option<String>,
plain [] value: Option<String>,
plain [] value_signal: Option<Signal<String>>,
plain [] caption_layout: Option<String>,
default [] disabled: bool,
plain [] children: Option<Element>,
) -> Element {
    let _ = children;
    let kind = kind
        .as_deref()
        .map(DatePickerKind::parse)
        .unwrap_or_default();
    let mut b = date_picker(&id).kind(kind).disabled(disabled);
    if let Some(sig) = value_signal {
        b = b.value_signal(sig);
    }
    if let Some(v) = value {
        b = b.default_value(v);
    }
    if let Some(c) = caption_layout {
        b = b.caption_layout(CalendarCaption::parse(&c));
    }
    b.into()
}
}

crate::__component_contract! {
TimePickerProps [];
/// HTML `input type="time"` selector.
#[allow(non_snake_case)]
pub fn TimePicker(
identity [] id: String,
plain [] value: Option<String>,
plain [] value_signal: Option<Signal<String>>,
default [] disabled: bool,
plain [] children: Option<Element>,
) -> Element {
    let _ = children;
    let mut b = time_picker(&id).disabled(disabled);
    if let Some(sig) = value_signal {
        b = b.value_signal(sig);
    }
    if let Some(v) = value {
        b = b.default_value(v);
    }
    b.into()
}
}

pub struct DatePickerBuilder {
    id: String,
    kind: DatePickerKind,
    value: Option<Signal<String>>,
    default_value: String,
    caption: CalendarCaption,
    disabled: bool,
    chrome: SlotChrome,
    slots: SlotMap,
}

pub fn date_picker(id: impl Into<String>) -> DatePickerBuilder {
    DatePickerBuilder {
        id: id.into(),
        kind: DatePickerKind::Date,
        value: None,
        default_value: String::new(),
        caption: CalendarCaption::Label,
        disabled: false,
        chrome: SlotChrome::default(),
        slots: SlotMap::default(),
    }
}

impl DatePickerBuilder {
    crate::impl_slot_chrome_methods!();
    crate::impl_slot_map_methods!();
    crate::impl_overlay_content_slot!(SLOT_CONTENT);

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
    pub fn caption_layout(mut self, c: CalendarCaption) -> Self {
        self.caption = c;
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
    caption: CalendarCaption,
    disabled: bool,
}

#[expect(
    clippy::too_many_arguments,
    reason = "The existing rendering adapter keeps its independent geometry, state and callback parameters"
)]
fn build_date_picker_view(
    props: DatePickerRenderProps,
    open: Signal<bool>,
    val: Signal<String>,
    year: Signal<i32>,
    month: Signal<u32>,
    hour: Signal<String>,
    minute: Signal<String>,
    second: Signal<String>,
    trigger_chrome: SlotChrome,
    content_chrome: SlotChrome,
) -> Element {
    let DatePickerRenderProps {
        id,
        kind,
        caption,
        disabled,
    } = props;

    let mut view: Element = vstack().gap("0").min_w("13.25rem").into();

    // 触发器单独订 val：改时间只刷新标签，不拆浮层。
    let trig_anchor = format!("{id}__trigger_view");
    {
        let id = id.clone();

        let trigger_chrome = trigger_chrome.clone();
        reactive_anchor(trig_anchor.clone(), move || {
            let current = val.get();
            let display = if current.is_empty() {
                kind.placeholder().to_string()
            } else {
                format_trigger_value(kind, &current)
            };
            let muted = current.is_empty();
            let trigger =
                trigger_chrome.apply(picker_trigger(&id, &display, muted, disabled, "13.25rem"));
            if disabled {
                return trigger;
            }
            let open_focus = picker_open_focus_id(&id, kind);
            let trig = trigger_focus_id(&id);
            trigger
                .on_click({
                    let open_t = open;
                    let year_t = year;
                    let month_t = month;
                    let val_peek = val;
                    let open_focus = open_focus.clone();
                    let trig = trig.clone();
                    move || {
                        if !open_t.peek() {
                            if let Some((yy, mm, _)) = parse_ymd(&val_peek.peek()) {
                                year_t.set(yy);
                                month_t.set(mm);
                            }
                            open_t.set(true);
                            focus_id_only(open_focus.clone());
                        } else {
                            close_picker(open_t, trig.clone());
                        }
                    }
                })
                .on_key_down({
                    let open_k = open;
                    let year_k = year;
                    let month_k = month;
                    let val_k = val;
                    let open_focus = open_focus.clone();
                    let trig = trig.clone();
                    move |ev| match &ev.logical_key {
                        Key::Named(NamedKey::Escape) => {
                            close_picker_keyboard(open_k, trig.clone());
                            true
                        }
                        k if crate::utils::keyboard::is_enter_or_space(k) => {
                            if open_k.peek() {
                                close_picker_keyboard(open_k, trig.clone());
                            } else {
                                if let Some((yy, mm, _)) = parse_ymd(&val_k.peek()) {
                                    year_k.set(yy);
                                    month_k.set(mm);
                                }
                                open_k.set(true);
                                focus_id_only(open_focus.clone());
                            }
                            true
                        }
                        _ => false,
                    }
                })
        });
    }
    view = view.child(reactive_placeholder_sized(trig_anchor, "100%", "auto"));

    if disabled {
        return view;
    }

    if matches!(kind, DatePickerKind::DateTimeLocal | DatePickerKind::Time) {
        let to_val = format!("{id}__hms_to_val");
        reactive_anchor(to_val.clone(), move || {
            if kind == DatePickerKind::Time {
                sync_time_value_from_hms(val, hour, minute, second);
            } else {
                sync_datetime_value_from_hms(val, hour, minute, second);
            }
            Element::new().w(0.0).h(0.0)
        });
        let from_val = format!("{id}__hms_from_val");
        reactive_anchor(from_val.clone(), move || {
            sync_hms_signals(&val.get(), hour, minute, second);
            Element::new().w(0.0).h(0.0)
        });
        view = view
            .child(flow_hidden_portal_host(
                reactive_placeholder_sized(to_val, 0.0, 0.0).pointer_events_box_none(),
            ))
            .child(flow_hidden_portal_host(
                reactive_placeholder_sized(from_val, 0.0, 0.0).pointer_events_box_none(),
            ));
    }

    let trigger_fid = use_signal(format!("{id}__dp_trigger_fid").as_str(), || {
        trigger_focus_id(&id)
    });
    let panel_slot = with_child_owner(|| {
        provide_anchored_overlay_context(open, id.clone(), trigger_fid);
        let ao_opts = AnchoredOverlayContentOpts {
            placement: Some(Placement::trigger(
                trigger_focus_id(&id),
                PopupPlacement::BottomStart,
                FLOATING_TRIGGER_GAP,
                FloatingStrategy::Fixed,
            )),
            kind: OverlayKind::Popover,
            motion: FloatingMotionKind::Fade,
            mode: OverlayMode::Stacked,
            ..Default::default()
        };
        let id = id.clone();

        let content_chrome = content_chrome.clone();
        anchored_overlay_content(ao_opts, move || {
            if kind == DatePickerKind::Week {
                // 周列表依赖年/月；Month 用 caption Select，勿订 year/month（否则换年重建 Select）。
                let _ = year.get();
                let _ = month.get();
            }
            // isolate_reads：is_tracking 仍为 true，Calendar/Combobox 才能注册 anchor。
            isolate_reads(|| {
                let mut panel = build_date_picker_panel_content(
                    &id, kind, caption, open, val, year, month, hour, minute, second,
                );
                panel = paint_slot(panel, SLOT_CONTENT, &content_chrome);
                panel.set_attr(ATTR_FLOATING_MATCH_TRIGGER, "0");
                panel.set_attr(ATTR_FLOATING_MIN_W, "0");
                panel
            })
        })
    });
    view = view.child(panel_slot);

    view
}

#[expect(
    clippy::too_many_arguments,
    reason = "The existing rendering adapter keeps its independent geometry, state and callback parameters"
)]
fn build_date_picker_panel_content(
    id: &str,
    kind: DatePickerKind,
    caption: CalendarCaption,
    open: Signal<bool>,
    val: Signal<String>,
    year: Signal<i32>,
    month: Signal<u32>,
    hour: Signal<String>,
    minute: Signal<String>,
    second: Signal<String>,
) -> Element {
    // Week 导航要订 year/month；Month/DateTime 只 peek，避免改选项拆掉浮层内 Select。
    let (y, m) = match kind {
        DatePickerKind::Week => (year.get(), month.get()),
        _ => (year.peek(), month.peek()),
    };
    let mut panel: Element = vstack().gap("0").into();
    panel = panel
        .bg(tokens::popover())
        .rounded("0.5rem")
        .border_style("solid")
        .border_width(1.0)
        .border_color(tokens::border())
        .popup_shadow()
        .overflow("hidden");

    match kind {
        DatePickerKind::Date | DatePickerKind::DateTimeLocal => {
            let v_sig = val;
            let o_sig = open;
            let kind_c = kind;
            let trig = trigger_focus_id(id);
            let h_sig = hour;
            let mi_sig = minute;
            let s_sig = second;
            let cal: Element = calendar(format!("{id}__grid"))
                .value_signal(val)
                .caption_layout(caption)
                .bordered(false)
                .on_select(move |iso| {
                    if kind_c == DatePickerKind::DateTimeLocal {
                        v_sig.set(format!(
                            "{iso}T{}",
                            format_hms_parts(&h_sig.peek(), &mi_sig.peek(), &s_sig.peek())
                        ));
                    } else {
                        close_picker(o_sig, trig.clone());
                    }
                })
                .into();
            panel = panel.child(cal);
            if kind == DatePickerKind::DateTimeLocal {
                // Hairline then HMS. Calendar already has `p-0.75rem`; do not also
                // put wrapper `pt`/`gap` on top of `time_hms_row` padding or the
                // comboboxes sit a full extra rem below the separator.
                let row = time_hms_row(&format!("{id}__time"), hour, minute, second, false)
                    .pt("0.75rem")
                    .pb("0.75rem");
                panel = panel.child(Element::from(
                    vstack()
                        .w("100%")
                        .child(Element::from(separator()))
                        .child(row),
                ));
            }
        }
        DatePickerKind::Month => {
            let y_sel = use_signal(format!("{id}__dp_msel_y").as_str(), || y.to_string());
            let m_sel = use_signal(format!("{id}__dp_msel_m").as_str(), || m.to_string());
            // Select → year/month：独立订阅，避免重建 caption Select。
            let sync_id = format!("{id}__dp_month_sync");
            {
                let id = id.to_string();

                reactive_anchor(sync_id.clone(), move || {
                    let ys = y_sel.get();
                    let ms = m_sel.get();
                    let y_now = year.peek();
                    let m_now = month.peek();
                    if let Ok(yy) = ys.parse::<i32>() {
                        if yy != y_now {
                            year.set(yy);
                        }
                    }
                    if let Ok(mm) = ms.parse::<u32>() {
                        if (1..=12).contains(&mm) && mm != m_now {
                            month.set(mm);
                            val.set(format!("{:04}-{:02}", year.peek(), mm));
                            close_picker(open, trigger_focus_id(&id));
                        }
                    }
                    let y_now = year.peek();
                    if y_sel.peek() != y_now.to_string() {
                        y_sel.set(y_now.to_string());
                    }
                    if m_sel.peek() != month.peek().to_string() {
                        m_sel.set(month.peek().to_string());
                    }
                    Element::new().w(0.0).h(0.0)
                });
            }
            let header: Element = hstack()
                .w("100%")
                .min_w(0.0)
                .items_center()
                .gap("0.375rem")
                .overflow("hidden")
                .child(calendar_month_select(format!("{id}:dp_m"), m_sel))
                .child(calendar_year_select(
                    format!("{id}:dp_y"),
                    y_sel,
                    year.peek(),
                ))
                .into();
            // Caption selects use flex-basis:0 / min-width:0 to share leftover width.
            // Without a definite panel width the row collapses (empty Month popover).
            // Sync anchor is 0×0 — keep it out of a gapped column or `gap` still
            // inserts 1rem above the only visible row.
            panel = panel
                .child(flow_hidden_portal_host(
                    reactive_placeholder_sized(sync_id, 0.0, 0.0).pointer_events_box_none(),
                ))
                .child(vstack().w(CALENDAR_W).p("0.75rem").child(header));
        }
        DatePickerKind::Week => {
            let (prev, next) = month_nav_buttons(id, year, month);
            let title = build_text_node(
                format_month_year(y, m),
                TextNodeConfig {
                    size_px: 14.0,
                    weight: 500,
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
            // MVP: Select a week → Write YYYY-MM-DD on Monday of that week (simplified)
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
                let trig = trigger_focus_id(id);
                let bid = format!("{id}:w{week_idx}");
                let label = format!("W{week_idx} · {y_c:04}-{m_c:02}-{day:02}");
                let row: Element = button(&bid)
                    .variant("ghost")
                    .size("sm")
                    .w("100%")
                    .on_click(move || {
                        v_sig.set(format!("{y_c:04}-{m_c:02}-{day:02}"));
                        close_picker(o_sig, trig.clone());
                    })
                    .text(label)
                    .into();
                weeks = weeks.child(row);
                d = d.saturating_add(7);
            }
            panel = panel.child(vstack().gap("1rem").p("0.75rem").child(header).child(weeks));
        }
        DatePickerKind::Time => {
            let row = time_hms_row(&format!("{id}__time"), hour, minute, second, false)
                .pt("0.75rem")
                .pb("0.75rem");
            panel = panel.child(vstack().w(CALENDAR_W).child(row));
        }
    }

    panel
}

impl From<DatePickerBuilder> for Element {
    fn from(builder: DatePickerBuilder) -> Self {
        let DatePickerBuilder {
            id,
            kind,
            value,
            default_value,
            caption,
            disabled,
            chrome,
            slots,
        } = builder;
        let open = resolve_open_signal(None, &id, false);
        let val = resolve_string_signal(value, &id, "value", default_value.clone());
        let (iy, im, _) = parse_ymd(&default_value).unwrap_or((2026, 8, 1));
        let year = use_signal(format!("{id}__dp_year").as_str(), move || iy);
        let month = use_signal(format!("{id}__dp_month").as_str(), move || im);
        let (hs, ms, ss) = parse_hms(&default_value)
            .map(|(h, m, s)| (format!("{h:02}"), format!("{m:02}"), format!("{s:02}")))
            .unwrap_or_else(|| {
                if kind == DatePickerKind::Time {
                    (String::new(), String::new(), String::new())
                } else {
                    ("09".into(), "00".into(), "00".into())
                }
            });
        let hour = use_signal(format!("{id}__dp_h").as_str(), move || hs.clone());
        let minute = use_signal(format!("{id}__dp_mi").as_str(), move || ms.clone());
        let second = use_signal(format!("{id}__dp_s").as_str(), move || ss.clone());
        let view_id = format!("{id}__dp_view");
        let props = DatePickerRenderProps {
            id,
            kind,
            caption,
            disabled,
        };
        reactive_component_live(view_id, props, move |p| {
            build_date_picker_view(
                p,
                open,
                val,
                year,
                month,
                hour,
                minute,
                second,
                chrome.clone(),
                slots.get(SLOT_CONTENT),
            )
        })
    }
}

pub struct TimePickerBuilder {
    id: String,
    value: Option<Signal<String>>,
    default_value: String,
    disabled: bool,
    chrome: SlotChrome,
    slots: SlotMap,
}

pub fn time_picker(id: impl Into<String>) -> TimePickerBuilder {
    TimePickerBuilder {
        id: id.into(),
        value: None,
        default_value: String::new(),
        disabled: false,
        chrome: SlotChrome::default(),
        slots: SlotMap::default(),
    }
}

impl TimePickerBuilder {
    crate::impl_slot_chrome_methods!();
    crate::impl_slot_map_methods!();
    crate::impl_overlay_content_slot!(SLOT_CONTENT);

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

impl From<TimePickerBuilder> for Element {
    fn from(builder: TimePickerBuilder) -> Self {
        let TimePickerBuilder {
            id,
            value,
            default_value,
            disabled,
            chrome,
            slots,
        } = builder;
        let mut dp = date_picker(id)
            .kind(DatePickerKind::Time)
            .disabled(disabled)
            .default_value(default_value);
        if let Some(s) = value {
            dp = dp.value_signal(s);
        }
        dp.chrome = chrome;
        dp.slots = slots;
        dp.into()
    }
}

#[cfg(all(test, feature = "product-tests"))]
#[path = "tests/date_picker.rs"]
mod tests;

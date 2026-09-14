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
//! TimePicker 为三个 Combobox（输入过滤 + ↑↓ / Esc）。
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
//! Builder `.class()` / `.style()` land on the **trigger**. Panel card: `.content_class()` /
//! `.content_style()`. Sugar internals (day cells, HMS comboboxes) stay default chrome.

use crate::app::portal_overlay::{OverlayKind, OverlayMode};
use crate::app::reactive::{reactive_anchor, reactive_component_live, reactive_placeholder_sized};
use crate::components::base::SlotChrome;
use crate::components::button::button;
use crate::components::calendar::{
    calendar, calendar_month_select, calendar_year_select, days_in_month, format_long_date,
    format_month_year, parse_ymd, weekday, CalendarCaption,
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
use crate::utils::lucide::lucide_glyph;
use crate::utils::popup::{
    PopupPlacement, ATTR_FLOATING_MATCH_TRIGGER, ATTR_FLOATING_MIN_W, FLOATING_TRIGGER_GAP,
};
use crate::utils::roving::{focus_by_id, focus_id_only};
use crate::utils::text_field::{commit_value_sig, input_value_signal_key};
use crate::utils::text_node::{build_icon_text_node, build_text_node, TextNodeConfig};
use rui_macros::component;
use winit::keyboard::{Key, NamedKey};

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
            Self::Date => "Pick a date",
            Self::DateTimeLocal => "Pick date & time",
            Self::Month => "Pick a month",
            Self::Week => "Pick a week",
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
        DatePickerKind::DateTimeLocal => {
            let date = parse_ymd(raw)
                .map(|(y, m, d)| format_long_date(y, m, d))
                .unwrap_or_else(|| raw.split('T').next().unwrap_or(raw).to_string());
            let hms = parse_hms(raw).map(|(h, mi, s)| format!("{h:02}:{mi:02}:{s:02}"));
            match hms {
                Some(t) => format!("{date}, {t}"),
                None => date,
            }
        }
        DatePickerKind::Month => parse_ymd(&format!("{raw}-01"))
            .or_else(|| parse_ymd(raw))
            .map(|(y, m, _)| format_month_year(y, m))
            .unwrap_or_else(|| raw.to_string()),
        DatePickerKind::Week => parse_ymd(raw)
            .map(|(y, m, d)| format!("Week of {}", format_long_date(y, m, d)))
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
        .class("w-full min-w-0");
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
        .child(chevron_nav_icon("chevron-left"))
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
        .child(chevron_nav_icon("chevron-right"))
        .into();
    (prev, next)
}

/// HTML `date` / `datetime-local` / `month` / `week` selectors.
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn DatePicker(
    id: String,
    kind: Option<String>,
    value: Option<String>,
    value_signal: Option<Signal<String>>,
    caption_layout: Option<String>,
    disabled: bool,
    children: Option<Element>,
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

/// HTML `input type="time"` selector.
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn TimePicker(
    id: String,
    value: Option<String>,
    value_signal: Option<Signal<String>>,
    disabled: bool,
    children: Option<Element>,
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

pub struct DatePickerBuilder {
    id: String,
    kind: DatePickerKind,
    value: Option<Signal<String>>,
    default_value: String,
    caption: CalendarCaption,
    disabled: bool,
    chrome: SlotChrome,
    content_chrome: SlotChrome,
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
        content_chrome: SlotChrome::default(),
    }
}

impl DatePickerBuilder {
    crate::impl_slot_chrome_methods!();

    /// Overlay `class` on the floating panel card (not the trigger).
    pub fn content_class(mut self, classes: impl Into<String>) -> Self {
        self.content_chrome.class = classes.into();
        self
    }

    /// Overlay style on the floating panel card.
    pub fn content_style(
        mut self,
        f: impl Fn(
                crate::components::base::BaseComponentBuilder,
            ) -> crate::components::base::BaseComponentBuilder
            + 'static,
    ) -> Self {
        self.content_chrome.style = Some(std::rc::Rc::new(f));
        self
    }
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
        let kind = kind;
        let disabled = disabled;
        let open = open;
        let val = val;
        let year = year;
        let month = month;
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
            let cal_focus = format!("{id}__grid__cal_focus");
            let trig = trigger_focus_id(&id);
            trigger
                .on_click({
                    let open_t = open;
                    let year_t = year;
                    let month_t = month;
                    let val_peek = val;
                    let cal_focus = cal_focus.clone();
                    let trig = trig.clone();
                    move || {
                        if !open_t.peek() {
                            if let Some((yy, mm, _)) = parse_ymd(&val_peek.peek()) {
                                year_t.set(yy);
                                month_t.set(mm);
                            }
                            open_t.set(true);
                            focus_id_only(cal_focus.clone());
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
                    let cal_focus = cal_focus.clone();
                    let trig = trig.clone();
                    move |ev| match &ev.logical_key {
                        Key::Named(NamedKey::Escape) => {
                            close_picker_keyboard(open_k, trig.clone());
                            true
                        }
                        Key::Named(NamedKey::Enter) | Key::Named(NamedKey::Space) => {
                            if open_k.peek() {
                                close_picker_keyboard(open_k, trig.clone());
                            } else {
                                if let Some((yy, mm, _)) = parse_ymd(&val_k.peek()) {
                                    year_k.set(yy);
                                    month_k.set(mm);
                                }
                                open_k.set(true);
                                focus_id_only(cal_focus.clone());
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

    if kind == DatePickerKind::DateTimeLocal {
        let to_val = format!("{id}__hms_to_val");
        reactive_anchor(to_val.clone(), move || {
            sync_datetime_value_from_hms(val, hour, minute, second);
            Element::new().w(0.0).h(0.0)
        });
        let from_val = format!("{id}__hms_from_val");
        reactive_anchor(from_val.clone(), move || {
            sync_hms_signals(&val.get(), hour, minute, second);
            Element::new().w(0.0).h(0.0)
        });
        view = view
            .child(reactive_placeholder_sized(to_val, 0.0, 0.0).pointer_events_box_none())
            .child(reactive_placeholder_sized(from_val, 0.0, 0.0).pointer_events_box_none());
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
        let kind = kind;
        let caption = caption;
        let open = open;
        let val = val;
        let year = year;
        let month = month;
        let hour = hour;
        let minute = minute;
        let second = second;
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
                panel = content_chrome.apply(panel);
                panel.set_attr(ATTR_FLOATING_MATCH_TRIGGER, "0");
                panel.set_attr(ATTR_FLOATING_MIN_W, "0");
                panel
            })
        })
    });
    view = view.child(panel_slot);

    view
}

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
                panel = panel.child(Element::from(
                    vstack()
                        .w("100%")
                        .pt("1rem")
                        .gap("0.75rem")
                        .child(Element::from(separator()))
                        .child(time_hms_row(
                            &format!("{id}__time"),
                            hour,
                            minute,
                            second,
                            false,
                        )),
                ));
            }
        }
        DatePickerKind::Month => {
            let y_sel = use_signal(format!("{id}__dp_msel_y").as_str(), || y.to_string());
            let m_sel = use_signal(format!("{id}__dp_msel_m").as_str(), || m.to_string());
            // Select → year/month：独立订阅，避免重建 caption Select。
            let sync_id = format!("{id}__dp_month_sync");
            {
                let year = year;
                let month = month;
                let val = val;
                let open = open;
                let id = id.to_string();
                let y_sel = y_sel;
                let m_sel = m_sel;
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
            panel = panel.child(
                vstack()
                    .gap("1rem")
                    .p("0.75rem")
                    .child(reactive_placeholder_sized(sync_id, 0.0, 0.0).pointer_events_box_none())
                    .child(header),
            );
        }
        DatePickerKind::Week => {
            let (prev, next) = month_nav_buttons(&id, year, month);
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
                let trig = trigger_focus_id(&id);
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
            content_chrome,
        } = builder;
        let open = resolve_open_signal(None, &id, false);
        let val = resolve_string_signal(value, &id, "value", default_value.clone());
        let (iy, im, _) = parse_ymd(&default_value).unwrap_or((2026, 8, 1));
        let year = use_signal(format!("{id}__dp_year").as_str(), move || iy);
        let month = use_signal(format!("{id}__dp_month").as_str(), move || im);
        let (ih, imi, is) = parse_hms(&default_value).unwrap_or((9, 0, 0));
        let hour = use_signal(format!("{id}__dp_h").as_str(), || format!("{ih:02}"));
        let minute = use_signal(format!("{id}__dp_mi").as_str(), || format!("{imi:02}"));
        let second = use_signal(format!("{id}__dp_s").as_str(), || format!("{is:02}"));
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
                content_chrome.clone(),
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
}

pub fn time_picker(id: impl Into<String>) -> TimePickerBuilder {
    TimePickerBuilder {
        id: id.into(),
        value: None,
        default_value: String::new(),
        disabled: false,
        chrome: SlotChrome::default(),
    }
}

impl TimePickerBuilder {
    crate::impl_slot_chrome_methods!();

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
    val: Signal<String>,
    hour: Signal<String>,
    minute: Signal<String>,
    second: Signal<String>,
    chrome: SlotChrome,
) -> Element {
    let TimePickerRenderProps { id, disabled } = props;
    let mut root: Element = vstack().gap("0").into();
    if !disabled {
        let to_val = format!("{id}__hms_to_val");
        reactive_anchor(to_val.clone(), move || {
            let hs = hour.get();
            let ms = minute.get();
            let ss = second.get();
            if !hs.is_empty() || !ms.is_empty() || !ss.is_empty() {
                let next = format_hms_parts(&hs, &ms, &ss);
                if val.peek() != next {
                    val.set(next);
                }
            }
            Element::new().w(0.0).h(0.0)
        });
        let from_val = format!("{id}__hms_from_val");
        reactive_anchor(from_val.clone(), move || {
            sync_hms_signals(&val.get(), hour, minute, second);
            Element::new().w(0.0).h(0.0)
        });
        root = root
            .child(reactive_placeholder_sized(to_val, 0.0, 0.0).pointer_events_box_none())
            .child(reactive_placeholder_sized(from_val, 0.0, 0.0).pointer_events_box_none());
    }
    chrome.apply(root.child(time_hms_row(&id, hour, minute, second, disabled)))
}

impl From<TimePickerBuilder> for Element {
    fn from(builder: TimePickerBuilder) -> Self {
        let TimePickerBuilder {
            id,
            value,
            default_value,
            disabled,
            chrome,
        } = builder;
        let initial = default_value.clone();
        let val = value.unwrap_or_else(|| {
            use_signal(format!("{id}__tp_value").as_str(), move || initial.clone())
        });
        let (h0, m0, s0) = parse_hms(&default_value)
            .or_else(|| parse_hms(&val.peek()))
            .map(|(h, m, s)| (format!("{h:02}"), format!("{m:02}"), format!("{s:02}")))
            .unwrap_or_else(|| (String::new(), String::new(), String::new()));
        let hour = use_signal(format!("{id}__tp_h").as_str(), move || h0.clone());
        let minute = use_signal(format!("{id}__tp_m").as_str(), move || m0.clone());
        let second = use_signal(format!("{id}__tp_s").as_str(), move || s0.clone());
        let view_id = format!("{id}__tp_view");
        let props = TimePickerRenderProps { id, disabled };
        reactive_component_live(view_id, props, move |p| {
            build_time_picker_view(p, val, hour, minute, second, chrome.clone())
        })
    }
}

#[cfg(test)]
mod tests {
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
}

//! # Calendar
//!
//! Compare ShadCN **React Aria** monthly calendar (`registry/bases/aria/ui/calendar.tsx` +
//! `examples/aria` / sidebar-12 `DatePicker`): independent monthly grid, DatePicker floating layer and
//! 侧栏内联共用同一组件。
//!
//! - **caption_layout**: `label` (‹ month year ›) or `dropdown` (reuse the month/year menu of [`select`], opposite Aria `CalendarMonthPicker` / `CalendarYearPicker`).
//! - **Value format**: ISO **`YYYY-MM-DD`** (local calendar day, no time zone).
//! - **键盘**：←/→ 日；↑/↓ 周；Home/End 周首尾；PageUp/PageDown 月；Enter 选定。
//!
//! ## Usage
//! ```ignore
//! // A · RSX (sidebar-12 style inline; captionLayout / valueSignal / defaultValue are macro aliases)
//! ui! {
//!     <Calendar id="due" captionLayout="dropdown" />
//!     <Calendar id="due-c" valueSignal={date_sig} captionLayout="label" />
//!     <Calendar id="due-d" defaultValue="2026-08-14" />
//! }
//!
//! // B · Builder
//! calendar("due")
//!     .caption_layout(CalendarCaption::Dropdown)
//!     .value_signal(date_sig)
//! ```

use std::rc::Rc;

use crate::app::reactive::{
    reactive_anchor, reactive_component_live_sized, reactive_placeholder_sized,
};
use crate::components::base::{paint_slot, SlotChrome, SlotMap};
use crate::components::button::button;
use crate::components::select::{select, SelectItemData};
use crate::core::reactivity::{use_signal, Signal};
use crate::core::tree::{parse_text_color, CursorKind, Element};
use crate::primitives::interactive::interactive;
use crate::primitives::r#box::{hstack, vstack};
use crate::tokens;
use crate::utils::lucide::lucide_glyph;
use crate::utils::popup::flow_hidden_portal_host;
use crate::utils::text_node::{build_icon_text_node, build_text_node, TextNodeConfig};
use jadix_macros::component;
use winit::keyboard::{Key, NamedKey};

/// ShadCN `--cell-size`（new-york `--spacing(8)` / sidebar-12 `2.1rem` ≈ 2rem）。
const CELL: &str = "2rem";
/// Weekday row + 6 weeks × `CELL`, line spacing `0.5rem`: `7×2rem + 6×0.5rem`.
/// `__days` placeholder `h:auto` will collapse to 0 before reconciliation, and the day button overflows and covers the DateTime `Separator`.
const CALENDAR_DAY_GRID_MIN_H: &str = "17rem";
/// 7×cell + p-3；固定宽度，避免换月/换标题文案时浮层 w-fit 跟着抖。
pub(crate) const CALENDAR_W: &str = "17.5rem";
const YEAR_SELECT_LO: i32 = 1900;
const YEAR_SELECT_HI: i32 = 2100;
const CAPTION_MONTH_GROW: f32 = 1.0;
const CAPTION_YEAR_GROW: f32 = 1.0;

#[cfg(test)]
const ATTR_SLOT: &str = "data-slot";
const SLOT_DAY: &str = "calendar-day";
const SLOT_DAY_TODAY: &str = "calendar-day-today";
const SLOT_WEEKDAY: &str = "calendar-weekday";
const SLOT_NAV_PREV: &str = "calendar-nav-prev";
const SLOT_NAV_NEXT: &str = "calendar-nav-next";

/// Title bar: plain label or month/year dropdown (opposite Aria `captionLayout`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CalendarCaption {
    #[default]
    Label,
    Dropdown,
}

impl CalendarCaption {
    pub fn parse(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "dropdown" | "dropdown-months" | "dropdown-years" => Self::Dropdown,
            _ => Self::Label,
        }
    }
}

pub fn is_leap(y: i32) -> bool {
    (y % 4 == 0 && y % 100 != 0) || (y % 400 == 0)
}

pub fn days_in_month(y: i32, m: u32) -> u32 {
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
pub fn weekday(y: i32, m: u32, d: u32) -> u32 {
    let t = [0, 3, 2, 5, 0, 3, 5, 1, 4, 6, 2, 4];
    let mut yy = y;
    if m < 3 {
        yy -= 1;
    }
    let w = (yy + yy / 4 - yy / 100 + yy / 400 + t[(m as usize) - 1] + d as i32) % 7;
    w.rem_euclid(7) as u32
}

pub fn parse_ymd(s: &str) -> Option<(i32, u32, u32)> {
    let parts: Vec<_> = s.trim().split(['-', 'T', ' ', '/']).collect();
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

pub fn format_ymd(y: i32, m: u32, d: u32) -> String {
    format!("{y:04}-{m:02}-{d:02}")
}

/// Unix day → Gregorian calendar (Howard Hinnant `civil_from_days`).
pub fn civil_from_unix_days(z: i64) -> (i32, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u32;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y as i32, m, d)
}

pub fn today_ymd() -> (i32, u32, u32) {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    civil_from_unix_days(secs.div_euclid(86_400))
}

pub fn add_months(y: i32, m: u32, delta: i32) -> (i32, u32) {
    let idx = (y as i32) * 12 + (m as i32 - 1) + delta;
    let yy = idx.div_euclid(12);
    let mm = idx.rem_euclid(12) + 1;
    (yy, mm as u32)
}

pub fn add_days(y: i32, m: u32, d: u32, delta: i32) -> (i32, u32, u32) {
    let mut yy = y;
    let mut mm = m;
    let mut dd = d as i32 + delta;
    loop {
        let dim = days_in_month(yy, mm) as i32;
        if dd < 1 {
            let (py, pm) = add_months(yy, mm, -1);
            yy = py;
            mm = pm;
            dd += days_in_month(yy, mm) as i32;
        } else if dd > dim {
            dd -= dim;
            let (ny, nm) = add_months(yy, mm, 1);
            yy = ny;
            mm = nm;
        } else {
            return (yy, mm, dd as u32);
        }
    }
}

fn clamp_day(y: i32, m: u32, d: u32) -> u32 {
    d.min(days_in_month(y, m)).max(1)
}

const MONTH_LONG: [(&str, &str, &str); 12] = [
    ("jadix.calendar.month.january", "January", "一月"),
    ("jadix.calendar.month.february", "February", "二月"),
    ("jadix.calendar.month.march", "March", "三月"),
    ("jadix.calendar.month.april", "April", "四月"),
    ("jadix.calendar.month.may", "May", "五月"),
    ("jadix.calendar.month.june", "June", "六月"),
    ("jadix.calendar.month.july", "July", "七月"),
    ("jadix.calendar.month.august", "August", "八月"),
    ("jadix.calendar.month.september", "September", "九月"),
    ("jadix.calendar.month.october", "October", "十月"),
    ("jadix.calendar.month.november", "November", "十一月"),
    ("jadix.calendar.month.december", "December", "十二月"),
];

const MONTH_SHORT: [(&str, &str, &str); 12] = [
    ("jadix.calendar.month_short.jan", "Jan", "1月"),
    ("jadix.calendar.month_short.feb", "Feb", "2月"),
    ("jadix.calendar.month_short.mar", "Mar", "3月"),
    ("jadix.calendar.month_short.apr", "Apr", "4月"),
    ("jadix.calendar.month_short.may", "May", "5月"),
    ("jadix.calendar.month_short.jun", "Jun", "6月"),
    ("jadix.calendar.month_short.jul", "Jul", "7月"),
    ("jadix.calendar.month_short.aug", "Aug", "8月"),
    ("jadix.calendar.month_short.sep", "Sep", "9月"),
    ("jadix.calendar.month_short.oct", "Oct", "10月"),
    ("jadix.calendar.month_short.nov", "Nov", "11月"),
    ("jadix.calendar.month_short.dec", "Dec", "12月"),
];

const WEEKDAY_SHORT: [(&str, &str, &str); 7] = [
    ("jadix.calendar.weekday_short.sun", "Su", "日"),
    ("jadix.calendar.weekday_short.mon", "Mo", "一"),
    ("jadix.calendar.weekday_short.tue", "Tu", "二"),
    ("jadix.calendar.weekday_short.wed", "We", "三"),
    ("jadix.calendar.weekday_short.thu", "Th", "四"),
    ("jadix.calendar.weekday_short.fri", "Fr", "五"),
    ("jadix.calendar.weekday_short.sat", "Sa", "六"),
];

fn month_long(m: u32) -> String {
    let i = m.saturating_sub(1).min(11) as usize;
    crate::tr_default_zh(MONTH_LONG[i].0, MONTH_LONG[i].1, MONTH_LONG[i].2)
}

fn month_short(m: u32) -> String {
    let i = m.saturating_sub(1).min(11) as usize;
    crate::tr_default_zh(MONTH_SHORT[i].0, MONTH_SHORT[i].1, MONTH_SHORT[i].2)
}

fn weekday_short(i: u32) -> String {
    let i = (i as usize) % 7;
    crate::tr_default_zh(WEEKDAY_SHORT[i].0, WEEKDAY_SHORT[i].1, WEEKDAY_SHORT[i].2)
}

/// ShadCN `dateStyle: "long"`：`August 14, 2026` / `2026年8月14日`.
pub fn format_long_date(y: i32, m: u32, d: u32) -> String {
    crate::format_date_long(y, m, d)
}

pub fn format_month_year(y: i32, m: u32) -> String {
    crate::format_year_month(y, m)
}

/// Leading blank cells before day 1 when the week starts at `week_start` (0 = Sunday).
pub fn week_grid_leading(first_of_month_weekday: u32, week_start: u32) -> i32 {
    (first_of_month_weekday as i32 - week_start as i32).rem_euclid(7)
}

/// ShadCN Aria `<Calendar>`。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn Calendar(
    id: String,
    value: Option<String>,
    value_signal: Option<Signal<String>>,
    default_value: Option<String>,
    caption_layout: Option<String>,
    bordered: Option<bool>,
    disabled: bool,
    children: Option<Element>,
) -> Element {
    let _ = children;
    let mut b = calendar(&id)
        .disabled(disabled)
        .bordered(bordered.unwrap_or(true));
    if let Some(sig) = value_signal {
        b = b.value_signal(sig);
    }
    if let Some(v) = value.or(default_value) {
        b = b.default_value(v);
    }
    if let Some(c) = caption_layout {
        b = b.caption_layout(CalendarCaption::parse(&c));
    }
    b.into()
}

pub struct CalendarBuilder {
    id: String,
    value: Option<Signal<String>>,
    default_value: String,
    caption: CalendarCaption,
    disabled: bool,
    /// 独立使用时 `rounded-lg border`（对位 examples）；浮层内关闭以免双框。
    bordered: bool,
    on_select: Option<Rc<dyn Fn(String)>>,
    chrome: SlotChrome,
    slots: SlotMap,
}

pub fn calendar(id: impl Into<String>) -> CalendarBuilder {
    CalendarBuilder {
        id: id.into(),
        value: None,
        default_value: String::new(),
        caption: CalendarCaption::Label,
        disabled: false,
        bordered: true,
        on_select: None,
        chrome: SlotChrome::default(),
        slots: SlotMap::default(),
    }
}

impl CalendarBuilder {
    crate::impl_slot_chrome_methods!();
    crate::impl_slot_map_methods!();

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
    pub fn bordered(mut self, v: bool) -> Self {
        self.bordered = v;
        self
    }
    pub fn on_select(mut self, f: impl Fn(String) + 'static) -> Self {
        self.on_select = Some(Rc::new(f));
        self
    }
}

#[derive(Clone, PartialEq)]
struct CalendarRenderProps {
    id: String,
    caption: CalendarCaption,
    disabled: bool,
    bordered: bool,
}

fn caption_label(text: &str) -> Element {
    build_text_node(
        text,
        TextNodeConfig {
            size_px: 14.0,
            weight: 500,
            color: parse_text_color("foreground"),
            ..Default::default()
        },
    )
}

fn weekday_label(text: &str) -> Element {
    build_text_node(
        text,
        TextNodeConfig {
            size_px: 0.8 * 16.0,
            weight: 400,
            color: parse_text_color("muted-foreground"),
            ..Default::default()
        },
    )
}

fn calendar_grid_cell(child: Element) -> Element {
    hstack()
        .flex_grow(1.0)
        .flex_shrink(1.0)
        .flex_basis(0.0)
        .min_w(0.0)
        .h(CELL)
        .items_center()
        .justify_center()
        .child(child)
        .into()
}

fn lucide_chevron(name: &str) -> Element {
    let glyph = lucide_glyph(name).unwrap_or_else(|| "?".into());
    build_icon_text_node(glyph)
        .text_size(16.0)
        .text_color(parse_text_color("foreground"))
}

/// `select()` 根节点默认 `width: 100%`。同一行两个这样的子项会各按父级全宽排，
/// 再叠加 `justify-center` 就会叠在中间（命中区重叠 → 悬停在两个 trigger 间闪）。
/// 标题栏槽位改成 `flex: grow 1 0 / min-width: 0`，只裁溢出，不抢同一块区域。
fn caption_select_flex_item(el: Element, grow: f32) -> Element {
    el.w("auto")
        .min_w(0.0)
        .max_w("100%")
        .flex_grow(grow)
        .flex_shrink(1.0)
        .flex_basis(0.0)
        .overflow("hidden")
}

/// Aria `CalendarMonthPicker`：复用产品面 [`select`]（勿自绘月份网格）。
/// 触发器吃满标题栏剩余宽度，菜单跟触发器同宽；文案溢出省略，避免撑破左右翻月按钮。
pub(crate) fn calendar_month_select(id: impl Into<String>, value: Signal<String>) -> Element {
    let mut b = select(id)
        .value_signal(value)
        .placeholder(crate::tr_default_zh(
            "jadix.calendar.month.placeholder",
            "Month",
            "月份",
        ))
        .trigger_width("100%")
        .max_height("16rem")
        .scroll_selected("center");
    for i in 0..12u32 {
        let name = month_long(i + 1);
        b = b.item(SelectItemData::new((i + 1).to_string(), name));
    }
    caption_select_flex_item(b.into(), CAPTION_MONTH_GROW)
}

/// Aria `CalendarYearPicker`：复用产品面 [`select`]。
/// 与月份共用 flex 槽；grow 相等、`flex-basis: 0`，比例不受文案宽度干扰（i18n 月份名可长可短）。
pub(crate) fn calendar_year_select(
    id: impl Into<String>,
    value: Signal<String>,
    around: i32,
) -> Element {
    let lo = around.min(YEAR_SELECT_LO);
    let hi = around.max(YEAR_SELECT_HI);
    let mut b = select(id)
        .value_signal(value)
        .placeholder(crate::tr_default_zh(
            "jadix.calendar.year_placeholder",
            "Year",
            "年份",
        ))
        .trigger_width("100%")
        .max_height("16rem")
        .scroll_selected("center");
    for yy in lo..=hi {
        b = b.item(SelectItemData::new(yy.to_string(), yy.to_string()));
    }
    caption_select_flex_item(b.into(), CAPTION_YEAR_GROW)
}

fn build_calendar_view(
    props: CalendarRenderProps,
    val: Signal<String>,
    year: Signal<i32>,
    month: Signal<u32>,
    focus_day: Signal<u32>,
    month_sel: Signal<String>,
    year_sel: Signal<String>,
    on_select: Option<Rc<dyn Fn(String)>>,
    slots: SlotMap,
) -> Element {
    let CalendarRenderProps {
        id,
        caption,
        disabled,
        bordered,
    } = props;

    // Select → month/year：独立订阅。外层若 `.get()` month_sel/year_sel（或随之
    // 改写的 year/month），会整树重建 caption Select → 选一次后再也打不开。
    let sync_id = format!("{id}__cap_sync");
    {
        let year = year;
        let month = month;
        let focus_day = focus_day;
        let month_sel = month_sel;
        let year_sel = year_sel;
        reactive_anchor(sync_id.clone(), move || {
            let ms = month_sel.get();
            let ys = year_sel.get();
            let y = year.peek();
            let m = month.peek();
            if let Ok(mm) = ms.parse::<u32>() {
                if (1..=12).contains(&mm) && mm != m {
                    month.set(mm);
                    focus_day.set(clamp_day(y, mm, focus_day.peek()));
                }
            }
            if let Ok(yy) = ys.parse::<i32>() {
                if yy != year.peek() {
                    year.set(yy);
                    focus_day.set(clamp_day(yy, month.peek(), focus_day.peek()));
                }
            }
            Element::new().w(0.0).h(0.0)
        });
    }

    // Dropdown：外层只 peek，保留 Select 实例；Label：订 year/month 刷新标题。
    let (y, m) = if caption == CalendarCaption::Dropdown {
        (year.peek(), month.peek())
    } else {
        (year.get(), month.get())
    };

    let y_sig = year;
    let m_sig = month;
    let f_sig = focus_day;
    let v_sig = val;
    let on_sel = on_select.clone();

    let mut root: Element = vstack()
        .gap("1rem")
        .p("0.75rem")
        .w(CALENDAR_W)
        .overflow("hidden")
        .into();
    if bordered {
        root = root
            .bg(tokens::background())
            .rounded("0.5rem")
            .border_style("solid")
            .border_width(1.0)
            .border_color(tokens::border());
    }
    root = root.child(flow_hidden_portal_host(
        reactive_placeholder_sized(sync_id, 0.0, 0.0).pointer_events_box_none(),
    ));
    if !disabled {
        let y_k = y_sig;
        let m_k = m_sig;
        let f_k = f_sig;
        let v_k = v_sig;
        let msel_k = month_sel;
        let ysel_k = year_sel;
        let on_k = on_sel.clone();
        let fid = format!("{id}__cal_focus");
        root = interactive(root)
            .focus_id(fid.as_str())
            .focusable(true)
            .focus_ring(true)
            .on_key_down(move |ev| {
                let mut yy = y_k.peek();
                let mut mm = m_k.peek();
                let mut dd = clamp_day(yy, mm, f_k.peek());
                match &ev.logical_key {
                    Key::Named(NamedKey::ArrowLeft) => {
                        let n = add_days(yy, mm, dd, -1);
                        yy = n.0;
                        mm = n.1;
                        dd = n.2;
                    }
                    Key::Named(NamedKey::ArrowRight) => {
                        let n = add_days(yy, mm, dd, 1);
                        yy = n.0;
                        mm = n.1;
                        dd = n.2;
                    }
                    Key::Named(NamedKey::ArrowUp) => {
                        let n = add_days(yy, mm, dd, -7);
                        yy = n.0;
                        mm = n.1;
                        dd = n.2;
                    }
                    Key::Named(NamedKey::ArrowDown) => {
                        let n = add_days(yy, mm, dd, 7);
                        yy = n.0;
                        mm = n.1;
                        dd = n.2;
                    }
                    Key::Named(NamedKey::Home) => {
                        let start = crate::first_weekday() as i32;
                        let wd = weekday(yy, mm, dd) as i32;
                        let from_start = (wd - start).rem_euclid(7);
                        let n = add_days(yy, mm, dd, -from_start);
                        yy = n.0;
                        mm = n.1;
                        dd = n.2;
                    }
                    Key::Named(NamedKey::End) => {
                        let start = crate::first_weekday() as i32;
                        let wd = weekday(yy, mm, dd) as i32;
                        let from_start = (wd - start).rem_euclid(7);
                        let n = add_days(yy, mm, dd, 6 - from_start);
                        yy = n.0;
                        mm = n.1;
                        dd = n.2;
                    }
                    Key::Named(NamedKey::PageUp) => {
                        let (ny, nm) = add_months(yy, mm, -1);
                        yy = ny;
                        mm = nm;
                        dd = clamp_day(yy, mm, dd);
                    }
                    Key::Named(NamedKey::PageDown) => {
                        let (ny, nm) = add_months(yy, mm, 1);
                        yy = ny;
                        mm = nm;
                        dd = clamp_day(yy, mm, dd);
                    }
                    k if crate::utils::keyboard::is_enter_or_space(&k) => {
                        let s = format_ymd(yy, mm, dd);
                        v_k.set(s.clone());
                        if let Some(cb) = &on_k {
                            cb(s);
                        }
                        return true;
                    }
                    Key::Named(NamedKey::Escape) => return false,
                    _ => return false,
                }
                y_k.set(yy);
                m_k.set(mm);
                f_k.set(dd);
                msel_k.set(mm.to_string());
                ysel_k.set(yy.to_string());
                true
            })
            .into();
        root = root.cursor_kind(CursorKind::Pointer);
    }

    let prev: Element = slots
        .paint(
            SLOT_NAV_PREV,
            button(format!("{id}:prev"))
                .variant("ghost")
                .size("icon-sm")
                .disabled(disabled)
                .on_click({
                    let y_sig = y_sig;
                    let m_sig = m_sig;
                    let f_sig = f_sig;
                    let msel = month_sel;
                    let ysel = year_sel;
                    move || {
                        let (yy, mm) = add_months(y_sig.peek(), m_sig.peek(), -1);
                        y_sig.set(yy);
                        m_sig.set(mm);
                        f_sig.set(clamp_day(yy, mm, f_sig.peek()));
                        msel.set(mm.to_string());
                        ysel.set(yy.to_string());
                    }
                })
                .child(lucide_chevron(crate::chrome_nav_chevron(true)))
                .into(),
        )
        .flex_shrink(0.0);
    let next: Element = slots
        .paint(
            SLOT_NAV_NEXT,
            button(format!("{id}:next"))
                .variant("ghost")
                .size("icon-sm")
                .disabled(disabled)
                .on_click({
                    let y_sig = y_sig;
                    let m_sig = m_sig;
                    let f_sig = f_sig;
                    let msel = month_sel;
                    let ysel = year_sel;
                    move || {
                        let (yy, mm) = add_months(y_sig.peek(), m_sig.peek(), 1);
                        y_sig.set(yy);
                        m_sig.set(mm);
                        f_sig.set(clamp_day(yy, mm, f_sig.peek()));
                        msel.set(mm.to_string());
                        ysel.set(yy.to_string());
                    }
                })
                .child(lucide_chevron(crate::chrome_nav_chevron(false)))
                .into(),
        )
        .flex_shrink(0.0);

    let dropdown = caption == CalendarCaption::Dropdown;
    let caption_el: Element = if dropdown && !disabled {
        hstack()
            .flex_grow(1.0)
            .min_w(0.0)
            .gap("0.375rem")
            .items_center()
            .overflow("hidden")
            .child(calendar_month_select(format!("{id}:cap_m"), month_sel))
            .child(calendar_year_select(format!("{id}:cap_y"), year_sel, y))
            .into()
    } else if dropdown {
        hstack()
            .flex_grow(1.0)
            .min_w(0.0)
            .gap("0.375rem")
            .items_center()
            .overflow("hidden")
            .child(caption_label(&month_short(m)))
            .child(caption_label(&format!("{y}")))
            .into()
    } else {
        caption_label(&format_month_year(y, m))
    };

    let mut caption_slot = hstack()
        .flex_grow(1.0)
        .min_w(0.0)
        .overflow("hidden")
        .items_center();
    // Label 标题在剩余空间里居中；Dropdown 的 Select 必须 stretch，
    // `justify-center` 会把溢出的 100% 宽子项叠到中间。
    if !dropdown {
        caption_slot = caption_slot.justify_center();
    }
    let header: Element = hstack()
        .w("100%")
        .h(CELL)
        .items_center()
        .justify("between")
        .gap("0.25rem")
        .overflow("hidden")
        .child(prev)
        .child(caption_slot.child(caption_el))
        .child(next)
        .into();
    root = root.child(header);

    // 日网格单独订阅 val/focus/year/month，避免点日或换月重建 caption Select。
    let grid_id = format!("{id}__days");
    let grid_id_key = id.clone();
    let on_grid = on_select.clone();
    let grid_slots = slots.clone();
    reactive_anchor(grid_id.clone(), move || {
        build_calendar_day_grid(
            &grid_id_key,
            disabled,
            val,
            year,
            month,
            focus_day,
            month_sel,
            year_sel,
            on_grid.clone(),
            grid_slots.clone(),
        )
    });
    root.child(
        reactive_placeholder_sized(grid_id, "100%", CALENDAR_DAY_GRID_MIN_H)
            .min_h(CALENDAR_DAY_GRID_MIN_H)
            .overflow("hidden"),
    )
}

fn build_calendar_day_grid(
    id: &str,
    disabled: bool,
    val: Signal<String>,
    year: Signal<i32>,
    month: Signal<u32>,
    focus_day: Signal<u32>,
    month_sel: Signal<String>,
    year_sel: Signal<String>,
    on_select: Option<Rc<dyn Fn(String)>>,
    slots: SlotMap,
) -> Element {
    let current = val.get();
    let y = year.get();
    let m = month.get();
    let _ = clamp_day(y, m, focus_day.get());
    let selected = parse_ymd(&current);
    let today = today_ymd();
    let commit: Rc<dyn Fn(i32, u32, u32)> = {
        let v_sig = val;
        Rc::new(move |yy: i32, mm: u32, dd: u32| {
            let s = format_ymd(yy, mm, dd);
            v_sig.set(s.clone());
            if let Some(cb) = &on_select {
                cb(s);
            }
        })
    };

    let week_start = crate::first_weekday();
    let mut head: Element = hstack().w("100%").into();
    for i in 0..7u32 {
        let name = weekday_short((week_start + i) % 7);
        let lab = slots.paint(SLOT_WEEKDAY, weekday_label(&name));
        head = head.child(calendar_grid_cell(lab));
    }

    let first = weekday(y, m, 1);
    let leading = week_grid_leading(first, week_start);
    let dim = days_in_month(y, m) as i32;
    let (py, pm) = add_months(y, m, -1);
    let pdim = days_in_month(py, pm) as i32;
    let (ny, nm) = add_months(y, m, 1);
    let mut grid: Element = vstack()
        .w("100%")
        .gap("0.5rem")
        .min_h(CALENDAR_DAY_GRID_MIN_H)
        .child(head)
        .into();
    for week in 0..6 {
        let mut row: Element = hstack().w("100%").into();
        for col in 0..7 {
            let offset = week * 7 + col - leading;
            let (cy, cm, d, outside) = if offset < 0 {
                (py, pm, (pdim + offset + 1) as u32, true)
            } else if offset >= dim {
                (ny, nm, (offset - dim + 1) as u32, true)
            } else {
                (y, m, (offset + 1) as u32, false)
            };
            let is_sel = selected
                .map(|(sy, sm, sd)| sy == cy && sm == cm && sd == d)
                .unwrap_or(false);
            let is_today = today == (cy, cm, d);
            let variant = if is_sel {
                "default"
            } else if is_today && !outside {
                "secondary"
            } else {
                "ghost"
            };
            let y_sig = year;
            let m_sig = month;
            let f_sig = focus_day;
            let msel = month_sel;
            let ysel = year_sel;
            let commit_c = commit.clone();
            let mut b = button(format!("{id}:d{week}-{col}"))
                .variant(variant)
                .size("icon-sm")
                .disabled(disabled)
                .on_click(move || {
                    y_sig.set(cy);
                    m_sig.set(cm);
                    f_sig.set(d);
                    msel.set(cm.to_string());
                    ysel.set(cy.to_string());
                    commit_c(cy, cm, d);
                })
                .text(format!("{d}"));
            if outside {
                b = b.text_color(parse_text_color("muted-foreground"));
            }
            let today = is_today && !outside;
            let slot = if today { SLOT_DAY_TODAY } else { SLOT_DAY };
            let mut day: Element = paint_slot(Element::from(b), slot, &slots.get(SLOT_DAY));
            if today {
                day = slots.get(SLOT_DAY_TODAY).apply(day);
            }
            row = row.child(calendar_grid_cell(day.w("100%").h("100%")));
        }
        grid = grid.child(row);
    }
    grid
}

impl From<CalendarBuilder> for Element {
    fn from(builder: CalendarBuilder) -> Self {
        let CalendarBuilder {
            id,
            value,
            default_value,
            caption,
            disabled,
            bordered,
            on_select,
            chrome,
            slots,
        } = builder;
        let id = crate::utils::resolve_instance_id(id, "calendar");
        let initial = default_value.clone();
        let val = value.unwrap_or_else(|| {
            use_signal(format!("{id}__cal_value").as_str(), move || initial.clone())
        });
        let (iy, im, iday) = parse_ymd(&val.peek())
            .or_else(|| parse_ymd(&default_value))
            .unwrap_or_else(today_ymd);
        let year = use_signal(format!("{id}__cal_year").as_str(), move || iy);
        let month = use_signal(format!("{id}__cal_month").as_str(), move || im);
        let focus_day = use_signal(format!("{id}__cal_focus_d").as_str(), move || iday);
        let month_sel = use_signal(format!("{id}__cal_msel").as_str(), || im.to_string());
        let year_sel = use_signal(format!("{id}__cal_ysel").as_str(), || iy.to_string());
        let view_id = format!("{id}__cal_view");
        let props = CalendarRenderProps {
            id,
            caption,
            disabled,
            bordered,
        };
        reactive_component_live_sized(view_id, props, CALENDAR_W, "auto", move |p| {
            chrome.apply(build_calendar_view(
                p,
                val,
                year,
                month,
                focus_day,
                month_sel,
                year_sel,
                on_select.clone(),
                slots.clone(),
            ))
        })
    }
}

#[cfg(test)]
mod tests {
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
            outside.authoring_state.get::<crate::components::button::ButtonHoverPaint>()
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
                let zero = crate::layout::dim_length_px(c.style.size.width)
                    .is_some_and(|w| w == 0.0)
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
}

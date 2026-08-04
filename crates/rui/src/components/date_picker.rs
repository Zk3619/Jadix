//! # DatePicker / TimePicker
//!
//! 日期与时间选择占位。对应 HTML `date` / `datetime-local` / `month` / `week` / `time`。
//! 同属日历/时钟弹层域，可共享后续日历内核；与文件、颜色选择器无关。
//!
//! **当前仅为 API + 视觉壳**：不打开日历或时间滚轮。
//!
//! ## 用法
//! ```ignore
//! <DatePicker id="due" kind="date" />
//! <DatePicker id="slot" kind="datetime-local" />
//! <DatePicker id="billing" kind="month" />
//! <TimePicker id="alarm" />
//! ```

use crate::components::picker_stub::{lucide, stub_label, stub_trigger};
use crate::core::tree::Element;
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

/// HTML `date` / `datetime-local` / `month` / `week` 占位。
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
    let display = match value.as_ref() {
        Some(s) if !s.is_empty() => s.clone(),
        _ => kind.placeholder().to_string(),
    };
    let muted = value.as_ref().map(|s| s.is_empty()).unwrap_or(true);
    let icon = lucide(0xe063); // calendar
    stub_trigger(&id, icon, stub_label(&display, muted), disabled)
}

/// HTML `input type="time"` 占位。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn TimePicker(
    id: String,
    value: Option<String>,
    disabled: bool,
    children: Option<Element>,
) -> Element {
    let _ = children;
    let display = match value.as_ref() {
        Some(s) if !s.is_empty() => s.clone(),
        _ => "Pick a time…".into(),
    };
    let muted = value.as_ref().map(|s| s.is_empty()).unwrap_or(true);
    let icon = lucide(0xe087); // clock
    stub_trigger(&id, icon, stub_label(&display, muted), disabled)
}

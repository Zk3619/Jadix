//! # UI 组件层
//!
//! 面向 `ui!` 的可组合控件：按钮、输入、滚动区、浮层等。多数以 `#[component]` 暴露
//! PascalCase 标签（如 `<Button>`）；少数仅提供函数 / Builder（如 [`dialog_panel`]、[`toast`]、[`dropdown`]）。
//!
//! 布局原语（`Box` / `HStack` / `VStack` / `GStack`）在 [`crate::primitives`]；本模块在其上拼装交互与样式。
//! 函数名故意与 RSX 标签一致，故关闭 `non_snake_case`。
//!
//! ## 横切约定
//! - 有状态控件尽量给稳定唯一 `id`（信号键 / `focus_id` 依赖它）。
//! - 受控状态用 [`crate::core::reactivity::Signal`]；未传则组件按 `id` 建内部信号。
//! - 浮层类（Select / Popover / Dialog）常需包在 `<Reactive>` 内，并走 [`crate::app::portal_overlay`] Esc 栈。
//! - 选择器占位按行为分文件：[`file_picker`] / [`color_picker`] / [`date_picker`]（勿再扩 `Input type`）。
#![allow(non_snake_case)]

pub mod base;
pub mod button;
pub mod card;
pub mod checkbox;
pub mod color_picker;
pub mod date_picker;
pub mod dialog;
pub mod dropdown;
pub mod file_picker;
pub mod focus_trap;
pub mod image;
pub mod input;
pub mod label;
pub mod navigator_host;
pub mod picker_stub;
pub mod popover;
pub mod portal;
pub mod radio_group;
pub mod scroll_area;
pub mod scrollbar;
pub mod select;
pub mod separator;
pub mod slider;
pub mod switch;
pub mod tabs;
pub mod toast;
pub mod toggle;
pub mod toggle_group;

pub use base::BaseComponentBuilder;
pub use button::{button, Button, ButtonBuilder, ButtonProps};
pub use card::{Card, CardProps};
pub use checkbox::{checkbox, Checkbox, CheckboxBuilder, CheckboxProps};
pub use color_picker::{ColorPicker, ColorPickerProps};
pub use date_picker::{DatePicker, DatePickerKind, DatePickerProps, TimePicker, TimePickerProps};
pub use dialog::{dialog_panel, DialogBuilder};
pub use dropdown::{dropdown, DropdownBuilder};
pub use file_picker::{DirectoryPicker, DirectoryPickerProps, FilePicker, FilePickerProps};
pub use focus_trap::{
    FocusTrap, FocusTrapProps, FOCUS_TRAP_OWNER_KEY, FOCUS_TRAP_RING_KEY,
};
pub use image::{Image, ImageProps};
pub use input::{
    filter_email_insert, filter_insert_for_type, filter_number_insert, filter_tel_insert,
    filter_url_insert, input, input_caret_signal_key, input_ime_anchor_signal_key,
    input_overwrite_signal_key, input_selection_signal_key, input_type_signal_key, Input,
    InputBuilder, InputProps, InputType, IME_ACTIVE_SIGNAL, INPUT_FOCUS_PREFIX,
};
pub use label::{label, text_node as text, Label, LabelBuilder, LabelProps};
pub use navigator_host::{navigation_host, NavigationHostBuilder};
pub use popover::{popover_panel, PopoverBuilder};
pub use portal::{Portal, PortalProps};
pub use radio_group::{radio_group, RadioGroup, RadioGroupBuilder, RadioGroupProps};
pub use scroll_area::{scroll_area, scrollarea, ScrollArea, ScrollAreaBuilder, ScrollAreaProps};
pub use scrollbar::{scrollbar, scrollbarbuilder, Scrollbar, ScrollbarBuilder, ScrollbarOrientation, ScrollbarProps};
pub use select::{select, Select, SelectBuilder, SelectProps};
pub use separator::{separator, Separator, SeparatorBuilder, SeparatorProps};
pub use slider::{slider, Slider, SliderBuilder, SliderProps};
pub use switch::{switch, Switch, SwitchBuilder, SwitchProps};
pub use tabs::{
    tabs_headless, tabs_headless_root, Tabs, TabsHeadlessBuilder, TabsProps, TABSTRIP_FOCUS_PREFIX,
};
pub use toast::toast;
pub use toggle::{toggle, Toggle, ToggleBuilder, ToggleProps};
pub use toggle_group::{toggle_group, ToggleGroup, ToggleGroupBuilder, ToggleGroupProps};

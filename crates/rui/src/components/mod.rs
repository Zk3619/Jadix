//! # UI 组件层
//!
//! 面向 `ui!` **与** 纯 Rust Builder 的可组合控件：按钮、输入、滚动区、浮层等。
//! 多数以 `#[component]` 暴露 PascalCase 标签（如 `<Button>`），并导出等价的
//! `fn` / `*Builder`（如 [`button`]、[`ButtonBuilder`]）；少数暂仅函数 / Builder
//! （如 [`dialog_panel`]、[`toast`]、[`dropdown`]）。两种写法互嵌兼容，见
//! `docs/COMPONENT_LIBRARY.md` §3（ADR-9）。
//!
//! 布局原语（`Box` / `HStack` / `VStack` / `GStack`）在 [`crate::primitives`]；本模块在其上拼装交互与样式。
//! 滚动视口契约（信号键 / `scroll:` focus）在 [`crate::core::scroll_viewport`]；本目录仅保留
//! [`ScrollArea`] / [`Scrollbar`] / RSX [`ScrollViewport`] 等产品层封装。
//! 函数名故意与 RSX 标签一致，故关闭 `non_snake_case`。
//!
//! ## 横切约定
//! - **身份**：勿给每个控件强行编造 `id`。展示型默认不需要；有状态非受控多实例才需要稳定实例键；
//!   焦点 / a11y / 浮层锚点用 `focus_id`（可带前缀）。详见 `docs/COMPONENT_LIBRARY.md` §4.2。
//! - 有状态控件若用全局 `use_signal("{id}__…")`，多实例必须稳定唯一键（空 id 撞键会串状态）。
//! - 受控状态用 [`crate::core::reactivity::Signal`]；未传则组件按实例键建内部信号。
//! - 浮层类（Select / Popover / Dialog / Dropdown / Tooltip / ContextMenu）走统一 floating sync 与 [`crate::app::portal_overlay`] Esc 栈。
//! - Tabs headless 条带自带 [`crate::app::reactive::reactive_view`]；面板仍可用 `<Show>` 或 `#[component(reactive)]`。
//! - 选择器占位按行为分文件：[`file_picker`] / [`color_picker`] / [`date_picker`]（勿再扩 `Input type`）。
#![allow(non_snake_case)]

pub mod base;
pub mod button;
pub mod card;
pub mod checkbox;
pub mod color_picker;
pub mod command;
pub mod combobox;
pub mod context_menu;
pub mod data_table;
pub mod date_picker;
pub mod dialog;
pub mod dropdown;
pub mod file_picker;
pub mod focus_trap;
pub mod form;
pub mod image;
pub mod input;
pub mod label;
pub mod menubar;
pub mod navbar;
pub mod navigator_host;
pub mod picker_stub;
pub mod popover;
pub mod portal;
pub mod progress;
pub mod radio_group;
pub mod resizable;
pub mod scroll_area;
pub mod scroll_viewport;
pub mod scrollbar;
pub mod segmented;
pub mod select;
pub mod separator;
pub mod sheet;
pub mod alert_dialog;
pub mod carousel;
pub mod chart;
pub mod sidebar;
pub mod slider;
pub mod stepper;
pub mod switch;
pub mod tabs;
pub mod textarea;
pub mod toast;
pub mod toggle;
pub mod toggle_group;
pub mod tooltip;
pub mod tree_view;
pub mod virtual_list;

pub use alert_dialog::{alert_dialog, AlertDialogBuilder};
pub use base::BaseComponentBuilder;
pub use button::{
    apply_bound_button_disabled, apply_bound_button_variant, button, Button, ButtonBuilder,
    ButtonProps,
};
pub use card::{Card, CardProps};
pub use carousel::{carousel, CarouselBuilder};
pub use chart::{bar_chart, line_chart, BarChartBuilder, LineChartBuilder};
pub use checkbox::{checkbox, Checkbox, CheckboxBuilder, CheckboxProps};
pub use color_picker::{color_picker, ColorPicker, ColorPickerBuilder, ColorPickerProps};
pub use command::{command, CommandAction, CommandBuilder};
pub use combobox::{combobox, ComboboxBuilder};
pub use context_menu::{context_menu, ContextMenuBuilder, ContextMenuItem};
pub use data_table::{data_table, DataColumn, DataTableBuilder};
pub use date_picker::{
    date_picker, time_picker, DatePicker, DatePickerBuilder, DatePickerKind, DatePickerProps,
    TimePicker, TimePickerBuilder, TimePickerProps,
};
pub use dialog::{dialog_panel, DialogBuilder};
pub use dropdown::{dropdown, DropdownBuilder};
pub use file_picker::{
    file_picker_paths, file_picker_paths_signal_key, DirectoryPicker, DirectoryPickerProps,
    FilePicker, FilePickerProps,
};
pub use focus_trap::{FocusTrap, FocusTrapProps, FOCUS_TRAP_OWNER_KEY, FOCUS_TRAP_RING_KEY};
pub use form::{
    clear_all_errors, clear_field_error, form_errors_signal, form_field, form_provider,
    form_validate, set_field_error, FormState,
};
pub use image::{Image, ImageProps};
pub use input::{
    apply_bound_input_disabled, apply_bound_input_invalid, filter_email_insert,
    filter_insert_for_type, filter_number_insert, filter_tel_insert, filter_url_insert, input,
    input_caret_signal_key, input_ime_anchor_signal_key, input_overwrite_signal_key,
    input_selection_signal_key, input_type_signal_key, Input, InputBuilder, InputProps, InputType,
    IME_ACTIVE_SIGNAL, INPUT_FOCUS_PREFIX,
};
pub use label::{
    enable_label_selection, label, label_selection_signal_key, selectable_text_key,
    text_node as text, try_copy_label_selection, Label, LabelBuilder, LabelProps,
    LABEL_SELECTION_COPY_SIGNAL,
};
pub use menubar::{menubar, MenuItem, MenuSection, MenubarBuilder};
pub use navbar::{navbar, NavbarBuilder};
pub use navigator_host::{navigation_host, NavigationHostBuilder};
pub use popover::{popover_panel, PopoverBuilder};
pub use portal::{Portal, PortalProps};
pub use progress::{progress, Progress, ProgressBuilder, ProgressProps};
pub use radio_group::{radio_group, RadioGroup, RadioGroupBuilder, RadioGroupProps};
pub use resizable::{resizable, ResizableBuilder};
pub use scroll_area::{scroll_area, scrollarea, ScrollArea, ScrollAreaBuilder, ScrollAreaProps};
pub use scroll_viewport::{
    scroll_viewport, scrollviewport, ScrollViewport, ScrollViewportBuilder, ScrollViewportProps,
    SCROLL_VIEWPORT_FOCUS_PREFIX,
};
pub use scrollbar::{
    scrollbar, scrollbarbuilder, Scrollbar, ScrollbarBuilder, ScrollbarOrientation, ScrollbarProps,
};
pub use segmented::{segmented, SegmentedBuilder};
pub use select::{select, Select, SelectBuilder, SelectProps};
pub use separator::{separator, Separator, SeparatorBuilder, SeparatorProps};
pub use sheet::{sheet, SheetBuilder, SheetSide};
pub use sidebar::{sidebar, SidebarBuilder};
pub use slider::{slider, Slider, SliderBuilder, SliderProps};
pub use stepper::{stepper, StepperBuilder};
pub use switch::{switch, Switch, SwitchBuilder, SwitchProps};
pub use tabs::{
    tabs_headless, tabs_headless_root, Tabs, TabsHeadlessBuilder, TabsProps, TABSTRIP_FOCUS_PREFIX,
};
pub use textarea::{textarea, Textarea, TextareaBuilder, TextareaProps, TEXTAREA_FOCUS_PREFIX};
pub use toast::{toast, toast_dismiss, toast_host, toast_push, toast_queue, ToastItem};
pub use toggle::{toggle, Toggle, ToggleBuilder, ToggleProps};
pub use toggle_group::{toggle_group, ToggleGroup, ToggleGroupBuilder, ToggleGroupProps};
pub use tooltip::{tooltip, TooltipBuilder};
pub use tree_view::{tree_view, TreeNode, TreeViewBuilder};
pub use virtual_list::{virtual_list, VirtualListBuilder};

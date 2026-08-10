//! 默认应用入口：`use rui::prelude::*;`
//!
//! # 双书写面（ADR-9）
//!
//! 本模块**同时** re-export：
//! - RSX / 标签入口（PascalCase：`Button`、`Input`、`Dialog`…）
//! - 等价 Builder / `fn`（`button`、`input`、`dialog`…）
//!
//! 禁止把 prelude 做成「只含标签」或「只含 Builder」。低层浮层几何
//! （[`crate::utils::popup`]）与私有壳（[`crate::components::internal`]）**不**进 prelude。
//!
//! # 不进本模块
//!
//! | 排除 | 原因 |
//! |------|------|
//! | `utils::popup::*`、`mark_floating` / placement | 低层能力；按需 `use rui::utils::popup` |
//! | `popover_panel` | 低层面板；可用 `components::popover` |
//! | `overlay` / `path` | 原语能力层，非默认产品面 |
//! | `BaseComponentBuilder`、`apply_bound_*`、信号键 helpers | 进阶 / 内部契约 |
//! | `planned` 组件（Badge…） | 尚未 `pub use` |
//!
//! # 示例
//!
//! ```ignore
//! use rui::prelude::*;
//!
//! // A · 标签
//! let _a = ui! { <Button variant="outline">"保存"</Button> };
//!
//! // B · Builder
//! let _b: Element = button("save").variant("outline").into();
//!
//! // 互嵌：标签树里嵌入 Builder
//! let _c = ui! {
//!     <VStack gap="0.5rem">
//!         { Element::from(button("x").variant("ghost").text("内嵌 Builder")) }
//!         <Button>"标签子节点"</Button>
//!     </VStack>
//! };
//!
//! // 互嵌：Builder 树里嵌入 ui!
//! let _d: Element = vstack()
//!     .gap("0.5rem")
//!     .child(ui! { <Label>"内嵌标签"</Label> })
//!     .into();
//! ```

#![allow(unused_imports)]

// ---------------------------------------------------------------------------
// 宏与书写面糖
// ---------------------------------------------------------------------------

pub use crate::{
    bind_attr, bind_style, bind_text, component, for_keyed, reactive, reactive_anchor_id, ui,
};

// ---------------------------------------------------------------------------
// 核心类型（双书写面共用）
// ---------------------------------------------------------------------------

pub use crate::core::reactivity::{create_signal, use_signal, Signal};
pub use crate::core::tree::Element;

// ---------------------------------------------------------------------------
// 布局 / 叶子原语（Builder 面常用）
// ---------------------------------------------------------------------------

pub use crate::primitives::{
    gstack, hstack, icon, interactive, r#box, text, vstack, BoxBuilder, InteractiveBuilder,
    Portal, PortalProps, TextBuilder,
};

// ---------------------------------------------------------------------------
// 主题（稳定面草案含 Theme / set_theme_mode）
// ---------------------------------------------------------------------------

pub use crate::style::{
    get_theme, is_dark_theme, set_theme_mode, theme_mode, toggle_theme_mode, FocusRingStyle,
    Theme, ThemeMode,
};

// ---------------------------------------------------------------------------
// 产品组件：标签 + Builder/`fn`（低层 popup 除外）
// ---------------------------------------------------------------------------

pub use crate::components::{
    // AlertDialog
    alert_dialog,
    AlertDialogBuilder,
    // Button
    button,
    Button,
    ButtonBuilder,
    ButtonProps,
    // Card
    Card,
    CardProps,
    // Carousel
    carousel,
    CarouselBuilder,
    // Chart
    bar_chart,
    line_chart,
    BarChartBuilder,
    LineChartBuilder,
    // Checkbox
    checkbox,
    Checkbox,
    CheckboxBuilder,
    CheckboxProps,
    // ColorPicker
    color_picker,
    ColorPicker,
    ColorPickerBuilder,
    ColorPickerProps,
    // Combobox
    combobox,
    ComboboxBuilder,
    // Command
    command,
    CommandAction,
    CommandBuilder,
    // ContextMenu
    context_menu,
    ContextMenuBuilder,
    ContextMenuItem,
    // Date / Time
    date_picker,
    time_picker,
    DatePicker,
    DatePickerBuilder,
    DatePickerKind,
    DatePickerProps,
    TimePicker,
    TimePickerBuilder,
    TimePickerProps,
    // Dialog（复合标签 + Builder）
    dialog,
    dialog_close,
    dialog_content,
    dialog_open,
    dialog_scope,
    use_dialog,
    DialogBody,
    DialogBodyProps,
    DialogBuilder,
    DialogContentOpts,
    DialogCtx,
    DialogDescription,
    DialogDescriptionProps,
    DialogFooter,
    DialogFooterProps,
    DialogHeader,
    DialogHeaderProps,
    DialogTitle,
    DialogTitleProps,
    DialogTrigger,
    DialogTriggerProps,
    // Dropdown（暂仅 Builder）
    dropdown,
    DropdownBuilder,
    // Form
    clear_all_errors,
    clear_field_error,
    form_errors_signal,
    form_field,
    form_provider,
    form_validate,
    set_field_error,
    FormState,
    // Image
    Image,
    ImageProps,
    // Input
    input,
    Input,
    InputBuilder,
    InputProps,
    InputType,
    // Label（不 re-export `text` 别名，避免与 primitives::text 撞名）
    enable_label_selection,
    label,
    try_copy_label_selection,
    Label,
    LabelBuilder,
    LabelProps,
    // Menubar
    menubar,
    MenuItem,
    MenuSection,
    MenubarBuilder,
    // Navbar / NavigationMenu / Sidebar
    navbar,
    NavbarBuilder,
    navigation_menu,
    NavigationMenuBuilder,
    NavigationMenuItem,
    NavigationMenuLink,
    sidebar,
    SidebarBuilder,
    // Progress
    progress,
    Progress,
    ProgressBuilder,
    ProgressProps,
    // Radio / Slider / Switch / Toggle*
    radio_group,
    RadioGroup,
    RadioGroupBuilder,
    RadioGroupProps,
    slider,
    Slider,
    SliderBuilder,
    SliderProps,
    switch,
    Switch,
    SwitchBuilder,
    SwitchProps,
    toggle,
    Toggle,
    ToggleBuilder,
    ToggleProps,
    toggle_group,
    ToggleGroup,
    ToggleGroupBuilder,
    ToggleGroupProps,
    // Resizable
    resizable,
    ResizableBuilder,
    // Scroll*
    scroll_area,
    scrollarea,
    ScrollArea,
    ScrollAreaBuilder,
    ScrollAreaProps,
    scroll_viewport,
    scrollviewport,
    ScrollViewport,
    ScrollViewportBuilder,
    ScrollViewportProps,
    scrollbar,
    Scrollbar,
    ScrollbarBuilder,
    ScrollbarOrientation,
    ScrollbarProps,
    // Select
    select,
    select_content,
    select_scope,
    use_select,
    Select,
    SelectBuilder,
    SelectContentOpts,
    SelectCtx,
    SelectGroup,
    SelectGroupData,
    SelectGroupProps,
    SelectItem,
    SelectItemAlign,
    SelectItemData,
    SelectItemProps,
    SelectLabel,
    SelectLabelProps,
    SelectProps,
    SelectSeparator,
    SelectSeparatorProps,
    SelectTrigger,
    SelectTriggerProps,
    SelectValue,
    SelectValueProps,
    // Separator
    separator,
    Separator,
    SeparatorBuilder,
    SeparatorProps,
    // Sheet
    sheet,
    SheetBuilder,
    SheetSide,
    // Stepper
    stepper,
    StepperBuilder,
    // Table / Tree / VirtualList
    table,
    TableBuilder,
    TableColumn,
    TableSortDir,
    TableSortState,
    tree_view,
    TreeNode,
    TreeViewBuilder,
    virtual_list,
    VirtualListBuilder,
    // Tabs
    tabs_headless,
    tabs_headless_root,
    tabs_list,
    Tabs,
    TabsHeadlessBuilder,
    TabsList,
    TabsListBuilder,
    TabsListProps,
    TabsProps,
    TABSTRIP_FOCUS_PREFIX,
    // Textarea
    textarea,
    Textarea,
    TextareaBuilder,
    TextareaProps,
    // Toast（暂仅函数面）
    toast,
    toast_dismiss,
    toast_host,
    toast_push,
    toast_queue,
    ToastItem,
    // Tooltip
    tooltip,
    TooltipBuilder,
};

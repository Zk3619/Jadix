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
//! | `overlay` | 原语能力层，非默认产品面 |
//! | `path` 核（`PathCommand` / tessellate） | 原语能力层；公开面是 [`svg`] / Chart |
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
    bind_attr, bind_style, bind_text, component, for_keyed, merge_props, reactive,
    reactive_anchor_id, split_props, ui,
};

// ---------------------------------------------------------------------------
// 核心类型（双书写面共用）
// ---------------------------------------------------------------------------

pub use crate::app::pointer_events::{MouseEvent, PointerEvent};
pub use crate::app::props::{children, resolve_children, Children, ChildrenAccessor, MergeProps};
pub use crate::app::reactive::{for_keyed_reactive, index_reactive};
pub use crate::core::reactivity::{
    component_root, create_signal, isolate_reads, untrack, use_signal, with_child_owner, Signal,
};
pub use crate::core::tree::{Element, LayoutStatus};

// ---------------------------------------------------------------------------
// 布局 / 叶子原语（Builder 面常用）
// ---------------------------------------------------------------------------

pub use crate::primitives::{
    canvas, circle, effect_wrap, gstack, hstack, icon, interactive, r#box, scene3d, svg, svg_path,
    svg_rect, text, vstack, BoxBuilder, Canvas2D, CanvasBuilder, CanvasSize, CircleBuilder,
    EffectWrapBuilder, InteractiveBuilder, PathBuilder, Portal, PortalProps, Scene3dBuilder,
    Scene3dCamera, Scene3dLight, Scene3dMaterial, SvgBuilder, TextBuilder,
};

// ---------------------------------------------------------------------------
// 主题（稳定面草案含 Theme / set_theme_mode）
// ---------------------------------------------------------------------------

pub use crate::style::{
    get_theme, is_dark_theme, set_theme_mode, theme_mode, toggle_theme_mode, FocusRingStyle, Theme,
    ThemeMode,
};

// ---------------------------------------------------------------------------
// 产品组件：标签 + Builder/`fn`（低层 popup 除外）
// ---------------------------------------------------------------------------

pub use crate::components::{
    // Chart
    area_chart,
    area_y,
    bar_chart,
    bar_x,
    bar_y,
    // Button
    button,
    calendar,
    // Carousel
    carousel,
    carousel_multi,
    carousel_scope,
    carousel_single,
    carousel_stack,
    // Checkbox
    checkbox,
    // Form
    clear_all_errors,
    clear_field_error,
    // ColorPicker
    color_picker,
    // Combobox
    combobox,
    combobox_content,
    combobox_scope,
    // Command
    command,
    command_dialog_scope,
    command_overlay_scope,
    command_scope,
    // ContextMenu
    context_menu,
    context_menu_content,
    context_menu_radio_group,
    context_menu_scope,
    context_menu_sub,
    context_menu_sub_content,
    custom_mark,
    // Date / Time
    date_picker,
    define_chart,
    // Dialog（复合标签 + Builder）
    dialog,
    dialog_close,
    dialog_content,
    dialog_open,
    dialog_scope,
    dot,
    // DropdownMenu（复合标签 + Builder）
    dropdown,
    dropdown_menu,
    dropdown_menu_content,
    dropdown_menu_radio_group,
    dropdown_menu_scope,
    dropdown_menu_sub,
    dropdown_menu_sub_content,
    // Label（不 re-export `text` 别名，避免与 primitives::text 撞名）
    enable_label_selection,
    field_error,
    field_invalid,
    form_errors_signal,
    form_field,
    form_provider,
    form_validate,
    // Input
    input,
    label,
    line_chart,
    line_y,
    // Menubar
    menubar,
    menubar_content,
    menubar_menu,
    menubar_radio_group,
    menubar_scope,
    menubar_sub,
    menubar_sub_content,
    use_menubar,
    // Navbar / NavigationMenu / Sidebar
    navbar,
    navigation_menu,
    pie,
    pie_chart,
    // Progress
    progress,
    radial_area,
    radial_bar,
    radial_dot,
    radial_line,
    // Radio / Slider / Switch / Toggle*
    radio_group,
    radio_group_scope,
    // Resizable
    resizable,
    rule_x,
    rule_x_num,
    rule_y,
    scale_log,
    scale_time,
    // Scroll*
    scroll_area,
    scroll_viewport,
    scrollarea,
    scrollbar,
    scrollviewport,
    // Select
    select,
    select_content,
    select_scope,
    // Separator
    separator,
    set_field_error,
    // Sheet
    sheet,
    sidebar,
    slider,
    // Stepper
    stepper,
    switch,
    // Table / Tree / VirtualList
    data_table,
    table,
    table_comfortable,
    table_compact,
    // Tabs
    tabs_content,
    tabs_headless,
    tabs_headless_root,
    tabs_list,
    tabs_scope,
    // Textarea
    textarea,
    time_picker,
    // Toast / Toaster（默认 stack 堆叠；list 为逐条展开）
    toast,
    toast_dismiss,
    toast_host,
    toast_pending,
    toast_push,
    toast_queue,
    toast_set_layout,
    toast_set_limit,
    toast_set_overflow,
    toast_set_position,
    toaster,
    toggle,
    toggle_group,
    // Tooltip
    tooltip,
    tooltip_bubble,
    tooltip_content,
    tooltip_scope,
    tooltip_side_to_placement,
    tree_view,
    try_copy_label_selection,
    use_carousel,
    use_combobox,
    use_context_menu,
    use_dialog,
    use_dropdown_menu,
    use_form,
    use_radio_group,
    use_select,
    use_tabs,
    use_tooltip,
    virtual_list,
    virtual_masonry,
    AreaChart,
    AreaChartProps,
    BarChart,
    BarChartBuilder,
    BarChartProps,
    Button,
    ButtonBuilder,
    ButtonProps,
    Calendar,
    CalendarBuilder,
    CalendarCaption,
    CalendarProps,
    // Card（复合分区）
    Card,
    CardAction,
    CardActionProps,
    CardContent,
    CardContentProps,
    CardDescription,
    CardDescriptionProps,
    CardFooter,
    CardFooterProps,
    CardHeader,
    CardHeaderProps,
    CardProps,
    CardTitle,
    CardTitleProps,
    CarouselBuilder,
    CarouselContent,
    CarouselContentProps,
    CarouselControls,
    CarouselCtx,
    CarouselDots,
    CarouselDotsProps,
    CarouselEndAlign,
    CarouselItem,
    CarouselItemProps,
    CarouselKind,
    CarouselVariant,
    CarouselNext,
    CarouselNextProps,
    CarouselOrientation,
    CarouselPageSnap,
    CarouselPrevious,
    CarouselPreviousProps,
    CarouselScrollBy,
    Chart,
    ChartBuilder,
    ChartDatum,
    ChartDefinition,
    ChartMark,
    ChartPaint,
    ChartProps,
    ChartSelect,
    ChartSelected,
    ChartTheme,
    Checkbox,
    CheckboxBuilder,
    CheckboxProps,
    ColorFormat,
    ColorPicker,
    ColorPickerBuilder,
    ColorPickerProps,
    ColorPickerVariant,
    Combobox,
    ComboboxBuilder,
    ComboboxContentOpts,
    ComboboxCtx,
    ComboboxEmpty,
    ComboboxEmptyProps,
    ComboboxGroup,
    ComboboxGroupData,
    ComboboxGroupProps,
    ComboboxInput,
    ComboboxInputProps,
    ComboboxItem,
    ComboboxItemData,
    ComboboxItemProps,
    ComboboxLabel,
    ComboboxLabelProps,
    ComboboxList,
    ComboboxListProps,
    ComboboxProps,
    ComboboxSeparator,
    ComboboxSeparatorProps,
    ComboboxVisibleItem,
    Command,
    CommandAction,
    CommandBuilder,
    CommandCtx,
    CommandEmpty,
    CommandGroup,
    CommandGroupData,
    CommandInput,
    CommandItem,
    CommandList,
    CommandProps,
    CommandSeparator,
    CommandShortcut,
    CommandSurface,
    CommandVisibleItem,
    ContextMenuAction,
    ContextMenuBuilder,
    ContextMenuCheckboxItem,
    ContextMenuContentOpts,
    ContextMenuCtx,
    ContextMenuGroup,
    ContextMenuItem,
    ContextMenuLabel,
    ContextMenuRadioGroupCtx,
    ContextMenuRadioItem,
    ContextMenuSeparator,
    ContextMenuShortcut,
    ContextMenuSubCtx,
    ContextMenuSubTrigger,
    ContextMenuTrigger,
    DatePicker,
    DatePickerBuilder,
    DatePickerKind,
    DatePickerProps,
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
    DropdownBuilder,
    DropdownMenuBuilder,
    DropdownMenuCheckboxItem,
    DropdownMenuContentOpts,
    DropdownMenuCtx,
    DropdownMenuGroup,
    DropdownMenuItem,
    DropdownMenuLabel,
    DropdownMenuRadioGroupCtx,
    DropdownMenuRadioItem,
    DropdownMenuSeparator,
    DropdownMenuShortcut,
    DropdownMenuSubCtx,
    DropdownMenuSubTrigger,
    DropdownMenuTrigger,
    FormState,
    // Image
    Image,
    ImageProps,
    Img,
    ImgProps,
    Input,
    InputBuilder,
    InputProps,
    InputType,
    Label,
    LabelBuilder,
    LabelProps,
    LineChart,
    LineChartBuilder,
    LineChartProps,
    MarkRenderContext,
    MarkStyle,
    MenuItem,
    MenuSection,
    MenubarBuilder,
    MenubarCheckboxItem,
    MenubarContentOpts,
    MenubarCtx,
    MenubarGroup,
    MenubarItem,
    MenubarLabel,
    MenubarRadioItem,
    MenubarSeparator,
    MenubarShortcut,
    MenubarSubTrigger,
    MenubarTrigger,
    NavbarBuilder,
    NavigationMenuBuilder,
    NavigationMenuItem,
    NavigationMenuLink,
    PieChart,
    PieChartBuilder,
    PieChartProps,
    Progress,
    ProgressBuilder,
    ProgressProps,
    RadioGroup,
    RadioGroupBuilder,
    RadioGroupCtx,
    RadioGroupItem,
    RadioGroupItemLabel,
    RadioGroupItemLabelProps,
    RadioGroupItemProps,
    RadioGroupProps,
    ResizableBuilder,
    SceneLayer,
    ScrollArea,
    ScrollAreaBuilder,
    ScrollAreaProps,
    ScrollViewport,
    ScrollViewportBuilder,
    ScrollViewportProps,
    Scrollbar,
    ScrollbarBuilder,
    ScrollbarOrientation,
    ScrollbarProps,
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
    SelectScrollSelected,
    SelectSeparator,
    SelectSeparatorProps,
    SelectTrigger,
    SelectTriggerProps,
    SelectValue,
    SelectValueProps,
    Separator,
    SeparatorBuilder,
    SeparatorProps,
    SheetBuilder,
    SheetSide,
    SidebarBuilder,
    Slider,
    SliderBuilder,
    SliderProps,
    StepperBuilder,
    Switch,
    SwitchBuilder,
    SwitchProps,
    DataTable,
    DataTableProps,
    Table,
    TableAlign,
    TableBody,
    TableBodyProps,
    TableBuilder,
    TableCaption,
    TableCaptionProps,
    TableCell,
    TableCellProps,
    TableColumn,
    TableDensity,
    TableFooter,
    TableFooterProps,
    TableHead,
    TableHeader,
    TableHeaderProps,
    TableHeadProps,
    TablePaginationState,
    TableProps,
    TableRow,
    TableRowProps,
    TableSortDir,
    TableSortState,
    Tabs,
    TabsContent,
    TabsContentProps,
    TabsCtx,
    TabsHeadlessBuilder,
    TabsList,
    TabsListBuilder,
    TabsListProps,
    TabsOrientation,
    TabsProps,
    TabsTrigger,
    TabsTriggerProps,
    Textarea,
    TextareaBuilder,
    TextareaProps,
    TimePicker,
    TimePickerBuilder,
    TimePickerProps,
    Toast,
    ToastBuilder,
    ToastClose,
    ToastCloseProps,
    ToastDescription,
    ToastDescriptionProps,
    ToastItem,
    ToastLayout,
    ToastOverflow,
    ToastPosition,
    ToastProps,
    ToastTitle,
    ToastTitleProps,
    ToastTone,
    Toaster,
    ToasterBuilder,
    ToasterProps,
    Toggle,
    ToggleBuilder,
    ToggleGroup,
    ToggleGroupBuilder,
    ToggleGroupProps,
    ToggleProps,
    TooltipBuilder,
    TooltipContentOpts,
    TooltipCtx,
    TooltipTrigger,
    TooltipTriggerProps,
    TreeNode,
    TreeViewBuilder,
    VirtualListBuilder,
    VirtualMasonryBuilder,
    TABSTRIP_FOCUS_PREFIX,
};

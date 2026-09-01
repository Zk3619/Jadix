//! # UI 组件层
//!
//! 面向 `ui!` **与** 纯 Rust Builder 的可组合控件：按钮、输入、滚动区、浮层等。
//! 多数以 `#[component]` 暴露 PascalCase 标签（如 `<Button>`），并导出等价的
//! `fn` / `*Builder`（如 [`button`]、[`ButtonBuilder`]）；少数暂仅函数 / Builder
//! （如 [`toast`]）。Dialog / DropdownMenu 为复合标签 + Builder。两种写法互嵌兼容，见
//! `docs/COMPONENT_LIBRARY.md` §3（ADR-9）。
//!
//! 布局原语（`Box` / `HStack` / `VStack` / `GStack` / [`crate::primitives::Portal`]）在
//! [`crate::primitives`]；本模块在其上拼装**产品组件**（Button、Input、Dialog…）。
//! 私有 chrome 在 [`internal`]（Input/Textarea → `text_edit`；弹出菜单共享 → `menu`）；
//! Dialog 模态壳在本目录 [`dialog`] 内，不进 prelude / 产品清单；后续分 crate 时随本目录迁到组件库。
//! 滚动视口契约（信号键 / `scroll:` focus）在 [`crate::core::scroll_viewport`]；本目录仅保留
//! [`ScrollArea`] / [`Scrollbar`] / RSX [`ScrollViewport`] 等产品层封装。
//! 函数名故意与 RSX 标签一致，故关闭 `non_snake_case`。
//!
//! ## Cross-cutting convention
//! - **身份**：勿给每个控件强行编造 `id`。展示型默认不需要；有状态非受控多实例才需要稳定实例键；
//!   焦点 / a11y / 浮层锚点用 `focus_id`（可带前缀）。详见 `docs/COMPONENT_LIBRARY.md` §4.2。
//! - 有状态控件若用全局 `use_signal("{id}__…")`，多实例必须稳定唯一键（空 id 撞键会串状态）。
//! - 受控状态用 [`crate::core::reactivity::Signal`]；未传则组件按实例键建内部信号。
//! - Floating layer classes (Select / Popover / Dialog / Dropdown / Tooltip / HoverCard / ContextMenu) use floating sync and [`crate::app::portal_overlay`] Esc stack to unify.
//! - HoverCard: `<HoverCard>` / Trigger / Content (macro `hover_card_scope` / `hover_card_content`); Builder [`hover_card`].
//! - ContextMenu: `<ContextMenu>` / Trigger / Content / Item / CheckboxItem / RadioGroup / RadioItem / Label / Separator / Shortcut / Group (macro `context_menu_scope`); Builder [`context_menu`].
//! - DropdownMenu: `<DropdownMenu>` / Trigger / Content / Item / CheckboxItem / RadioGroup / RadioItem / Label / Separator / Shortcut / Group / Sub (macro `dropdown_menu_scope`); Builder [`dropdown_menu`].
//! - Menubar: `<Menubar>` / Menu / Trigger / Content / Item… (macro `menubar_scope` / `menubar_menu`; content reuse DropdownMenu); Builder [`menubar`].
//! - Tabs: Composite `<Tabs>` / `TabsList` / `TabsTrigger` / `TabsContent` (macro `tabs_scope`); headless bands are still available [`tabs_headless`] + `<Show>`.
//! - Avatar: `<Avatar>` / `<AvatarImage>` / `<AvatarFallback>` / `<AvatarBadge>` / `<AvatarGroup>` (macro → [`avatar_scope`]); Builder [`avatar`].
//! - Badge：展示型 `<Badge variant>` / [`badge`]（default/secondary/destructive/outline/ghost/link）。
//! - Breadcrumb：`<Breadcrumb>` / `BreadcrumbList` / `BreadcrumbItem` / `BreadcrumbLink` / `BreadcrumbPage` / `BreadcrumbSeparator` / `BreadcrumbEllipsis`；Builder [`breadcrumb`]。
//! - ButtonGroup：`<ButtonGroup>` / `ButtonGroupText` / `ButtonGroupSeparator`；Builder [`button_group`]。
//! - InputAdornment: `<InputAdornment>` / Addon / Text / Button / Input / Textarea (opposite ShadCN InputGroup); Builder [`input_adornment`].
//! - InputOTP: `<InputOTP>` / Group / Slot / Separator (对位 ShadCN InputOTP); Builder [`input_otp`].
//! - AspectRatio：展示型 `<AspectRatio ratio>` / [`aspect_ratio`]（CSS `aspect-ratio` 容器；Image 等亦可直接写 `aspect_ratio`/`aspect-ratio` 属性）。
//! - Kbd: presentational `<Kbd>` / `<KbdGroup>` / [`kbd`] / [`kbd_group`] (keycap chrome; no special `ui!` scope).
//! - Collapsible: Composite `<Collapsible>` / `CollapsibleTrigger` / `CollapsibleContent` (macro `collapsible_scope` / `collapsible_content`); Builder [`collapsible`].
//! - Carousel: `<Carousel variant>` / `<CarouselSingle|Multi|Stack>` + Content/Item/Prev/Next/Dots (macro `carousel_scope`); Builder [`.slide`](carousel::CarouselBuilder::slide).
//! - 颜色 / 日期：[`calendar`] / [`date_picker`] / [`time_picker`] / [`color_picker`]（触发器样式各自维护）。原生文件对话框在 [`crate::utils::file_dialog`]；Tab 焦点陷阱在 [`crate::utils::focus_trap`]（能力，非组件）。
//! - 导航分工：[`navbar`] 布局壳；[`navigation_menu`] 链接导览（ShadCN）；[`menubar`] 应用命令；
//! [`pagination`] Data paging navigation (ShadCN). The page stack host is [`crate::app::navigator::navigation_host`].
#![allow(non_snake_case)]

/// 组件库私有实现（不进产品面）。
pub(crate) mod internal;

pub mod accordion;
pub mod aspect_ratio;
pub mod avatar;
pub mod badge;
pub mod base;
pub mod breadcrumb;
pub mod bubble;
pub mod button;
pub mod button_group;
pub mod calendar;
pub mod card;
pub mod carousel;
pub mod chart;
pub mod checkbox;
pub mod collapsible;
pub mod color_picker;
pub mod combobox;
pub mod command;
pub mod context_menu;
pub mod date_picker;
pub mod dialog;
pub mod drawer;
pub mod dropdown_menu;
pub mod form;
pub mod hover_card;
pub mod image;
pub mod input;
pub mod input_adornment;
pub mod input_otp;
pub mod kbd;
pub mod label;
pub mod menubar;
pub mod navbar;
pub mod navigation_menu;
pub mod pagination;
pub mod popover;
pub mod progress;
pub mod radio_group;
pub mod resizable;
pub mod scroll_area;
pub mod scroll_viewport;
pub mod scrollbar;
pub mod select;
pub mod separator;
pub mod sheet;
pub mod sidebar;
pub mod slider;
pub mod stepper;
pub mod switch;
pub mod table;
pub mod tabs;
pub mod textarea;
pub mod toast;
pub mod toggle;
pub mod toggle_group;
pub mod tooltip;
pub mod tree_view;
pub mod virtual_list;
pub mod virtual_masonry;

pub use accordion::{
    accordion, accordion_content, accordion_item_scope, accordion_scope, use_accordion,
    use_accordion_item, Accordion, AccordionBuilder, AccordionContent, AccordionContentProps,
    AccordionCtx, AccordionItem, AccordionItemCtx, AccordionItemProps, AccordionProps,
    AccordionTrigger, AccordionTriggerProps,
};
pub use aspect_ratio::{
    aspect_ratio, AspectRatio, AspectRatioBuilder, AspectRatioProps, AspectRatioValue,
};
pub use avatar::{
    apply_bound_avatar_badge_variant, apply_bound_avatar_shape, apply_bound_avatar_size, avatar,
    avatar_fallback, avatar_scope, use_avatar, Avatar, AvatarBadge, AvatarBadgeProps,
    AvatarBuilder, AvatarCtx, AvatarFallback, AvatarFallbackBuilder, AvatarFallbackProps,
    AvatarGroup, AvatarGroupCount, AvatarGroupCountProps, AvatarGroupProps, AvatarImage,
    AvatarImageProps, AvatarProps, AvatarShape, AvatarSize,
};
pub use badge::{apply_bound_badge_variant, badge, Badge, BadgeBuilder, BadgeProps, BadgeVariant};
pub use base::BaseComponentBuilder;
pub use breadcrumb::{
    breadcrumb, Breadcrumb, BreadcrumbBuilder, BreadcrumbEllipsis, BreadcrumbEllipsisProps,
    BreadcrumbItem, BreadcrumbItemProps, BreadcrumbLink, BreadcrumbLinkProps, BreadcrumbList,
    BreadcrumbListProps, BreadcrumbPage, BreadcrumbPageProps, BreadcrumbProps, BreadcrumbSeparator,
    BreadcrumbSeparatorProps,
};
pub use bubble::{
    apply_bound_bubble_align, apply_bound_bubble_variant, apply_bubble_variant_paint, bubble,
    bubble_content, bubble_group, bubble_reactions, bubble_scope, use_bubble, Bubble, BubbleAlign,
    BubbleBuilder, BubbleContent, BubbleContentBuilder, BubbleContentProps, BubbleCtx, BubbleGroup,
    BubbleGroupBuilder, BubbleGroupProps, BubbleProps, BubbleReactions, BubbleReactionsAlign,
    BubbleReactionsBuilder, BubbleReactionsLayout, BubbleReactionsProps, BubbleReactionsSide,
    BubbleVariant,
};
pub use button::{
    apply_bound_button_disabled, apply_bound_button_variant, button, Button, ButtonBuilder,
    ButtonProps,
};
pub use button_group::{
    button_group, ButtonGroup, ButtonGroupBuilder, ButtonGroupOrientation, ButtonGroupProps,
    ButtonGroupSeparator, ButtonGroupSeparatorProps, ButtonGroupText, ButtonGroupTextProps,
};
pub use calendar::{
    calendar, days_in_month, format_long_date, format_month_year, format_ymd, parse_ymd, today_ymd,
    weekday, Calendar, CalendarBuilder, CalendarCaption, CalendarProps,
};
pub use card::{
    Card, CardAction, CardActionProps, CardContent, CardContentProps, CardDescription,
    CardDescriptionProps, CardFooter, CardFooterProps, CardHeader, CardHeaderProps, CardProps,
    CardTitle, CardTitleProps,
};
pub use carousel::builder::{
    carousel, carousel_multi, carousel_single, carousel_stack, CarouselBuilder,
};
pub use carousel::content::{
    CarouselContent, CarouselContentProps, CarouselDots, CarouselDotsProps, CarouselItem,
    CarouselItemProps, CarouselNext, CarouselNextProps, CarouselPrevious, CarouselPreviousProps,
};
pub use carousel::{
    carousel_scope, use_carousel, CarouselControls, CarouselCtx, CarouselEndAlign,
    CarouselItemSizing, CarouselKind, CarouselOrientation, CarouselPageSnap, CarouselScrollBy,
    CarouselVariant,
};
pub use chart::{
    area_chart, area_y, bar_chart, bar_x, bar_y, custom_mark, define_chart, donut_chart, dot,
    format_time_label, line_chart, line_y, pie, pie_chart, pie_layout, pie_layout_span,
    radial_area, radial_bar, radial_chart, radial_dot, radial_line, rule_x, rule_x_num, rule_y,
    scale_band, scale_linear, scale_log, scale_point, scale_time, unix_ms_to_utc, AreaChart,
    AreaChartProps, BarChart, BarChartBuilder, BarChartProps, Chart, ChartBuilder, ChartConfig,
    ChartDatum, ChartDefinition, ChartMark, ChartPaint, ChartProps, ChartSelect, ChartSelected,
    ChartSeriesConfig, ChartTheme, ChartTooltipItem, CustomMarkBuilder, DonutChart,
    DonutChartProps, LineChart, LineChartBuilder, LineChartProps, MarkChannels, MarkRenderContext,
    MarkStyle, PieChart, PieChartBuilder, PieChartProps, RadialChart, RadialChartBuilder,
    RadialChartProps, SceneLayer, TooltipIndicator,
};
pub use checkbox::{checkbox, Checkbox, CheckboxBuilder, CheckboxProps};
pub use collapsible::{
    collapsible, collapsible_content, collapsible_scope, use_collapsible, Collapsible,
    CollapsibleBuilder, CollapsibleContent, CollapsibleContentProps, CollapsibleCtx,
    CollapsibleProps, CollapsibleTrigger, CollapsibleTriggerProps,
};
pub use color_picker::{
    color_picker, format_color, parse_color_str, ColorFormat, ColorPicker, ColorPickerBuilder,
    ColorPickerProps, ColorPickerVariant, Rgba,
};
pub use combobox::{
    combobox, combobox_content, combobox_scope, use_combobox, Combobox, ComboboxBuilder,
    ComboboxContentOpts, ComboboxCtx, ComboboxEmpty, ComboboxEmptyProps, ComboboxGroup,
    ComboboxGroupData, ComboboxGroupProps, ComboboxInput, ComboboxInputProps, ComboboxItem,
    ComboboxItemData, ComboboxItemProps, ComboboxLabel, ComboboxLabelProps, ComboboxList,
    ComboboxListProps, ComboboxProps, ComboboxSeparator, ComboboxSeparatorProps,
    ComboboxVisibleItem,
};
pub use command::{
    command, command_dialog_scope, command_overlay_scope, command_scope, use_command, Command,
    CommandAction, CommandBuilder, CommandCtx, CommandEmpty, CommandEmptyProps, CommandGroup,
    CommandGroupData, CommandGroupProps, CommandInput, CommandInputProps, CommandItem,
    CommandItemProps, CommandList, CommandListProps, CommandProps, CommandSeparator,
    CommandSeparatorProps, CommandShortcut, CommandShortcutProps, CommandSurface,
    CommandVisibleItem,
};
pub use context_menu::{
    context_menu, context_menu_content, context_menu_radio_group, context_menu_scope,
    context_menu_sub, context_menu_sub_content, use_context_menu, ContextMenuAction,
    ContextMenuBuilder, ContextMenuCheckboxItem, ContextMenuCheckboxItemProps,
    ContextMenuContentOpts, ContextMenuCtx, ContextMenuGroup, ContextMenuGroupProps,
    ContextMenuItem, ContextMenuItemProps, ContextMenuLabel, ContextMenuLabelProps,
    ContextMenuRadioGroupCtx, ContextMenuRadioItem, ContextMenuRadioItemProps,
    ContextMenuSeparator, ContextMenuSeparatorProps, ContextMenuShortcut, ContextMenuShortcutProps,
    ContextMenuSubCtx, ContextMenuSubTrigger, ContextMenuSubTriggerProps, ContextMenuTrigger,
    ContextMenuTriggerProps,
};
pub use date_picker::{
    date_picker, time_picker, DatePicker, DatePickerBuilder, DatePickerKind, DatePickerProps,
    TimePicker, TimePickerBuilder, TimePickerProps,
};
pub use dialog::{
    dialog, dialog_close, dialog_content, dialog_open, dialog_scope, use_dialog, DialogBody,
    DialogBodyProps, DialogBuilder, DialogContentOpts, DialogCtx, DialogDescription,
    DialogDescriptionProps, DialogFooter, DialogFooterProps, DialogHeader, DialogHeaderProps,
    DialogTitle, DialogTitleProps, DialogTrigger, DialogTriggerProps,
};
pub use drawer::{
    coerce_snap_point_list, coerce_snap_points, drawer, drawer_active_snap_point, drawer_close,
    drawer_content, drawer_open, drawer_scope, drawer_set_active_snap_point, drawer_snap_to_next,
    drawer_snap_to_prev, parse_snap_point_token, parse_snap_points, use_drawer, DrawerBody,
    DrawerBodyProps, DrawerBuilder, DrawerClose, DrawerCloseProps, DrawerContentOpts, DrawerCtx,
    DrawerDescription, DrawerDescriptionProps, DrawerDirection, DrawerFooter, DrawerFooterProps,
    DrawerHeader, DrawerHeaderProps, DrawerSnapPoint, DrawerSwipeHandle, DrawerSwipeHandleProps,
    DrawerTitle, DrawerTitleProps, DrawerTrigger, DrawerTriggerProps,
};
pub use dropdown_menu::{
    dropdown, dropdown_menu, dropdown_menu_content, dropdown_menu_radio_group, dropdown_menu_scope,
    dropdown_menu_sub, dropdown_menu_sub_content, use_dropdown_menu, DropdownBuilder,
    DropdownMenuBuilder, DropdownMenuCheckboxItem, DropdownMenuCheckboxItemProps,
    DropdownMenuContentOpts, DropdownMenuCtx, DropdownMenuGroup, DropdownMenuGroupProps,
    DropdownMenuItem, DropdownMenuItemProps, DropdownMenuLabel, DropdownMenuLabelProps,
    DropdownMenuRadioGroupCtx, DropdownMenuRadioItem, DropdownMenuRadioItemProps,
    DropdownMenuSeparator, DropdownMenuSeparatorProps, DropdownMenuShortcut,
    DropdownMenuShortcutProps, DropdownMenuSubCtx, DropdownMenuSubTrigger,
    DropdownMenuSubTriggerProps, DropdownMenuTrigger, DropdownMenuTriggerProps,
};
pub use form::{
    clear_all_errors, clear_field_error, field_error, field_invalid, form_errors_signal,
    form_field, form_provider, form_validate, set_field_error, use_form, FormState,
};
pub use hover_card::{
    hover_card, hover_card_content, hover_card_scope, hover_card_surface, use_hover_card,
    HoverCardBuilder, HoverCardContentOpts, HoverCardCtx, HoverCardTrigger, HoverCardTriggerProps,
};
pub use image::{Image, ImageProps, Img, ImgProps};
pub use input::{
    apply_bound_input_disabled, apply_bound_input_invalid, filter_email_insert,
    filter_insert_for_type, filter_number_insert, filter_password_insert, filter_tel_insert,
    filter_url_insert, input, input_caret_signal_key, input_ime_anchor_signal_key,
    input_overwrite_signal_key, input_password_reveal_signal_key, input_selection_signal_key,
    input_type_signal_key, input_value_signal_key, Input, InputBuilder, InputProps, InputType,
    IME_ACTIVE_SIGNAL, INPUT_FOCUS_PREFIX,
};
pub use input_adornment::{
    input_adornment, InputAdornment, InputAdornmentAddon, InputAdornmentAddonProps,
    InputAdornmentAlign, InputAdornmentBuilder, InputAdornmentButton, InputAdornmentButtonProps,
    InputAdornmentInput, InputAdornmentInputProps, InputAdornmentProps, InputAdornmentText,
    InputAdornmentTextProps, InputAdornmentTextarea, InputAdornmentTextareaProps,
};
pub use input_otp::{
    input_otp, input_otp_focus_id, input_otp_scope, use_input_otp, InputOTP, InputOTPGroup,
    InputOTPGroupProps, InputOTPProps, InputOTPSeparator, InputOTPSeparatorProps, InputOTPSlot,
    InputOTPSlotProps, InputOtpBuilder, InputOtpCtx, InputOtpPattern, INPUT_OTP_FOCUS_PREFIX,
};
pub use kbd::{kbd, kbd_group, Kbd, KbdBuilder, KbdGroup, KbdGroupBuilder, KbdGroupProps, KbdProps};
pub use label::{
    enable_label_selection, label, label_selection_signal_key, selectable_text_key,
    text_node as text, try_copy_label_selection, Label, LabelBuilder, LabelProps,
    LABEL_SELECTION_COPY_SIGNAL,
};
pub use menubar::{
    menubar, menubar_content, menubar_menu, menubar_radio_group, menubar_scope, menubar_sub,
    menubar_sub_content, use_menubar, MenuItem, MenuSection, MenubarBuilder, MenubarCheckboxItem,
    MenubarCheckboxItemProps, MenubarContentOpts, MenubarCtx, MenubarGroup, MenubarGroupProps,
    MenubarItem, MenubarItemProps, MenubarLabel, MenubarLabelProps, MenubarRadioItem,
    MenubarRadioItemProps, MenubarSeparator, MenubarSeparatorProps, MenubarShortcut,
    MenubarShortcutProps, MenubarSubTrigger, MenubarSubTriggerProps, MenubarTrigger,
    MenubarTriggerProps,
};
pub use navbar::{
    navbar, Navbar, NavbarActions, NavbarActionsProps, NavbarBrand, NavbarBrandProps,
    NavbarBuilder, NavbarCenter, NavbarCenterProps, NavbarProps,
};
pub use navigation_menu::{
    navigation_menu, navigation_menu_content, navigation_menu_item, navigation_menu_scope,
    use_navigation_menu, use_navigation_menu_item, NavigationMenuBuilder,
    NavigationMenuContentOpts, NavigationMenuCtx, NavigationMenuEntry, NavigationMenuItem,
    NavigationMenuItemCtx, NavigationMenuItemProps, NavigationMenuLink, NavigationMenuLinkData,
    NavigationMenuLinkProps, NavigationMenuList, NavigationMenuListProps, NavigationMenuTrigger,
    NavigationMenuTriggerProps, NavigationMenuViewport, NavigationMenuViewportProps,
};
pub use pagination::{
    estimate_jumper_chrome_px, fit_pagination_for_width, fit_pagination_for_width_ex, pagination,
    pagination_range, pagination_range_simple, parse_jumper_page, Pagination, PaginationBuilder,
    PaginationContent, PaginationContentProps, PaginationEllipsis, PaginationEllipsisProps,
    PaginationItem, PaginationItemProps, PaginationJumper, PaginationJumperProps, PaginationLink,
    PaginationLinkProps, PaginationNext, PaginationNextProps, PaginationPage, PaginationPrevious,
    PaginationPreviousProps, PaginationProps, PaginationRangeOpts,
};
pub use popover::{
    popover, popover_content, popover_panel, popover_scope, popover_side_align_to_placement,
    popover_surface, use_popover, PopoverBuilder, PopoverContentOpts, PopoverCtx,
    PopoverDescription, PopoverDescriptionProps, PopoverHeader, PopoverHeaderProps,
    PopoverPanelBuilder, PopoverTitle, PopoverTitleProps, PopoverTrigger, PopoverTriggerProps,
};
pub use progress::{
    progress, progress_circle, progress_circle_svg, progress_circle_view, progress_scope,
    progress_track, use_progress,
    Progress, ProgressBuilder, ProgressCircle, ProgressCircleBuilder, ProgressCircleOpts,
    ProgressCircleSize, ProgressCircleProps, ProgressCtx, ProgressIndicator, ProgressIndicatorProps, ProgressLabel,
    ProgressLabelProps, ProgressOrientation, ProgressProps, ProgressTrack, ProgressTrackOpts, ProgressTrackProps,
    ProgressValue, ProgressValueProps,
};
pub use radio_group::{
    radio_group, radio_group_scope, use_radio_group, RadioGroup, RadioGroupBuilder, RadioGroupCtx,
    RadioGroupItem, RadioGroupItemLabel, RadioGroupItemLabelProps, RadioGroupItemProps,
    RadioGroupProps,
};
pub use resizable::{
    resizable, resizable_panel_group_scope, use_resizable, ResizableBuilder, ResizableCtx,
    ResizableHandle, ResizableHandleProps, ResizableOrientation, ResizablePanel,
    ResizablePanelGroup, ResizablePanelGroupProps, ResizablePanelProps,
};
pub use scroll_area::{scroll_area, scrollarea, ScrollArea, ScrollAreaBuilder, ScrollAreaProps};
pub use scroll_viewport::{
    scroll_viewport, scrollviewport, ScrollViewport, ScrollViewportBuilder, ScrollViewportProps,
    SCROLL_VIEWPORT_FOCUS_PREFIX,
};
pub use scrollbar::{
    scrollbar, scrollbarbuilder, Scrollbar, ScrollbarBuilder, ScrollbarOrientation, ScrollbarProps,
};
pub use select::{
    select, select_content, select_scope, use_select, Select, SelectBuilder, SelectContentOpts,
    SelectCtx, SelectGroup, SelectGroupData, SelectGroupProps, SelectItem, SelectItemAlign,
    SelectItemData, SelectItemProps, SelectLabel, SelectLabelProps, SelectProps,
    SelectScrollSelected, SelectSeparator, SelectSeparatorProps, SelectTrigger, SelectTriggerProps,
    SelectValue, SelectValueProps,
};
pub use separator::{separator, Separator, SeparatorBuilder, SeparatorProps};
pub use sheet::{
    sheet, sheet_close, sheet_content, sheet_open, sheet_scope, use_sheet, SheetBody,
    SheetBodyProps, SheetBuilder, SheetClose, SheetCloseProps, SheetContentOpts, SheetCtx,
    SheetDescription, SheetDescriptionProps, SheetFooter, SheetFooterProps, SheetHeader,
    SheetHeaderProps, SheetSide, SheetTitle, SheetTitleProps, SheetTrigger, SheetTriggerProps,
};
pub use sidebar::{
    apply_bound_sidebar_menu_button_active, sidebar, sidebar_menu_button, sidebar_provider_scope,
    toggle_sidebar, use_sidebar, Sidebar, SidebarBuilder, SidebarCollapsible, SidebarContent,
    SidebarContentProps, SidebarCtx, SidebarFooter, SidebarFooterProps, SidebarGroup,
    SidebarGroupContent, SidebarGroupContentProps, SidebarGroupLabel, SidebarGroupLabelProps,
    SidebarGroupProps, SidebarHeader, SidebarHeaderProps, SidebarInput, SidebarInputProps,
    SidebarInset, SidebarInsetProps, SidebarMenu, SidebarMenuButton, SidebarMenuButtonBuilder,
    SidebarMenuButtonProps, SidebarMenuButtonSize, SidebarMenuButtonVariant, SidebarMenuItem,
    SidebarMenuItemProps, SidebarMenuProps, SidebarProps, SidebarProvider, SidebarProviderProps,
    SidebarRail, SidebarRailProps, SidebarSeparator, SidebarSeparatorProps, SidebarSide,
    SidebarTrigger, SidebarTriggerProps, SidebarVariant,
};
pub use slider::{slider, Slider, SliderBuilder, SliderOrientation, SliderProps};
pub use stepper::{stepper, StepperBuilder};
pub use switch::{switch, Switch, SwitchBuilder, SwitchProps};
pub use table::{
    data_table, table, table_comfortable, table_compact, DataTable, DataTableProps, Table,
    TableAlign, TableBody, TableBodyProps, TableBuilder, TableCaption, TableCaptionProps,
    TableCell, TableCellProps, TableColumn, TableDensity, TableFooter, TableFooterProps, TableHead,
    TableHeadProps, TableHeader, TableHeaderProps, TablePaginationState, TableProps, TableRow,
    TableRowProps, TableSortDir, TableSortState,
};
pub use tabs::{
    tabs_content, tabs_headless, tabs_headless_root, tabs_list, tabs_scope, use_tabs, Tabs,
    TabsContent, TabsContentProps, TabsCtx, TabsHeadlessBuilder, TabsList, TabsListBuilder,
    TabsListProps, TabsOrientation, TabsProps, TabsTrigger, TabsTriggerProps,
    TABSTRIP_FOCUS_PREFIX,
};
pub use textarea::{textarea, Textarea, TextareaBuilder, TextareaProps, TEXTAREA_FOCUS_PREFIX};
pub use toast::host::{toast_host, toaster, Toaster, ToasterBuilder, ToasterProps};
pub use toast::view::{
    Toast, ToastClose, ToastCloseProps, ToastDescription, ToastDescriptionProps, ToastProps,
    ToastTitle, ToastTitleProps,
};
pub use toast::{
    toast, toast_dismiss, toast_pending, toast_push, toast_queue, toast_set_layout,
    toast_set_limit, toast_set_overflow, toast_set_position, ToastBuilder, ToastItem, ToastLayout,
    ToastOverflow, ToastPosition, ToastTone,
};
pub use toggle::{toggle, Toggle, ToggleBuilder, ToggleProps};
pub use toggle_group::{toggle_group, ToggleGroup, ToggleGroupBuilder, ToggleGroupProps};
pub use tooltip::{
    tooltip, tooltip_bubble, tooltip_content, tooltip_scope, tooltip_side_to_placement,
    use_tooltip, TooltipBuilder, TooltipContentOpts, TooltipCtx, TooltipTrigger,
    TooltipTriggerProps,
};
pub use tree_view::{
    tree_view, tree_view_item_scope, tree_view_scope, use_tree_view, use_tree_view_item, TreeNode,
    TreeView, TreeViewBuilder, TreeViewCtx, TreeViewItem, TreeViewItemCtx, TreeViewVariant,
};
pub use virtual_list::{virtual_list, VirtualListBuilder};
pub use virtual_masonry::{virtual_masonry, VirtualMasonryBuilder};

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
//! - ToggleGroup: `<ToggleGroup>` / `ToggleGroupItem` (macro `toggle_group_scope`); `type=single|multiple`; `spacing=0` / `joined` reuses ButtonGroup fused corners. Builder [`toggle_group`].
//! - Avatar: `<Avatar>` / `<AvatarImage>` / `<AvatarFallback>` / `<AvatarBadge>` / `<AvatarGroup>` (macro → [`avatar_scope`]); Builder [`avatar`].
//! - Badge：展示型 `<Badge variant>` / [`badge`]（default/secondary/destructive/outline/ghost/link）。
//! - Breadcrumb：`<Breadcrumb>` / `BreadcrumbList` / `BreadcrumbItem` / `BreadcrumbLink` / `BreadcrumbPage` / `BreadcrumbSeparator` / `BreadcrumbEllipsis`；Builder [`breadcrumb`]。
//! - ButtonGroup：`<ButtonGroup>` / `ButtonGroupText` / `ButtonGroupSeparator`；Builder [`button_group`]。
//! - InputAdornment: `<InputAdornment>` / Addon / Text / Button / Input / Textarea (opposite ShadCN InputGroup); Builder [`input_adornment`].
//! - InputOTP: `<InputOTP>` / Group / Slot / Separator (对位 ShadCN InputOTP); Builder [`input_otp`].
//! - Questionnaire: `<Questionnaire>` / Item / Choices / Choice / Input / Actions (macro `questionnaire_scope`); Builder [`questionnaire`].
//! - AspectRatio：展示型 `<AspectRatio ratio>` / [`aspect_ratio`]（CSS `aspect-ratio` 容器；Image 等亦可直接写 `aspect_ratio`/`aspect-ratio` 属性）。
//! - Kbd: presentational `<Kbd>` / `<KbdGroup>` / [`kbd`] / [`kbd_group`] (keycap chrome; no special `ui!` scope).
//! - Marker: presentational `<Marker>` / `MarkerIcon` / `MarkerContent` + [`marker`] / [`ui_marker`] (`default`/`border`/`separator`). Lowercase `<marker>` remains the SVG primitive.
//! - Message: presentational `<Message>` / `MessageGroup` / `MessageAvatar` / `MessageContent` / `MessageHeader` / `MessageFooter` + [`message`] (`align` start/end; macro `bind_attr`).
//! - MessageScroller: compound `<MessageScrollerProvider>` + Viewport / Content / Item / Button (macro [`message_scroller_scope`]); Builder [`message_scroller_provider`].
//! - Skeleton: presentational `<Skeleton>` / [`skeleton`] (loading placeholder; `animate-pulse` + `bg-accent`).
//! - Empty: `<Empty>` / `EmptyHeader` / `EmptyMedia` / `EmptyTitle` / `EmptyDescription` / `EmptyContent` + [`empty`] builders.
//! - Alert: `<Alert variant>` / `AlertTitle` / `AlertDescription` / `AlertAction` + [`alert`] (`default`/`destructive`/`info`/`success`/`warning`).
//! - AlertDialog: `<AlertDialog>` / `AlertDialogContent` / `AlertDialogCancel` + [`alert_dialog`] (blocking confirm; semantic variants via [`apply_bound_alert_dialog_variant`]).
//! - Item: `<Item>` / `ItemGroup` / `ItemMedia` / `ItemContent` / … + [`item`] / [`item_group`] (`variant`/`size`).
//! - Attachment: `<Attachment>` / `AttachmentGroup` / `AttachmentMedia` / `AttachmentContent` / … + [`attachment`] / [`attachment_group`] (`state`/`size`/`orientation`).
//! - List: `<List>` / `ListItem` / `ListHeader` / `ListDescription` + [`list`] (`marker`); nest `Item` for rich rows.
//! - Table: `<Table>` / `TableHeader` / `TableRow` / `TableHead` / `TableCell` / `TableFooter` / `TableCaption` + [`table`] / [`data_table`]（TanStack 排序·筛选·分页）；宏 `bind_attr`：`selected` / `align`。
//! - Collapsible: Composite `<Collapsible>` / `CollapsibleTrigger` / `CollapsibleContent` (macro `collapsible_scope` / `collapsible_content`); Builder [`collapsible`].
//! - Carousel: `<Carousel variant>` / `<CarouselSingle|Multi|Stack>` + Content/Item/Prev/Next/Dots (macro `carousel_scope`); Builder [`.slide`](carousel::CarouselBuilder::slide).
//! - 颜色 / 日期：[`calendar`] / [`date_picker`] / [`time_picker`] / [`color_picker`]（触发器样式各自维护）。原生文件对话框在 [`crate::utils::file_dialog`]；Tab 焦点陷阱在 [`crate::utils::focus_trap`]（能力，非组件）。
//! - 导航分工：[`navbar`] 布局壳；[`navigation_menu`] 链接导览（ShadCN）；[`menubar`] 应用命令；
//! - Pagination: Data paging navigation (ShadCN). The page stack host is [`crate::app::navigator::navigation_host`].
//! - Stepper: numbered step indicator (RUI, not an official ShadCN primitive) — `<Stepper>` / `StepperItem` / Indicator / Title / Description (macro `stepper_scope`); Builder [`stepper`].
//! - Timeline: vertical event list (solid-ui `timeline.tsx`, ShadCN-style; not an official ShadCN primitive) — `<Timeline>` / `TimelineItem` / Bullet / Title / Description (macro `timeline_scope`); Builder [`timeline`].
#![allow(non_snake_case)]

/// 组件库私有实现（不进产品面）。
pub(crate) mod internal;

pub mod accordion;
pub mod alert;
pub mod alert_dialog;
pub mod aspect_ratio;
pub mod attachment;
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
pub mod empty;
pub mod form;
pub mod hover_card;
pub mod image;
pub mod input;
pub mod input_adornment;
pub mod input_otp;
pub mod item;
pub mod kbd;
pub mod label;
pub mod list;
pub mod marker;
pub mod menubar;
pub mod message;
pub mod message_scroller;
pub mod navbar;
pub mod navigation_menu;
pub mod pagination;
pub mod popover;
pub mod progress;
pub mod questionnaire;
pub mod radio_group;
pub mod resizable;
pub mod scroll_area;
pub mod scroll_viewport;
pub mod scrollbar;
pub mod select;
pub mod separator;
pub mod sheet;
pub mod sidebar;
pub mod skeleton;
pub mod slider;
pub mod stepper;
pub mod switch;
pub mod table;
pub mod tabs;
pub mod textarea;
pub mod timeline;
pub mod toast;
pub mod toggle;
pub mod toggle_group;
pub mod tooltip;
pub mod tree_view;
pub mod virtual_list;
pub mod virtual_masonry;

pub use accordion::{
    accordion, accordion_content, accordion_item, accordion_item_scope, accordion_scope,
    accordion_trigger, accordion_use_grid_fr, set_accordion_use_grid_fr, use_accordion,
    use_accordion_item, Accordion, AccordionBuilder, AccordionContent, AccordionContentBuilder,
    AccordionContentProps, AccordionCtx, AccordionItem, AccordionItemBuilder, AccordionItemCtx,
    AccordionItemProps, AccordionProps, AccordionTrigger, AccordionTriggerBuilder,
    AccordionTriggerProps,
};
pub use alert::{
    alert, alert_action, alert_description, alert_title, apply_alert_variant_paint,
    apply_bound_alert_chrome, apply_bound_alert_variant, Alert, AlertAction, AlertActionBuilder,
    AlertActionProps, AlertBuilder, AlertDescription, AlertDescriptionBuilder,
    AlertDescriptionProps, AlertProps, AlertTitle, AlertTitleBuilder, AlertTitleProps,
    AlertVariant,
};
pub use alert_dialog::{
    alert_dialog, alert_dialog_action, alert_dialog_body, alert_dialog_cancel, alert_dialog_close,
    alert_dialog_content, alert_dialog_content_slot, alert_dialog_description, alert_dialog_footer,
    alert_dialog_header, alert_dialog_media, alert_dialog_open, alert_dialog_scope,
    alert_dialog_title, alert_dialog_trigger, apply_alert_dialog_variant_paint,
    apply_bound_alert_dialog_chrome, apply_bound_alert_dialog_variant, use_alert_dialog,
    AlertDialogAction, AlertDialogActionBuilder, AlertDialogActionProps, AlertDialogBody,
    AlertDialogBodyBuilder, AlertDialogBodyProps, AlertDialogBuilder, AlertDialogCancel,
    AlertDialogCancelBuilder, AlertDialogCancelProps, AlertDialogContentOpts,
    AlertDialogContentSlotBuilder, AlertDialogCtx, AlertDialogDescription,
    AlertDialogDescriptionBuilder, AlertDialogDescriptionProps, AlertDialogFooter,
    AlertDialogFooterBuilder, AlertDialogFooterProps, AlertDialogHeader, AlertDialogHeaderBuilder,
    AlertDialogHeaderProps, AlertDialogMedia, AlertDialogMediaBuilder, AlertDialogMediaProps,
    AlertDialogSize, AlertDialogTitle, AlertDialogTitleBuilder, AlertDialogTitleProps,
    AlertDialogTrigger, AlertDialogTriggerBuilder, AlertDialogTriggerProps, AlertDialogVariant,
};
pub use aspect_ratio::{
    aspect_ratio, AspectRatio, AspectRatioBuilder, AspectRatioProps, AspectRatioValue,
};
pub use attachment::{
    apply_attachment_paint, apply_bound_attachment_media_variant,
    apply_bound_attachment_orientation, apply_bound_attachment_size, apply_bound_attachment_state,
    attachment, attachment_action, attachment_actions, attachment_content, attachment_description,
    attachment_group, attachment_media, attachment_title, attachment_trigger, Attachment,
    AttachmentAction, AttachmentActionBuilder, AttachmentActionProps, AttachmentActions,
    AttachmentActionsBuilder, AttachmentActionsProps, AttachmentBuilder, AttachmentContent,
    AttachmentContentBuilder, AttachmentContentProps, AttachmentDescription,
    AttachmentDescriptionBuilder, AttachmentDescriptionProps, AttachmentGroup,
    AttachmentGroupBuilder, AttachmentGroupProps, AttachmentMedia, AttachmentMediaBuilder,
    AttachmentMediaProps, AttachmentMediaVariant, AttachmentOrientation, AttachmentProps,
    AttachmentSize, AttachmentState, AttachmentTitle, AttachmentTitleBuilder, AttachmentTitleProps,
    AttachmentTrigger, AttachmentTriggerBuilder, AttachmentTriggerProps,
};
pub use avatar::{
    apply_bound_avatar_badge_variant, apply_bound_avatar_shape, apply_bound_avatar_size, avatar,
    avatar_fallback, avatar_image, avatar_scope, use_avatar, Avatar, AvatarBadge, AvatarBadgeProps,
    AvatarBuilder, AvatarCtx, AvatarFallback, AvatarFallbackBuilder, AvatarFallbackProps,
    AvatarGroup, AvatarGroupCount, AvatarGroupCountProps, AvatarGroupProps, AvatarImage,
    AvatarImageBuilder, AvatarImageProps, AvatarProps, AvatarShape, AvatarSize,
};
pub use badge::{
    apply_bound_badge_chrome, apply_bound_badge_variant, badge, Badge, BadgeBuilder, BadgeProps,
    BadgeVariant,
};
pub use base::{BaseComponentBuilder, SlotChrome, SlotMap, SlotStyleFn};
pub use breadcrumb::{
    breadcrumb, breadcrumb_ellipsis, breadcrumb_item, breadcrumb_link, breadcrumb_list,
    breadcrumb_page, breadcrumb_separator, Breadcrumb, BreadcrumbBuilder, BreadcrumbEllipsis,
    BreadcrumbEllipsisBuilder, BreadcrumbEllipsisProps, BreadcrumbItem, BreadcrumbItemBuilder,
    BreadcrumbItemProps, BreadcrumbLink, BreadcrumbLinkBuilder, BreadcrumbLinkProps,
    BreadcrumbList, BreadcrumbListBuilder, BreadcrumbListProps, BreadcrumbPage,
    BreadcrumbPageBuilder, BreadcrumbPageProps, BreadcrumbProps, BreadcrumbSeparator,
    BreadcrumbSeparatorBuilder, BreadcrumbSeparatorProps,
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
    button_group, button_group_separator, button_group_text, ButtonGroup, ButtonGroupBuilder,
    ButtonGroupOrientation, ButtonGroupProps, ButtonGroupSeparator, ButtonGroupSeparatorBuilder,
    ButtonGroupSeparatorProps, ButtonGroupText, ButtonGroupTextBuilder, ButtonGroupTextProps,
};
pub use calendar::{
    calendar, days_in_month, format_long_date, format_month_year, format_ymd, parse_ymd, today_ymd,
    weekday, Calendar, CalendarBuilder, CalendarCaption, CalendarProps,
};
pub use card::{
    card, card_action, card_content, card_description, card_footer, card_header, card_title, Card,
    CardAction, CardActionBuilder, CardActionProps, CardBuilder, CardContent, CardContentBuilder,
    CardContentProps, CardDescription, CardDescriptionBuilder, CardDescriptionProps, CardFooter,
    CardFooterBuilder, CardFooterProps, CardHeader, CardHeaderBuilder, CardHeaderProps, CardProps,
    CardTitle, CardTitleBuilder, CardTitleProps,
};
pub use carousel::builder::{
    carousel, carousel_multi, carousel_single, carousel_stack, CarouselBuilder, CarouselSlotBuilder,
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
    collapsible, collapsible_content, collapsible_content_slot, collapsible_scope,
    collapsible_trigger, use_collapsible, Collapsible, CollapsibleBuilder, CollapsibleContent,
    CollapsibleContentProps, CollapsibleContentSlotBuilder, CollapsibleCtx, CollapsibleProps,
    CollapsibleTrigger, CollapsibleTriggerBuilder, CollapsibleTriggerProps,
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
    command, command_dialog_scope, command_empty, command_group, command_input, command_item,
    command_list, command_overlay_scope, command_scope, command_separator, command_shortcut,
    use_command, Command, CommandAction, CommandBuilder, CommandCtx, CommandEmpty,
    CommandEmptyBuilder, CommandEmptyProps, CommandGroup, CommandGroupBuilder, CommandGroupData,
    CommandGroupProps, CommandInput, CommandInputBuilder, CommandInputProps, CommandItem,
    CommandItemBuilder, CommandItemProps, CommandList, CommandListBuilder, CommandListProps,
    CommandProps, CommandSeparator, CommandSeparatorBuilder, CommandSeparatorProps,
    CommandShortcut, CommandShortcutBuilder, CommandShortcutProps, CommandSurface,
    CommandVisibleItem,
};
pub use context_menu::{
    context_menu, context_menu_content, context_menu_content_slot, context_menu_group,
    context_menu_item, context_menu_label, context_menu_radio_group, context_menu_scope,
    context_menu_separator, context_menu_shortcut, context_menu_sub, context_menu_sub_content,
    context_menu_trigger, use_context_menu, ContextMenuAction, ContextMenuBuilder,
    ContextMenuCheckboxItem, ContextMenuCheckboxItemProps, ContextMenuContentOpts,
    ContextMenuContentSlotBuilder, ContextMenuCtx, ContextMenuGroup, ContextMenuGroupBuilder,
    ContextMenuGroupProps, ContextMenuItem, ContextMenuItemBuilder, ContextMenuItemProps,
    ContextMenuLabel, ContextMenuLabelBuilder, ContextMenuLabelProps, ContextMenuRadioGroupCtx,
    ContextMenuRadioItem, ContextMenuRadioItemProps, ContextMenuSeparator,
    ContextMenuSeparatorBuilder, ContextMenuSeparatorProps, ContextMenuShortcut,
    ContextMenuShortcutBuilder, ContextMenuShortcutProps, ContextMenuSubCtx, ContextMenuSubTrigger,
    ContextMenuSubTriggerProps, ContextMenuTrigger, ContextMenuTriggerBuilder,
    ContextMenuTriggerProps,
};
pub use date_picker::{
    date_picker, time_picker, DatePicker, DatePickerBuilder, DatePickerKind, DatePickerProps,
    TimePicker, TimePickerBuilder, TimePickerProps,
};
pub use dialog::{
    dialog, dialog_body, dialog_close, dialog_content, dialog_content_slot, dialog_description,
    dialog_footer, dialog_header, dialog_open, dialog_scope, dialog_title, dialog_trigger,
    use_dialog, DialogBody, DialogBodyBuilder, DialogBodyProps, DialogBuilder, DialogContentOpts,
    DialogContentSlotBuilder, DialogCtx, DialogDescription, DialogDescriptionBuilder,
    DialogDescriptionProps, DialogFooter, DialogFooterBuilder, DialogFooterProps, DialogHeader,
    DialogHeaderBuilder, DialogHeaderProps, DialogTitle, DialogTitleBuilder, DialogTitleProps,
    DialogTrigger, DialogTriggerBuilder, DialogTriggerProps,
};
pub use drawer::{
    coerce_snap_point_list, coerce_snap_points, drawer, drawer_active_snap_point, drawer_body,
    drawer_close, drawer_close_part, drawer_content, drawer_content_slot, drawer_description,
    drawer_footer, drawer_header, drawer_open, drawer_scope, drawer_set_active_snap_point,
    drawer_snap_to_next, drawer_snap_to_prev, drawer_title, drawer_trigger, parse_snap_point_token,
    parse_snap_points, use_drawer, DrawerBody, DrawerBodyBuilder, DrawerBodyProps, DrawerBuilder,
    DrawerClose, DrawerCloseBuilder, DrawerCloseProps, DrawerContentOpts, DrawerContentSlotBuilder,
    DrawerCtx, DrawerDescription, DrawerDescriptionBuilder, DrawerDescriptionProps,
    DrawerDirection, DrawerFooter, DrawerFooterBuilder, DrawerFooterProps, DrawerHeader,
    DrawerHeaderBuilder, DrawerHeaderProps, DrawerSnapPoint, DrawerSwipeHandle,
    DrawerSwipeHandleProps, DrawerTitle, DrawerTitleBuilder, DrawerTitleProps, DrawerTrigger,
    DrawerTriggerBuilder, DrawerTriggerProps,
};
pub use dropdown_menu::{
    dropdown, dropdown_menu, dropdown_menu_content, dropdown_menu_content_slot,
    dropdown_menu_group, dropdown_menu_item, dropdown_menu_label, dropdown_menu_radio_group,
    dropdown_menu_scope, dropdown_menu_separator, dropdown_menu_shortcut, dropdown_menu_sub,
    dropdown_menu_sub_content, dropdown_menu_trigger, use_dropdown_menu, DropdownBuilder,
    DropdownMenuBuilder, DropdownMenuCheckboxItem, DropdownMenuCheckboxItemProps,
    DropdownMenuContentOpts, DropdownMenuContentSlotBuilder, DropdownMenuCtx, DropdownMenuGroup,
    DropdownMenuGroupBuilder, DropdownMenuGroupProps, DropdownMenuItem, DropdownMenuItemBuilder,
    DropdownMenuItemProps, DropdownMenuLabel, DropdownMenuLabelBuilder, DropdownMenuLabelProps,
    DropdownMenuRadioGroupCtx, DropdownMenuRadioItem, DropdownMenuRadioItemProps,
    DropdownMenuSeparator, DropdownMenuSeparatorBuilder, DropdownMenuSeparatorProps,
    DropdownMenuShortcut, DropdownMenuShortcutBuilder, DropdownMenuShortcutProps,
    DropdownMenuSubCtx, DropdownMenuSubTrigger, DropdownMenuSubTriggerProps, DropdownMenuTrigger,
    DropdownMenuTriggerBuilder, DropdownMenuTriggerProps,
};
pub use empty::{
    empty, empty_content, empty_description, empty_header, empty_media, empty_title, Empty,
    EmptyBuilder, EmptyContent, EmptyContentBuilder, EmptyContentProps, EmptyDescription,
    EmptyDescriptionBuilder, EmptyDescriptionProps, EmptyHeader, EmptyHeaderBuilder,
    EmptyHeaderProps, EmptyMedia, EmptyMediaBuilder, EmptyMediaProps, EmptyMediaVariant,
    EmptyProps, EmptyTitle, EmptyTitleBuilder, EmptyTitleProps,
};
pub use form::{
    clear_all_errors, clear_field_error, field_error, field_invalid, form_errors_signal,
    form_field, form_provider, form_validate, set_field_error, use_form, FormState,
};
pub use hover_card::{
    hover_card, hover_card_content, hover_card_content_slot, hover_card_scope, hover_card_surface,
    hover_card_trigger, use_hover_card, HoverCardBuilder, HoverCardContentOpts,
    HoverCardContentSlotBuilder, HoverCardCtx, HoverCardTrigger, HoverCardTriggerBuilder,
    HoverCardTriggerProps,
};
pub use image::{image, ui_image, Image, ImageBuilder, ImageProps, Img, ImgProps};
pub use input::{
    apply_bound_input_disabled, apply_bound_input_invalid, filter_email_insert,
    filter_insert_for_type, filter_number_insert, filter_password_insert, filter_tel_insert,
    filter_url_insert, input, input_caret_signal_key, input_ime_anchor_signal_key,
    input_overwrite_signal_key, input_password_reveal_signal_key, input_selection_signal_key,
    input_type_signal_key, input_value_signal_key, Input, InputBuilder, InputProps, InputType,
    IME_ACTIVE_SIGNAL, INPUT_FOCUS_PREFIX,
};
pub use input_adornment::{
    input_adornment, input_adornment_addon, input_adornment_text, InputAdornment,
    InputAdornmentAddon, InputAdornmentAddonBuilder, InputAdornmentAddonProps, InputAdornmentAlign,
    InputAdornmentBuilder, InputAdornmentButton, InputAdornmentButtonProps, InputAdornmentInput,
    InputAdornmentInputProps, InputAdornmentProps, InputAdornmentText, InputAdornmentTextBuilder,
    InputAdornmentTextProps, InputAdornmentTextarea, InputAdornmentTextareaProps,
};
pub use input_otp::{
    input_otp, input_otp_focus_id, input_otp_group, input_otp_scope, input_otp_separator,
    input_otp_slot, use_input_otp, InputOTP, InputOTPGroup, InputOTPGroupProps, InputOTPProps,
    InputOTPSeparator, InputOTPSeparatorProps, InputOTPSlot, InputOTPSlotProps, InputOtpBuilder,
    InputOtpCtx, InputOtpGroupBuilder, InputOtpPattern, InputOtpSeparatorBuilder,
    InputOtpSlotBuilder, INPUT_OTP_FOCUS_PREFIX,
};
pub use item::{
    apply_bound_item_media_variant, apply_bound_item_size, apply_bound_item_variant,
    apply_item_paint, item, item_actions, item_content, item_description, item_footer, item_group,
    item_header, item_media, item_separator, item_title, Item, ItemActions, ItemActionsBuilder,
    ItemActionsProps, ItemBuilder, ItemContent, ItemContentBuilder, ItemContentProps,
    ItemDescription, ItemDescriptionBuilder, ItemDescriptionProps, ItemFooter, ItemFooterBuilder,
    ItemFooterProps, ItemGroup, ItemGroupBuilder, ItemGroupProps, ItemHeader, ItemHeaderBuilder,
    ItemHeaderProps, ItemMedia, ItemMediaBuilder, ItemMediaProps, ItemMediaVariant, ItemProps,
    ItemSeparator, ItemSeparatorBuilder, ItemSeparatorProps, ItemSize, ItemTitle, ItemTitleBuilder,
    ItemTitleProps, ItemVariant,
};
pub use kbd::{
    kbd, kbd_group, Kbd, KbdBuilder, KbdGroup, KbdGroupBuilder, KbdGroupProps, KbdProps,
};
pub use label::{
    enable_label_selection, label, label_selection_signal_key, selectable_text_key,
    text_node as text, try_copy_label_selection, Label, LabelBuilder, LabelProps,
    LABEL_SELECTION_COPY_SIGNAL,
};
pub use list::{
    list, list_description, list_header, list_item, List, ListBuilder, ListDescription,
    ListDescriptionBuilder, ListDescriptionProps, ListHeader, ListHeaderBuilder, ListHeaderProps,
    ListItem, ListItemBuilder, ListItemProps, ListMarker, ListProps,
};
pub use marker::{
    apply_bound_marker_variant, apply_marker_paint, marker, marker_content, marker_icon, ui_marker,
    Marker, MarkerBuilder, MarkerContent, MarkerContentBuilder, MarkerContentProps, MarkerIcon,
    MarkerIconBuilder, MarkerIconProps, MarkerProps, MarkerVariant,
};
pub use menubar::{
    menubar, menubar_content, menubar_content_slot, menubar_group, menubar_item, menubar_label,
    menubar_menu, menubar_radio_group, menubar_scope, menubar_separator, menubar_shortcut,
    menubar_sub, menubar_sub_content, menubar_trigger, use_menubar, MenuItem, MenuSection,
    MenubarBuilder, MenubarCheckboxItem, MenubarCheckboxItemProps, MenubarContentOpts,
    MenubarContentSlotBuilder, MenubarCtx, MenubarGroup, MenubarGroupBuilder, MenubarGroupProps,
    MenubarItem, MenubarItemBuilder, MenubarItemProps, MenubarLabel, MenubarLabelBuilder,
    MenubarLabelProps, MenubarRadioItem, MenubarRadioItemProps, MenubarSeparator,
    MenubarSeparatorBuilder, MenubarSeparatorProps, MenubarShortcut, MenubarShortcutBuilder,
    MenubarShortcutProps, MenubarSubTrigger, MenubarSubTriggerProps, MenubarTrigger,
    MenubarTriggerBuilder, MenubarTriggerProps,
};
pub use message::{
    apply_bound_message_align, apply_message_paint, message, message_avatar, message_content,
    message_footer, message_group, message_header, Message, MessageAlign, MessageAvatar,
    MessageAvatarBuilder, MessageAvatarProps, MessageBuilder, MessageContent,
    MessageContentBuilder, MessageContentProps, MessageFooter, MessageFooterBuilder,
    MessageFooterProps, MessageGroup, MessageGroupBuilder, MessageGroupProps, MessageHeader,
    MessageHeaderBuilder, MessageHeaderProps, MessageProps,
};
pub use message_scroller::{
    message_scroller, message_scroller_button, message_scroller_content, message_scroller_item,
    message_scroller_provider, message_scroller_scope, message_scroller_stick_to_end,
    message_scroller_viewport, use_message_scroller, use_message_scroller_scrollable,
    use_message_scroller_visibility, MessageScroller, MessageScrollerBuilder,
    MessageScrollerButton, MessageScrollerButtonBuilder, MessageScrollerButtonProps,
    MessageScrollerContent, MessageScrollerContentBuilder, MessageScrollerContentProps,
    MessageScrollerCtx, MessageScrollerDefaultPosition, MessageScrollerDirection,
    MessageScrollerItem, MessageScrollerItemBuilder, MessageScrollerItemProps,
    MessageScrollerProps, MessageScrollerProvider, MessageScrollerProviderBuilder,
    MessageScrollerProviderProps, MessageScrollerScrollable, MessageScrollerViewport,
    MessageScrollerViewportBuilder, MessageScrollerViewportProps, MessageScrollerVisibility,
};
pub use navbar::{
    navbar, navbar_actions, navbar_brand, navbar_center, Navbar, NavbarActions,
    NavbarActionsBuilder, NavbarActionsProps, NavbarBrand, NavbarBrandBuilder, NavbarBrandProps,
    NavbarBuilder, NavbarCenter, NavbarCenterBuilder, NavbarCenterProps, NavbarProps,
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
    PaginationPreviousProps, PaginationProps, PaginationRangeOpts, PaginationSlotBuilder,
};
pub use popover::{
    popover, popover_content, popover_content_slot, popover_description, popover_header,
    popover_panel, popover_scope, popover_side_align_to_placement, popover_surface, popover_title,
    popover_trigger, use_popover, PopoverBuilder, PopoverContentOpts, PopoverContentSlotBuilder,
    PopoverCtx, PopoverDescription, PopoverDescriptionBuilder, PopoverDescriptionProps,
    PopoverHeader, PopoverHeaderBuilder, PopoverHeaderProps, PopoverPanelBuilder, PopoverTitle,
    PopoverTitleBuilder, PopoverTitleProps, PopoverTrigger, PopoverTriggerBuilder,
    PopoverTriggerProps,
};
pub use progress::{
    progress, progress_circle, progress_circle_svg, progress_circle_view, progress_label,
    progress_scope, progress_track, progress_track_slot, progress_value, use_progress, Progress,
    ProgressBuilder, ProgressCircle, ProgressCircleBuilder, ProgressCircleOpts,
    ProgressCircleProps, ProgressCircleSize, ProgressCtx, ProgressIndicator,
    ProgressIndicatorProps, ProgressLabel, ProgressLabelBuilder, ProgressLabelProps,
    ProgressOrientation, ProgressProps, ProgressTrack, ProgressTrackBuilder, ProgressTrackOpts,
    ProgressTrackProps, ProgressValue, ProgressValueBuilder, ProgressValueProps,
};
pub use questionnaire::{
    assign_shortcut, enabled_item_names, item_has_valid_answer, questionnaire,
    questionnaire_actions, questionnaire_choice, questionnaire_choice_description,
    questionnaire_choices, questionnaire_description, questionnaire_error,
    questionnaire_item_change_fn, questionnaire_item_scope, questionnaire_next,
    questionnaire_previous, questionnaire_progress, questionnaire_scope, questionnaire_skip,
    questionnaire_status_change_fn, questionnaire_submit, questionnaire_submit_fn,
    questionnaire_title, use_questionnaire, use_questionnaire_item, Questionnaire,
    QuestionnaireActions, QuestionnaireActionsBuilder, QuestionnaireActionsProps,
    QuestionnaireAnswers, QuestionnaireBuilder, QuestionnaireChoice, QuestionnaireChoiceBuilder,
    QuestionnaireChoiceDef, QuestionnaireChoiceDescription, QuestionnaireChoiceDescriptionBuilder,
    QuestionnaireChoiceDescriptionProps, QuestionnaireChoiceProps, QuestionnaireChoices,
    QuestionnaireChoicesBuilder, QuestionnaireChoicesProps, QuestionnaireCtx,
    QuestionnaireDescription, QuestionnaireDescriptionBuilder, QuestionnaireDescriptionProps,
    QuestionnaireError, QuestionnaireErrorBuilder, QuestionnaireErrorProps, QuestionnaireInput,
    QuestionnaireInputProps, QuestionnaireItem, QuestionnaireItemAnswer, QuestionnaireItemCtx,
    QuestionnaireItemDef, QuestionnaireItemHandle, QuestionnaireItemMeta, QuestionnaireItemProps,
    QuestionnaireItemStatus, QuestionnaireNext, QuestionnaireNextBuilder, QuestionnaireNextProps,
    QuestionnairePartsClass, QuestionnairePrevious, QuestionnairePreviousBuilder,
    QuestionnairePreviousProps, QuestionnaireProgress, QuestionnaireProgressBuilder,
    QuestionnaireProgressProps, QuestionnaireProps, QuestionnaireQuestionBuilder,
    QuestionnaireShortcuts, QuestionnaireSkip, QuestionnaireSkipBuilder, QuestionnaireSkipProps,
    QuestionnaireSubmit, QuestionnaireSubmitBuilder, QuestionnaireSubmitFn,
    QuestionnaireSubmitProps, QuestionnaireTitle, QuestionnaireTitleBuilder,
    QuestionnaireTitleProps,
};
pub use radio_group::{
    radio_group, radio_group_item, radio_group_item_label, radio_group_scope, radio_option,
    use_radio_group, RadioGroup, RadioGroupBuilder, RadioGroupCtx, RadioGroupItem,
    RadioGroupItemBuilder, RadioGroupItemLabel, RadioGroupItemLabelBuilder,
    RadioGroupItemLabelProps, RadioGroupItemProps, RadioGroupProps, RadioOption,
};
pub use resizable::{
    resizable, resizable_handle, resizable_panel, resizable_panel_group_scope, use_resizable,
    ResizableBuilder, ResizableCtx, ResizableHandle, ResizableHandleBuilder, ResizableHandleProps,
    ResizableOrientation, ResizablePanel, ResizablePanelBuilder, ResizablePanelGroup,
    ResizablePanelGroupProps, ResizablePanelProps,
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
    select, select_content, select_content_slot, select_group, select_item, select_scope,
    select_separator, select_trigger, select_value, use_select, Select, SelectBuilder,
    SelectContentOpts, SelectContentSlotBuilder, SelectCtx, SelectGroup, SelectGroupBuilder,
    SelectGroupData, SelectGroupProps, SelectItem, SelectItemAlign, SelectItemBuilder,
    SelectItemData, SelectItemProps, SelectLabel, SelectLabelProps, SelectProps,
    SelectScrollSelected, SelectSeparator, SelectSeparatorBuilder, SelectSeparatorProps,
    SelectTrigger, SelectTriggerBuilder, SelectTriggerProps, SelectValue, SelectValueBuilder,
    SelectValueProps,
};
pub use separator::{separator, Separator, SeparatorBuilder, SeparatorProps};
pub use sheet::{
    sheet, sheet_body, sheet_close, sheet_close_part, sheet_content, sheet_content_slot,
    sheet_description, sheet_footer, sheet_header, sheet_open, sheet_scope, sheet_title,
    sheet_trigger, use_sheet, SheetBody, SheetBodyBuilder, SheetBodyProps, SheetBuilder,
    SheetClose, SheetCloseBuilder, SheetCloseProps, SheetContentOpts, SheetContentSlotBuilder,
    SheetCtx, SheetDescription, SheetDescriptionBuilder, SheetDescriptionProps, SheetFooter,
    SheetFooterBuilder, SheetFooterProps, SheetHeader, SheetHeaderBuilder, SheetHeaderProps,
    SheetSide, SheetTitle, SheetTitleBuilder, SheetTitleProps, SheetTrigger, SheetTriggerBuilder,
    SheetTriggerProps,
};
pub use sidebar::{
    apply_bound_sidebar_menu_button_active, sidebar, sidebar_content, sidebar_footer,
    sidebar_group, sidebar_group_content, sidebar_group_label, sidebar_header, sidebar_inset,
    sidebar_menu, sidebar_menu_button, sidebar_menu_item, sidebar_provider_scope, sidebar_tree,
    sidebar_trigger, toggle_sidebar, use_sidebar, Sidebar, SidebarBuilder, SidebarCollapsible,
    SidebarContent, SidebarContentBuilder, SidebarContentProps, SidebarCtx, SidebarFooter,
    SidebarFooterBuilder, SidebarFooterProps, SidebarGroup, SidebarGroupBuilder,
    SidebarGroupContent, SidebarGroupContentBuilder, SidebarGroupContentProps, SidebarGroupLabel,
    SidebarGroupLabelBuilder, SidebarGroupLabelProps, SidebarGroupProps, SidebarHeader,
    SidebarHeaderBuilder, SidebarHeaderProps, SidebarInput, SidebarInputProps, SidebarInset,
    SidebarInsetBuilder, SidebarInsetProps, SidebarMenu, SidebarMenuBuilder, SidebarMenuButton,
    SidebarMenuButtonBuilder, SidebarMenuButtonProps, SidebarMenuButtonSize,
    SidebarMenuButtonVariant, SidebarMenuItem, SidebarMenuItemBuilder, SidebarMenuItemProps,
    SidebarMenuProps, SidebarProps, SidebarProvider, SidebarProviderProps, SidebarRail,
    SidebarRailProps, SidebarSeparator, SidebarSeparatorProps, SidebarSide, SidebarTrigger,
    SidebarTriggerBuilder, SidebarTriggerProps, SidebarVariant,
};
pub use skeleton::{skeleton, Skeleton, SkeletonBuilder, SkeletonProps};
pub use slider::{slider, Slider, SliderBuilder, SliderOrientation, SliderProps};
pub use stepper::{
    stepper, stepper_description, stepper_indicator, stepper_item, stepper_scope, stepper_step,
    stepper_title, use_stepper, Stepper, StepperBuilder, StepperCtx, StepperDescription,
    StepperDescriptionBuilder, StepperDescriptionProps, StepperIndicator, StepperIndicatorBuilder,
    StepperIndicatorProps, StepperItem, StepperItemBuilder, StepperItemProps, StepperOrientation,
    StepperProps, StepperStep, StepperTitle, StepperTitleBuilder, StepperTitleProps,
};
pub use switch::{switch, Switch, SwitchBuilder, SwitchProps};
pub use table::{
    apply_bound_table_align, apply_bound_table_row_selected, data_table, table, table_body,
    table_caption, table_cell, table_comfortable, table_compact, table_footer, table_head,
    table_header, table_row, ui_table, DataTable, DataTableProps, Table, TableAlign, TableBody,
    TableBodyBuilder, TableBodyProps, TableBuilder, TableCaption, TableCaptionBuilder,
    TableCaptionProps, TableCell, TableCellBuilder, TableCellProps, TableColumn, TableDensity,
    TableFooter, TableFooterBuilder, TableFooterProps, TableHead, TableHeadBuilder, TableHeadProps,
    TableHeader, TableHeaderBuilder, TableHeaderProps, TablePaginationState, TableProps, TableRow,
    TableRowBuilder, TableRowProps, TableSortDir, TableSortState, TableViewBuilder,
};
pub use tabs::{
    tabs, tabs_content, tabs_headless, tabs_headless_root, tabs_list, tabs_list_slot, tabs_scope,
    tabs_trigger, use_tabs, Tabs, TabsBuilder, TabsContent, TabsContentProps, TabsCtx,
    TabsHeadlessBuilder, TabsList, TabsListBuilder, TabsListProps, TabsListSlotBuilder,
    TabsOrientation, TabsProps, TabsTrigger, TabsTriggerBuilder, TabsTriggerProps,
    TABSTRIP_FOCUS_PREFIX,
};
pub use textarea::{textarea, Textarea, TextareaBuilder, TextareaProps, TEXTAREA_FOCUS_PREFIX};
pub use timeline::{
    bullet_glyph_px, timeline, timeline_bullet_active, timeline_entry, timeline_item,
    timeline_item_bullet, timeline_item_description, timeline_item_title, timeline_line_active,
    timeline_scope, use_timeline, Timeline, TimelineBuilder, TimelineCtx, TimelineEntry,
    TimelineItem, TimelineItemBuilder, TimelineItemBullet, TimelineItemBulletBuilder,
    TimelineItemBulletProps, TimelineItemDescription, TimelineItemDescriptionBuilder,
    TimelineItemDescriptionProps, TimelineItemProps, TimelineItemTitle, TimelineItemTitleBuilder,
    TimelineItemTitleProps, TimelineProps,
};
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
pub use toggle_group::{
    parse_toggle_group_spacing, toggle_group, toggle_group_item, toggle_group_scope, toggle_option,
    use_toggle_group, ToggleGroup, ToggleGroupBuilder, ToggleGroupCtx, ToggleGroupItem,
    ToggleGroupItemBuilder, ToggleGroupItemProps, ToggleGroupLayout, ToggleGroupOrientation,
    ToggleGroupProps, ToggleGroupSize, ToggleGroupVariant, ToggleOption,
};
pub use tooltip::{
    tooltip, tooltip_bubble, tooltip_content, tooltip_content_slot, tooltip_scope,
    tooltip_side_to_placement, tooltip_trigger, use_tooltip, TooltipBuilder, TooltipContentOpts,
    TooltipContentSlotBuilder, TooltipCtx, TooltipTrigger, TooltipTriggerBuilder,
    TooltipTriggerProps,
};
pub use tree_view::{
    tree_node, tree_view, tree_view_item_scope, tree_view_scope, use_tree_view, use_tree_view_item,
    TreeNode, TreeView, TreeViewBuilder, TreeViewChrome, TreeViewCtx, TreeViewItem,
    TreeViewItemCtx, TreeViewVariant,
};
pub use virtual_list::{virtual_list, VirtualListBuilder};
pub use virtual_masonry::{virtual_masonry, VirtualMasonryBuilder};

//! UI 组件（`#[component]` + `ui!` 中的 PascalCase 标签）。函数名故意与 RSX 一致，故关闭 `non_snake_case` 警告。
#![allow(non_snake_case)]

pub mod base;
pub mod button;
pub mod card;
pub mod checkbox;
pub mod dialog;
pub mod dropdown;
pub mod input;
pub mod label;
pub mod scroll_area;
pub mod scrollbar;
pub mod select;
pub mod separator;
pub mod switch;
pub mod tabs;
pub mod text_node;
pub mod radio_group;
pub mod slider;
pub mod popover;
pub mod toast;
pub mod image;
pub mod portal;
pub mod focus_trap;

pub use base::BaseComponentBuilder;
pub use button::{button, Button, ButtonBuilder, ButtonProps};
pub use card::{Card, CardProps};
pub use checkbox::{checkbox, Checkbox, CheckboxBuilder, CheckboxProps};
pub use dialog::{dialog_panel, DialogBuilder};
pub use dropdown::{dropdown, DropdownBuilder};
pub use input::{
    input, Input, InputBuilder, InputProps, IME_ACTIVE_SIGNAL, INPUT_FOCUS_PREFIX, input_caret_signal_key,
    input_ime_anchor_signal_key, input_overwrite_signal_key, input_selection_signal_key,
};
pub use label::{label, text_node as text, Label, LabelBuilder, LabelProps};
pub use scroll_area::{scroll_area, scrollarea, ScrollArea, ScrollAreaBuilder, ScrollAreaProps};
pub use scrollbar::{scrollbar, scrollbarbuilder, Scrollbar, ScrollbarBuilder, ScrollbarOrientation, ScrollbarProps};
pub use select::{select, Select, SelectBuilder, SelectProps};
pub use separator::{separator, Separator, SeparatorBuilder, SeparatorProps};
pub use switch::{switch, Switch, SwitchBuilder, SwitchProps};
pub use tabs::{tabs_headless_root, Tabs, TabsProps};
pub use radio_group::{radio_group, RadioGroup, RadioGroupBuilder, RadioGroupProps};
pub use slider::{slider, Slider, SliderBuilder, SliderProps};
pub use popover::{popover_panel, PopoverBuilder};
pub use toast::toast;
pub use image::{Image, ImageProps};
pub use portal::{Portal, PortalProps};
pub use focus_trap::{
    FocusTrap, FocusTrapProps, FOCUS_TRAP_OWNER_KEY, FOCUS_TRAP_RING_KEY,
};

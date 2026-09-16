//! # InputAdornment
//!
//! 对位 ShadCN **InputGroup**（`bases/base/ui/input-group.tsx`）：单字段外壳 + 前后缀
//! addon（图标、标签、按钮），**不是**多个 Input 成组（对比 [`super::button_group`]）。
//!
//! ```text
//! InputAdornment                 // role=group · 共享边框 / 焦点环
//! ├── InputAdornmentAddon        // align=inline-start|inline-end|block-start|block-end
//! │   ├── InputAdornmentText     // 静默文案 / 图标槽
//! │ └── InputAdornmentButton // Compact Button in the group
//! ├── InputAdornmentInput        // plain_chrome Input
//! └── InputAdornmentTextarea     // plain_chrome Textarea
//! ```
//!
//! ## Usage (dual style · ADR-9)
//! ```ignore
//! // A · RSX / tags
//! <InputAdornment>
//!     <InputAdornmentAddon align="inline-start">
//!         <InputAdornmentText><Icon name="search" /></InputAdornmentText>
//!     </InputAdornmentAddon>
//!     <InputAdornmentInput id="q" placeholder="Search…" />
//!     <InputAdornmentAddon align="inline-end">
//!         <InputAdornmentButton size="icon-xs" variant="ghost">
//!             <Icon name="info" />
//!         </InputAdornmentButton>
//!     </InputAdornmentAddon>
//! </InputAdornment>
//!
//! // B · Builder
//! Element::from(
//!     input_adornment()
//!         .addon("inline-start", InputAdornmentText(…))
//!         .child(InputAdornmentInput(…))
//!         .addon("inline-end", InputAdornmentButton(…))
//! )
//! ```
//!
//! ## Notes
//! - **展示型外壳**：空 `id` 不写 `template_id`（WP-2.8）；内嵌 Input/Textarea 仍要稳定字段 `id`。
//! - Addon 点击（非 Button）会把焦点交给组内 control（对位 shadcn）。
//! - `<InputAdornment*>` uses general PascalCase → `*Props`, no need for dedicated macro scope.

use crate::components::base::{data_slot, ensure_data_slot, packed_children, SlotChrome};
use crate::components::button::button;
use crate::components::input::input;
use crate::components::textarea::textarea;
use crate::core::reactivity::use_signal;
use crate::core::tree::Element;
use crate::impl_slot_chrome_methods;
use crate::primitives::r#box::{hstack, vstack};
use crate::style::{
    apply_focus_ring_shadow_color, focus_ring_color, focus_ring_destructive_color, parse_color,
    parse_text_color,
};
use crate::tokens;
use crate::utils::take_packed;
use crate::utils::text_node::{build_text_node, TextNodeConfig};
use rui_macros::component;

const ATTR_SLOT: &str = "data-slot";
const SLOT_ROOT: &str = "input-adornment";
const SLOT_ADDON: &str = "input-adornment-addon";
const SLOT_TEXT: &str = "input-adornment-text";
const SLOT_BUTTON: &str = "input-adornment-button";
const SLOT_CONTROL: &str = "input-adornment-control";

const ATTR_ALIGN: &str = "data-align";

/// Addon 相对 control 的视觉位置（对位 shadcn `align`）。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum InputAdornmentAlign {
    #[default]
    InlineStart,
    InlineEnd,
    BlockStart,
    BlockEnd,
}

impl InputAdornmentAlign {
    pub fn parse(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "inline-end" | "end" | "right" => Self::InlineEnd,
            "block-start" | "top" => Self::BlockStart,
            "block-end" | "bottom" => Self::BlockEnd,
            _ => Self::InlineStart,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::InlineStart => "inline-start",
            Self::InlineEnd => "inline-end",
            Self::BlockStart => "block-start",
            Self::BlockEnd => "block-end",
        }
    }

    pub fn is_block(self) -> bool {
        matches!(self, Self::BlockStart | Self::BlockEnd)
    }
}

impl From<&str> for InputAdornmentAlign {
    fn from(s: &str) -> Self {
        Self::parse(s)
    }
}

impl From<String> for InputAdornmentAlign {
    fn from(s: String) -> Self {
        Self::parse(&s)
    }
}
fn slot_of(el: &Element) -> Option<&str> {
    el.attrs.get(ATTR_SLOT).map(String::as_str)
}

fn align_of(el: &Element) -> InputAdornmentAlign {
    el.attrs
        .get(ATTR_ALIGN)
        .map(|s| InputAdornmentAlign::parse(s))
        .unwrap_or_default()
}

fn is_control(el: &Element) -> bool {
    slot_of(el) == Some(SLOT_CONTROL) || el.input_paint_sync.is_some()
}

fn find_control_focus_id(kids: &[Element]) -> Option<String> {
    for k in kids {
        if is_control(k) {
            if let Some(fid) = &k.focus_id {
                return Some(fid.clone());
            }
        }
        if let Some(f) = find_control_focus_id(&k.children) {
            return Some(f);
        }
    }
    None
}

fn control_invalid(kids: &[Element]) -> bool {
    for k in kids {
        if is_control(k) {
            if k.attrs.get("data-invalid").map(|s| s.as_str()) == Some("true") {
                return true;
            }
            if k.input_paint_sync.as_ref().is_some_and(|s| s.invalid) {
                return true;
            }
        }
        if control_invalid(&k.children) {
            return true;
        }
    }
    false
}

fn control_is_textarea(kids: &[Element]) -> bool {
    for k in kids {
        if is_control(k) {
            if k.input_paint_sync.as_ref().is_some_and(|s| s.multiline) {
                return true;
            }
            if k.focus_id
                .as_deref()
                .is_some_and(|f| f.starts_with("textarea:"))
            {
                return true;
            }
        }
        if control_is_textarea(&k.children) {
            return true;
        }
    }
    false
}

fn wire_addon_focus(mut addon: Element, control_fid: Option<&str>) -> Element {
    let Some(fid) = control_fid.map(str::to_string) else {
        return addon;
    };
    // The deepest on_click priority: Button in the group has its own handler and will not fall here.
    let prev = addon.on_click.clone();
    addon = addon.on_click(move || {
        if let Some(ref p) = prev {
            p.borrow_mut()();
        }
        use_signal("__rui_button_focus_id", String::new).set(fid.clone());
        use_signal("__rui_focus_visible", || false).set(true);
    });
    addon
}

fn build_input_adornment(id: &str, children: Vec<Element>) -> Element {
    let mut block_start = Vec::new();
    let mut inline_start = Vec::new();
    let mut controls = Vec::new();
    let mut inline_end = Vec::new();
    let mut block_end = Vec::new();
    let mut other = Vec::new();

    for kid in children {
        match slot_of(&kid) {
            Some(SLOT_ADDON) => match align_of(&kid) {
                InputAdornmentAlign::InlineStart => inline_start.push(kid),
                InputAdornmentAlign::InlineEnd => inline_end.push(kid),
                InputAdornmentAlign::BlockStart => block_start.push(kid),
                InputAdornmentAlign::BlockEnd => block_end.push(kid),
            },
            Some(SLOT_CONTROL) => controls.push(kid),
            _ if is_control(&kid) => controls.push(kid),
            _ => other.push(kid),
        }
    }

    // 未标 slot 的其它子节点夹在 control 附近（容错）。
    controls.extend(other);

    let control_fid = find_control_focus_id(&controls)
        .or_else(|| find_control_focus_id(&inline_start))
        .or_else(|| find_control_focus_id(&inline_end));
    let fid_ref = control_fid.as_deref();

    let wire = |kids: Vec<Element>| -> Vec<Element> {
        kids.into_iter()
            .map(|k| {
                if slot_of(&k) == Some(SLOT_ADDON) {
                    wire_addon_focus(k, fid_ref)
                } else {
                    k
                }
            })
            .collect()
    };
    let block_start = wire(block_start);
    let inline_start = wire(inline_start);
    let inline_end = wire(inline_end);
    let block_end = wire(block_end);

    let has_textarea = control_is_textarea(&controls);
    let invalid = control_invalid(&controls);
    let focused = use_signal("__rui_button_focus_id", String::new).peek();
    let is_focused = control_fid.as_ref().is_some_and(|f| f == &focused);

    let mut row = hstack()
        .w("100%")
        .min_w("0")
        .items_center()
        .gap("0")
        .flex_grow(1.0);
    for k in inline_start {
        row = row.child(k);
    }
    for k in controls {
        row = row.child(k);
    }
    for k in inline_end {
        row = row.child(k);
    }

    let has_block = !block_start.is_empty() || !block_end.is_empty();
    let body: Element = if has_block {
        let mut col = vstack().w("100%").min_w("0").gap("0").items("stretch");
        for k in block_start {
            col = col.child(k);
        }
        col = col.child(row);
        for k in block_end {
            col = col.child(k);
        }
        col.into()
    } else {
        row.into()
    };

    let border_c = if invalid {
        parse_color("destructive")
    } else if is_focused {
        crate::style::focus_ring_border_color()
    } else {
        parse_color("input")
    };

    let mut shell: Element = if has_textarea || has_block {
        vstack()
            .w("100%")
            .min_w("0")
            .h("auto")
            .rounded("0.375rem")
            .items("stretch")
            .overflow("hidden")
            .into()
    } else {
        hstack()
            .w("100%")
            .min_w("0")
            .h("2.25rem")
            .rounded("0.375rem")
            .items_center()
            .overflow("hidden")
            .into()
    };

    shell = shell.bg(parse_color("transparent")).border(border_c, 1.0);
    if is_focused {
        let ring = if invalid {
            focus_ring_destructive_color()
        } else {
            focus_ring_color()
        };
        apply_focus_ring_shadow_color(&mut shell, true, false, ring);
    } else {
        shell = shell.shadow(tokens::shadow_rgba(tokens::Elevation::Xs), 1.0);
    }

    shell = shell.child(body);
    shell.set_attr(ATTR_SLOT, SLOT_ROOT);
    if let Some(ref fid) = control_fid {
        shell.set_attr("rui_ia_control_fid", fid.as_str());
    }
    shell = shell.accesskit_role("group");
    if !id.trim().is_empty() {
        shell.template_id = Some(id.to_string());
    }

    // 每帧同步外壳边框 / 焦点环（内嵌 control 为 plain_chrome）。
    shell = shell.reactive_sync(move |el, _layout| sync_adornment_shell_chrome(el));
    shell
}

fn sync_adornment_shell_chrome(el: &mut Element) -> crate::core::tree::LayoutStatus {
    use crate::core::tree::LayoutStatus;
    use crate::style::{
        apply_focus_ring_shadow_color, focus_ring_border_color, focus_ring_destructive_color,
        focus_ring_shadow_matches_color, parse_color,
    };

    let fid = el
        .attrs
        .get("rui_ia_control_fid")
        .cloned()
        .unwrap_or_default();
    let focused = use_signal("__rui_button_focus_id", String::new).peek();
    let is_focused = !fid.is_empty() && focused == fid;
    let invalid = control_invalid(&el.children);

    let border_c = if invalid {
        parse_color("destructive")
    } else if is_focused {
        focus_ring_border_color()
    } else {
        parse_color("input")
    };

    let mut status = LayoutStatus::Clean;
    if el.style.border_color != border_c {
        el.style.border_color = border_c;
        status = status.merge(LayoutStatus::StyleChanged);
    }

    if is_focused {
        let ring = if invalid {
            focus_ring_destructive_color()
        } else {
            focus_ring_color()
        };
        if !focus_ring_shadow_matches_color(el, true, ring) {
            apply_focus_ring_shadow_color(el, true, false, ring);
            // Outset ring is paint chrome. LayoutChanged here sets
            // `page_layout_changed` → window FullClear (Gallery Kbd Search
            // leftover glyphs + Marker spin jump).
            status = status.merge(LayoutStatus::StyleChanged);
        }
    } else {
        let want_color = tokens::shadow_rgba(tokens::Elevation::Xs);
        let want_blur = 1.0;
        let geom_ok = (el.style.shadow_blur - want_blur).abs() <= 0.01
            && el.style.shadow_spread.abs() <= 0.01
            && el.style.shadow_offset == [0.0, 0.0]
            && !el.style.shadow_inset;
        let color_ok = (el.style.shadow_color[0] - want_color[0]).abs() <= 0.001
            && (el.style.shadow_color[1] - want_color[1]).abs() <= 0.001
            && (el.style.shadow_color[2] - want_color[2]).abs() <= 0.001
            && (el.style.shadow_color[3] - want_color[3]).abs() <= 0.001;
        if !geom_ok || !color_ok {
            el.style.shadow_color = want_color;
            el.style.shadow_blur = want_blur;
            el.style.shadow_offset = [0.0, 0.0];
            el.style.shadow_spread = 0.0;
            el.style.shadow_inset = false;
            status = status.merge(LayoutStatus::StyleChanged);
        }
    }
    if status != LayoutStatus::Clean {
        crate::app::damage::push_frame_paint_element(el, el.paint_scroll_acc);
    }
    status
}

// ---------------------------------------------------------------------------
// RSX parts
// ---------------------------------------------------------------------------

/// 根：共享边框 / 焦点环 · 按 Addon `align` 重排子项。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn InputAdornment(id: String, children: Option<Element>) -> Element {
    let kids = children.map(take_packed).unwrap_or_default();
    build_input_adornment(&id, kids)
}

/// 前后缀槽。`align`：`inline-start`（默认）/ `inline-end` / `block-start` / `block-end`。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn InputAdornmentAddon(align: Option<String>, children: Option<Element>) -> Element {
    let align = align
        .as_deref()
        .map(InputAdornmentAlign::parse)
        .unwrap_or_default();
    let kids = children.map(take_packed).unwrap_or_default();

    let mut row = match align {
        InputAdornmentAlign::BlockStart | InputAdornmentAlign::BlockEnd => hstack()
            .w("100%")
            .items_center()
            .justify("start")
            .gap("0.5rem")
            .px("0.75rem")
            .py("0.5rem")
            .flex_shrink(0.0),
        InputAdornmentAlign::InlineStart => hstack()
            .items_center()
            .justify_center()
            .gap("0.5rem")
            .pl("0.75rem")
            .pr("0.25rem")
            .py("0.25rem")
            .flex_shrink(0.0)
            .h("100%"),
        InputAdornmentAlign::InlineEnd => hstack()
            .items_center()
            .justify_center()
            .gap("0.5rem")
            .pr("0.75rem")
            .pl("0.25rem")
            .py("0.25rem")
            .flex_shrink(0.0)
            .h("100%"),
    };

    for k in kids {
        row = row.child(k);
    }

    let mut el: Element = row.into();
    el.set_attr(ATTR_SLOT, SLOT_ADDON);
    el.set_attr(ATTR_ALIGN, align.as_str());
    el = el.accesskit_role("group").cursor_text();
    el
}

/// 组内静默文案 / 图标容器（muted）。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn InputAdornmentText(children: Option<Element>) -> Element {
    let kids = children.map(take_packed).unwrap_or_default();
    let mut row = hstack().items_center().gap("0.25rem").flex_shrink(0.0);

    if kids.is_empty() {
        // no-op
    } else if kids.len() == 1 && kids[0].text.as_ref().is_some_and(|t| !t.is_empty()) {
        let label = kids[0].text.clone().unwrap_or_default();
        let caption = build_text_node(
            label,
            TextNodeConfig {
                size_px: 14.0,
                weight: 500,
                color: parse_text_color("muted-foreground"),
                ..Default::default()
            },
        );
        row = row.child(caption);
    } else {
        for mut k in kids {
            // 图标 / 文案默认 muted
            if k.text_style.color == Some([0, 0, 0, 0]) || k.text.is_some() {
                if k.text_style.font_family.as_deref() == Some("lucide") || k.text.is_some() {
                    k = k.text_color(parse_text_color("muted-foreground"));
                }
            }
            row = row.child(k);
        }
    }

    let mut el: Element = row.into();
    el.set_attr(ATTR_SLOT, SLOT_TEXT);
    el = el.pointer_events("none");
    el
}

/// Compact buttons within the group (default `variant=ghost` · `size=xs`).
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn InputAdornmentButton(
    id: String,
    variant: Option<String>,
    size: Option<String>,
    disabled: bool,
    mut on_click: crate::core::tree::Handler,
    children: Option<Element>,
) -> Element {
    let variant = variant.unwrap_or_else(|| "ghost".into());
    let size = size.unwrap_or_else(|| "xs".into());
    let mut b = button(&id)
        .variant(variant.as_str())
        .size(size.as_str())
        .disabled(disabled);
    if let Some(c) = children {
        b = b.child(c);
    }
    let mut el: Element = b.into();
    el.set_attr(ATTR_SLOT, SLOT_BUTTON);
    if !disabled {
        let mut old = el.on_click.clone();
        el = el.on_click(move || {
            if let Some(ref mut rc) = old {
                rc.borrow_mut()();
            }
            (on_click.0)(());
        });
    }
    el
}

/// 组内单行 input（`plain_chrome`）。
#[component(crate_path = "crate")]
pub fn InputAdornmentInput(
    id: String,
    placeholder: String,
    disabled: bool,
    invalid: bool,
    clearable: bool,
    password_toggle: bool,
    number_steppers: bool,
    input_type: Option<String>,
    step: Option<f32>,
    value: Option<crate::core::reactivity::Signal<String>>,
    children: Option<Element>,
) -> Element {
    let _ = children;
    let mut b = input(&id)
        .placeholder(placeholder)
        .disabled(disabled)
        .invalid(invalid)
        .clearable(clearable)
        .password_toggle(password_toggle)
        .number_steppers(number_steppers)
        .plain_chrome(true);
    if let Some(t) = input_type {
        b = b.input_type(t);
    }
    if let Some(s) = step {
        b = b.step(s);
    }
    if let Some(sig) = value {
        b = b.value_signal(sig);
    }
    let mut el: Element = b.into();
    el.set_attr(ATTR_SLOT, SLOT_CONTROL);
    if invalid {
        el.set_attr("data-invalid", "true");
    }
    el
}

///Multiple lines of input in the group (`plain_chrome`; default off resize).
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn InputAdornmentTextarea(
    id: String,
    placeholder: String,
    rows: Option<i32>,
    disabled: bool,
    invalid: bool,
    value: Option<crate::core::reactivity::Signal<String>>,
    children: Option<Element>,
) -> Element {
    let _ = children;
    let mut b = textarea(&id)
        .placeholder(placeholder)
        .disabled(disabled)
        .invalid(invalid)
        .resizable(false)
        .plain_chrome(true);
    if let Some(r) = rows {
        b = b.rows(r.max(1) as u32);
    }
    if let Some(sig) = value {
        b = b.value_signal(sig);
    }
    let mut el: Element = b.into();
    el.set_attr(ATTR_SLOT, SLOT_CONTROL);
    if invalid {
        el.set_attr("data-invalid", "true");
    }
    el
}

// ---------------------------------------------------------------------------
// Builder (ADR-9 surface B)
// ---------------------------------------------------------------------------

pub struct InputAdornmentAddonBuilder {
    align: InputAdornmentAlign,
    children: Vec<Element>,
    chrome: SlotChrome,
}

pub fn input_adornment_addon() -> InputAdornmentAddonBuilder {
    InputAdornmentAddonBuilder {
        align: InputAdornmentAlign::default(),
        children: Vec::new(),
        chrome: SlotChrome::default(),
    }
}

impl InputAdornmentAddonBuilder {
    impl_slot_chrome_methods!();

    pub fn align(mut self, align: impl Into<InputAdornmentAlign>) -> Self {
        self.align = align.into();
        self
    }

    pub fn child(mut self, child: impl Into<Element>) -> Self {
        self.children.extend(take_packed(child.into()));
        self
    }
}

impl crate::core::tree::IntoElementList for InputAdornmentAddonBuilder {
    fn into_element_list(self) -> Vec<Element> {
        vec![self.into()]
    }
}

impl From<InputAdornmentAddonBuilder> for Element {
    fn from(builder: InputAdornmentAddonBuilder) -> Self {
        let el = InputAdornmentAddon(InputAdornmentAddonProps {
            align: Some(builder.align.as_str().to_string()),
            children: packed_children(builder.children),
            base: None,
            ..Default::default()
        });
        builder.chrome.apply(el)
    }
}

pub struct InputAdornmentTextBuilder {
    children: Vec<Element>,
    chrome: SlotChrome,
}

pub fn input_adornment_text() -> InputAdornmentTextBuilder {
    InputAdornmentTextBuilder {
        children: Vec::new(),
        chrome: SlotChrome::default(),
    }
}

impl InputAdornmentTextBuilder {
    impl_slot_chrome_methods!();

    pub fn child(mut self, child: impl Into<Element>) -> Self {
        self.children.extend(take_packed(child.into()));
        self
    }

    pub fn text(mut self, label: impl Into<String>) -> Self {
        self.children.push(Element::new().text(label.into()));
        self
    }
}

impl crate::core::tree::IntoElementList for InputAdornmentTextBuilder {
    fn into_element_list(self) -> Vec<Element> {
        vec![self.into()]
    }
}

impl From<InputAdornmentTextBuilder> for Element {
    fn from(builder: InputAdornmentTextBuilder) -> Self {
        let el = InputAdornmentText(InputAdornmentTextProps {
            children: packed_children(builder.children),
            base: None,
            ..Default::default()
        });
        builder.chrome.apply(el)
    }
}

/// Builder state.
pub struct InputAdornmentBuilder {
    id: String,
    chrome: SlotChrome,
    parts: Vec<Element>,
}

/// Builder entry.
pub fn input_adornment() -> InputAdornmentBuilder {
    InputAdornmentBuilder {
        id: String::new(),
        chrome: SlotChrome::default(),
        parts: Vec::new(),
    }
}

impl InputAdornmentBuilder {
    impl_slot_chrome_methods!();

    pub fn id(mut self, id: impl Into<String>) -> Self {
        self.id = id.into();
        self
    }

    pub fn child(mut self, el: impl Into<Element>) -> Self {
        self.parts.push(el.into());
        self
    }

    /// Convenience: build an Addon with `align`, then append.
    pub fn addon(
        mut self,
        align: impl Into<InputAdornmentAlign>,
        body: impl Into<Element>,
    ) -> Self {
        let align = align.into();
        let body = body.into();
        let addon = if data_slot(&body) == Some(SLOT_ADDON) {
            body
        } else {
            input_adornment_addon().align(align).child(body).into()
        };
        self.parts.push(addon);
        self
    }

    pub fn addon_with(
        mut self,
        align: impl Into<InputAdornmentAlign>,
        f: impl FnOnce(InputAdornmentAddonBuilder) -> InputAdornmentAddonBuilder,
    ) -> Self {
        let addon = f(input_adornment_addon().align(align)).into();
        self.parts
            .push(ensure_data_slot(addon, SLOT_ADDON, |el| el));
        self
    }

    pub fn text(mut self, label: impl Into<String>) -> Self {
        let el = input_adornment_text()
            .child(Element::new().text(label.into()))
            .into();
        self.parts
            .push(ensure_data_slot(el, SLOT_TEXT, |inner| inner));
        self
    }

    pub fn text_with(
        mut self,
        f: impl FnOnce(InputAdornmentTextBuilder) -> InputAdornmentTextBuilder,
    ) -> Self {
        let el = f(input_adornment_text()).into();
        self.parts
            .push(ensure_data_slot(el, SLOT_TEXT, |inner| inner));
        self
    }
}

impl From<InputAdornmentBuilder> for Element {
    fn from(b: InputAdornmentBuilder) -> Self {
        let el = build_input_adornment(&b.id, b.parts);
        b.chrome.apply(el)
    }
}

impl crate::core::tree::IntoElementList for InputAdornmentBuilder {
    fn into_element_list(self) -> Vec<Element> {
        vec![self.into()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::reactivity::create_root;

    #[test]
    fn align_parse() {
        assert_eq!(
            InputAdornmentAlign::parse("inline-end"),
            InputAdornmentAlign::InlineEnd
        );
        assert_eq!(
            InputAdornmentAlign::parse("block-start"),
            InputAdornmentAlign::BlockStart
        );
        assert_eq!(
            InputAdornmentAlign::parse(""),
            InputAdornmentAlign::InlineStart
        );
    }

    #[test]
    fn dual_surface_compiles_shape() {
        create_root(|| {
            let rsx = InputAdornment(InputAdornmentProps {
                id: String::new(),
                children: Some({
                    let mut frag = Element::new();
                    frag.set_attr("rui_fragment", "1");
                    frag.children(vec![
                        InputAdornmentAddon(InputAdornmentAddonProps {
                            align: Some("inline-start".into()),
                            children: Some(InputAdornmentText(InputAdornmentTextProps {
                                children: Some(Element::new().text("https://")),
                                base: None,
                                ..Default::default()
                            })),
                            base: None,
                            ..Default::default()
                        }),
                        InputAdornmentInput(InputAdornmentInputProps {
                            id: "t-ia-in".into(),
                            placeholder: Some("example.com".into()),
                            ..Default::default()
                        }),
                    ])
                }),
                base: None,
                ..Default::default()
            });
            let built: Element = input_adornment()
                .addon(
                    "inline-start",
                    InputAdornmentText(InputAdornmentTextProps {
                        children: Some(Element::new().text("https://")),
                        base: None,
                        ..Default::default()
                    }),
                )
                .child(
                    input("t-ia-b")
                        .placeholder("example.com")
                        .plain_chrome(true),
                )
                .into();
            assert_eq!(
                rsx.attrs.get(ATTR_SLOT).map(String::as_str),
                Some(SLOT_ROOT)
            );
            assert_eq!(
                built.attrs.get(ATTR_SLOT).map(String::as_str),
                Some(SLOT_ROOT)
            );
        });
    }

    #[test]
    fn reorder_puts_inline_start_before_control() {
        create_root(|| {
            let el: Element = InputAdornment(InputAdornmentProps {
                id: String::new(),
                children: Some({
                    let mut frag = Element::new();
                    frag.set_attr("rui_fragment", "1");
                    // DOM: control first, addon after (matches shadcn); visual order still start → control
                    frag.children(vec![
                        InputAdornmentInput(InputAdornmentInputProps {
                            id: "t-ia-order".into(),
                            ..Default::default()
                        }),
                        InputAdornmentAddon(InputAdornmentAddonProps {
                            align: Some("inline-start".into()),
                            children: Some(Element::new().text("pre")),
                            base: None,
                            ..Default::default()
                        }),
                    ])
                }),
                base: None,
                ..Default::default()
            });
            let row = el.children.first().expect("shell child");
            assert!(
                row.children.len() >= 2,
                "expected addon + control, got {}",
                row.children.len()
            );
            assert_eq!(
                row.children[0].attrs.get(ATTR_SLOT).map(String::as_str),
                Some(SLOT_ADDON)
            );
            assert_eq!(
                row.children[1].attrs.get(ATTR_SLOT).map(String::as_str),
                Some(SLOT_CONTROL)
            );
        });
    }
}

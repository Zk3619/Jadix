//! # Label
//!
//! 文本 / 标签节点：字号、字重、颜色、行高、装饰与常用布局透传。
//! RSX 标签为 `<Label>`；Builder 入口为 [`label`]，旧别名 [`text_node`]（re-export 为 `text`）。
//!
//! ## What it is for
//! 界面文案、标题、表单旁注等静态或轻量文本，不是可编辑输入。
//!
//! ## Usage
//! ```ignore
//! <Label text_size="0.875" font_weight=600>"标题"</Label>
//! <Label id="hint" text_color="#888">"说明文字"</Label>
//! // 关联控件（Solid `for`）：点击标签转发到 `focus_id` 匹配的控件
//! <Checkbox id="terms" />
//! <Label for="terms">"Accept terms"</Label>
//! // 组合文案请写在 Checkbox 内：`<Checkbox>"Accept"</Checkbox>` 或内嵌 `<Label>`
//! // Element::from(label("t").text("Hello").text_size(14.0).for_target("terms"))
//! ```
//!
//! 字号 / 颜色 / 字重走框架 `text_size` / `text_color`（RSX 也可用 `color`）/ `font_weight`，
//! 不要再给 Label 开 `size` / `color` / `weight` prop。Builder 仍保留 `.size()` / `.color()` 别名。
//!
//! ## Notes
//! 默认经 [`crate::utils::text_node::apply_label_defaults`]：
//! 约 14px / weight 500 / leading-none / `min_w=0`。非空 `id` 才写入 `template_id`（I-1：禁 `Some("")`）。
//! `for` 写入 `attrs["for"]`；指针命中时由 hit path 转发到目标 `on_click`。
//! 关联时写入稳定 `accesskit_id`（`lbl:for:…`），桥侧给目标补 `labelled_by`。

use crate::components::base::SlotChrome;
use crate::core::tree::{Element, IntoCssDim, IntoF32, IntoNodeBg, IntoNodeFg, IntoNodeSize};
use crate::primitives::text::TextBuilder;
use crate::utils::text_node::apply_label_defaults;

#[cfg(all(test, feature = "product-tests"))]
use crate::core::tree::LayoutStatus;
#[cfg(all(test, feature = "product-tests"))]
use crate::style::InputCaretDraw;
#[cfg(all(test, feature = "product-tests"))]
pub(crate) use crate::utils::text_selection::sync_label_selection_paint_subtree;
pub use crate::utils::text_selection::{
    label_selection_signal_key, selectable_text_key, try_copy_label_selection,
    LABEL_SELECTION_COPY_SIGNAL,
};

/// `attrs` 键：Label → 控件关联（Solid / HTML 原生属性名 `for`）。
pub use crate::utils::text_selection::ATTR_LABEL_FOR;

/// Label 关联控件时的稳定 AccessKit / `aria-labelledby` 目标键。
pub fn label_accesskit_id_for(for_target: &str) -> String {
    format!("lbl:for:{for_target}")
}

/// Solid `for`: Write attrs, and complete `accesskit_id` for reading screen `labelled_by`.
fn apply_label_for(el: &mut Element, for_id: &str) {
    let for_id = for_id.trim();
    if for_id.is_empty() {
        return;
    }
    // A text-only label owns its caption, not the width of a longer sibling
    // description. Container labels and explicitly authored widths remain boxes.
    if el.children.is_empty()
        && el.text.as_deref().is_some_and(|text| !text.is_empty())
        && el.style.size.width == taffy::style::Dimension::auto()
        && !el.style.unit_specs.iter().any(|(slot, _)| {
            matches!(
                slot,
                crate::style::UnitSlot::Width | crate::style::UnitSlot::InlineSize
            )
        })
    {
        *el = std::mem::take(el).w("fit-content");
    }
    el.attrs
        .insert(ATTR_LABEL_FOR.to_string(), for_id.to_string());
    if el
        .attrs
        .get("accesskit_id")
        .map(|s| s.as_str())
        .is_none_or(|s| s.is_empty())
    {
        el.attrs
            .insert("accesskit_id".to_string(), label_accesskit_id_for(for_id));
    }
    if el.cursor.is_none() {
        el.cursor = Some(crate::core::tree::CursorKind::Pointer.into());
    }
}

pub struct LabelBuilder {
    pub(crate) id: String,
    pub(crate) for_target: String,
    pub(crate) inner: TextBuilder,
    chrome: SlotChrome,
}

crate::__component_contract! {
LabelProps [];
/// RSX `<Label>`：见模块文档。
#[allow(non_snake_case)]
pub fn Label(
identity [] id: String,
default [] text: String,
default [] center: bool,
default [] for_target: String,
default [] truncate: bool,
plain [] text_overflow: Option<String>,
plain [] children: Option<Element>,
) -> Element {
    let mut builder = label(&id).text(text).for_target(for_target);
    if center {
        builder = builder.text_center();
    }
    if truncate {
        builder = builder.truncate(true);
    } else if let Some(ref mode) = text_overflow {
        builder = builder.text_overflow(mode.as_str());
    }
    if let Some(c) = children {
        builder = builder.child(c);
    }
    builder.into()
}
}

pub fn label(id: impl Into<String>) -> LabelBuilder {
    let id = id.into();
    let mut el = apply_label_defaults(Element::new());
    if !id.trim().is_empty() {
        // Bindings register before `From<LabelBuilder>` runs. Seed the explicit
        // identity now so repeated helper/list sites do not share an auto id.
        el.template_id = Some(id.clone());
    }
    LabelBuilder {
        id,
        for_target: String::new(),
        inner: TextBuilder { el },
        chrome: SlotChrome::default(),
    }
}

// Legacy alias for existing <Text> tags.
pub fn text_node(id: impl Into<String>) -> LabelBuilder {
    label(id)
}

impl LabelBuilder {
    crate::impl_slot_chrome_methods!();

    #[track_caller]
    pub fn text(mut self, content: impl crate::core::tree::IntoNodeText) -> Self {
        self.inner = self.inner.text(content);
        self
    }

    pub fn size(mut self, size: impl IntoF32) -> Self {
        self.inner = self.inner.size(size);
        self
    }

    pub fn text_size(mut self, size: impl IntoF32) -> Self {
        self.inner = self.inner.size(size);
        self
    }

    pub fn font_weight(mut self, weight: impl crate::core::tree::IntoF32Raw) -> Self {
        self.inner = self.inner.weight(weight);
        self
    }

    pub fn font_family(mut self, family: impl Into<String>) -> Self {
        self.inner = self.inner.family(family);
        self
    }

    #[track_caller]
    pub fn color(mut self, color: impl IntoNodeFg) -> Self {
        self.inner.el = color.apply_node_fg(self.inner.el);
        self
    }

    #[track_caller]
    pub fn text_color(mut self, color: impl IntoNodeFg) -> Self {
        self.inner.el = color.apply_node_fg(self.inner.el);
        self
    }

    pub fn text_align(mut self, align: impl Into<crate::style::TextAlignDeclaration>) -> Self {
        self.inner = self.inner.text_align(align);
        self
    }

    pub fn text_center(self) -> Self {
        self.text_align("center")
    }

    pub fn line_height(mut self, line_height: impl IntoF32) -> Self {
        self.inner = self.inner.line_height(line_height);
        self
    }

    pub fn leading(mut self, scale: impl crate::core::tree::IntoF32Raw) -> Self {
        self.inner = self.inner.leading(scale);
        self
    }

    pub fn underline(mut self, underline: bool) -> Self {
        self.inner = self.inner.underline(underline);
        self
    }

    // Pass-through some layout/base methods from Element
    #[track_caller]
    pub fn w(mut self, w: impl IntoNodeSize) -> Self {
        self.inner.el = w.apply_node_size(self.inner.el, "w");
        self
    }
    #[track_caller]
    pub fn h(mut self, h: impl IntoNodeSize) -> Self {
        self.inner.el = h.apply_node_size(self.inner.el, "h");
        self
    }
    #[track_caller]
    pub fn bg(mut self, bg: impl IntoNodeBg) -> Self {
        self.inner.el = bg.apply_node_bg(self.inner.el);
        self
    }
    pub fn p(mut self, p: impl IntoCssDim) -> Self {
        self.inner.el = self.inner.el.p(p);
        self
    }
    pub fn px(mut self, p: impl IntoCssDim) -> Self {
        self.inner.el = self.inner.el.px(p);
        self
    }
    pub fn py(mut self, p: impl IntoCssDim) -> Self {
        self.inner.el = self.inner.el.py(p);
        self
    }
    /// 文案叶自带 padding 且 `text-align:center` 时请用 ContentBox（见 `docs/engine/box_model.md`）。
    pub fn content_box(mut self) -> Self {
        self.inner.el = self.inner.el.content_box();
        self
    }
    pub fn border_box(mut self) -> Self {
        self.inner.el = self.inner.el.border_box();
        self
    }
    pub fn pt(mut self, p: impl IntoCssDim) -> Self {
        self.inner.el = self.inner.el.pt(p);
        self
    }
    pub fn pr(mut self, p: impl IntoCssDim) -> Self {
        self.inner.el = self.inner.el.pr(p);
        self
    }
    pub fn pb(mut self, p: impl IntoCssDim) -> Self {
        self.inner.el = self.inner.el.pb(p);
        self
    }
    pub fn pl(mut self, p: impl IntoCssDim) -> Self {
        self.inner.el = self.inner.el.pl(p);
        self
    }
    pub fn m(mut self, m: impl IntoCssDim) -> Self {
        self.inner.el = self.inner.el.m(m);
        self
    }
    pub fn mx(mut self, m: impl IntoCssDim) -> Self {
        self.inner.el = self.inner.el.mx(m);
        self
    }
    pub fn my(mut self, m: impl IntoCssDim) -> Self {
        self.inner.el = self.inner.el.my(m);
        self
    }
    pub fn mt(mut self, m: impl IntoCssDim) -> Self {
        self.inner.el = self.inner.el.mt(m);
        self
    }
    pub fn mr(mut self, m: impl IntoCssDim) -> Self {
        self.inner.el = self.inner.el.mr(m);
        self
    }
    pub fn mb(mut self, m: impl IntoCssDim) -> Self {
        self.inner.el = self.inner.el.mb(m);
        self
    }
    pub fn ml(mut self, m: impl IntoCssDim) -> Self {
        self.inner.el = self.inner.el.ml(m);
        self
    }
    pub fn child(mut self, child: impl Into<Element>) -> Self {
        self.inner = self.inner.child(child);
        self
    }
    pub fn children(mut self, children: impl IntoIterator<Item = Element>) -> Self {
        for c in children {
            self = self.child(c);
        }
        self
    }
    pub fn text_decoration(mut self, decoration: impl Into<String>) -> Self {
        self.inner.el = self.inner.el.text_decoration(decoration);
        self
    }
    pub fn text_decoration_line(mut self, line: impl Into<String>) -> Self {
        self.inner.el = self.inner.el.text_decoration_line(line);
        self
    }
    pub fn text_decoration_style(mut self, style: impl Into<String>) -> Self {
        self.inner.el = self.inner.el.text_decoration_style(style);
        self
    }
    pub fn text_decoration_color(mut self, color: [u8; 4]) -> Self {
        self.inner.el = self.inner.el.text_decoration_color(color);
        self
    }
    pub fn text_decoration_thickness(mut self, thickness: impl IntoF32) -> Self {
        self.inner.el = self.inner.el.text_decoration_thickness(thickness);
        self
    }
    pub fn text_underline_offset(mut self, offset: impl IntoF32) -> Self {
        self.inner.el = self.inner.el.text_underline_offset(offset);
        self
    }

    pub fn text_line_height(mut self, line_height: impl IntoF32) -> Self {
        self.inner = self.inner.line_height(line_height);
        self
    }

    pub fn text_leading(mut self, scale: impl crate::core::tree::IntoF32Raw) -> Self {
        self.inner = self.inner.leading(scale);
        self
    }

    pub fn text_leading_none(mut self) -> Self {
        self.inner = self.inner.leading(1.0);
        self
    }

    pub fn text_leading_tight(mut self) -> Self {
        self.inner = self.inner.leading(1.25);
        self
    }

    pub fn text_leading_normal(mut self) -> Self {
        self.inner = self.inner.leading(1.5);
        self
    }

    pub fn text_leading_relaxed(mut self) -> Self {
        self.inner = self.inner.leading(1.625);
        self
    }

    pub fn text_overflow_ellipsis(mut self) -> Self {
        self.inner.el = self.inner.el.text_overflow_ellipsis();
        self
    }

    pub fn text_overflow_clip(mut self) -> Self {
        self.inner.el = self.inner.el.text_overflow_clip();
        self
    }

    pub fn text_overflow_hidden(mut self) -> Self {
        self.inner.el = self.inner.el.text_overflow_hidden();
        self
    }

    pub fn text_overflow_newline(mut self) -> Self {
        self.inner.el = self.inner.el.text_overflow_newline();
        self
    }

    pub fn text_overflow(mut self, overflow: impl Into<crate::style::TextOverflow>) -> Self {
        self.inner.el = self.inner.el.text_overflow(overflow);
        self
    }

    /// Same as `text-overflow: ellipsis` / `truncate=true`.
    pub fn truncate(mut self, on: bool) -> Self {
        if on {
            self.inner.el = self.inner.el.text_overflow_ellipsis();
        }
        self
    }

    pub fn selectable(mut self, selectable: bool) -> Self {
        self.inner.el = self.inner.el.selectable(selectable);
        if selectable {
            let sel_id = crate::utils::resolve_instance_id(self.id.clone(), "label");
            self.id = sel_id.clone();
            enable_label_selection(&mut self.inner.el, &sel_id);
            self.inner.el = self.inner.el.cursor_text();
        }
        self
    }

    /// 关联控件 `focus_id`（RSX `<Label for="…">` 映射到此 prop）。
    pub fn for_target(mut self, target_id: impl Into<String>) -> Self {
        self.for_target = target_id.into();
        self
    }

    /// [`for_target`] 的别名（保留 `for` 关键字逃逸写法）。
    #[inline]
    pub fn r#for(self, target_id: impl Into<String>) -> Self {
        self.for_target(target_id)
    }

    /// 可选 `template_id` 槽；空串 / 省略 → 不写身份（展示型，WP-2.8）。
    /// 在 live 绑定前设置；已注册的绑定或子叶身份不会被改名。
    pub fn id(mut self, id: impl Into<String>) -> Self {
        if self.inner.el.on_unmount.is_none()
            && self.inner.el.template_id.as_deref() == Some(self.id.as_str())
        {
            self.inner.el.template_id = None;
        }
        self.id = id.into();
        if self.inner.el.template_id.is_none() && !self.id.trim().is_empty() {
            self.inner.el.template_id = Some(self.id.clone());
        }
        self
    }
}

impl From<LabelBuilder> for Element {
    fn from(builder: LabelBuilder) -> Element {
        let mut el: Element = builder.inner.into();
        // Prefer Solid-style bind identity adopted from a text child over the label's auto id.
        // Display labels: omit template_id when user id is empty (I-1 — never Some("")).
        if el.template_id.is_none() && !builder.id.trim().is_empty() {
            el.template_id = Some(builder.id);
        }
        // Static text enters AccessKit as Label (not Tab-focusable); see `a11y::bridge` + a11y_smoke.
        if !el.attrs.contains_key("accesskit_role") {
            el = el.accesskit_role("label");
        }
        let for_id = builder.for_target.trim().to_string();
        apply_label_for(&mut el, &for_id);
        builder.chrome.apply(el)
    }
}

/// Enable static label text selection (MVP scaffolding).
///
/// Sets `text_style.selectable`, assigns `focus_id`, and marks the element for future
/// paint sync (`input_caret` selection path when a caret draw sync is installed).
pub fn enable_label_selection(el: &mut Element, focus_id: impl Into<String>) {
    let id = crate::utils::resolve_instance_id(focus_id, "label");
    el.focus_id = Some(id.clone());
    el.text_style.selectable = true;
    el.cursor = Some(crate::core::tree::CursorKind::Text.into());
    el.attrs
        .insert("text_selectable".to_string(), "1".to_string());
    el.attrs
        .insert(format!("__jadix_label_sel:{id}"), "1".to_string());
}

impl crate::core::tree::IntoElementList for LabelBuilder {
    fn into_element_list(self) -> Vec<Element> {
        vec![self.into()]
    }
}

#[cfg(all(test, feature = "product-tests"))]
mod tests {
    use super::*;
    use crate::core::reactivity::use_signal;
    use crate::core::tree::Element;

    #[test]
    fn label_selection_sync_does_not_wipe_input_caret() {
        let mut text = Element::new().text("hello");
        text.template_id = Some("__jadix_input:field:text".into());
        text.text_style.input_caret = Some(InputCaretDraw {
            caret_byte_index: 5,
            selection: Some((0, 5)),
            selection_bg: [80, 120, 200, 90],
            overwrite_highlight: None,
            preedit_underline: None,
            extra_carets: Vec::new(),
            extra_selections: Vec::new(),
            extra_preedit_underlines: Vec::new(),
            preedit_ul: Vec::new(),
            drop_caret: false,
            caret_color: None,
            selection_foreground: None,
        });
        let mut root = Element::new().child(text);
        let mut impact = LayoutStatus::Clean;
        sync_label_selection_paint_subtree(&mut root, &mut impact);
        assert!(
            root.children[0].text_style.input_caret.is_some(),
            "Input-owned input_caret must survive label selection sync"
        );
    }

    #[test]
    fn selectable_label_records_selection_range() {
        let key = "lbl1";
        let sk = label_selection_signal_key(key);
        use_signal(&sk, || None::<(usize, usize)>).set(Some((1, 4)));
        assert_eq!(
            use_signal(&sk, || None::<(usize, usize)>).get(),
            Some((1, 4))
        );
    }

    #[test]
    fn non_selectable_label_does_not_participate_in_selection() {
        let el = Element::new().text("hello").selectable(false);
        assert!(selectable_text_key(&el).is_none());

        let mut el2 = Element::new().text("hello").selectable(true);
        el2.template_id = Some("t2".into());
        assert_eq!(selectable_text_key(&el2).as_deref(), Some("t2"));

        let mut el3 = Element::new().text("hello").selectable(true);
        enable_label_selection(&mut el3, "focus-me");
        assert_eq!(selectable_text_key(&el3).as_deref(), Some("focus-me"));
    }

    #[test]
    fn copy_label_selection_updates_global_signal() {
        let mut root = Element::new();
        let mut lbl = Element::new().text("hello world").selectable(true);
        enable_label_selection(&mut lbl, "copy-lbl");
        root.children.push(lbl);

        let sk = label_selection_signal_key("copy-lbl");
        use_signal(&sk, || None::<(usize, usize)>).set(Some((0, 5)));
        assert!(try_copy_label_selection(&root));
        assert_eq!(
            use_signal(LABEL_SELECTION_COPY_SIGNAL, String::new).get(),
            "hello"
        );
    }

    #[test]
    fn label_child_preserves_bind_text_identity() {
        use crate::app::reactive::{bind_text, reset_registered_anchors};
        use crate::core::reactivity::{create_effect, dispose_effect};
        use std::cell::RefCell;
        use std::rc::Rc;

        reset_registered_anchors();
        let mounted = Rc::new(RefCell::new(None::<Element>));
        let mounted_a = Rc::clone(&mounted);
        let root = create_effect(move || {
            let bound = bind_text("bind:label-adopt", || {
                "Selected variant: default".to_string()
            });
            assert!(bound.on_unmount.is_some());
            let hook = Rc::downgrade(bound.on_unmount.as_ref().unwrap());
            let el: Element = label("lbl-auto").child(bound).into();
            assert_eq!(el.text.as_deref(), Some("Selected variant: default"));
            assert_eq!(el.template_id.as_deref(), Some("bind:label-adopt"));
            assert!(
                el.on_unmount.is_some(),
                "adopting bind_text must keep unmount hook alive"
            );
            assert!(
                hook.upgrade().is_some(),
                "bind hook must not fire during Label adoption"
            );
            *mounted_a.borrow_mut() = Some(el);
        });
        assert!(mounted.borrow().is_some());
        drop(mounted.borrow_mut().take());
        dispose_effect(root);
        reset_registered_anchors();
    }

    #[test]
    fn label_for_target_sets_accesskit_id() {
        let el: Element = label("").text("Email").for_target("input:email").into();
        assert_eq!(
            el.attrs.get(ATTR_LABEL_FOR).map(|s| s.as_str()),
            Some("input:email")
        );
        assert_eq!(
            el.attrs.get("accesskit_id").map(|s| s.as_str()),
            Some("lbl:for:input:email")
        );
    }

    #[test]
    fn linked_label_explicit_width_and_container_keep_their_click_box() {
        for mut el in [
            Element::from(label("").text("Title").w(240.0).for_target("terms")),
            Element::from(
                label("")
                    .child(
                        Element::new()
                            .flex_col()
                            .child(Element::new().text("Title"))
                            .child(Element::new().text("Description")),
                    )
                    .for_target("terms"),
            ),
        ] {
            let mut root = Element::new()
                .flex_col()
                .w(240.0)
                .h(100.0)
                .child(std::mem::take(&mut el));
            crate::layout::layout_tree::LayoutTree::new().build_and_layout(
                &mut root,
                240.0,
                100.0,
                [-1.0, -1.0],
            );
            assert_eq!(root.children[0].layout_bounds[2], 240.0);
        }
    }

    #[test]
    fn label_r_for_alias_sets_accesskit_id() {
        let el: Element = label("").text("Email").r#for("input:email").into();
        assert_eq!(
            el.attrs.get(ATTR_LABEL_FOR).map(|s| s.as_str()),
            Some("input:email")
        );
    }

    #[test]
    fn label_empty_id_does_not_write_empty_template_id() {
        let el: Element = label("").text("x").into();
        assert_ne!(el.template_id.as_deref(), Some(""));
        assert!(
            el.template_id.is_none(),
            "display Label empty id → no template_id"
        );
    }
}

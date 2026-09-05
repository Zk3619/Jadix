//! # Message
//!
//! ShadCN-style conversation row. Aligns with
//! `Reference/ShadCN-UI/.../bases/base/ui/message.tsx` — avatar, header,
//! surface, and footer around a chat turn.
//!
//! Chrome (https://ui.shadcn.com/docs/components/aria/message):
//! - Group: `flex min-w-0 flex-col`
//! - Row: `relative flex w-full min-w-0`; `align=end` → `flex-row-reverse`
//!   (avatar on the trailing edge). RUI splits footer onto a second row so
//!   the avatar bottom-aligns with the bubble, not the actions.
//! - Avatar: `w-fit shrink-0 self-end overflow-hidden rounded-full bg-muted`
//! - Content: `flex w-full min-w-0 flex-col wrap-break-word`. On `align=end`,
//!   slotted children get `self-end` so the bubble sits against the avatar.
//! - Header: always start-justified; the box still `self-end`s on `align=end`
//! - Footer: `justify-end` when the parent row is `align=end`
//!
//! ```text
//! MessageGroup
//! └── Message                         // column · w-full
//!     ├── message-upper               // w-full · row / row-reverse · items-end
//!     │   ├── MessageAvatar
//!     │   └── MessageContent          // w-full · children self-end when end
//!     │       ├── MessageHeader?
//!     │       └── Bubble / surface
//!     └── message-footer-row          // w-full · ghost spacer + footer
//!         ├── avatar-ghost
//!         └── MessageFooter           // status / actions
//! ```
//!
//! ## Usage (dual style · ADR-9)
//! ```ignore
//! // A · RSX / tags
//! <MessageGroup>
//!     <Message align="start">
//!         <MessageAvatar>
//!             <Avatar>
//!                 <AvatarFallback>"CN"</AvatarFallback>
//!             </Avatar>
//!         </MessageAvatar>
//!         <MessageContent>
//!             <MessageHeader>"Oliver"</MessageHeader>
//!             <Bubble>
//!                 <BubbleContent>"How can I help you today?"</BubbleContent>
//!             </Bubble>
//!             <MessageFooter>"Delivered"</MessageFooter>
//!         </MessageContent>
//!     </Message>
//!     <Message align="end">
//!         <MessageAvatar>
//!             <Avatar>
//!                 <AvatarFallback>"ME"</AvatarFallback>
//!             </Avatar>
//!         </MessageAvatar>
//!         <MessageContent>
//!             <Bubble align="end">
//!                 <BubbleContent>"On it, one-line change."</BubbleContent>
//!             </Bubble>
//!         </MessageContent>
//!     </Message>
//! </MessageGroup>
//!
//! // B · Builder
//! Element::from(
//!     message_group()
//!         .child(
//!             message()
//!                 .align("end")
//!                 .avatar(avatar_child)
//!                 .header("You")
//!                 .content_child(bubble_child)
//!                 .footer("Delivered")
//!         )
//! )
//! ```
//!
//! ## Notes
//! - **Presentational**: empty `id` omits `template_id` (WP-2.8). Message does
//!   not own download, lightbox, or double-click preview — put those on
//!   [`crate::components::AttachmentAction`] / the image node (`on_click` /
//!   `on_double_click`). Image and file chrome both use
//!   [`crate::components::Attachment`] (`orientation=vertical` +
//!   `AttachmentMedia variant=image` for photos; icon + title + actions for files).
//! - `<Message>` / `MessageGroup` / slots use the generic PascalCase → `*Props`
//!   path (same as Kbd / Empty; no special `ui!` scope).
//! - Dynamic `align={sig.get()}` reduces to [`apply_bound_message_align`].
//! - CSS `data-[align=end]:flex-row-reverse` is `FlexDirection::RowReverse` on
//!   the full-width avatar row (`message-upper`) and the footer row. Content
//!   stays `w-full` (ShadCN). Slotted children `self-start` (or `self-end` when
//!   `align=end`) so bubbles and file chips hug instead of stretching. Header stays
//!   start-justified; footer `justify-end` follows the message side.
//! - Empty [`MessageAvatar`] is a 2rem spacer (same as default Avatar) so
//!   grouped turns stay aligned without repeating the portrait.
//! - When a footer is present the row splits: avatar sits beside header+bubble
//!   (`items-end`), and actions live on a second row under the bubble (ghost
//!   spacer matches the avatar column).

use taffy::style::{AlignSelf, FlexDirection};

use crate::components::base::{BaseComponentBuilder, SlotChrome};
use crate::core::tree::{parse_color, parse_text_color, Element, LayoutStatus};
use crate::impl_slot_chrome_methods;
use crate::primitives::r#box::{hstack, vstack};
use crate::tokens;
use crate::utils::take_packed;
use crate::utils::text_node::{build_text_node, TextNodeConfig};
use rui_macros::component;

const ATTR_SLOT: &str = "data-slot";
const ATTR_ALIGN: &str = "data-align";
const ATTR_MESSAGE_ALIGN: &str = "rui_message_align";

const SLOT_GROUP: &str = "message-group";
const SLOT_ROOT: &str = "message";
const SLOT_UPPER: &str = "message-upper";
const SLOT_FOOTER_ROW: &str = "message-footer-row";
const SLOT_AVATAR: &str = "message-avatar";
const SLOT_AVATAR_GHOST: &str = "message-avatar-ghost";
const SLOT_CONTENT: &str = "message-content";
const SLOT_HEADER: &str = "message-header";
const SLOT_FOOTER: &str = "message-footer";

/// Default Avatar `size-8` so an empty avatar slot still reserves a column.
const AVATAR_MIN: &str = "2rem";

const HEADER_SIZE_PX: f32 = 14.0;
const HEADER_WEIGHT: u16 = 500;
const FOOTER_SIZE_PX: f32 = 12.0;
const FOOTER_WEIGHT: u16 = 400;

// ---------------------------------------------------------------------------
// Align
// ---------------------------------------------------------------------------

/// Message row alignment: `start` (other) | `end` (current user).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MessageAlign {
    #[default]
    Start,
    End,
}

impl MessageAlign {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Start => "start",
            Self::End => "end",
        }
    }

    pub fn is_end(self) -> bool {
        self == Self::End
    }
}

impl From<&str> for MessageAlign {
    fn from(s: &str) -> Self {
        s.parse().unwrap_or(Self::Start)
    }
}

impl From<String> for MessageAlign {
    fn from(s: String) -> Self {
        Self::from(s.as_str())
    }
}

impl std::str::FromStr for MessageAlign {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s.trim().to_ascii_lowercase().as_str() {
            "end" | "right" | "user" => Self::End,
            _ => Self::Start,
        })
    }
}

fn message_align_from_attrs(attrs: &std::collections::HashMap<String, String>) -> MessageAlign {
    attrs
        .get(ATTR_MESSAGE_ALIGN)
        .or_else(|| attrs.get(ATTR_ALIGN))
        .map(|s| MessageAlign::from(s.as_str()))
        .unwrap_or_default()
}

fn slot_name(el: &Element) -> Option<&str> {
    el.attrs.get(ATTR_SLOT).map(String::as_str)
}

fn header_text_config() -> TextNodeConfig {
    TextNodeConfig {
        size_px: HEADER_SIZE_PX,
        weight: HEADER_WEIGHT,
        color: parse_text_color("foreground"),
        text_align: crate::style::TextAlign::Start,
        ..Default::default()
    }
}

fn footer_text_config() -> TextNodeConfig {
    TextNodeConfig {
        size_px: FOOTER_SIZE_PX,
        weight: FOOTER_WEIGHT,
        color: parse_text_color("muted-foreground"),
        text_align: crate::style::TextAlign::Start,
        ..Default::default()
    }
}

fn preserve_binding_identity(from: &Element, to: &mut Element) {
    to.template_id = from.template_id.clone();
    to.focus_id = from.focus_id.clone();
    to.on_click = from.on_click.clone();
    to.on_unmount = from.on_unmount.clone();
    to.layout_node = from.layout_node;
}

fn is_plain_text_leaf(el: &Element) -> bool {
    el.text.as_ref().is_some_and(|t| !t.is_empty())
        && el.children.is_empty()
        && el.on_click.is_none()
}

fn caption_from_children(children: Vec<Element>, cfg: &TextNodeConfig) -> Vec<Element> {
    children
        .into_iter()
        .map(|mut child| {
            if is_plain_text_leaf(&child) {
                if let Some(text) = child.text.take() {
                    let mut node = build_text_node(text, cfg.clone());
                    preserve_binding_identity(&child, &mut node);
                    return node;
                }
            }
            child
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Paint
// ---------------------------------------------------------------------------

struct MessageParts {
    avatar: Option<Element>,
    surface: Vec<Element>,
    footers: Vec<Element>,
}

fn absorb_message_child(mut child: Element, parts: &mut MessageParts) {
    let slot = child.attrs.get(ATTR_SLOT).map(|s| s.clone());
    match slot.as_deref() {
        Some(SLOT_UPPER | SLOT_FOOTER_ROW | SLOT_CONTENT) => {
            for nested in std::mem::take(&mut child.children) {
                absorb_message_child(nested, parts);
            }
        }
        Some(SLOT_AVATAR_GHOST) => {}
        Some(SLOT_AVATAR) => parts.avatar = Some(child),
        Some(SLOT_FOOTER) => parts.footers.push(child),
        _ => parts.surface.push(child),
    }
}

fn collect_message_parts(el: &mut Element) -> MessageParts {
    let mut parts = MessageParts {
        avatar: None,
        surface: Vec::new(),
        footers: Vec::new(),
    };
    for child in std::mem::take(&mut el.children) {
        absorb_message_child(child, &mut parts);
    }
    parts
}

fn make_avatar_ghost() -> Element {
    let mut el: Element = hstack()
        .flex_shrink(0.0)
        .w(AVATAR_MIN)
        .h(AVATAR_MIN)
        .min_w(AVATAR_MIN)
        .min_h(AVATAR_MIN)
        .into();
    el = el.opacity(0.0).pointer_events_none();
    el.set_attr(ATTR_SLOT, SLOT_AVATAR_GHOST);
    el.set_attr("aria-hidden", "true");
    el
}

/// Pack surface children to the message side and prevent stretch-to-full-width.
/// Taffy has no `width: fit-content`; `align-self: start|end` sizes to content.
/// End matches ShadCN `group-data-[align=end]/message:*:data-slot:self-end`.
fn pack_slotted_children(el: &mut Element, align: MessageAlign) {
    let pack = if align.is_end() {
        AlignSelf::FLEX_END
    } else {
        AlignSelf::FLEX_START
    };
    for child in &mut el.children {
        child.style.align_self = Some(pack);
    }
}

fn wrap_surface(align: MessageAlign, kids: Vec<Element>) -> Element {
    let mut el: Element = vstack()
        .w("100%")
        .flex_grow(1.0)
        .flex_shrink(1.0)
        .min_w(0.0)
        .max_w("100%")
        .gap(tokens::SPACE_1)
        .items("start")
        .children(kids)
        .into();
    pack_slotted_children(&mut el, align);
    el = el.text_wrap(crate::text::WrapMode::Word);
    el.set_attr(ATTR_SLOT, SLOT_CONTENT);
    el
}

fn wrap_upper(align: MessageAlign, kids: Vec<Element>) -> Element {
    let mut el: Element = hstack()
        .items("end")
        .justify("start")
        .gap(tokens::SPACE_2)
        .w("100%")
        .min_w(0.0)
        .max_w("100%")
        .children(kids)
        .into();
    el.style.flex_direction = if align.is_end() {
        FlexDirection::RowReverse
    } else {
        FlexDirection::Row
    };
    el.set_attr(ATTR_SLOT, SLOT_UPPER);
    el.set_attr(ATTR_ALIGN, align.as_str());
    el
}

fn wrap_footer_row(align: MessageAlign, kids: Vec<Element>) -> Element {
    let mut el: Element = hstack()
        .items_center()
        .justify("start")
        .gap(tokens::SPACE_2)
        .w("100%")
        .min_w(0.0)
        .max_w("100%")
        .children(kids)
        .into();
    el.style.flex_direction = if align.is_end() {
        FlexDirection::RowReverse
    } else {
        FlexDirection::Row
    };
    el.set_attr(ATTR_SLOT, SLOT_FOOTER_ROW);
    el.set_attr(ATTR_ALIGN, align.as_str());
    el
}

/// Split footer onto its own row so the avatar bottom-aligns with the bubble.
fn ensure_message_chrome(el: &mut Element, align: MessageAlign) {
    let parts = collect_message_parts(el);
    let has_avatar = parts.avatar.is_some();
    let mut upper_kids = Vec::new();
    if let Some(avatar) = parts.avatar {
        upper_kids.push(avatar);
    }
    if !parts.surface.is_empty() {
        upper_kids.push(wrap_surface(align, parts.surface));
    }
    let mut kids = vec![wrap_upper(align, upper_kids)];
    if !parts.footers.is_empty() {
        let mut row = Vec::new();
        if has_avatar {
            row.push(make_avatar_ghost());
        }
        row.extend(parts.footers);
        kids.push(wrap_footer_row(align, row));
    }
    el.children = kids;
}

fn paint_avatar_slot(el: &mut Element) {
    let taken = std::mem::take(el);
    let mut painted = taken.align_self_kw("end").flex_shrink(0.0).mb(0.0);
    painted.set_attr(ATTR_SLOT, SLOT_AVATAR);
    *el = painted;
}

fn paint_content_slot(el: &mut Element, align: MessageAlign) {
    let taken = std::mem::take(el);
    let mut painted = taken
        .w("100%")
        .flex_grow(1.0)
        .flex_shrink(1.0)
        .min_w(0.0)
        .max_w("100%")
        .flex_items("start");
    pack_slotted_children(&mut painted, align);
    painted = painted.text_wrap(crate::text::WrapMode::Word);
    painted.set_attr(ATTR_SLOT, SLOT_CONTENT);
    *el = painted;
}

fn paint_header_slot(el: &mut Element, align: MessageAlign) {
    // Docs: header stays start-justified. CSS `*:data-slot:self-end` still
    // packs the header *box* to the message side when `align=end`.
    let taken = std::mem::take(el);
    let mut painted = taken
        .align_self_kw(if align.is_end() { "end" } else { "start" })
        .justify_start()
        .max_w("100%")
        .min_w(0.0);
    painted.set_attr(ATTR_SLOT, SLOT_HEADER);
    *el = painted;
}

fn paint_footer_slot(el: &mut Element, align: MessageAlign) {
    let taken = std::mem::take(el);
    let mut painted = taken.max_w("100%").min_w(0.0);
    painted = if align.is_end() {
        painted.justify_end()
    } else {
        painted.justify_start()
    };
    painted.set_attr(ATTR_SLOT, SLOT_FOOTER);
    painted.set_attr(ATTR_ALIGN, align.as_str());
    *el = painted;
}

fn paint_row_dir(el: &mut Element, align: MessageAlign, slot: &'static str) {
    let taken = std::mem::take(el);
    let mut painted = taken.w("100%").min_w(0.0).max_w("100%").justify("start");
    painted.style.flex_direction = if align.is_end() {
        FlexDirection::RowReverse
    } else {
        FlexDirection::Row
    };
    painted.set_attr(ATTR_SLOT, slot);
    painted.set_attr(ATTR_ALIGN, align.as_str());
    *el = painted;
}

fn paint_message_slots(el: &mut Element, align: MessageAlign) {
    for child in &mut el.children {
        match slot_name(child) {
            Some(SLOT_UPPER) => {
                paint_row_dir(child, align, SLOT_UPPER);
                paint_message_slots(child, align);
            }
            Some(SLOT_FOOTER_ROW) => {
                paint_row_dir(child, align, SLOT_FOOTER_ROW);
                paint_message_slots(child, align);
            }
            Some(SLOT_AVATAR) => paint_avatar_slot(child),
            Some(SLOT_CONTENT) => {
                paint_content_slot(child, align);
                paint_message_slots(child, align);
            }
            Some(SLOT_HEADER) => paint_header_slot(child, align),
            Some(SLOT_FOOTER) => paint_footer_slot(child, align),
            Some(SLOT_AVATAR_GHOST) => {}
            _ => paint_message_slots(child, align),
        }
    }
}

/// Apply align chrome in place so [`bind_attr`] can restyle a mounted row.
///
/// A live `align` change edits **descendant** flex direction (`message-upper` /
/// footer row) and `align-self` on slotted children. The frame's LayoutChanged
/// fast path only dirties this root's Taffy node (still a column), so the row
/// would keep the previous `Row` geometry. [`LayoutStatus::StructureChanged`]
/// forces a full Taffy rebuild from the element tree.
pub fn apply_message_paint(el: &mut Element, align: MessageAlign) -> LayoutStatus {
    let prev = message_align_from_attrs(&el.attrs);
    let already_root = slot_name(el) == Some(SLOT_ROOT);
    if already_root && prev == align {
        return LayoutStatus::Clean;
    }

    let mut painted = std::mem::take(el)
        .relative()
        .w("100%")
        .min_w(0.0)
        .max_w("100%")
        .gap(tokens::SPACE_1);
    painted.style.flex_direction = FlexDirection::Column;
    painted = painted.flex_items("stretch");

    painted.set_attr(ATTR_SLOT, SLOT_ROOT);
    painted.set_attr(ATTR_ALIGN, align.as_str());
    painted.set_attr(ATTR_MESSAGE_ALIGN, align.as_str());

    if already_root {
        paint_message_slots(&mut painted, align);
    } else {
        ensure_message_chrome(&mut painted, align);
        paint_message_slots(&mut painted, align);
    }
    *el = painted;

    if already_root {
        LayoutStatus::StructureChanged
    } else {
        LayoutStatus::LayoutChanged
    }
}

/// Macro bind: reactive `align={sig.get()}` on `<Message>`.
pub fn apply_bound_message_align(el: &mut Element, align: impl AsRef<str>) -> LayoutStatus {
    apply_message_paint(el, MessageAlign::from(align.as_ref()))
}

fn build_message_root(align: MessageAlign, children: Vec<Element>) -> Element {
    let mut el: Element = vstack().children(children).into();
    apply_message_paint(&mut el, align);
    el
}

fn build_message_group(children: Vec<Element>) -> Element {
    let mut el: Element = vstack()
        .w("100%")
        .min_w(0.0)
        .gap(tokens::SPACE_2)
        .children(children)
        .into();
    el.set_attr(ATTR_SLOT, SLOT_GROUP);
    el
}

fn build_message_avatar(children: Vec<Element>) -> Element {
    let filled = !children.is_empty();
    let mut shell = hstack()
        .items_center()
        .justify("center")
        .flex_shrink(0.0)
        .w("auto")
        .min_w(AVATAR_MIN)
        .min_h(AVATAR_MIN)
        .align_self_kw("end")
        .overflow("hidden")
        .rounded_full()
        .children(children);
    if filled {
        shell = shell.bg(parse_color("muted"));
    }
    let mut el: Element = shell.into();
    el.set_attr(ATTR_SLOT, SLOT_AVATAR);
    el
}

fn build_message_content(children: Vec<Element>) -> Element {
    let mut el: Element = vstack()
        .w("100%")
        .min_w(0.0)
        .flex_grow(1.0)
        .flex_shrink(1.0)
        .gap(tokens::SPACE_1)
        .items("start")
        .children(children)
        .into();
    el = el.text_wrap(crate::text::WrapMode::Word);
    el.set_attr(ATTR_SLOT, SLOT_CONTENT);
    el
}

fn build_message_header(children: Vec<Element>) -> Element {
    let kids = caption_from_children(children, &header_text_config());
    let mut el: Element = hstack()
        .max_w("100%")
        .min_w(0.0)
        .items_center()
        .gap(tokens::SPACE_2)
        .align_self_kw("start")
        .children(kids)
        .into();
    el.set_attr(ATTR_SLOT, SLOT_HEADER);
    el
}

fn build_message_footer(children: Vec<Element>) -> Element {
    let kids = caption_from_children(children, &footer_text_config());
    let mut el: Element = hstack()
        .max_w("100%")
        .min_w(0.0)
        .items_center()
        .gap(tokens::SPACE_1)
        .children(kids)
        .into();
    el.set_attr(ATTR_SLOT, SLOT_FOOTER);
    el
}

fn packed_children(children: Vec<Element>) -> Option<Element> {
    if children.is_empty() {
        None
    } else {
        let mut frag = Element::new();
        frag.set_attr("rui_fragment", "1");
        Some(frag.children(children))
    }
}

fn apply_base(
    mut el: Element,
    base_builder: Option<Box<dyn FnOnce(BaseComponentBuilder) -> BaseComponentBuilder>>,
) -> Element {
    if let Some(b) = base_builder {
        let base = BaseComponentBuilder::new(el);
        el = b(base).el;
    }
    el
}

// ---------------------------------------------------------------------------
// RSX parts
// ---------------------------------------------------------------------------

/// Groups consecutive turns from the same sender (`flex-col`).
///
/// RSX `<MessageGroup>`.
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn MessageGroup(
    id: String,
    base_builder: Option<Box<dyn FnOnce(BaseComponentBuilder) -> BaseComponentBuilder>>,
    children: Option<Element>,
) -> Element {
    let kids = children.map(take_packed).unwrap_or_default();
    let mut el = build_message_group(kids);
    el = apply_base(el, base_builder);
    if !id.trim().is_empty() {
        el.template_id = Some(id);
    }
    el
}

/// Conversation row (`align`: `start` | `end`).
///
/// RSX `<Message align="…">`.
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn Message(
    id: String,
    align: MessageAlign,
    base_builder: Option<Box<dyn FnOnce(BaseComponentBuilder) -> BaseComponentBuilder>>,
    children: Option<Element>,
) -> Element {
    let kids = children.map(take_packed).unwrap_or_default();
    let mut el = build_message_root(align, kids);
    el = apply_base(el, base_builder);
    if !id.trim().is_empty() {
        el.template_id = Some(id);
    }
    el
}

/// Circular avatar slot (`self-end`). Empty children reserve a 2rem spacer.
///
/// RSX `<MessageAvatar>`.
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn MessageAvatar(
    base_builder: Option<Box<dyn FnOnce(BaseComponentBuilder) -> BaseComponentBuilder>>,
    children: Option<Element>,
) -> Element {
    let kids = children.map(take_packed).unwrap_or_default();
    apply_base(build_message_avatar(kids), base_builder)
}

/// Header + surface column (`w-full` · `min-w-0` · `wrap-break-word`).
///
/// RSX `<MessageContent>`.
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn MessageContent(
    base_builder: Option<Box<dyn FnOnce(BaseComponentBuilder) -> BaseComponentBuilder>>,
    children: Option<Element>,
) -> Element {
    let kids = children.map(take_packed).unwrap_or_default();
    apply_base(build_message_content(kids), base_builder)
}

/// Sender name / timestamp row. Always start-justified; the box `self-end`s
/// when the parent [`Message`] is `align=end` (ShadCN `*:data-slot:self-end`).
///
/// RSX `<MessageHeader>`.
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn MessageHeader(
    base_builder: Option<Box<dyn FnOnce(BaseComponentBuilder) -> BaseComponentBuilder>>,
    children: Option<Element>,
) -> Element {
    let kids = children.map(take_packed).unwrap_or_default();
    apply_base(build_message_header(kids), base_builder)
}

/// Status / actions row. `justify-end` when the parent [`Message`] is `align=end`.
///
/// RSX `<MessageFooter>`.
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn MessageFooter(
    base_builder: Option<Box<dyn FnOnce(BaseComponentBuilder) -> BaseComponentBuilder>>,
    children: Option<Element>,
) -> Element {
    let kids = children.map(take_packed).unwrap_or_default();
    apply_base(build_message_footer(kids), base_builder)
}

// ---------------------------------------------------------------------------
// Builders (ADR-9 surface B)
// ---------------------------------------------------------------------------

pub struct MessageGroupBuilder {
    id: String,
    chrome: SlotChrome,
    children: Vec<Element>,
}

/// Builder entry for [`MessageGroup`].
pub fn message_group() -> MessageGroupBuilder {
    MessageGroupBuilder {
        id: String::new(),
        chrome: SlotChrome::default(),
        children: Vec::new(),
    }
}

impl MessageGroupBuilder {
    impl_slot_chrome_methods!();

    pub fn id(mut self, id: impl Into<String>) -> Self {
        self.id = id.into();
        self
    }

    pub fn child(mut self, child: impl Into<Element>) -> Self {
        self.children.push(child.into());
        self
    }
}

impl crate::core::tree::IntoElementList for MessageGroupBuilder {
    fn into_element_list(self) -> Vec<Element> {
        vec![self.into()]
    }
}

impl From<MessageGroupBuilder> for Element {
    fn from(builder: MessageGroupBuilder) -> Self {
        let el = MessageGroup(MessageGroupProps {
            id: builder.id,
            children: packed_children(builder.children),
            base: None,
            base_builder: None,
            ..Default::default()
        });
        builder.chrome.apply(el)
    }
}

pub struct MessageAvatarBuilder {
    children: Vec<Element>,
    chrome: SlotChrome,
}

/// Builder entry for [`MessageAvatar`].
pub fn message_avatar() -> MessageAvatarBuilder {
    MessageAvatarBuilder {
        children: Vec::new(),
        chrome: SlotChrome::default(),
    }
}

impl MessageAvatarBuilder {
    impl_slot_chrome_methods!();

    pub fn child(mut self, child: impl Into<Element>) -> Self {
        self.children.extend(take_packed(child.into()));
        self
    }
}

impl crate::core::tree::IntoElementList for MessageAvatarBuilder {
    fn into_element_list(self) -> Vec<Element> {
        vec![self.into()]
    }
}

impl From<MessageAvatarBuilder> for Element {
    fn from(builder: MessageAvatarBuilder) -> Self {
        let el = MessageAvatar(MessageAvatarProps {
            children: packed_children(builder.children),
            base: None,
            base_builder: None,
            ..Default::default()
        });
        builder.chrome.apply(el)
    }
}

pub struct MessageHeaderBuilder {
    children: Vec<Element>,
    chrome: SlotChrome,
}

/// Builder entry for [`MessageHeader`].
pub fn message_header() -> MessageHeaderBuilder {
    MessageHeaderBuilder {
        children: Vec::new(),
        chrome: SlotChrome::default(),
    }
}

impl MessageHeaderBuilder {
    impl_slot_chrome_methods!();

    pub fn text(mut self, content: impl Into<String>) -> Self {
        self.children.push(Element::new().text(content.into()));
        self
    }

    pub fn child(mut self, child: impl Into<Element>) -> Self {
        self.children.extend(take_packed(child.into()));
        self
    }
}

impl crate::core::tree::IntoElementList for MessageHeaderBuilder {
    fn into_element_list(self) -> Vec<Element> {
        vec![self.into()]
    }
}

impl From<MessageHeaderBuilder> for Element {
    fn from(builder: MessageHeaderBuilder) -> Self {
        let el = MessageHeader(MessageHeaderProps {
            children: packed_children(builder.children),
            base: None,
            base_builder: None,
            ..Default::default()
        });
        builder.chrome.apply(el)
    }
}

pub struct MessageFooterBuilder {
    children: Vec<Element>,
    chrome: SlotChrome,
}

/// Builder entry for [`MessageFooter`].
pub fn message_footer() -> MessageFooterBuilder {
    MessageFooterBuilder {
        children: Vec::new(),
        chrome: SlotChrome::default(),
    }
}

impl MessageFooterBuilder {
    impl_slot_chrome_methods!();

    pub fn text(mut self, content: impl Into<String>) -> Self {
        self.children.push(Element::new().text(content.into()));
        self
    }

    pub fn child(mut self, child: impl Into<Element>) -> Self {
        self.children.extend(take_packed(child.into()));
        self
    }
}

impl crate::core::tree::IntoElementList for MessageFooterBuilder {
    fn into_element_list(self) -> Vec<Element> {
        vec![self.into()]
    }
}

impl From<MessageFooterBuilder> for Element {
    fn from(builder: MessageFooterBuilder) -> Self {
        let el = MessageFooter(MessageFooterProps {
            children: packed_children(builder.children),
            base: None,
            base_builder: None,
            ..Default::default()
        });
        builder.chrome.apply(el)
    }
}

pub struct MessageContentBuilder {
    children: Vec<Element>,
    chrome: SlotChrome,
}

/// Builder entry for [`MessageContent`].
pub fn message_content() -> MessageContentBuilder {
    MessageContentBuilder {
        children: Vec::new(),
        chrome: SlotChrome::default(),
    }
}

impl MessageContentBuilder {
    impl_slot_chrome_methods!();

    pub fn child(mut self, child: impl Into<Element>) -> Self {
        self.children.push(child.into());
        self
    }

    pub fn header(mut self, text: impl Into<String>) -> Self {
        self.children
            .push(message_header().text(text.into()).into());
        self
    }

    pub fn header_child(mut self, child: impl Into<Element>) -> Self {
        self.children
            .push(message_header().child(child.into()).into());
        self
    }

    pub fn footer(mut self, text: impl Into<String>) -> Self {
        self.children
            .push(message_footer().text(text.into()).into());
        self
    }

    pub fn footer_child(mut self, child: impl Into<Element>) -> Self {
        self.children
            .push(message_footer().child(child.into()).into());
        self
    }
}

impl crate::core::tree::IntoElementList for MessageContentBuilder {
    fn into_element_list(self) -> Vec<Element> {
        vec![self.into()]
    }
}

impl From<MessageContentBuilder> for Element {
    fn from(builder: MessageContentBuilder) -> Self {
        let el = MessageContent(MessageContentProps {
            children: packed_children(builder.children),
            base: None,
            base_builder: None,
            ..Default::default()
        });
        builder.chrome.apply(el)
    }
}

pub struct MessageBuilder {
    id: String,
    align: MessageAlign,
    chrome: SlotChrome,
    children: Vec<Element>,
    avatar: Option<Element>,
    header: Option<Element>,
    content: Option<Element>,
    body: Vec<Element>,
    footer: Option<Element>,
}

/// Builder entry for [`Message`].
pub fn message() -> MessageBuilder {
    MessageBuilder {
        id: String::new(),
        align: MessageAlign::Start,
        chrome: SlotChrome::default(),
        children: Vec::new(),
        avatar: None,
        header: None,
        content: None,
        body: Vec::new(),
        footer: None,
    }
}

impl MessageBuilder {
    impl_slot_chrome_methods!();

    pub fn id(mut self, id: impl Into<String>) -> Self {
        self.id = id.into();
        self
    }

    pub fn align(mut self, align: impl Into<MessageAlign>) -> Self {
        self.align = align.into();
        self
    }

    pub fn child(mut self, child: impl Into<Element>) -> Self {
        self.children.push(child.into());
        self
    }

    pub fn avatar(mut self, child: impl Into<Element>) -> Self {
        self.avatar = Some(message_avatar().child(child.into()).into());
        self
    }

    pub fn avatar_with(
        mut self,
        f: impl FnOnce(MessageAvatarBuilder) -> MessageAvatarBuilder,
    ) -> Self {
        self.avatar = Some(f(message_avatar()).into());
        self
    }

    pub fn avatar_slot(mut self, slot: impl Into<Element>) -> Self {
        self.avatar = Some(slot.into());
        self
    }

    pub fn header(mut self, text: impl Into<String>) -> Self {
        self.header = Some(message_header().text(text.into()).into());
        self
    }

    pub fn header_with(
        mut self,
        f: impl FnOnce(MessageHeaderBuilder) -> MessageHeaderBuilder,
    ) -> Self {
        self.header = Some(f(message_header()).into());
        self
    }

    pub fn header_child(mut self, child: impl Into<Element>) -> Self {
        self.header = Some(message_header().child(child.into()).into());
        self
    }

    pub fn content_with(
        mut self,
        f: impl FnOnce(MessageContentBuilder) -> MessageContentBuilder,
    ) -> Self {
        self.content = Some(f(message_content()).into());
        self
    }

    pub fn content_child(mut self, child: impl Into<Element>) -> Self {
        self.body.push(child.into());
        self
    }

    pub fn footer(mut self, text: impl Into<String>) -> Self {
        self.footer = Some(message_footer().text(text.into()).into());
        self
    }

    pub fn footer_with(
        mut self,
        f: impl FnOnce(MessageFooterBuilder) -> MessageFooterBuilder,
    ) -> Self {
        self.footer = Some(f(message_footer()).into());
        self
    }

    pub fn footer_child(mut self, child: impl Into<Element>) -> Self {
        self.footer = Some(message_footer().child(child.into()).into());
        self
    }
}

impl crate::core::tree::IntoElementList for MessageBuilder {
    fn into_element_list(self) -> Vec<Element> {
        vec![self.into()]
    }
}

impl From<MessageBuilder> for Element {
    fn from(builder: MessageBuilder) -> Self {
        let mut kids = builder.children;
        if kids.is_empty() {
            if let Some(avatar) = builder.avatar {
                kids.push(avatar);
            }
            if let Some(content) = builder.content {
                kids.push(content);
            } else {
                let mut content_kids = Vec::new();
                if let Some(header) = builder.header {
                    content_kids.push(header);
                }
                content_kids.extend(builder.body);
                if let Some(footer) = builder.footer {
                    content_kids.push(footer);
                }
                if !content_kids.is_empty() {
                    kids.push(message_content().children_from(content_kids).into());
                }
            }
        }
        let el = Message(MessageProps {
            id: builder.id,
            align: Some(builder.align),
            children: packed_children(kids),
            base: None,
            base_builder: None,
            ..Default::default()
        });
        builder.chrome.apply(el)
    }
}

impl MessageContentBuilder {
    fn children_from(mut self, kids: Vec<Element>) -> Self {
        self.children.extend(kids);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use taffy::style::JustifyContent;

    fn find_slot<'a>(el: &'a Element, slot: &str) -> Option<&'a Element> {
        if slot_name(el) == Some(slot) {
            return Some(el);
        }
        el.children.iter().find_map(|c| find_slot(c, slot))
    }

    #[test]
    fn message_default_align_start_row() {
        let el: Element = message().content_child(Element::new().text("Hello")).into();
        assert_eq!(slot_name(&el), Some(SLOT_ROOT));
        assert_eq!(el.attrs.get(ATTR_ALIGN).map(String::as_str), Some("start"));
        assert_eq!(el.style.flex_direction, FlexDirection::Column);
        let upper = find_slot(&el, SLOT_UPPER).expect("upper");
        assert_eq!(upper.style.flex_direction, FlexDirection::Row);
        assert!(el.template_id.is_none());
    }

    #[test]
    fn message_start_surface_children_self_start() {
        let el: Element = message().content_child(Element::new().text("Hello")).into();
        let upper = find_slot(&el, SLOT_UPPER).expect("upper");
        let content = find_slot(upper, SLOT_CONTENT).expect("content");
        assert!(
            content
                .children
                .iter()
                .all(|c| c.style.align_self == Some(AlignSelf::FLEX_START)),
            "start: surface children hug; Taffy has no fit-content"
        );
    }

    #[test]
    fn file_chip_in_start_message_hugs_not_full_width() {
        let chip = Element::from(
            crate::components::attachment::attachment()
                .media_icon(Element::new().w(16.0).h(16.0))
                .title_description("sales-dashboard.pdf", "PDF · 2.4 MB"),
        );
        let mut root: Element = crate::primitives::r#box::vstack()
            .w(400.0)
            .child(
                message()
                    .align("start")
                    .avatar(Element::new().text("CN").w(32.0).h(32.0))
                    .content_child(chip),
            )
            .into();
        let mut tree = crate::layout::layout_tree::LayoutTree::new();
        let _ = tree.build_and_layout(&mut root, 400.0, 240.0, [-1.0, -1.0]);
        let msg = find_slot(&root, SLOT_ROOT).expect("message");
        let content = find_slot(msg, SLOT_CONTENT).expect("content");
        fn find_attachment<'a>(el: &'a Element) -> Option<&'a Element> {
            if el.attrs.get(ATTR_SLOT).map(String::as_str) == Some("attachment") {
                return Some(el);
            }
            el.children.iter().find_map(find_attachment)
        }
        let att = find_attachment(content).expect("attachment");
        assert!(
            att.layout_bounds[2] > 80.0,
            "file chip should keep an intrinsic width (w={:.1})",
            att.layout_bounds[2]
        );
        assert!(
            att.layout_bounds[2] < content.layout_bounds[2] - 40.0,
            "file chip must hug, not fill the message column (att={:.1} content={:.1})",
            att.layout_bounds[2],
            content.layout_bounds[2]
        );
    }

    #[test]
    fn message_align_end_reverses_row() {
        let el: Element = message()
            .align("end")
            .content_child(Element::new().text("On it."))
            .into();
        assert_eq!(el.attrs.get(ATTR_ALIGN).map(String::as_str), Some("end"));
        assert_eq!(el.style.flex_direction, FlexDirection::Column);
        let upper = find_slot(&el, SLOT_UPPER).expect("upper");
        assert_eq!(upper.style.flex_direction, FlexDirection::RowReverse);
    }

    fn rect_right(el: &Element) -> f32 {
        el.layout_bounds[0] + el.layout_bounds[2]
    }

    /// `align=end`: avatar on the trailing edge; bubble/header self-end against it
    /// (content stays `w-full` like ShadCN, leftover sits inside the content column).
    #[test]
    fn message_align_end_packs_avatar_right_content_against_it() {
        let mut root: Element = crate::primitives::r#box::vstack()
            .w(400.0)
            .child(
                message()
                    .align("end")
                    .avatar(Element::new().text("ME").w(32.0).h(32.0))
                    .header("You")
                    .content_child(Element::new().text("Hi"))
                    .footer("sent"),
            )
            .into();
        let mut tree = crate::layout::layout_tree::LayoutTree::new();
        let _ = tree.build_and_layout(&mut root, 400.0, 240.0, [-1.0, -1.0]);
        let msg = find_slot(&root, SLOT_ROOT).expect("message");
        let upper = find_slot(msg, SLOT_UPPER).expect("upper");
        let avatar = find_slot(upper, SLOT_AVATAR).expect("avatar");
        let content = find_slot(upper, SLOT_CONTENT).expect("content");
        let header = find_slot(content, SLOT_HEADER).expect("header");
        let msg_right = rect_right(msg);
        let av_x = avatar.layout_bounds[0];
        let av_right = rect_right(avatar);
        let co_x = content.layout_bounds[0];
        let co_right = rect_right(content);

        assert!(
            av_x > co_x + 1.0,
            "end: avatar must sit to the right of content (av_x={av_x:.1} co_x={co_x:.1})"
        );
        assert!(
            (msg_right - av_right).abs() < 3.0,
            "end: avatar must sit on the trailing edge (av_right={av_right:.1} msg_right={msg_right:.1} msg_w={:.1} av_x={av_x:.1} co_x={co_x:.1} co_w={:.1})",
            msg.layout_bounds[2],
            content.layout_bounds[2]
        );
        assert!(
            (av_x - co_right).abs() < 12.0,
            "end: content fills up to the avatar (gap={:.1} av_x={av_x:.1} co_right={co_right:.1})",
            av_x - co_right
        );
        assert!(
            content.layout_bounds[2] > msg.layout_bounds[2] * 0.5,
            "end: content stays w-full (content_w={:.1} msg_w={:.1})",
            content.layout_bounds[2],
            msg.layout_bounds[2]
        );
        assert!(
            (rect_right(header) - co_right).abs() < 4.0,
            "end: header box self-ends against the content trailing edge"
        );
    }

    #[test]
    fn message_align_end_bind_moves_avatar_to_trailing_edge() {
        let mut root: Element = crate::primitives::r#box::vstack()
            .w(400.0)
            .child(
                message()
                    .avatar(Element::new().text("ME").w(32.0).h(32.0))
                    .content_child(Element::new().text("Hi")),
            )
            .into();
        let mut tree = crate::layout::layout_tree::LayoutTree::new();
        let _ = tree.build_and_layout(&mut root, 400.0, 240.0, [-1.0, -1.0]);
        let status = apply_bound_message_align(root.children.first_mut().expect("child"), "end");
        assert_eq!(
            status,
            LayoutStatus::StructureChanged,
            "align bind must full-rebuild Taffy so descendant row-reverse is applied"
        );
        let _ = tree.build_and_layout(&mut root, 400.0, 240.0, [-1.0, -1.0]);
        let msg = find_slot(&root, SLOT_ROOT).expect("message after bind");
        let upper = find_slot(msg, SLOT_UPPER).expect("upper");
        let avatar = find_slot(upper, SLOT_AVATAR).expect("avatar");
        let content = find_slot(upper, SLOT_CONTENT).expect("content");
        assert_eq!(upper.style.flex_direction, FlexDirection::RowReverse);
        assert!(
            (rect_right(msg) - rect_right(avatar)).abs() < 3.0,
            "bind end: avatar on trailing edge (av_right={:.1} msg_right={:.1})",
            rect_right(avatar),
            rect_right(msg)
        );
        assert!(
            avatar.layout_bounds[0] > content.layout_bounds[0] + 1.0,
            "bind end: avatar right of content"
        );
    }

    #[test]
    fn message_align_end_with_bubble_keeps_avatar_on_trailing_edge() {
        let bubble_el: Element = crate::components::bubble::bubble()
            .variant("default")
            .align("end")
            .child(crate::components::bubble::bubble_content().text("It's a one-line change."))
            .into();
        let mut root: Element = crate::primitives::r#box::vstack()
            .w(448.0)
            .child(
                message()
                    .align("end")
                    .avatar(Element::new().text("ME").w(32.0).h(32.0))
                    .content_child(bubble_el)
                    .footer("Delivered"),
            )
            .into();
        let mut tree = crate::layout::layout_tree::LayoutTree::new();
        let _ = tree.build_and_layout(&mut root, 448.0, 240.0, [-1.0, -1.0]);
        let msg = find_slot(&root, SLOT_ROOT).expect("message");
        let upper = find_slot(msg, SLOT_UPPER).expect("upper");
        let avatar = find_slot(upper, SLOT_AVATAR).expect("avatar");
        let content = find_slot(upper, SLOT_CONTENT).expect("content");
        assert!(
            (rect_right(msg) - rect_right(avatar)).abs() < 3.0,
            "gallery-like end: avatar on trailing edge (av_right={:.1} msg_right={:.1} co_x={:.1} co_w={:.1})",
            rect_right(avatar),
            rect_right(msg),
            content.layout_bounds[0],
            content.layout_bounds[2]
        );
        assert!(
            (avatar.layout_bounds[0] - rect_right(content)).abs() < 12.0,
            "gallery-like end: content fills up to the avatar"
        );
        let bubble = content
            .children
            .iter()
            .find(|c| c.attrs.get("data-slot").map(String::as_str) == Some("bubble"))
            .expect("bubble");
        assert!(
            (rect_right(bubble) - rect_right(content)).abs() < 4.0,
            "gallery-like end: bubble self-ends against the avatar"
        );
    }

    #[test]
    fn message_footer_justifies_end_when_align_end() {
        let el: Element = message()
            .align("end")
            .content_child(Element::new().text("Ping"))
            .footer("Delivered")
            .into();
        let footer = find_slot(&el, SLOT_FOOTER).expect("footer");
        assert_eq!(footer.style.justify_content, Some(JustifyContent::FLEX_END));
        let header = find_slot(&el, SLOT_HEADER);
        assert!(
            header.is_none()
                || header.unwrap().style.justify_content != Some(JustifyContent::FLEX_END)
        );
    }

    #[test]
    fn message_header_stays_start_justified_on_end() {
        let el: Element = message()
            .align("end")
            .header("Oliver")
            .content_child(Element::new().text("Hey"))
            .into();
        let header = find_slot(&el, SLOT_HEADER).expect("header");
        assert_eq!(
            header.style.align_self,
            Some(taffy::style::AlignSelf::FLEX_END),
            "header box self-ends with the bubble (*:data-slot:self-end)"
        );
        assert_ne!(
            header.style.justify_content,
            Some(JustifyContent::FLEX_END),
            "header content stays start-justified"
        );
        let leaf = header
            .children
            .iter()
            .find(|c| c.text.as_deref() == Some("Oliver"))
            .expect("header copy");
        assert_eq!(leaf.text_style.size_resolved(), HEADER_SIZE_PX);
    }

    #[test]
    fn empty_avatar_reserves_default_size() {
        let el: Element = message_avatar().into();
        assert_eq!(slot_name(&el), Some(SLOT_AVATAR));
        assert!(el.children.is_empty());
        assert_eq!(el.style.align_self, Some(taffy::style::AlignSelf::FLEX_END));
    }

    #[test]
    fn apply_bound_message_align_restyles_in_place() {
        let mut el: Element = message()
            .header("A")
            .content_child(Element::new().text("body"))
            .footer("sent")
            .into();
        assert_eq!(el.style.flex_direction, FlexDirection::Column);
        let status = apply_bound_message_align(&mut el, "end");
        assert_eq!(
            status,
            LayoutStatus::StructureChanged,
            "live align change must StructureChanged (LayoutChanged only dirties the column root)"
        );
        assert_eq!(el.style.flex_direction, FlexDirection::Column);
        let upper = find_slot(&el, SLOT_UPPER).expect("upper");
        assert_eq!(upper.style.flex_direction, FlexDirection::RowReverse);
        let footer = find_slot(&el, SLOT_FOOTER).expect("footer");
        assert_eq!(footer.style.justify_content, Some(JustifyContent::FLEX_END));
    }

    #[test]
    fn footer_row_keeps_avatar_off_actions() {
        let el: Element = message()
            .avatar(Element::new().text("OL"))
            .header("Oliver")
            .content_child(Element::new().text("Send the report."))
            .footer("Copy")
            .into();
        let upper = find_slot(&el, SLOT_UPPER).expect("upper");
        assert!(find_slot(upper, SLOT_AVATAR).is_some());
        assert!(find_slot(upper, SLOT_FOOTER).is_none());
        let footer_row = find_slot(&el, SLOT_FOOTER_ROW).expect("footer row");
        assert!(find_slot(footer_row, SLOT_FOOTER).is_some());
        assert!(find_slot(footer_row, SLOT_AVATAR_GHOST).is_some());
    }

    #[test]
    fn caption_preserves_bind_identity_and_skips_controls() {
        let mut bound = Element::new().text("align=start");
        bound.template_id = Some("gal-msg-align".into());
        let styled = caption_from_children(vec![bound], &header_text_config());
        assert_eq!(styled[0].template_id.as_deref(), Some("gal-msg-align"));
        assert_eq!(styled[0].text.as_deref(), Some("align=start"));

        let mut btn = Element::new();
        btn.children.push(Element::new().text("copy"));
        let kept = caption_from_children(vec![btn], &footer_text_config());
        assert_eq!(kept[0].children.len(), 1);
        assert!(kept[0].text.is_none());
    }

    #[test]
    fn builder_header_class_and_style() {
        let el: Element = message_header()
            .class("x")
            .style(|b| b.text_size(12.0))
            .text("Hi")
            .into();
        assert_eq!(slot_name(&el), Some(SLOT_HEADER));
        assert_eq!(el.text_style.size_resolved(), 12.0);
    }

    #[test]
    fn group_and_builder_assemble_slots() {
        let el: Element = message_group()
            .child(
                message()
                    .avatar(Element::new().text("CN"))
                    .header("Oliver")
                    .content_child(Element::new().text("Hi"))
                    .footer("Delivered"),
            )
            .into();
        assert_eq!(slot_name(&el), Some(SLOT_GROUP));
        let row = find_slot(&el, SLOT_ROOT).expect("row");
        assert!(find_slot(row, SLOT_AVATAR).is_some());
        assert!(find_slot(row, SLOT_CONTENT).is_some());
        assert!(find_slot(row, SLOT_HEADER).is_some());
        assert!(find_slot(row, SLOT_FOOTER).is_some());
    }
}

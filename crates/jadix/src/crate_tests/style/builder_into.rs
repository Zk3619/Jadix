#![cfg(feature = "product-tests")]
//! Builder chain dual-accept: strings stay the product default; typed values share the same method.

use crate::prelude::*;
use crate::style::{parse_color, CursorKind};
use taffy::style::Overflow;

#[test]
fn element_chain_accepts_string_and_typed_values() {
    let a = Element::new()
        .bg("primary")
        .pointer_events("none")
        .overflow("hidden")
        .justify("center")
        .text_align("right")
        .border_style("dashed")
        .text_overflow("ellipsis")
        .cursor("text");
    assert_eq!(a.style.bg_color, parse_color("primary"));
    assert_eq!(a.pointer_events_mode(), PointerEvents::None);
    assert_eq!(a.style.overflow.x, Overflow::Hidden);
    assert_eq!(
        a.style.justify_content,
        Some(taffy::prelude::JustifyContent::CENTER)
    );
    assert_eq!(a.text_style.align, Some(TextAlign::Right));
    assert_eq!(a.style.border_style, LineStyle::Dashed);
    assert_eq!(a.text_style.overflow, TextOverflow::Ellipsis);
    assert_eq!(a.cursor.as_ref().map(|c| c.kind), Some(CursorKind::Text));

    let primary = parse_color("primary");
    let b = Element::new()
        .bg(primary)
        .pointer_events(PointerEvents::None)
        .overflow(Overflow::Hidden)
        .text_align(TextAlign::Right)
        .border_style(LineStyle::Dashed)
        .text_overflow(TextOverflow::Ellipsis)
        .cursor(CursorKind::Text);
    assert_eq!(b.style.bg_color, primary);
    assert_eq!(b.pointer_events_mode(), PointerEvents::None);
    assert_eq!(b.style.overflow.x, Overflow::Hidden);
}

#[test]
fn inherit_text_align_still_clears_local_declaration() {
    let el = Element::new()
        .text_align(TextAlign::Center)
        .text_align("inherit");
    assert_eq!(el.text_style.align, None);
}

#[test]
fn button_variant_size_dual_accept() {
    let a: Element = button("a").variant("outline").size("sm").into();
    let b: Element = button("b")
        .variant(ButtonVariant::Outline)
        .size(ButtonSize::Sm)
        .into();
    assert_eq!(a.style.bg_color, b.style.bg_color);
    assert_eq!(a.style.size.height, b.style.size.height);
}

#[test]
fn toast_and_hstack_bg_accept_token_or_array() {
    let token = parse_color("muted");
    let box_el: Element = hstack().bg("muted").into();
    assert_eq!(box_el.style.bg_color, token);
    let box_el2: Element = hstack().bg(token).into();
    assert_eq!(box_el2.style.bg_color, token);
}

#[test]
fn component_variant_size_dual_accept() {
    use crate::components::{
        avatar, bubble, sidebar_menu_button, AvatarShape, AvatarSize, BadgeVariant, BubbleAlign,
        BubbleVariant, SidebarMenuButtonSize, SidebarMenuButtonVariant,
    };

    assert_eq!(BadgeVariant::from("outline"), BadgeVariant::Outline);
    let a: Element = badge().variant("outline").text("n").into();
    let b: Element = badge().variant(BadgeVariant::Outline).text("n").into();
    assert_eq!(a.attrs.get("data-variant"), b.attrs.get("data-variant"));

    assert_eq!(BubbleVariant::from("outline"), BubbleVariant::Outline);
    let _ = bubble().variant("outline").align("end");
    let _ = bubble()
        .variant(BubbleVariant::Outline)
        .align(BubbleAlign::End);

    assert_eq!(ScrollbarVariant::from("rect"), ScrollbarVariant::Rect);
    let _ = scrollbar("sb").variant("rect").visibility("always");
    let _ = scrollbar("sb")
        .variant(ScrollbarVariant::Rect)
        .visibility(ScrollbarVisibility::Always);

    assert_eq!(AvatarSize::from("sm"), AvatarSize::Sm);
    let _ = avatar().size("sm").shape("rounded");
    let _ = avatar()
        .size(AvatarSize::Sm)
        .shape(AvatarShape::RoundedSquare);

    let _ = sidebar_menu_button("nav")
        .variant("outline")
        .size("sm")
        .text("x");
    let _ = sidebar_menu_button("nav")
        .variant(SidebarMenuButtonVariant::Outline)
        .size(SidebarMenuButtonSize::Sm)
        .text("x");

    let _ = toast("ok").variant("success");
    let _ = toast("ok").variant(ToastTone::Success);
    let _ = toaster()
        .variant("list")
        .position("top-right")
        .overflow("wait");
    let _ = toaster()
        .variant(ToastLayout::List)
        .position(ToastPosition::TopRight)
        .overflow(ToastOverflow::Wait);

    assert_eq!(ToggleVariant::from("outline"), ToggleVariant::Outline);
    let _ = toggle("t").variant("outline").size("sm");
    let _ = toggle("t")
        .variant(ToggleVariant::Outline)
        .size(ToggleSize::Sm);
    let _ = toggle_group("g").variant("outline").size("lg");
    let _ = toggle_group("g")
        .variant(ToggleGroupVariant::Outline)
        .size(ToggleGroupSize::Lg);

    let _ = message_scroller_button().variant("ghost").size("xs");
    let _ = message_scroller_button()
        .variant(ButtonVariant::Ghost)
        .size(ButtonSize::Xs);
}

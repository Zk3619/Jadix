//! Semantic colors via the same parser as [`crate::style::parse_color`].
use crate::style::{parse_color, parse_text_color};

#[inline]
pub fn primary() -> [f32; 4] {
    parse_color("primary")
}

#[inline]
pub fn secondary() -> [f32; 4] {
    parse_color("secondary")
}

#[inline]
pub fn accent() -> [f32; 4] {
    parse_color("accent")
}

#[inline]
pub fn destructive() -> [f32; 4] {
    parse_color("destructive")
}

#[inline]
pub fn border() -> [f32; 4] {
    parse_color("border")
}

#[inline]
pub fn background() -> [f32; 4] {
    parse_color("background")
}

#[inline]
pub fn card() -> [f32; 4] {
    parse_color("card")
}

#[inline]
pub fn popover() -> [f32; 4] {
    parse_color("popover")
}

#[inline]
pub fn surface() -> [f32; 4] {
    parse_color("surface")
}

#[inline]
pub fn sidebar() -> [f32; 4] {
    parse_color("sidebar")
}

#[inline]
pub fn input() -> [f32; 4] {
    parse_color("input")
}

#[inline]
pub fn muted() -> [f32; 4] {
    parse_color("muted")
}

#[inline]
pub fn scrollbar_track() -> [f32; 4] {
    parse_color("scrollbar-track")
}

#[inline]
pub fn scrollbar_thumb() -> [f32; 4] {
    parse_color("scrollbar-thumb")
}

#[inline]
pub fn scrollbar_track_border() -> [f32; 4] {
    parse_color("scrollbar-track-border")
}

#[inline]
pub fn scrollbar_thumb_border() -> [f32; 4] {
    parse_color("scrollbar-thumb-border")
}

#[inline]
pub fn chart(n: u8) -> [f32; 4] {
    parse_color(match n {
        1 => "chart-1",
        2 => "chart-2",
        3 => "chart-3",
        4 => "chart-4",
        5 => "chart-5",
        _ => "chart-1",
    })
}

#[inline]
pub fn foreground_text() -> [u8; 4] {
    parse_text_color("foreground")
}

#[inline]
pub fn primary_foreground_text() -> [u8; 4] {
    parse_text_color("primary-foreground")
}

#[inline]
pub fn secondary_foreground_text() -> [u8; 4] {
    parse_text_color("secondary-foreground")
}

#[inline]
pub fn destructive_foreground_text() -> [u8; 4] {
    parse_text_color("destructive-foreground")
}

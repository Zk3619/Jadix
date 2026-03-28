//! Semantic colors via the same parser as [`crate::style::parse_color`].
use crate::style::{parse_color, parse_text_color};

#[inline]
pub fn primary() -> [f32; 4] {
    parse_color("primary")
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
pub fn input() -> [f32; 4] {
    parse_color("input")
}

#[inline]
pub fn muted() -> [f32; 4] {
    parse_color("muted")
}

#[inline]
pub fn foreground_text() -> [u8; 4] {
    parse_text_color("foreground")
}

#[inline]
pub fn primary_foreground_text() -> [u8; 4] {
    parse_text_color("primary-foreground")
}

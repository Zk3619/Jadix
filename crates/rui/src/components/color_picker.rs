//! # ColorPicker
//!
//! 颜色选择占位。对应 HTML `input type="color"`。
//! 落地后为色板 / 取色弹层，与文件、日期选择器无关。
//!
//! **当前仅为 API + 视觉壳**：色块 + 十六进制预览，不打开色板。
//!
//! ## 用法
//! ```ignore
//! <ColorPicker id="accent" value="#3b82f6" />
//! ```

use crate::components::picker_stub::{stub_label, stub_trigger};
use crate::core::tree::{parse_color, Element};
use crate::primitives::r#box::r#box;
use crate::tokens;
use rui_macros::component;

fn parse_hex_swatch(value: &str) -> [f32; 4] {
    let s = value.trim().trim_start_matches('#');
    if s.len() == 6 {
        if let (Ok(r), Ok(g), Ok(b)) = (
            u8::from_str_radix(&s[0..2], 16),
            u8::from_str_radix(&s[2..4], 16),
            u8::from_str_radix(&s[4..6], 16),
        ) {
            return [r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, 1.0];
        }
    }
    tokens::primary()
}

/// HTML `input type="color"` 占位：色块 + 十六进制预览。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn ColorPicker(
    id: String,
    value: Option<String>,
    disabled: bool,
    children: Option<Element>,
) -> Element {
    let _ = children;
    let hex = value.unwrap_or_else(|| "#3b82f6".into());
    let swatch_color = parse_hex_swatch(&hex);
    let mut swatch: Element = r#box().w("1.25rem").h("1.25rem").rounded("0.25rem").into();
    swatch = swatch.bg(swatch_color);
    swatch.style.border_width = 1.0;
    swatch.style.border_color = parse_color("border");
    let main = stub_label(&hex, false);
    stub_trigger(&id, swatch, main, disabled)
}

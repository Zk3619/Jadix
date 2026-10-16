//! Tailwind CSS **v4** 默认 `--text-*` 与 `--text-*--line-height` 的比值
//!（见 <https://tailwindcss.com/docs/font-size>：`line-height` 为 `calc(…)` 相对字号）。
//!
//! 未显式设置行高时，[`tailwind_default_leading_scale`] 返回「行高 ÷ 字号」，与 v4 预设一致。

/// `text-xs`：`calc(1 / 0.75)`
pub const TAILWIND_TEXT_XS_LEADING_SCALE: f32 = 1.0 / 0.75;
/// `text-sm`：`calc(1.25 / 0.875)`
pub const TAILWIND_TEXT_SM_LEADING_SCALE: f32 = 1.25 / 0.875;
/// `text-base`：`calc(1.5 / 1)`
pub const TAILWIND_TEXT_BASE_LEADING_SCALE: f32 = 1.5 / 1.0;
/// `text-lg`：`calc(1.75 / 1.125)`
pub const TAILWIND_TEXT_LG_LEADING_SCALE: f32 = 1.75 / 1.125;
/// `text-xl`：`calc(1.75 / 1.25)`
pub const TAILWIND_TEXT_XL_LEADING_SCALE: f32 = 1.75 / 1.25;
/// `text-2xl`：`calc(2 / 1.5)`
pub const TAILWIND_TEXT_2XL_LEADING_SCALE: f32 = 2.0 / 1.5;
/// `text-3xl`：`calc(2.25 / 1.875)`
pub const TAILWIND_TEXT_3XL_LEADING_SCALE: f32 = 2.25 / 1.875;
/// `text-4xl`：`calc(2.5 / 2.25)`
pub const TAILWIND_TEXT_4XL_LEADING_SCALE: f32 = 2.5 / 2.25;

/// 按字号（逻辑 px，假定 `1rem = 16px`）取最近一档 `text-*`，返回 v4 默认 **行高/字号**。
pub fn tailwind_default_leading_scale(font_size_px: f32) -> f32 {
    if !font_size_px.is_finite() || font_size_px <= 0.0 {
        return TAILWIND_TEXT_BASE_LEADING_SCALE;
    }
    let s = font_size_px;
    // (字号 px @ root 16px, Tailwind v4 默认 line-height / font-size)
    const TIERS: &[(f32, f32)] = &[
        (12.0, TAILWIND_TEXT_XS_LEADING_SCALE),
        (14.0, TAILWIND_TEXT_SM_LEADING_SCALE),
        (16.0, TAILWIND_TEXT_BASE_LEADING_SCALE),
        (18.0, TAILWIND_TEXT_LG_LEADING_SCALE),
        (20.0, TAILWIND_TEXT_XL_LEADING_SCALE),
        (24.0, TAILWIND_TEXT_2XL_LEADING_SCALE),
        (30.0, TAILWIND_TEXT_3XL_LEADING_SCALE),
        (36.0, TAILWIND_TEXT_4XL_LEADING_SCALE),
        (48.0, 1.0), // text-5xl … text-9xl：文档为 `line-height: 1`
        (60.0, 1.0),
        (72.0, 1.0),
        (96.0, 1.0),
        (128.0, 1.0), // text-9xl：8rem
    ];
    let mut best = f32::MAX;
    let mut scale = TAILWIND_TEXT_BASE_LEADING_SCALE;
    for &(sz, lh_over_fs) in TIERS {
        let d = (s - sz).abs();
        if d < best {
            best = d;
            scale = lh_over_fs;
        }
    }
    scale
}

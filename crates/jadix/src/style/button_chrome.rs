use super::{button_soft_semantic_chrome, color_mix_over_background, is_dark_theme, lerp_paint_chrome, parse_color, parse_text_color, PaintChrome, SemanticTone};

/// 文档站 Aria Button 采样配方（透明度叠底 → 不透明等价色）：
/// - default: `bg-primary` → hover `primary/80`
/// - secondary: `bg-secondary` → hover `secondary/80`
/// - success / warning / destructive: soft tint via [`button_soft_semantic_chrome`] / [`SemanticTone`]
/// - outline / ghost / link: see each branch
pub(crate) fn button_chrome_for_variant(
    variant: &str,
    hover_t: f32,
) -> ([f32; 4], [u8; 4], Option<[f32; 4]>) {
    let t = hover_t.clamp(0.0, 1.0);
    let dark = is_dark_theme();
    if let Some(tone) = match variant {
        "success" | "warning" | "destructive" | "info" => Some(SemanticTone::parse(variant)),
        _ => None,
    } {
        if tone.is_semantic() {
            return button_soft_semantic_chrome(tone, t);
        }
    }
    match variant {
        "default" => {
            let fg = parse_text_color("primary-foreground");
            let c = lerp_paint_chrome(
                PaintChrome {
                    bg: parse_color("primary"),
                    fg,
                    border: None,
                },
                PaintChrome {
                    bg: color_mix_over_background(parse_color("primary"), 0.8),
                    fg,
                    border: None,
                },
                t,
            );
            (c.bg, c.fg, c.border)
        }
        "secondary" => {
            let fg = parse_text_color("secondary-foreground");
            let c = lerp_paint_chrome(
                PaintChrome {
                    bg: parse_color("secondary"),
                    fg,
                    border: None,
                },
                PaintChrome {
                    bg: parse_color("secondary/80"),
                    fg,
                    border: None,
                },
                t,
            );
            (c.bg, c.fg, c.border)
        }
        "outline" => {
            let (rest_bg, hover_bg, border) = if dark {
                (
                    color_mix_over_background(parse_color("input"), 0.3),
                    color_mix_over_background(parse_color("input"), 0.5),
                    parse_color("input"),
                )
            } else {
                (
                    parse_color("background"),
                    parse_color("accent"),
                    parse_color("border"),
                )
            };
            let fg = parse_text_color("foreground");
            let c = lerp_paint_chrome(
                PaintChrome {
                    bg: rest_bg,
                    fg,
                    border: Some(border),
                },
                PaintChrome {
                    bg: hover_bg,
                    fg,
                    border: Some(border),
                },
                t,
            );
            (c.bg, c.fg, c.border)
        }
        "ghost" => {
            let hover_bg = if dark {
                { let mut color = parse_color("accent"); color[3] = 0.5; color }
            } else {
                parse_color("accent")
            };
            // 与 hover 同 RGB、α=0：CSS 非预乘插值只动 alpha，避免透明黑→有色脏闪。
            // Do not use `lerp_premultiplied` (it will fold into black when α≈0), the endpoints must retain RGB.
            let rest_bg = [hover_bg[0], hover_bg[1], hover_bg[2], 0.0];
            let bg = if t >= 0.5 { hover_bg } else { rest_bg };
            (bg, parse_text_color("foreground"), None)
        }
        "link" => ([0.0; 4], parse_text_color("primary"), None),
        _ => {
            let fg = parse_text_color("primary-foreground");
            let c = lerp_paint_chrome(
                PaintChrome {
                    bg: parse_color("primary"),
                    fg,
                    border: None,
                },
                PaintChrome {
                    bg: color_mix_over_background(parse_color("primary"), 0.8),
                    fg,
                    border: None,
                },
                t,
            );
            (c.bg, c.fg, c.border)
        }
    }
}

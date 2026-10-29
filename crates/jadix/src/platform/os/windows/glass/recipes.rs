//! Blur / Aero / Acrylic / Mica composition brushes (independent reimplementation).

use windows::core::{Interface, HSTRING};
use windows::Graphics::Effects::IGraphicsEffect;
use windows::UI::Composition::{CompositionBrush, CompositionEffectBrush, Compositor};
use windows_collections::IIterable;

use crate::app::glass_params::{
    clamp_unit, effective_luminosity_color, effective_tint_color, GlassTint,
};
use crate::app::{WindowBackdrop, WindowOptions};

use super::effects::{
    as_source, prop_f32, prop_u32, prop_vec4, source_param, D2dEffect, BLEND_COLOR,
    BLEND_LUMINOSITY, BORDER_MODE_SOFT, CLSID_BLEND, CLSID_COMPOSITE, CLSID_FLOOD,
    CLSID_GAUSSIAN_BLUR, CLSID_OPACITY, CLSID_SATURATION, COMPOSITE_SOURCE_OVER,
    GAUSSIAN_OPT_QUALITY,
};

pub fn create_recipe_brush(
    compositor: &Compositor,
    options: &WindowOptions,
    active: bool,
    dark: bool,
) -> windows::core::Result<CompositionBrush> {
    let tint = options.resolved_glass_tint_for(dark);
    let blur = options.resolved_blur_amount();
    match options.backdrop {
        WindowBackdrop::Aero => aero_brush(compositor, options, tint, blur, active, dark),
        WindowBackdrop::Mica => mica_brush(compositor, tint, options.luminosity_opacity, false),
        WindowBackdrop::MicaAlt => mica_brush(compositor, tint, options.luminosity_opacity, true),
        WindowBackdrop::Blur => blur_brush(compositor, tint, blur),
        _ => acrylic_brush(compositor, tint, options.luminosity_opacity, blur),
    }
}

fn argb_to_vec4(c: GlassTint) -> (f32, f32, f32, f32) {
    (
        c[1] as f32 / 255.0,
        c[2] as f32 / 255.0,
        c[3] as f32 / 255.0,
        c[0] as f32 / 255.0,
    )
}

fn flood(name: &str, c: GlassTint) -> windows::core::Result<IGraphicsEffect> {
    let (r, g, b, a) = argb_to_vec4(c);
    Ok(
        D2dEffect::new(CLSID_FLOOD, name, vec![prop_vec4(r, g, b, a)?], vec![])
            .into_graphics_effect(),
    )
}

fn gaussian(
    std_dev: f32,
    input: windows::Graphics::Effects::IGraphicsEffectSource,
) -> windows::core::Result<IGraphicsEffect> {
    Ok(D2dEffect::new(
        CLSID_GAUSSIAN_BLUR,
        "Blur",
        vec![
            prop_f32(std_dev.max(0.1))?,
            prop_u32(GAUSSIAN_OPT_QUALITY)?,
            prop_u32(BORDER_MODE_SOFT)?,
        ],
        vec![input],
    )
    .into_graphics_effect())
}

fn opacity(
    name: &str,
    amount: f32,
    input: windows::Graphics::Effects::IGraphicsEffectSource,
) -> windows::core::Result<IGraphicsEffect> {
    Ok(D2dEffect::new(
        CLSID_OPACITY,
        name,
        vec![prop_f32(clamp_unit(amount))?],
        vec![input],
    )
    .into_graphics_effect())
}

fn composite(
    mode: u32,
    dest: windows::Graphics::Effects::IGraphicsEffectSource,
    src: windows::Graphics::Effects::IGraphicsEffectSource,
) -> windows::core::Result<IGraphicsEffect> {
    Ok(D2dEffect::new(
        CLSID_COMPOSITE,
        "Composite",
        vec![prop_u32(mode)?],
        vec![dest, src],
    )
    .into_graphics_effect())
}

fn blend(
    name: &str,
    mode: u32,
    background: windows::Graphics::Effects::IGraphicsEffectSource,
    foreground: windows::Graphics::Effects::IGraphicsEffectSource,
) -> windows::core::Result<IGraphicsEffect> {
    Ok(D2dEffect::new(
        CLSID_BLEND,
        name,
        vec![prop_u32(mode)?],
        vec![background, foreground],
    )
    .into_graphics_effect())
}

fn host_backdrop(
    compositor: &Compositor,
) -> windows::core::Result<windows::UI::Composition::CompositionBackdropBrush> {
    compositor
        .CreateHostBackdropBrush()
        .or_else(|_| compositor.CreateBackdropBrush())
}

fn animatable_blur_names() -> IIterable<HSTRING> {
    IIterable::<HSTRING>::from(vec![
        HSTRING::from("Blur.BlurAmount"),
        HSTRING::from("Blur.StandardDeviation"),
    ])
}

fn factory_blur_brush(
    compositor: &Compositor,
    effect: IGraphicsEffect,
    backdrop_name: &str,
    std_dev: f32,
) -> windows::core::Result<CompositionBrush> {
    let names = animatable_blur_names();
    let factory = compositor
        .CreateEffectFactoryWithProperties(&effect, &names)
        .or_else(|_| compositor.CreateEffectFactory(&effect))?;
    let brush: CompositionEffectBrush = factory.CreateBrush()?;
    let backdrop = host_backdrop(compositor)?;
    brush.SetSourceParameter(&HSTRING::from(backdrop_name), &backdrop)?;
    if let Ok(props) = brush.Properties() {
        let v = std_dev.max(0.1);
        let _ = props.InsertScalar(&HSTRING::from("Blur.BlurAmount"), v);
        let _ = props.InsertScalar(&HSTRING::from("Blur.StandardDeviation"), v);
    }
    Interface::cast(&brush)
}

/// Gaussian(HostBackdrop) — frost fallback when a full recipe fails.
pub fn gaussian_host_brush(
    compositor: &Compositor,
    blur_amount: f32,
) -> windows::core::Result<CompositionBrush> {
    let std_dev = blur_amount.max(0.1);
    let g = gaussian(std_dev, source_param("Backdrop")?)?;
    factory_blur_brush(compositor, g, "Backdrop", std_dev)
}

fn blur_brush(
    compositor: &Compositor,
    tint: GlassTint,
    blur_amount: f32,
) -> windows::core::Result<CompositionBrush> {
    let effective = effective_tint_color(tint, 1.0, None);
    if effective[0] == 255 {
        return Interface::cast(&compositor.CreateColorBrushWithColor(windows::UI::Color {
            A: 255,
            R: effective[1],
            G: effective[2],
            B: effective[3],
        })?);
    }
    if blur_amount <= 0.0 {
        return Interface::cast(&compositor.CreateColorBrushWithColor(windows::UI::Color {
            A: effective[0],
            R: effective[1],
            G: effective[2],
            B: effective[3],
        })?);
    }
    let blurred = gaussian(blur_amount, source_param("Backdrop")?)?;
    if effective[0] == 0 {
        return factory_blur_brush(compositor, blurred, "Backdrop", blur_amount);
    }
    let tint_src = as_source(opacity(
        "TintOpacity",
        effective[0] as f32 / 255.0,
        as_source(flood(
            "TintColor",
            [255, effective[1], effective[2], effective[3]],
        )?)?,
    )?)?;
    let overlay = composite(COMPOSITE_SOURCE_OVER, as_source(blurred)?, tint_src)?;
    factory_blur_brush(compositor, overlay, "Backdrop", blur_amount)
}

fn acrylic_brush(
    compositor: &Compositor,
    tint: GlassTint,
    luminosity: Option<f32>,
    blur_amount: f32,
) -> windows::core::Result<CompositionBrush> {
    let tint_eff = effective_tint_color(tint, 1.0, luminosity);
    if tint_eff[0] == 255 {
        return Interface::cast(&compositor.CreateColorBrushWithColor(windows::UI::Color {
            A: 255,
            R: tint_eff[1],
            G: tint_eff[2],
            B: tint_eff[3],
        })?);
    }
    let lum_c = effective_luminosity_color(tint, 1.0, luminosity);
    let lum = flood("LuminosityColor", lum_c)?;
    let blurred = gaussian(blur_amount, source_param("Backdrop")?)?;
    // D2D Color/Luminosity names are historically swapped in this graph.
    let lum_blend = blend(
        "LuminosityBlend",
        BLEND_COLOR,
        as_source(blurred)?,
        as_source(lum)?,
    )?;
    let color_blend = blend(
        "TintBlend",
        BLEND_LUMINOSITY,
        as_source(lum_blend)?,
        as_source(flood("TintColor", tint_eff)?)?,
    )?;
    // Skip D2D Turbulence — WUC reports Unsupported effect type (0x80078057).
    factory_blur_brush(compositor, color_blend, "Backdrop", blur_amount)
}

fn mica_brush(
    compositor: &Compositor,
    tint: GlassTint,
    luminosity: Option<f32>,
    alt: bool,
) -> windows::core::Result<CompositionBrush> {
    let tint_eff = effective_tint_color(tint, 1.0, luminosity);
    let lum_c = effective_luminosity_color(tint, 1.0, luminosity);
    let wallpaper = source_param("BlurredWallpaperBackdrop")?;
    let backdrop_src = if alt {
        as_source(
            D2dEffect::new(
                CLSID_SATURATION,
                "MicaAltDesat",
                vec![prop_f32(0.0)?],
                vec![wallpaper],
            )
            .into_graphics_effect(),
        )?
    } else {
        wallpaper
    };
    let lum_blend = blend(
        "LuminosityBlend",
        BLEND_COLOR,
        backdrop_src,
        as_source(flood("LuminosityColor", lum_c)?)?,
    )?;
    let color_blend = blend(
        "TintBlend",
        BLEND_LUMINOSITY,
        as_source(lum_blend)?,
        as_source(flood("TintColor", tint_eff)?)?,
    )?;
    let factory = compositor.CreateEffectFactory(&color_blend)?;
    let brush = factory.CreateBrush()?;
    match compositor.TryCreateBlurredWallpaperBackdropBrush() {
        Ok(wp) => {
            brush.SetSourceParameter(&HSTRING::from("BlurredWallpaperBackdrop"), &wp)?;
            Interface::cast(&brush)
        }
        Err(e) => {
            eprintln!("jadix glass: Mica wallpaper brush failed ({e}); Acrylic fallback");
            acrylic_brush(compositor, tint, luminosity, 40.0)
        }
    }
}

fn aero_brush(
    compositor: &Compositor,
    _options: &WindowOptions,
    _tint: GlassTint,
    blur_amount: f32,
    _active: bool,
    _dark: bool,
) -> windows::core::Result<CompositionBrush> {
    // Color / afterglow / reflection are SpriteVisual overlays in `dcomp`.
    // WUC CreateEffectFactory rejects D2D Tint and Composite-PLUS (0x80078057).
    if blur_amount > 0.0 {
        gaussian_host_brush(compositor, blur_amount)
    } else {
        Interface::cast(&host_backdrop(compositor)?)
    }
}

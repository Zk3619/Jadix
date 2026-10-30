//! Validation of a surface configuration against its capabilities, including
//! resolving `SurfaceColorSpace::Auto` (and present/alpha `Auto`) to concrete
//! values. Split out of the very large `resource.rs`.

use crate::__core::{api_log, present};
use crate::__types::TextureFormat;

/// The concrete color space [`SurfaceColorSpace::Auto`] resolves to for `format`,
/// given the color spaces a surface supports for it, or `None` if `Auto` cannot
/// be satisfied.
///
/// Reproduces wgpu's historical behavior: extended linear scRGB for fp16 formats
/// when supported, sRGB otherwise. `Auto` never resolves to a wide-gamut or HDR
/// color space (DisplayP3, ExtendedSrgb, ExtendedDisplayP3, Bt2100Pq, Bt2100Hlg),
/// because those change how the application must encode its output, so they must
/// be requested explicitly.
///
/// This is the single source of truth shared by [`validate_surface_configuration`]
/// and the `get_capabilities` `formats` filter, so a format is listed in
/// [`SurfaceCapabilities::formats`] exactly when `Auto` resolves for it.
///
/// [`SurfaceColorSpace::Auto`]: wgt::SurfaceColorSpace::Auto
/// [`SurfaceCapabilities::formats`]: wgt::SurfaceCapabilities::formats
pub(crate) fn resolve_auto_color_space(
    format: TextureFormat,
    color_spaces: crate::__types::SurfaceColorSpaces,
) -> Option<crate::__types::SurfaceColorSpace> {
    let fallbacks: &[_] = if format == TextureFormat::Rgba16Float {
        &[
            crate::__types::SurfaceColorSpace::ExtendedSrgbLinear,
            crate::__types::SurfaceColorSpace::Srgb,
        ]
    } else {
        &[crate::__types::SurfaceColorSpace::Srgb]
    };
    fallbacks
        .iter()
        .copied()
        .find(|fallback| color_spaces.contains(fallback.to_color_spaces().unwrap()))
}

/// Validate `config` against `caps`, resolving the `Auto` values in
/// `config` to concrete ones.
pub(crate) fn validate_surface_configuration(
    config: &mut crate::__hal::SurfaceConfiguration,
    caps: &crate::__hal::SurfaceCapabilities,
    max_texture_dimension_2d: u32,
) -> Result<(), present::ConfigureSurfaceError> {
    use present::ConfigureSurfaceError as E;
    let width = config.extent.width;
    let height = config.extent.height;

    if width > max_texture_dimension_2d || height > max_texture_dimension_2d {
        return Err(E::TooLarge {
            width,
            height,
            max_texture_dimension_2d,
        });
    }

    if !caps.present_modes.contains(&config.present_mode) {
        // Automatic present mode checks.
        //
        // The "Automatic" modes are never supported by the backends.
        let fallbacks = match config.present_mode {
            crate::__types::PresentMode::AutoVsync => &[
                crate::__types::PresentMode::FifoRelaxed,
                crate::__types::PresentMode::Fifo,
            ][..],
            // Always end in FIFO to make sure it's always supported
            crate::__types::PresentMode::AutoNoVsync => &[
                crate::__types::PresentMode::Immediate,
                crate::__types::PresentMode::Mailbox,
                crate::__types::PresentMode::Fifo,
            ][..],
            _ => {
                return Err(E::UnsupportedPresentMode {
                    requested: config.present_mode,
                    available: caps.present_modes.clone(),
                });
            }
        };

        let new_mode = fallbacks
            .iter()
            .copied()
            .find(|fallback| caps.present_modes.contains(fallback))
            .unwrap_or_else(|| {
                unreachable!(
                    "Fallback system failed to choose present mode. \
                    This is a bug. Mode: {:?}, Options: {:?}",
                    config.present_mode, &caps.present_modes
                );
            });

        api_log!(
            "Automatically choosing presentation mode by rule {:?}. Chose {new_mode:?}",
            config.present_mode
        );
        config.present_mode = new_mode;
    }
    let Some(format_caps) = caps.formats.iter().find(|fc| fc.format == config.format) else {
        return Err(E::UnsupportedFormat {
            requested: config.format,
            available: caps.texture_formats().collect(),
        });
    };
    if config.color_space == crate::__types::SurfaceColorSpace::Auto {
        let Some(new_color_space) =
            resolve_auto_color_space(config.format, format_caps.color_spaces)
        else {
            // The format is only available in color spaces that must be
            // explicitly requested (e.g. HDR10-only on some drivers when the OS
            // is in HDR mode).
            return Err(E::UnsupportedColorSpace {
                requested: config.color_space,
                format: config.format,
                available: format_caps.color_spaces,
            });
        };

        api_log!(
            "Automatically choosing color space by rule {:?}. Chose {new_color_space:?}",
            config.color_space
        );
        config.color_space = new_color_space;
    }
    if !format_caps
        .color_spaces
        .contains(config.color_space.to_color_spaces().unwrap())
    {
        return Err(E::UnsupportedColorSpace {
            requested: config.color_space,
            format: config.format,
            available: format_caps.color_spaces,
        });
    }
    if !caps
        .composite_alpha_modes
        .contains(&config.composite_alpha_mode)
    {
        let new_alpha_mode = 'alpha: {
            // Automatic alpha mode checks.
            let fallbacks = match config.composite_alpha_mode {
                crate::__types::CompositeAlphaMode::Auto => &[
                    crate::__types::CompositeAlphaMode::Opaque,
                    crate::__types::CompositeAlphaMode::Inherit,
                ][..],
                _ => {
                    return Err(E::UnsupportedAlphaMode {
                        requested: config.composite_alpha_mode,
                        available: caps.composite_alpha_modes.clone(),
                    });
                }
            };

            for &fallback in fallbacks {
                if caps.composite_alpha_modes.contains(&fallback) {
                    break 'alpha fallback;
                }
            }

            unreachable!(
                "Fallback system failed to choose alpha mode. This is a bug. \
                          AlphaMode: {:?}, Options: {:?}",
                config.composite_alpha_mode, &caps.composite_alpha_modes
            );
        };

        api_log!(
            "Automatically choosing alpha mode by rule {:?}. Chose {new_alpha_mode:?}",
            config.composite_alpha_mode
        );
        config.composite_alpha_mode = new_alpha_mode;
    }
    if !caps.usage.contains(config.usage) {
        return Err(E::UnsupportedUsage {
            requested: config.usage,
            available: caps.usage,
        });
    }
    if width == 0 || height == 0 {
        return Err(E::ZeroArea);
    }
    Ok(())
}

#[cfg(test)]
mod surface_configuration_tests {
    use alloc::{vec, vec::Vec};

    use super::validate_surface_configuration;
    use crate::__core::present::ConfigureSurfaceError;

    fn caps(
        formats: Vec<crate::__types::SurfaceFormatCapabilities>,
    ) -> crate::__hal::SurfaceCapabilities {
        crate::__hal::SurfaceCapabilities {
            formats,
            maximum_frame_latency: 1..=3,
            current_extent: None,
            usage: crate::__types::TextureUses::COLOR_TARGET,
            present_modes: vec![crate::__types::PresentMode::Fifo],
            composite_alpha_modes: vec![crate::__types::CompositeAlphaMode::Opaque],
        }
    }

    fn config(
        format: crate::__types::TextureFormat,
        color_space: crate::__types::SurfaceColorSpace,
    ) -> crate::__hal::SurfaceConfiguration {
        crate::__hal::SurfaceConfiguration {
            maximum_frame_latency: 2,
            present_mode: crate::__types::PresentMode::Fifo,
            composite_alpha_mode: crate::__types::CompositeAlphaMode::Opaque,
            format,
            color_space,
            extent: crate::__types::Extent3d {
                width: 100,
                height: 100,
                depth_or_array_layers: 1,
            },
            usage: crate::__types::TextureUses::COLOR_TARGET,
            view_formats: Vec::new(),
        }
    }

    fn format_caps(
        format: crate::__types::TextureFormat,
        color_spaces: crate::__types::SurfaceColorSpaces,
    ) -> crate::__types::SurfaceFormatCapabilities {
        crate::__types::SurfaceFormatCapabilities {
            format,
            color_spaces,
        }
    }

    /// `Auto` resolves to extended linear scRGB for fp16 when supported,
    /// reproducing the historical hardcoded behavior.
    #[test]
    fn auto_resolves_fp16_to_extended_srgb_linear() {
        let caps = caps(vec![format_caps(
            crate::__types::TextureFormat::Rgba16Float,
            crate::__types::SurfaceColorSpaces::EXTENDED_SRGB_LINEAR
                | crate::__types::SurfaceColorSpaces::BT2100_PQ,
        )]);
        let mut config = config(
            crate::__types::TextureFormat::Rgba16Float,
            crate::__types::SurfaceColorSpace::Auto,
        );
        validate_surface_configuration(&mut config, &caps, 4096).unwrap();
        assert_eq!(
            config.color_space,
            crate::__types::SurfaceColorSpace::ExtendedSrgbLinear
        );
    }

    /// `Auto` never resolves to the encoded extended-range sRGB color space,
    /// even for fp16 formats: it prefers extended *linear* sRGB and falls back
    /// to plain sRGB, but `ExtendedSrgb` must be requested explicitly.
    #[test]
    fn auto_never_resolves_to_extended_srgb() {
        let caps = caps(vec![format_caps(
            crate::__types::TextureFormat::Rgba16Float,
            crate::__types::SurfaceColorSpaces::SRGB
                | crate::__types::SurfaceColorSpaces::EXTENDED_SRGB,
        )]);
        let mut config = config(
            crate::__types::TextureFormat::Rgba16Float,
            crate::__types::SurfaceColorSpace::Auto,
        );
        validate_surface_configuration(&mut config, &caps, 4096).unwrap();
        assert_eq!(config.color_space, crate::__types::SurfaceColorSpace::Srgb);
    }

    /// `Auto` resolves fp16 to sRGB when extended linear is unavailable
    /// (e.g. the GLES backend).
    #[test]
    fn auto_resolves_fp16_to_srgb_without_extended() {
        let caps = caps(vec![format_caps(
            crate::__types::TextureFormat::Rgba16Float,
            crate::__types::SurfaceColorSpaces::SRGB,
        )]);
        let mut config = config(
            crate::__types::TextureFormat::Rgba16Float,
            crate::__types::SurfaceColorSpace::Auto,
        );
        validate_surface_configuration(&mut config, &caps, 4096).unwrap();
        assert_eq!(config.color_space, crate::__types::SurfaceColorSpace::Srgb);
    }

    /// `Auto` never resolves to an HDR color space, even if it is the only
    /// one supported for the format: HDR output changes how the application
    /// must encode its colors, so it must be requested explicitly.
    #[test]
    fn auto_refuses_hdr_only_formats() {
        let caps = caps(vec![format_caps(
            crate::__types::TextureFormat::Rgb10a2Unorm,
            crate::__types::SurfaceColorSpaces::BT2100_PQ,
        )]);
        let mut config = config(
            crate::__types::TextureFormat::Rgb10a2Unorm,
            crate::__types::SurfaceColorSpace::Auto,
        );
        let err = validate_surface_configuration(&mut config, &caps, 4096).unwrap_err();
        assert!(matches!(
            err,
            ConfigureSurfaceError::UnsupportedColorSpace { .. }
        ));
    }

    /// `Auto` prefers sRGB for non-fp16 formats even when HDR spaces are
    /// also supported.
    #[test]
    fn auto_prefers_srgb_for_non_fp16() {
        let caps = caps(vec![format_caps(
            crate::__types::TextureFormat::Rgb10a2Unorm,
            crate::__types::SurfaceColorSpaces::SRGB
                | crate::__types::SurfaceColorSpaces::BT2100_PQ,
        )]);
        let mut config = config(
            crate::__types::TextureFormat::Rgb10a2Unorm,
            crate::__types::SurfaceColorSpace::Auto,
        );
        validate_surface_configuration(&mut config, &caps, 4096).unwrap();
        assert_eq!(config.color_space, crate::__types::SurfaceColorSpace::Srgb);
    }

    /// A non-fp16 format that reports *both* sRGB and extended-linear sRGB still
    /// resolves `Auto` deterministically to `Srgb`: the extended-linear fallback
    /// is gated on `Rgba16Float`, so there is never ambiguity about which color
    /// space `Auto` picks even when a format advertises both.
    #[test]
    fn auto_non_fp16_with_srgb_and_extended_linear_resolves_to_srgb() {
        let caps = caps(vec![format_caps(
            crate::__types::TextureFormat::Rgb10a2Unorm,
            crate::__types::SurfaceColorSpaces::SRGB
                | crate::__types::SurfaceColorSpaces::EXTENDED_SRGB_LINEAR,
        )]);
        let mut config = config(
            crate::__types::TextureFormat::Rgb10a2Unorm,
            crate::__types::SurfaceColorSpace::Auto,
        );
        validate_surface_configuration(&mut config, &caps, 4096).unwrap();
        assert_eq!(config.color_space, crate::__types::SurfaceColorSpace::Srgb);
    }

    /// Explicitly requested color spaces are honored when supported.
    #[test]
    fn explicit_hdr10_is_honored() {
        let caps = caps(vec![format_caps(
            crate::__types::TextureFormat::Rgb10a2Unorm,
            crate::__types::SurfaceColorSpaces::SRGB
                | crate::__types::SurfaceColorSpaces::BT2100_PQ,
        )]);
        let mut config = config(
            crate::__types::TextureFormat::Rgb10a2Unorm,
            crate::__types::SurfaceColorSpace::Bt2100Pq,
        );
        validate_surface_configuration(&mut config, &caps, 4096).unwrap();
        assert_eq!(
            config.color_space,
            crate::__types::SurfaceColorSpace::Bt2100Pq
        );
    }

    /// Explicitly requesting an unsupported color space fails validation.
    #[test]
    fn explicit_unsupported_color_space_errors() {
        let caps = caps(vec![format_caps(
            crate::__types::TextureFormat::Bgra8UnormSrgb,
            crate::__types::SurfaceColorSpaces::SRGB,
        )]);
        let mut config = config(
            crate::__types::TextureFormat::Bgra8UnormSrgb,
            crate::__types::SurfaceColorSpace::Bt2100Pq,
        );
        let err = validate_surface_configuration(&mut config, &caps, 4096).unwrap_err();
        assert!(matches!(
            err,
            ConfigureSurfaceError::UnsupportedColorSpace {
                requested: crate::__types::SurfaceColorSpace::Bt2100Pq,
                ..
            }
        ));
    }
}

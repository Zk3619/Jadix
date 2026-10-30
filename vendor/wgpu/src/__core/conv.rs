use crate::__types::TextureFormatFeatures;

use crate::__core::resource::{self, TextureDescriptor};

// Some core-only texture format helpers. The helper methods on `TextureFormat`
// defined in wgpu-types may need to be modified along with the ones here.

#[cfg_attr(any(not(jadix_wgpu_core_webgl)), expect(unused))]
pub fn is_valid_external_image_copy_dst_texture_format(
    format: crate::__types::TextureFormat,
) -> bool {
    use crate::__types::TextureFormat as Tf;
    match format {
        Tf::R8Unorm
        | Tf::R16Float
        | Tf::R32Float
        | Tf::Rg8Unorm
        | Tf::Rg16Float
        | Tf::Rg32Float
        | Tf::Rgba8Unorm
        | Tf::Rgba8UnormSrgb
        | Tf::Bgra8Unorm
        | Tf::Bgra8UnormSrgb
        | Tf::Rgb10a2Unorm
        | Tf::Rgba16Float
        | Tf::Rgba32Float => true,
        _ => false,
    }
}

pub fn map_buffer_usage(usage: crate::__types::BufferUsages) -> crate::__types::BufferUses {
    let mut u = crate::__types::BufferUses::empty();
    u.set(
        crate::__types::BufferUses::MAP_READ,
        usage.contains(crate::__types::BufferUsages::MAP_READ),
    );
    u.set(
        crate::__types::BufferUses::MAP_WRITE,
        usage.contains(crate::__types::BufferUsages::MAP_WRITE),
    );
    u.set(
        crate::__types::BufferUses::COPY_SRC,
        usage.contains(crate::__types::BufferUsages::COPY_SRC),
    );
    u.set(
        crate::__types::BufferUses::COPY_DST,
        usage.contains(crate::__types::BufferUsages::COPY_DST),
    );
    u.set(
        crate::__types::BufferUses::INDEX,
        usage.contains(crate::__types::BufferUsages::INDEX),
    );
    u.set(
        crate::__types::BufferUses::VERTEX,
        usage.contains(crate::__types::BufferUsages::VERTEX),
    );
    u.set(
        crate::__types::BufferUses::UNIFORM,
        usage.contains(crate::__types::BufferUsages::UNIFORM),
    );
    u.set(
        crate::__types::BufferUses::STORAGE_READ_ONLY
            | crate::__types::BufferUses::STORAGE_READ_WRITE,
        usage.contains(crate::__types::BufferUsages::STORAGE),
    );
    u.set(
        crate::__types::BufferUses::INDIRECT,
        usage.contains(crate::__types::BufferUsages::INDIRECT),
    );
    u.set(
        crate::__types::BufferUses::QUERY_RESOLVE,
        usage.contains(crate::__types::BufferUsages::QUERY_RESOLVE),
    );
    u.set(
        crate::__types::BufferUses::BOTTOM_LEVEL_ACCELERATION_STRUCTURE_INPUT,
        usage.contains(crate::__types::BufferUsages::BLAS_INPUT),
    );
    u.set(
        crate::__types::BufferUses::TOP_LEVEL_ACCELERATION_STRUCTURE_INPUT,
        usage.contains(crate::__types::BufferUsages::TLAS_INPUT),
    );
    u
}

pub fn map_texture_usage(
    usage: crate::__types::TextureUsages,
    aspect: crate::__hal::FormatAspects,
    flags: crate::__types::TextureFormatFeatureFlags,
) -> crate::__types::TextureUses {
    let mut u = crate::__types::TextureUses::empty();
    u.set(
        crate::__types::TextureUses::COPY_SRC,
        usage.contains(crate::__types::TextureUsages::COPY_SRC),
    );
    u.set(
        crate::__types::TextureUses::COPY_DST,
        usage.contains(crate::__types::TextureUsages::COPY_DST),
    );
    u.set(
        crate::__types::TextureUses::RESOURCE,
        usage.contains(crate::__types::TextureUsages::TEXTURE_BINDING),
    );
    if usage.contains(crate::__types::TextureUsages::STORAGE_BINDING) {
        u.set(
            crate::__types::TextureUses::STORAGE_READ_ONLY,
            flags.contains(crate::__types::TextureFormatFeatureFlags::STORAGE_READ_ONLY),
        );
        u.set(
            crate::__types::TextureUses::STORAGE_WRITE_ONLY,
            flags.contains(crate::__types::TextureFormatFeatureFlags::STORAGE_WRITE_ONLY),
        );
        u.set(
            crate::__types::TextureUses::STORAGE_READ_WRITE,
            flags.contains(crate::__types::TextureFormatFeatureFlags::STORAGE_READ_WRITE),
        );
    }
    let is_color = aspect.intersects(
        crate::__hal::FormatAspects::COLOR
            | crate::__hal::FormatAspects::PLANE_0
            | crate::__hal::FormatAspects::PLANE_1
            | crate::__hal::FormatAspects::PLANE_2,
    );
    u.set(
        crate::__types::TextureUses::COLOR_TARGET,
        usage.contains(crate::__types::TextureUsages::RENDER_ATTACHMENT) && is_color,
    );
    u.set(
        crate::__types::TextureUses::DEPTH_STENCIL_READ
            | crate::__types::TextureUses::DEPTH_STENCIL_WRITE,
        usage.contains(crate::__types::TextureUsages::RENDER_ATTACHMENT) && !is_color,
    );
    u.set(
        crate::__types::TextureUses::STORAGE_ATOMIC,
        usage.contains(crate::__types::TextureUsages::STORAGE_ATOMIC),
    );
    u.set(
        crate::__types::TextureUses::TRANSIENT,
        usage.contains(crate::__types::TextureUsages::TRANSIENT_ATTACHMENT),
    );
    u
}

pub fn map_texture_usage_for_texture(
    desc: &TextureDescriptor,
    format_features: &TextureFormatFeatures,
) -> crate::__types::TextureUses {
    // Enforce having COPY_DST/DEPTH_STENCIL_WRITE/COLOR_TARGET otherwise we
    // wouldn't be able to initialize the texture.
    map_texture_usage(desc.usage, desc.format.into(), format_features.flags)
        | if desc.format.is_depth_stencil_format() {
            crate::__types::TextureUses::DEPTH_STENCIL_WRITE
        } else if desc.usage.contains(crate::__types::TextureUsages::COPY_DST) {
            crate::__types::TextureUses::COPY_DST // (set already)
        } else {
            // Use COPY_DST only if we can't use COLOR_TARGET
            if format_features
                .allowed_usages
                .contains(crate::__types::TextureUsages::RENDER_ATTACHMENT)
                && desc.dimension == crate::__types::TextureDimension::D2
            // Render targets dimension must be 2d
            {
                crate::__types::TextureUses::COLOR_TARGET
            } else {
                crate::__types::TextureUses::COPY_DST
            }
        }
}

pub fn map_texture_usage_from_hal(
    uses: crate::__types::TextureUses,
) -> crate::__types::TextureUsages {
    let mut u = crate::__types::TextureUsages::empty();
    u.set(
        crate::__types::TextureUsages::COPY_SRC,
        uses.contains(crate::__types::TextureUses::COPY_SRC),
    );
    u.set(
        crate::__types::TextureUsages::COPY_DST,
        uses.contains(crate::__types::TextureUses::COPY_DST),
    );
    u.set(
        crate::__types::TextureUsages::TEXTURE_BINDING,
        uses.contains(crate::__types::TextureUses::RESOURCE),
    );
    u.set(
        crate::__types::TextureUsages::STORAGE_BINDING,
        uses.intersects(
            crate::__types::TextureUses::STORAGE_READ_ONLY
                | crate::__types::TextureUses::STORAGE_WRITE_ONLY
                | crate::__types::TextureUses::STORAGE_READ_WRITE,
        ),
    );
    u.set(
        crate::__types::TextureUsages::RENDER_ATTACHMENT,
        uses.contains(crate::__types::TextureUses::COLOR_TARGET),
    );
    u.set(
        crate::__types::TextureUsages::STORAGE_ATOMIC,
        uses.contains(crate::__types::TextureUses::STORAGE_ATOMIC),
    );
    u.set(
        crate::__types::TextureUsages::TRANSIENT_ATTACHMENT,
        uses.contains(crate::__types::TextureUses::TRANSIENT),
    );
    u
}

/// Check the requested texture size against the supported limits.
///
/// This function implements the texture size and sample count checks in [vtd
/// dimension step]. The format checks are elsewhere in [`create_texture`]`.
///
/// Note that while there is some basic checking of the sample count here, there
/// is an additional set of checks when `sample_count > 1` elsewhere in
/// [`create_texture`]`.
///
/// [vtd dimension step]: https://www.w3.org/TR/2025/CRD-webgpu-20251120/#:~:text=or%204.-,If%20descriptor.dimension%20is
/// [`create_texture`]: crate::device::Device::create_texture
pub fn check_texture_dimension_size(
    dimension: crate::__types::TextureDimension,
    crate::__types::Extent3d {
        width,
        height,
        depth_or_array_layers,
    }: crate::__types::Extent3d,
    sample_size: u32,
    limits: &crate::__types::Limits,
) -> Result<(), resource::TextureDimensionError> {
    use crate::__types::TextureDimension::*;
    use resource::{TextureDimensionError as Tde, TextureErrorDimension as Ted};

    let (extent_limits, sample_limit) = match dimension {
        D1 => ([limits.max_texture_dimension_1d, 1, 1], 1),
        D2 => (
            [
                limits.max_texture_dimension_2d,
                limits.max_texture_dimension_2d,
                limits.max_texture_array_layers,
            ],
            32,
        ),
        D3 => (
            [
                limits.max_texture_dimension_3d,
                limits.max_texture_dimension_3d,
                limits.max_texture_dimension_3d,
            ],
            1,
        ),
    };

    for (&dim, (&given, &limit)) in [Ted::X, Ted::Y, Ted::Z].iter().zip(
        [width, height, depth_or_array_layers]
            .iter()
            .zip(extent_limits.iter()),
    ) {
        if given == 0 {
            return Err(Tde::Zero(dim));
        }
        if given > limit {
            return Err(Tde::LimitExceeded { dim, given, limit });
        }
    }
    if sample_size == 0 || sample_size > sample_limit || !sample_size.is_power_of_two() {
        return Err(Tde::InvalidSampleCount(sample_size));
    }

    Ok(())
}

pub fn bind_group_layout_flags(
    features: crate::__types::Features,
) -> crate::__hal::BindGroupLayoutFlags {
    let mut flags = crate::__hal::BindGroupLayoutFlags::empty();
    flags.set(
        crate::__hal::BindGroupLayoutFlags::PARTIALLY_BOUND,
        features.contains(crate::__types::Features::PARTIALLY_BOUND_BINDING_ARRAY),
    );
    flags
}

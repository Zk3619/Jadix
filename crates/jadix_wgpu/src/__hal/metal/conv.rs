use objc2::rc::Retained;
use objc2_foundation::{NSArray, NSRange};
use objc2_metal::{
    MTLAccelerationStructureBoundingBoxGeometryDescriptor, MTLAccelerationStructureDescriptor,
    MTLAccelerationStructureGeometryDescriptor, MTLAccelerationStructureInstanceDescriptorType,
    MTLAccelerationStructureTriangleGeometryDescriptor, MTLAccelerationStructureUsage,
    MTLAttributeFormat, MTLBlendFactor, MTLBlendOperation, MTLBlitOption, MTLClearColor,
    MTLColorWriteMask, MTLCompareFunction, MTLCullMode, MTLIndexType,
    MTLInstanceAccelerationStructureDescriptor, MTLOrigin,
    MTLPrimitiveAccelerationStructureDescriptor, MTLPrimitiveTopologyClass, MTLPrimitiveType,
    MTLRenderStages, MTLResourceUsage, MTLSamplerAddressMode, MTLSamplerBorderColor,
    MTLSamplerMinMagFilter, MTLSize, MTLStencilOperation, MTLStoreAction, MTLTextureType,
    MTLTextureUsage, MTLVertexFormat, MTLVertexStepFunction, MTLWinding,
};

pub fn map_texture_usage(
    format: crate::__types::TextureFormat,
    usage: crate::__types::TextureUses,
) -> MTLTextureUsage {
    use crate::__types::TextureUses as Tu;

    let mut mtl_usage = MTLTextureUsage::Unknown;

    mtl_usage.set(
        MTLTextureUsage::RenderTarget,
        usage.intersects(Tu::COLOR_TARGET | Tu::DEPTH_STENCIL_READ | Tu::DEPTH_STENCIL_WRITE),
    );
    mtl_usage.set(
        MTLTextureUsage::ShaderRead,
        usage.intersects(
            Tu::RESOURCE | Tu::DEPTH_STENCIL_READ | Tu::STORAGE_READ_ONLY | Tu::STORAGE_READ_WRITE,
        ),
    );
    mtl_usage.set(
        MTLTextureUsage::ShaderWrite,
        usage.intersects(Tu::STORAGE_WRITE_ONLY | Tu::STORAGE_READ_WRITE),
    );
    // needed for combined depth/stencil formats since we might
    // create a stencil-only view from them
    mtl_usage.set(
        MTLTextureUsage::PixelFormatView,
        format.is_combined_depth_stencil_format(),
    );

    mtl_usage.set(
        MTLTextureUsage::ShaderAtomic,
        usage.intersects(Tu::STORAGE_ATOMIC),
    );

    mtl_usage
}

pub fn map_texture_view_dimension(dim: crate::__types::TextureViewDimension) -> MTLTextureType {
    use crate::__types::TextureViewDimension as Tvd;
    use MTLTextureType as MTL;
    match dim {
        Tvd::D1 => MTL::Type1D,
        Tvd::D2 => MTL::Type2D,
        Tvd::D2Array => MTL::Type2DArray,
        Tvd::D3 => MTL::Type3D,
        Tvd::Cube => MTL::TypeCube,
        Tvd::CubeArray => MTL::TypeCubeArray,
    }
}

pub fn map_compare_function(fun: crate::__types::CompareFunction) -> MTLCompareFunction {
    use crate::__types::CompareFunction as Cf;
    use MTLCompareFunction as MTL;
    match fun {
        Cf::Never => MTL::Never,
        Cf::Less => MTL::Less,
        Cf::LessEqual => MTL::LessEqual,
        Cf::Equal => MTL::Equal,
        Cf::GreaterEqual => MTL::GreaterEqual,
        Cf::Greater => MTL::Greater,
        Cf::NotEqual => MTL::NotEqual,
        Cf::Always => MTL::Always,
    }
}

pub fn map_filter_mode(filter: crate::__types::FilterMode) -> MTLSamplerMinMagFilter {
    use MTLSamplerMinMagFilter as MTL;
    match filter {
        crate::__types::FilterMode::Nearest => MTL::Nearest,
        crate::__types::FilterMode::Linear => MTL::Linear,
    }
}

pub fn map_address_mode(address: crate::__types::AddressMode) -> MTLSamplerAddressMode {
    use crate::__types::AddressMode as Fm;
    use MTLSamplerAddressMode as MTL;
    match address {
        Fm::Repeat => MTL::Repeat,
        Fm::MirrorRepeat => MTL::MirrorRepeat,
        Fm::ClampToEdge => MTL::ClampToEdge,
        Fm::ClampToBorder => MTL::ClampToBorderColor,
        //Fm::MirrorClamp => MTL::MirrorClampToEdge,
    }
}

pub fn map_border_color(border_color: crate::__types::SamplerBorderColor) -> MTLSamplerBorderColor {
    use MTLSamplerBorderColor as MTL;
    match border_color {
        crate::__types::SamplerBorderColor::TransparentBlack => MTL::TransparentBlack,
        crate::__types::SamplerBorderColor::OpaqueBlack => MTL::OpaqueBlack,
        crate::__types::SamplerBorderColor::OpaqueWhite => MTL::OpaqueWhite,
        crate::__types::SamplerBorderColor::Zero => unreachable!(),
    }
}

pub fn map_primitive_topology(
    topology: crate::__types::PrimitiveTopology,
) -> (MTLPrimitiveTopologyClass, MTLPrimitiveType) {
    use crate::__types::PrimitiveTopology as Pt;
    match topology {
        Pt::PointList => (MTLPrimitiveTopologyClass::Point, MTLPrimitiveType::Point),
        Pt::LineList => (MTLPrimitiveTopologyClass::Line, MTLPrimitiveType::Line),
        Pt::LineStrip => (MTLPrimitiveTopologyClass::Line, MTLPrimitiveType::LineStrip),
        Pt::TriangleList => (
            MTLPrimitiveTopologyClass::Triangle,
            MTLPrimitiveType::Triangle,
        ),
        Pt::TriangleStrip => (
            MTLPrimitiveTopologyClass::Triangle,
            MTLPrimitiveType::TriangleStrip,
        ),
    }
}

pub fn map_color_write(mask: crate::__types::ColorWrites) -> MTLColorWriteMask {
    let mut raw_mask = MTLColorWriteMask::empty();

    if mask.contains(crate::__types::ColorWrites::RED) {
        raw_mask |= MTLColorWriteMask::Red;
    }
    if mask.contains(crate::__types::ColorWrites::GREEN) {
        raw_mask |= MTLColorWriteMask::Green;
    }
    if mask.contains(crate::__types::ColorWrites::BLUE) {
        raw_mask |= MTLColorWriteMask::Blue;
    }
    if mask.contains(crate::__types::ColorWrites::ALPHA) {
        raw_mask |= MTLColorWriteMask::Alpha;
    }

    raw_mask
}

pub fn map_blend_factor(factor: crate::__types::BlendFactor) -> MTLBlendFactor {
    use crate::__types::BlendFactor as Bf;
    use MTLBlendFactor as MTL;

    match factor {
        Bf::Zero => MTL::Zero,
        Bf::One => MTL::One,
        Bf::Src => MTL::SourceColor,
        Bf::OneMinusSrc => MTL::OneMinusSourceColor,
        Bf::Dst => MTL::DestinationColor,
        Bf::OneMinusDst => MTL::OneMinusDestinationColor,
        Bf::SrcAlpha => MTL::SourceAlpha,
        Bf::OneMinusSrcAlpha => MTL::OneMinusSourceAlpha,
        Bf::DstAlpha => MTL::DestinationAlpha,
        Bf::OneMinusDstAlpha => MTL::OneMinusDestinationAlpha,
        Bf::Constant => MTL::BlendColor,
        Bf::OneMinusConstant => MTL::OneMinusBlendColor,
        Bf::SrcAlphaSaturated => MTL::SourceAlphaSaturated,
        Bf::Src1 => MTL::Source1Color,
        Bf::OneMinusSrc1 => MTL::OneMinusSource1Color,
        Bf::Src1Alpha => MTL::Source1Alpha,
        Bf::OneMinusSrc1Alpha => MTL::OneMinusSource1Alpha,
    }
}

pub fn map_blend_op(operation: crate::__types::BlendOperation) -> MTLBlendOperation {
    use crate::__types::BlendOperation as Bo;
    use MTLBlendOperation as MTL;

    match operation {
        Bo::Add => MTL::Add,
        Bo::Subtract => MTL::Subtract,
        Bo::ReverseSubtract => MTL::ReverseSubtract,
        Bo::Min => MTL::Min,
        Bo::Max => MTL::Max,
    }
}

pub fn map_blend_component(
    component: &crate::__types::BlendComponent,
) -> (MTLBlendOperation, MTLBlendFactor, MTLBlendFactor) {
    (
        map_blend_op(component.operation),
        map_blend_factor(component.src_factor),
        map_blend_factor(component.dst_factor),
    )
}

pub fn map_vertex_format(format: crate::__types::VertexFormat) -> MTLVertexFormat {
    use crate::__types::VertexFormat as Vf;
    use MTLVertexFormat as MTL;

    match format {
        Vf::Unorm8 => MTL::UCharNormalized,
        Vf::Snorm8 => MTL::CharNormalized,
        Vf::Uint8 => MTL::UChar,
        Vf::Sint8 => MTL::Char,
        Vf::Unorm8x2 => MTL::UChar2Normalized,
        Vf::Snorm8x2 => MTL::Char2Normalized,
        Vf::Uint8x2 => MTL::UChar2,
        Vf::Sint8x2 => MTL::Char2,
        Vf::Unorm8x4 => MTL::UChar4Normalized,
        Vf::Snorm8x4 => MTL::Char4Normalized,
        Vf::Uint8x4 => MTL::UChar4,
        Vf::Sint8x4 => MTL::Char4,
        Vf::Unorm16 => MTL::UShortNormalized,
        Vf::Snorm16 => MTL::ShortNormalized,
        Vf::Uint16 => MTL::UShort,
        Vf::Sint16 => MTL::Short,
        Vf::Float16 => MTL::Half,
        Vf::Unorm16x2 => MTL::UShort2Normalized,
        Vf::Snorm16x2 => MTL::Short2Normalized,
        Vf::Uint16x2 => MTL::UShort2,
        Vf::Sint16x2 => MTL::Short2,
        Vf::Float16x2 => MTL::Half2,
        Vf::Unorm16x4 => MTL::UShort4Normalized,
        Vf::Snorm16x4 => MTL::Short4Normalized,
        Vf::Uint16x4 => MTL::UShort4,
        Vf::Sint16x4 => MTL::Short4,
        Vf::Float16x4 => MTL::Half4,
        Vf::Uint32 => MTL::UInt,
        Vf::Sint32 => MTL::Int,
        Vf::Float32 => MTL::Float,
        Vf::Uint32x2 => MTL::UInt2,
        Vf::Sint32x2 => MTL::Int2,
        Vf::Float32x2 => MTL::Float2,
        Vf::Uint32x3 => MTL::UInt3,
        Vf::Sint32x3 => MTL::Int3,
        Vf::Float32x3 => MTL::Float3,
        Vf::Uint32x4 => MTL::UInt4,
        Vf::Sint32x4 => MTL::Int4,
        Vf::Float32x4 => MTL::Float4,
        Vf::Unorm10_10_10_2 => MTL::UInt1010102Normalized,
        Vf::Unorm8x4Bgra => MTL::UChar4Normalized_BGRA,
        Vf::Float64 | Vf::Float64x2 | Vf::Float64x3 | Vf::Float64x4 => unimplemented!(),
    }
}

pub fn map_index_format(format: crate::__types::IndexFormat) -> (u64, MTLIndexType) {
    match format {
        crate::__types::IndexFormat::Uint16 => (2, MTLIndexType::UInt16),
        crate::__types::IndexFormat::Uint32 => (4, MTLIndexType::UInt32),
    }
}

pub fn map_step_mode(mode: crate::__types::VertexStepMode) -> MTLVertexStepFunction {
    match mode {
        crate::__types::VertexStepMode::Vertex => MTLVertexStepFunction::PerVertex,
        crate::__types::VertexStepMode::Instance => MTLVertexStepFunction::PerInstance,
    }
}

pub fn map_stencil_op(op: crate::__types::StencilOperation) -> MTLStencilOperation {
    use crate::__types::StencilOperation as So;
    use MTLStencilOperation as MTL;

    match op {
        So::Keep => MTL::Keep,
        So::Zero => MTL::Zero,
        So::Replace => MTL::Replace,
        So::IncrementClamp => MTL::IncrementClamp,
        So::IncrementWrap => MTL::IncrementWrap,
        So::DecrementClamp => MTL::DecrementClamp,
        So::DecrementWrap => MTL::DecrementWrap,
        So::Invert => MTL::Invert,
    }
}

pub fn map_winding(winding: crate::__types::FrontFace) -> MTLWinding {
    match winding {
        crate::__types::FrontFace::Cw => MTLWinding::Clockwise,
        crate::__types::FrontFace::Ccw => MTLWinding::CounterClockwise,
    }
}

pub fn map_cull_mode(face: Option<crate::__types::Face>) -> MTLCullMode {
    match face {
        None => MTLCullMode::None,
        Some(crate::__types::Face::Front) => MTLCullMode::Front,
        Some(crate::__types::Face::Back) => MTLCullMode::Back,
    }
}

pub fn map_range(range: &crate::__hal::MemoryRange) -> NSRange {
    NSRange {
        location: range.start as usize,
        length: (range.end - range.start) as usize,
    }
}

pub fn map_copy_extent(extent: &crate::__hal::CopyExtent) -> MTLSize {
    MTLSize {
        width: extent.width as usize,
        height: extent.height as usize,
        depth: extent.depth as usize,
    }
}

pub fn map_origin(origin: &crate::__types::Origin3d) -> MTLOrigin {
    MTLOrigin {
        x: origin.x as usize,
        y: origin.y as usize,
        z: origin.z as usize,
    }
}

pub fn map_store_action(store: bool, resolve: bool) -> MTLStoreAction {
    use MTLStoreAction as MTL;
    match (store, resolve) {
        (true, true) => MTL::StoreAndMultisampleResolve,
        (false, true) => MTL::MultisampleResolve,
        (true, false) => MTL::Store,
        (false, false) => MTL::DontCare,
    }
}

pub fn map_clear_color(color: &crate::__types::Color) -> MTLClearColor {
    MTLClearColor {
        red: color.r,
        green: color.g,
        blue: color.b,
        alpha: color.a,
    }
}

pub fn get_blit_option(
    format: crate::__types::TextureFormat,
    aspect: crate::__hal::FormatAspects,
) -> MTLBlitOption {
    if format.is_combined_depth_stencil_format() {
        match aspect {
            crate::__hal::FormatAspects::DEPTH => MTLBlitOption::DepthFromDepthStencil,
            crate::__hal::FormatAspects::STENCIL => MTLBlitOption::StencilFromDepthStencil,
            _ => unreachable!(),
        }
    } else {
        MTLBlitOption::None
    }
}

pub fn map_render_stages(stage: crate::__types::ShaderStages) -> MTLRenderStages {
    let mut raw_stages = MTLRenderStages::empty();

    if stage.contains(crate::__types::ShaderStages::VERTEX) {
        raw_stages |= MTLRenderStages::Vertex;
    }
    if stage.contains(crate::__types::ShaderStages::FRAGMENT) {
        raw_stages |= MTLRenderStages::Fragment;
    }

    raw_stages
}

pub fn map_resource_usage(ty: &crate::__types::BindingType) -> MTLResourceUsage {
    match ty {
        #[allow(deprecated)]
        crate::__types::BindingType::Texture { .. } => MTLResourceUsage::Sample,
        crate::__types::BindingType::StorageTexture { access, .. } => match access {
            crate::__types::StorageTextureAccess::WriteOnly => MTLResourceUsage::Write,
            crate::__types::StorageTextureAccess::ReadOnly => MTLResourceUsage::Read,
            crate::__types::StorageTextureAccess::Atomic
            | crate::__types::StorageTextureAccess::ReadWrite => {
                MTLResourceUsage::Read | MTLResourceUsage::Write
            }
        },
        crate::__types::BindingType::Sampler(..) => MTLResourceUsage::empty(),
        crate::__types::BindingType::Buffer { ty, .. } => match ty {
            crate::__types::BufferBindingType::Uniform
            | crate::__types::BufferBindingType::Storage { read_only: true } => {
                MTLResourceUsage::Read
            }
            crate::__types::BufferBindingType::Storage { read_only: false } => {
                MTLResourceUsage::Read | MTLResourceUsage::Write
            }
        },
        _ => unreachable!(),
    }
}

pub fn map_acceleration_structure_descriptor<'a>(
    entries: &'a crate::__hal::AccelerationStructureEntries<'a, super::Buffer>,
    flags: crate::__hal::AccelerationStructureBuildFlags,
) -> Retained<MTLAccelerationStructureDescriptor> {
    let descriptor = match entries {
        crate::__hal::AccelerationStructureEntries::Instances(instances) => {
            let descriptor = MTLInstanceAccelerationStructureDescriptor::new();
            descriptor.setInstanceDescriptorType(
                MTLAccelerationStructureInstanceDescriptorType::Indirect,
            );
            descriptor.setInstanceCount(instances.count as usize);
            descriptor.setInstanceDescriptorBuffer(Some(&instances.buffer.unwrap().raw));
            unsafe {
                descriptor.setInstanceDescriptorBufferOffset(instances.offset as usize);
            }
            descriptor.into_super()
        }
        crate::__hal::AccelerationStructureEntries::Triangles(entries) => {
            let geometry_descriptors = entries
                .iter()
                .map(|triangles| {
                    let descriptor = MTLAccelerationStructureTriangleGeometryDescriptor::new();
                    if let Some(indices) = triangles.indices.as_ref() {
                        descriptor.setIndexBuffer(Some(&indices.buffer.unwrap().raw));
                        unsafe {
                            descriptor.setIndexBufferOffset(indices.offset as usize);
                        }
                        descriptor.setIndexType(map_index_format(indices.format).1);
                        descriptor.setTriangleCount(indices.count as usize / 3);
                    } else {
                        descriptor.setTriangleCount(triangles.vertex_count as usize / 3);
                    }
                    descriptor.setVertexBuffer(Some(&triangles.vertex_buffer.unwrap().raw));
                    unsafe {
                        descriptor.setVertexBufferOffset(
                            triangles.first_vertex as usize * triangles.vertex_stride as usize,
                        );
                    }
                    descriptor.setVertexStride(triangles.vertex_stride as usize);
                    // Safety: MTLVertexFormat and MTLAttributeFormat are identical.
                    // https://docs.rs/objc2-metal/latest/objc2_metal/struct.MTLAttributeFormat.html
                    // https://docs.rs/objc2-metal/latest/objc2_metal/struct.MTLVertexFormat.html
                    descriptor.setVertexFormat(unsafe {
                        core::mem::transmute::<MTLVertexFormat, MTLAttributeFormat>(
                            map_vertex_format(triangles.vertex_format),
                        )
                    });
                    if let Some(transform) = triangles.transform.as_ref() {
                        unsafe {
                            descriptor.setTransformationMatrixBuffer(Some(&transform.buffer.raw));
                            descriptor
                                .setTransformationMatrixBufferOffset(transform.offset as usize);
                        }
                    }
                    descriptor.setOpaque(
                        triangles
                            .flags
                            .contains(crate::__types::AccelerationStructureGeometryFlags::OPAQUE),
                    );
                    if !triangles.flags.contains(
                        crate::__types::AccelerationStructureGeometryFlags::NO_DUPLICATE_ANY_HIT_INVOCATION,
                    ) {
                        descriptor.allowDuplicateIntersectionFunctionInvocation();
                    }
                    // descriptor.setIntersectionFunctionTableOffset(offset);
                    descriptor.into_super()
                })
                .collect::<alloc::vec::Vec<Retained<MTLAccelerationStructureGeometryDescriptor>>>();
            let descriptor = MTLPrimitiveAccelerationStructureDescriptor::new();
            descriptor.setGeometryDescriptors(Some(&NSArray::from_retained_slice(
                geometry_descriptors.as_slice(),
            )));
            descriptor.into_super()
        }
        crate::__hal::AccelerationStructureEntries::AABBs(entries) => {
            let geometry_descriptors = entries
                .iter()
                .map(|aabbs| {
                    let descriptor = MTLAccelerationStructureBoundingBoxGeometryDescriptor::new();
                    descriptor.setBoundingBoxBuffer(Some(&aabbs.buffer.unwrap().raw));
                    descriptor.setBoundingBoxCount(aabbs.count as usize);
                    unsafe {
                        descriptor.setBoundingBoxStride(aabbs.stride as usize);
                        descriptor.setBoundingBoxBufferOffset(aabbs.offset as usize);
                    }
                    descriptor.setOpaque(
                        aabbs
                            .flags
                            .contains(crate::__types::AccelerationStructureGeometryFlags::OPAQUE),
                    );
                    if !aabbs.flags.contains(
                        crate::__types::AccelerationStructureGeometryFlags::NO_DUPLICATE_ANY_HIT_INVOCATION,
                    ) {
                        descriptor.allowDuplicateIntersectionFunctionInvocation();
                    }
                    // descriptor.setIntersectionFunctionTableOffset(offset);
                    descriptor.into_super()
                })
                .collect::<alloc::vec::Vec<Retained<MTLAccelerationStructureGeometryDescriptor>>>();
            let descriptor = MTLPrimitiveAccelerationStructureDescriptor::new();
            descriptor.setGeometryDescriptors(Some(&NSArray::from_retained_slice(
                geometry_descriptors.as_slice(),
            )));
            descriptor.into_super()
        }
    };
    let mut usage = MTLAccelerationStructureUsage::None;
    if flags.contains(crate::__types::AccelerationStructureFlags::ALLOW_UPDATE) {
        usage |= MTLAccelerationStructureUsage::Refit;
    }
    if flags.contains(crate::__types::AccelerationStructureFlags::PREFER_FAST_BUILD) {
        usage |= MTLAccelerationStructureUsage::PreferFastBuild;
    }
    descriptor.setUsage(usage);
    descriptor
}

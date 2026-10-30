use crate::__naga::front::wgsl::parse::directive::enable_extension::{
    EnableExtensions, ImplementedEnableExtension,
};
use crate::__naga::front::wgsl::{Error, Result, Scalar};
use crate::__naga::{ImageClass, ImageDimension, Span, TypeInner, VectorSize};

use alloc::boxed::Box;

pub fn map_address_space<'a>(
    word: &str,
    span: Span,
    enable_extensions: &EnableExtensions,
) -> Result<'a, crate::__naga::AddressSpace> {
    match word {
        "private" => Ok(crate::__naga::AddressSpace::Private),
        "workgroup" => Ok(crate::__naga::AddressSpace::WorkGroup),
        "uniform" => Ok(crate::__naga::AddressSpace::Uniform),
        "storage" => Ok(crate::__naga::AddressSpace::Storage {
            access: crate::__naga::StorageAccess::default(),
        }),
        "immediate" => Ok(crate::__naga::AddressSpace::Immediate),
        "function" => Ok(crate::__naga::AddressSpace::Function),
        "task_payload" => {
            enable_extensions.require(ImplementedEnableExtension::WgpuMeshShader, span)?;
            Ok(crate::__naga::AddressSpace::TaskPayload)
        }
        "ray_payload" => {
            if enable_extensions.contains(ImplementedEnableExtension::WgpuRayTracingPipeline) {
                Ok(crate::__naga::AddressSpace::RayPayload)
            } else {
                Err(Box::new(Error::EnableExtensionNotEnabled {
                    span,
                    kind: ImplementedEnableExtension::WgpuRayTracingPipeline.into(),
                }))
            }
        }
        "incoming_ray_payload" => {
            if enable_extensions.contains(ImplementedEnableExtension::WgpuRayTracingPipeline) {
                Ok(crate::__naga::AddressSpace::IncomingRayPayload)
            } else {
                Err(Box::new(Error::EnableExtensionNotEnabled {
                    span,
                    kind: ImplementedEnableExtension::WgpuRayTracingPipeline.into(),
                }))
            }
        }
        _ => Err(Box::new(Error::UnknownAddressSpace(span))),
    }
}

pub fn map_access_mode(word: &str, span: Span) -> Result<'_, crate::__naga::StorageAccess> {
    match word {
        "read" => Ok(crate::__naga::StorageAccess::LOAD),
        "write" => Ok(crate::__naga::StorageAccess::STORE),
        "read_write" => {
            Ok(crate::__naga::StorageAccess::LOAD | crate::__naga::StorageAccess::STORE)
        }
        "atomic" => Ok(crate::__naga::StorageAccess::ATOMIC
            | crate::__naga::StorageAccess::LOAD
            | crate::__naga::StorageAccess::STORE),
        _ => Err(Box::new(Error::UnknownAccess(span))),
    }
}

pub fn map_ray_flag(
    enable_extensions: &EnableExtensions,
    word: &str,
    span: Span,
) -> Result<'static, ()> {
    match word {
        "vertex_return" => {
            if !enable_extensions.contains(ImplementedEnableExtension::WgpuRayQueryVertexReturn) {
                return Err(Box::new(Error::EnableExtensionNotEnabled {
                    span,
                    kind: ImplementedEnableExtension::WgpuRayQueryVertexReturn.into(),
                }));
            }
            Ok(())
        }
        _ => Err(Box::new(Error::UnknownRayFlag(span))),
    }
}

pub fn map_cooperative_role(word: &str, span: Span) -> Result<'_, crate::__naga::CooperativeRole> {
    match word {
        "A" => Ok(crate::__naga::CooperativeRole::A),
        "B" => Ok(crate::__naga::CooperativeRole::B),
        "C" => Ok(crate::__naga::CooperativeRole::C),
        _ => Err(Box::new(Error::UnknownAccess(span))),
    }
}

pub fn map_built_in(
    enable_extensions: &EnableExtensions,
    word: &str,
    span: Span,
) -> Result<'static, crate::__naga::BuiltIn> {
    let built_in = match word {
        "position" => crate::__naga::BuiltIn::Position { invariant: false },
        // vertex
        "vertex_index" => crate::__naga::BuiltIn::VertexIndex,
        "instance_index" => crate::__naga::BuiltIn::InstanceIndex,
        "view_index" => crate::__naga::BuiltIn::ViewIndex,
        "clip_distances" => crate::__naga::BuiltIn::ClipDistances,
        // fragment
        "front_facing" => crate::__naga::BuiltIn::FrontFacing,
        "frag_depth" => crate::__naga::BuiltIn::FragDepth,
        "primitive_index" => crate::__naga::BuiltIn::PrimitiveIndex,
        "draw_index" => crate::__naga::BuiltIn::DrawIndex,
        "barycentric" => crate::__naga::BuiltIn::Barycentric { perspective: true },
        "barycentric_no_perspective" => crate::__naga::BuiltIn::Barycentric { perspective: false },
        "sample_index" => crate::__naga::BuiltIn::SampleIndex,
        "sample_mask" => crate::__naga::BuiltIn::SampleMask,
        // compute
        "global_invocation_id" => crate::__naga::BuiltIn::GlobalInvocationId,
        "local_invocation_id" => crate::__naga::BuiltIn::LocalInvocationId,
        "local_invocation_index" => crate::__naga::BuiltIn::LocalInvocationIndex,
        "workgroup_id" => crate::__naga::BuiltIn::WorkGroupId,
        "num_workgroups" => crate::__naga::BuiltIn::NumWorkGroups,
        // subgroup
        "num_subgroups" => crate::__naga::BuiltIn::NumSubgroups,
        "subgroup_id" => crate::__naga::BuiltIn::SubgroupId,
        "subgroup_size" => crate::__naga::BuiltIn::SubgroupSize,
        "subgroup_invocation_id" => crate::__naga::BuiltIn::SubgroupInvocationId,
        // mesh
        "cull_primitive" => crate::__naga::BuiltIn::CullPrimitive,
        "point_index" => crate::__naga::BuiltIn::PointIndex,
        "line_indices" => crate::__naga::BuiltIn::LineIndices,
        "triangle_indices" => crate::__naga::BuiltIn::TriangleIndices,
        "mesh_task_size" => crate::__naga::BuiltIn::MeshTaskSize,
        // mesh global variable
        "vertex_count" => crate::__naga::BuiltIn::VertexCount,
        "vertices" => crate::__naga::BuiltIn::Vertices,
        "primitive_count" => crate::__naga::BuiltIn::PrimitiveCount,
        "primitives" => crate::__naga::BuiltIn::Primitives,
        // ray tracing pipeline
        "ray_invocation_id" => crate::__naga::BuiltIn::RayInvocationId,
        "num_ray_invocations" => crate::__naga::BuiltIn::NumRayInvocations,
        "instance_custom_data" => crate::__naga::BuiltIn::InstanceCustomData,
        "geometry_index" => crate::__naga::BuiltIn::GeometryIndex,
        "world_ray_origin" => crate::__naga::BuiltIn::WorldRayOrigin,
        "world_ray_direction" => crate::__naga::BuiltIn::WorldRayDirection,
        "object_ray_origin" => crate::__naga::BuiltIn::ObjectRayOrigin,
        "object_ray_direction" => crate::__naga::BuiltIn::ObjectRayDirection,
        "ray_t_min" => crate::__naga::BuiltIn::RayTmin,
        "ray_t_current_max" => crate::__naga::BuiltIn::RayTCurrentMax,
        "object_to_world" => crate::__naga::BuiltIn::ObjectToWorld,
        "world_to_object" => crate::__naga::BuiltIn::WorldToObject,
        "hit_kind" => crate::__naga::BuiltIn::HitKind,
        _ => return Err(Box::new(Error::UnknownBuiltin(span))),
    };
    match built_in {
        crate::__naga::BuiltIn::ClipDistances => {
            enable_extensions.require(ImplementedEnableExtension::ClipDistances, span)?
        }
        crate::__naga::BuiltIn::PrimitiveIndex => {
            enable_extensions.require(ImplementedEnableExtension::PrimitiveIndex, span)?
        }
        crate::__naga::BuiltIn::DrawIndex => {
            enable_extensions.require(ImplementedEnableExtension::DrawIndex, span)?
        }
        crate::__naga::BuiltIn::CullPrimitive
        | crate::__naga::BuiltIn::PointIndex
        | crate::__naga::BuiltIn::LineIndices
        | crate::__naga::BuiltIn::TriangleIndices
        | crate::__naga::BuiltIn::VertexCount
        | crate::__naga::BuiltIn::Vertices
        | crate::__naga::BuiltIn::PrimitiveCount
        | crate::__naga::BuiltIn::Primitives => {
            enable_extensions.require(ImplementedEnableExtension::WgpuMeshShader, span)?
        }
        _ => {}
    }
    Ok(built_in)
}

pub fn map_interpolation(
    enable_extensions: &EnableExtensions,
    word: &str,
    span: Span,
) -> Result<'static, crate::__naga::Interpolation> {
    match word {
        "linear" => Ok(crate::__naga::Interpolation::Linear),
        "flat" => Ok(crate::__naga::Interpolation::Flat),
        "perspective" => Ok(crate::__naga::Interpolation::Perspective),
        "per_vertex" => {
            enable_extensions.require(ImplementedEnableExtension::WgpuPerVertex, span)?;
            Ok(crate::__naga::Interpolation::PerVertex)
        }
        _ => Err(Box::new(Error::UnknownAttribute(span))),
    }
}

pub fn map_sampling(word: &str, span: Span) -> Result<'_, crate::__naga::Sampling> {
    match word {
        "center" => Ok(crate::__naga::Sampling::Center),
        "centroid" => Ok(crate::__naga::Sampling::Centroid),
        "sample" => Ok(crate::__naga::Sampling::Sample),
        "first" => Ok(crate::__naga::Sampling::First),
        "either" => Ok(crate::__naga::Sampling::Either),
        _ => Err(Box::new(Error::UnknownAttribute(span))),
    }
}

pub fn map_storage_format(word: &str, span: Span) -> Result<'_, crate::__naga::StorageFormat> {
    use crate::__naga::StorageFormat as Sf;
    Ok(match word {
        "r8unorm" => Sf::R8Unorm,
        "r8snorm" => Sf::R8Snorm,
        "r8uint" => Sf::R8Uint,
        "r8sint" => Sf::R8Sint,
        "r16unorm" => Sf::R16Unorm,
        "r16snorm" => Sf::R16Snorm,
        "r16uint" => Sf::R16Uint,
        "r16sint" => Sf::R16Sint,
        "r16float" => Sf::R16Float,
        "rg8unorm" => Sf::Rg8Unorm,
        "rg8snorm" => Sf::Rg8Snorm,
        "rg8uint" => Sf::Rg8Uint,
        "rg8sint" => Sf::Rg8Sint,
        "r32uint" => Sf::R32Uint,
        "r32sint" => Sf::R32Sint,
        "r32float" => Sf::R32Float,
        "rg16unorm" => Sf::Rg16Unorm,
        "rg16snorm" => Sf::Rg16Snorm,
        "rg16uint" => Sf::Rg16Uint,
        "rg16sint" => Sf::Rg16Sint,
        "rg16float" => Sf::Rg16Float,
        "rgba8unorm" => Sf::Rgba8Unorm,
        "rgba8snorm" => Sf::Rgba8Snorm,
        "rgba8uint" => Sf::Rgba8Uint,
        "rgba8sint" => Sf::Rgba8Sint,
        "rgb10a2uint" => Sf::Rgb10a2Uint,
        "rgb10a2unorm" => Sf::Rgb10a2Unorm,
        "rg11b10ufloat" => Sf::Rg11b10Ufloat,
        "r64uint" => Sf::R64Uint,
        "rg32uint" => Sf::Rg32Uint,
        "rg32sint" => Sf::Rg32Sint,
        "rg32float" => Sf::Rg32Float,
        "rgba16unorm" => Sf::Rgba16Unorm,
        "rgba16snorm" => Sf::Rgba16Snorm,
        "rgba16uint" => Sf::Rgba16Uint,
        "rgba16sint" => Sf::Rgba16Sint,
        "rgba16float" => Sf::Rgba16Float,
        "rgba32uint" => Sf::Rgba32Uint,
        "rgba32sint" => Sf::Rgba32Sint,
        "rgba32float" => Sf::Rgba32Float,
        "bgra8unorm" => Sf::Bgra8Unorm,
        _ => return Err(Box::new(Error::UnknownStorageFormat(span))),
    })
}

pub fn map_derivative(
    word: &str,
) -> Option<(
    crate::__naga::DerivativeAxis,
    crate::__naga::DerivativeControl,
)> {
    use crate::__naga::{DerivativeAxis as Axis, DerivativeControl as Ctrl};
    match word {
        "dpdxCoarse" => Some((Axis::X, Ctrl::Coarse)),
        "dpdyCoarse" => Some((Axis::Y, Ctrl::Coarse)),
        "fwidthCoarse" => Some((Axis::Width, Ctrl::Coarse)),
        "dpdxFine" => Some((Axis::X, Ctrl::Fine)),
        "dpdyFine" => Some((Axis::Y, Ctrl::Fine)),
        "fwidthFine" => Some((Axis::Width, Ctrl::Fine)),
        "dpdx" => Some((Axis::X, Ctrl::None)),
        "dpdy" => Some((Axis::Y, Ctrl::None)),
        "fwidth" => Some((Axis::Width, Ctrl::None)),
        _ => None,
    }
}

pub fn map_relational_fun(word: &str) -> Option<crate::__naga::RelationalFunction> {
    match word {
        "any" => Some(crate::__naga::RelationalFunction::Any),
        "all" => Some(crate::__naga::RelationalFunction::All),
        _ => None,
    }
}

pub fn map_standard_fun(word: &str) -> Option<crate::__naga::MathFunction> {
    use crate::__naga::MathFunction as Mf;
    Some(match word {
        // comparison
        "abs" => Mf::Abs,
        "min" => Mf::Min,
        "max" => Mf::Max,
        "clamp" => Mf::Clamp,
        "saturate" => Mf::Saturate,
        // trigonometry
        "cos" => Mf::Cos,
        "cosh" => Mf::Cosh,
        "sin" => Mf::Sin,
        "sinh" => Mf::Sinh,
        "tan" => Mf::Tan,
        "tanh" => Mf::Tanh,
        "acos" => Mf::Acos,
        "acosh" => Mf::Acosh,
        "asin" => Mf::Asin,
        "asinh" => Mf::Asinh,
        "atan" => Mf::Atan,
        "atanh" => Mf::Atanh,
        "atan2" => Mf::Atan2,
        "radians" => Mf::Radians,
        "degrees" => Mf::Degrees,
        // decomposition
        "ceil" => Mf::Ceil,
        "floor" => Mf::Floor,
        "round" => Mf::Round,
        "fract" => Mf::Fract,
        "trunc" => Mf::Trunc,
        "modf" => Mf::Modf,
        "frexp" => Mf::Frexp,
        "ldexp" => Mf::Ldexp,
        // exponent
        "exp" => Mf::Exp,
        "exp2" => Mf::Exp2,
        "log" => Mf::Log,
        "log2" => Mf::Log2,
        "pow" => Mf::Pow,
        // geometry
        "dot" => Mf::Dot,
        "dot4I8Packed" => Mf::Dot4I8Packed,
        "dot4U8Packed" => Mf::Dot4U8Packed,
        "cross" => Mf::Cross,
        "distance" => Mf::Distance,
        "length" => Mf::Length,
        "normalize" => Mf::Normalize,
        "faceForward" => Mf::FaceForward,
        "reflect" => Mf::Reflect,
        "refract" => Mf::Refract,
        // computational
        "sign" => Mf::Sign,
        "fma" => Mf::Fma,
        "mix" => Mf::Mix,
        "step" => Mf::Step,
        "smoothstep" => Mf::SmoothStep,
        "sqrt" => Mf::Sqrt,
        "inverseSqrt" => Mf::InverseSqrt,
        "transpose" => Mf::Transpose,
        "determinant" => Mf::Determinant,
        "quantizeToF16" => Mf::QuantizeToF16,
        // bits
        "countTrailingZeros" => Mf::CountTrailingZeros,
        "countLeadingZeros" => Mf::CountLeadingZeros,
        "countOneBits" => Mf::CountOneBits,
        "reverseBits" => Mf::ReverseBits,
        "extractBits" => Mf::ExtractBits,
        "insertBits" => Mf::InsertBits,
        "firstTrailingBit" => Mf::FirstTrailingBit,
        "firstLeadingBit" => Mf::FirstLeadingBit,
        // data packing
        "pack4x8snorm" => Mf::Pack4x8snorm,
        "pack4x8unorm" => Mf::Pack4x8unorm,
        "pack2x16snorm" => Mf::Pack2x16snorm,
        "pack2x16unorm" => Mf::Pack2x16unorm,
        "pack2x16float" => Mf::Pack2x16float,
        "pack4xI8" => Mf::Pack4xI8,
        "pack4xU8" => Mf::Pack4xU8,
        "pack4xI8Clamp" => Mf::Pack4xI8Clamp,
        "pack4xU8Clamp" => Mf::Pack4xU8Clamp,
        // data unpacking
        "unpack4x8snorm" => Mf::Unpack4x8snorm,
        "unpack4x8unorm" => Mf::Unpack4x8unorm,
        "unpack2x16snorm" => Mf::Unpack2x16snorm,
        "unpack2x16unorm" => Mf::Unpack2x16unorm,
        "unpack2x16float" => Mf::Unpack2x16float,
        "unpack4xI8" => Mf::Unpack4xI8,
        "unpack4xU8" => Mf::Unpack4xU8,
        _ => return None,
    })
}

pub fn map_conservative_depth(
    word: &str,
    span: Span,
) -> Result<'_, crate::__naga::ConservativeDepth> {
    use crate::__naga::ConservativeDepth as Cd;
    match word {
        "greater_equal" => Ok(Cd::GreaterEqual),
        "less_equal" => Ok(Cd::LessEqual),
        "unchanged" => Ok(Cd::Unchanged),
        _ => Err(Box::new(Error::UnknownConservativeDepth(span))),
    }
}

pub fn map_subgroup_operation(
    word: &str,
) -> Option<(
    crate::__naga::SubgroupOperation,
    crate::__naga::CollectiveOperation,
)> {
    use crate::__naga::CollectiveOperation as co;
    use crate::__naga::SubgroupOperation as sg;
    Some(match word {
        "subgroupAll" => (sg::All, co::Reduce),
        "subgroupAny" => (sg::Any, co::Reduce),
        "subgroupAdd" => (sg::Add, co::Reduce),
        "subgroupMul" => (sg::Mul, co::Reduce),
        "subgroupMin" => (sg::Min, co::Reduce),
        "subgroupMax" => (sg::Max, co::Reduce),
        "subgroupAnd" => (sg::And, co::Reduce),
        "subgroupOr" => (sg::Or, co::Reduce),
        "subgroupXor" => (sg::Xor, co::Reduce),
        "subgroupExclusiveAdd" => (sg::Add, co::ExclusiveScan),
        "subgroupExclusiveMul" => (sg::Mul, co::ExclusiveScan),
        "subgroupInclusiveAdd" => (sg::Add, co::InclusiveScan),
        "subgroupInclusiveMul" => (sg::Mul, co::InclusiveScan),
        _ => return None,
    })
}

pub enum TypeGenerator {
    Vector {
        size: VectorSize,
    },
    Matrix {
        columns: VectorSize,
        rows: VectorSize,
    },
    Array,
    Atomic,
    Pointer,
    SampledTexture {
        dim: ImageDimension,
        arrayed: bool,
        multi: bool,
    },
    StorageTexture {
        dim: ImageDimension,
        arrayed: bool,
    },
    BindingArray,
    AccelerationStructure,
    RayQuery,
    CooperativeMatrix {
        columns: crate::__naga::CooperativeSize,
        rows: crate::__naga::CooperativeSize,
    },
}

pub enum PredeclaredType {
    TypeInner(TypeInner),
    RayDesc,
    RayIntersection,
    TypeGenerator(TypeGenerator),
}
impl From<TypeInner> for PredeclaredType {
    fn from(value: TypeInner) -> Self {
        Self::TypeInner(value)
    }
}
impl From<TypeGenerator> for PredeclaredType {
    fn from(value: TypeGenerator) -> Self {
        Self::TypeGenerator(value)
    }
}

pub fn map_predeclared_type(
    enable_extensions: &EnableExtensions,
    span: Span,
    word: &str,
) -> Result<'static, Option<PredeclaredType>> {
    use Scalar as Sc;
    use TypeInner as Ti;
    use VectorSize as Vs;

    #[rustfmt::skip]
    let ty = match word {
        // predeclared types

        // scalars
        "bool" => Ti::Scalar(Sc::BOOL).into(),
        "i32" => Ti::Scalar(Sc::I32).into(),
        "u32" => Ti::Scalar(Sc::U32).into(),
        "f32" => Ti::Scalar(Sc::F32).into(),
        "i16" => Ti::Scalar(Sc::I16).into(),
        "u16" => Ti::Scalar(Sc::U16).into(),
        "f16" => Ti::Scalar(Sc::F16).into(),
        "i64" => Ti::Scalar(Sc::I64).into(),
        "u64" => Ti::Scalar(Sc::U64).into(),
        "f64" => Ti::Scalar(Sc::F64).into(),
        // vector aliases
        "vec2i" => Ti::Vector { size: Vs::Bi,   scalar: Sc::I32 }.into(),
        "vec3i" => Ti::Vector { size: Vs::Tri,  scalar: Sc::I32 }.into(),
        "vec4i" => Ti::Vector { size: Vs::Quad, scalar: Sc::I32 }.into(),
        "vec2u" => Ti::Vector { size: Vs::Bi,   scalar: Sc::U32 }.into(),
        "vec3u" => Ti::Vector { size: Vs::Tri,  scalar: Sc::U32 }.into(),
        "vec4u" => Ti::Vector { size: Vs::Quad, scalar: Sc::U32 }.into(),
        "vec2f" => Ti::Vector { size: Vs::Bi,   scalar: Sc::F32 }.into(),
        "vec3f" => Ti::Vector { size: Vs::Tri,  scalar: Sc::F32 }.into(),
        "vec4f" => Ti::Vector { size: Vs::Quad, scalar: Sc::F32 }.into(),
        "vec2h" => Ti::Vector { size: Vs::Bi,   scalar: Sc::F16 }.into(),
        "vec3h" => Ti::Vector { size: Vs::Tri,  scalar: Sc::F16 }.into(),
        "vec4h" => Ti::Vector { size: Vs::Quad, scalar: Sc::F16 }.into(),
        // matrix aliases
        "mat2x2f" => Ti::Matrix { columns: Vs::Bi,   rows: Vs::Bi,   scalar: Sc::F32 }.into(),
        "mat2x3f" => Ti::Matrix { columns: Vs::Bi,   rows: Vs::Tri,  scalar: Sc::F32 }.into(),
        "mat2x4f" => Ti::Matrix { columns: Vs::Bi,   rows: Vs::Quad, scalar: Sc::F32 }.into(),
        "mat3x2f" => Ti::Matrix { columns: Vs::Tri,  rows: Vs::Bi,   scalar: Sc::F32 }.into(),
        "mat3x3f" => Ti::Matrix { columns: Vs::Tri,  rows: Vs::Tri,  scalar: Sc::F32 }.into(),
        "mat3x4f" => Ti::Matrix { columns: Vs::Tri,  rows: Vs::Quad, scalar: Sc::F32 }.into(),
        "mat4x2f" => Ti::Matrix { columns: Vs::Quad, rows: Vs::Bi,   scalar: Sc::F32 }.into(),
        "mat4x3f" => Ti::Matrix { columns: Vs::Quad, rows: Vs::Tri,  scalar: Sc::F32 }.into(),
        "mat4x4f" => Ti::Matrix { columns: Vs::Quad, rows: Vs::Quad, scalar: Sc::F32 }.into(),
        "mat2x2h" => Ti::Matrix { columns: Vs::Bi,   rows: Vs::Bi,   scalar: Sc::F16 }.into(),
        "mat2x3h" => Ti::Matrix { columns: Vs::Bi,   rows: Vs::Tri,  scalar: Sc::F16 }.into(),
        "mat2x4h" => Ti::Matrix { columns: Vs::Bi,   rows: Vs::Quad, scalar: Sc::F16 }.into(),
        "mat3x2h" => Ti::Matrix { columns: Vs::Tri,  rows: Vs::Bi,   scalar: Sc::F16 }.into(),
        "mat3x3h" => Ti::Matrix { columns: Vs::Tri,  rows: Vs::Tri,  scalar: Sc::F16 }.into(),
        "mat3x4h" => Ti::Matrix { columns: Vs::Tri,  rows: Vs::Quad, scalar: Sc::F16 }.into(),
        "mat4x2h" => Ti::Matrix { columns: Vs::Quad, rows: Vs::Bi,   scalar: Sc::F16 }.into(),
        "mat4x3h" => Ti::Matrix { columns: Vs::Quad, rows: Vs::Tri,  scalar: Sc::F16 }.into(),
        "mat4x4h" => Ti::Matrix { columns: Vs::Quad, rows: Vs::Quad, scalar: Sc::F16 }.into(),
        // samplers
        "sampler" =>            Ti::Sampler { comparison: false }.into(),
        "sampler_comparison" => Ti::Sampler { comparison: true }.into(),
        // depth textures
        "texture_depth_2d" =>              Ti::Image { dim: ImageDimension::D2,   arrayed: false, class: ImageClass::Depth { multi: false } }.into(),
        "texture_depth_2d_array" =>        Ti::Image { dim: ImageDimension::D2,   arrayed: true,  class: ImageClass::Depth { multi: false } }.into(),
        "texture_depth_cube" =>            Ti::Image { dim: ImageDimension::Cube, arrayed: false, class: ImageClass::Depth { multi: false } }.into(),
        "texture_depth_cube_array" =>      Ti::Image { dim: ImageDimension::Cube, arrayed: true,  class: ImageClass::Depth { multi: false } }.into(),
        "texture_depth_multisampled_2d" => Ti::Image { dim: ImageDimension::D2,   arrayed: false, class: ImageClass::Depth { multi: true  } }.into(),
        // external texture
        "texture_external" => Ti::Image { dim: ImageDimension::D2, arrayed: false, class: ImageClass::External }.into(),
        // ray desc
        "RayDesc" => PredeclaredType::RayDesc,
        // ray intersection
        "RayIntersection" => PredeclaredType::RayIntersection,

        // predeclared type generators

        // vector
        "vec2" => TypeGenerator::Vector { size: Vs::Bi   }.into(),
        "vec3" => TypeGenerator::Vector { size: Vs::Tri  }.into(),
        "vec4" => TypeGenerator::Vector { size: Vs::Quad }.into(),
        // matrix
        "mat2x2" => TypeGenerator::Matrix { columns: Vs::Bi,   rows: Vs::Bi   }.into(),
        "mat2x3" => TypeGenerator::Matrix { columns: Vs::Bi,   rows: Vs::Tri  }.into(),
        "mat2x4" => TypeGenerator::Matrix { columns: Vs::Bi,   rows: Vs::Quad }.into(),
        "mat3x2" => TypeGenerator::Matrix { columns: Vs::Tri,  rows: Vs::Bi   }.into(),
        "mat3x3" => TypeGenerator::Matrix { columns: Vs::Tri,  rows: Vs::Tri  }.into(),
        "mat3x4" => TypeGenerator::Matrix { columns: Vs::Tri,  rows: Vs::Quad }.into(),
        "mat4x2" => TypeGenerator::Matrix { columns: Vs::Quad, rows: Vs::Bi   }.into(),
        "mat4x3" => TypeGenerator::Matrix { columns: Vs::Quad, rows: Vs::Tri  }.into(),
        "mat4x4" => TypeGenerator::Matrix { columns: Vs::Quad, rows: Vs::Quad }.into(),
        // array
        "array" => TypeGenerator::Array.into(),
        // atomic
        "atomic" => TypeGenerator::Atomic.into(),
        // pointer
        "ptr" => TypeGenerator::Pointer.into(),
        // sampled textures
        "texture_1d" =>               TypeGenerator::SampledTexture { dim: ImageDimension::D1,   arrayed: false, multi: false }.into(),
        "texture_2d" =>               TypeGenerator::SampledTexture { dim: ImageDimension::D2,   arrayed: false, multi: false }.into(),
        "texture_2d_array" =>         TypeGenerator::SampledTexture { dim: ImageDimension::D2,   arrayed: true,  multi: false }.into(),
        "texture_3d" =>               TypeGenerator::SampledTexture { dim: ImageDimension::D3,   arrayed: false, multi: false }.into(),
        "texture_cube" =>             TypeGenerator::SampledTexture { dim: ImageDimension::Cube, arrayed: false, multi: false }.into(),
        "texture_cube_array" =>       TypeGenerator::SampledTexture { dim: ImageDimension::Cube, arrayed: true,  multi: false }.into(),
        "texture_multisampled_2d" =>  TypeGenerator::SampledTexture { dim: ImageDimension::D2,   arrayed: false, multi: true  }.into(),
        // storage textures
        "texture_storage_1d" =>       TypeGenerator::StorageTexture { dim: ImageDimension::D1,   arrayed: false }.into(),
        "texture_storage_2d" =>       TypeGenerator::StorageTexture { dim: ImageDimension::D2,   arrayed: false }.into(),
        "texture_storage_2d_array" => TypeGenerator::StorageTexture { dim: ImageDimension::D2,   arrayed: true  }.into(),
        "texture_storage_3d" =>       TypeGenerator::StorageTexture { dim: ImageDimension::D3,   arrayed: false }.into(),
        // binding array
        "binding_array" => TypeGenerator::BindingArray.into(),
        // acceleration structure
        "acceleration_structure" => TypeGenerator::AccelerationStructure.into(),
        // ray query
        "ray_query" => TypeGenerator::RayQuery.into(),
        // cooperative matrix
        "coop_mat8x8" => TypeGenerator::CooperativeMatrix {
            columns: crate::__naga::CooperativeSize::Eight,
            rows: crate::__naga::CooperativeSize::Eight,
        }.into(),
        "coop_mat16x16" => TypeGenerator::CooperativeMatrix {
            columns: crate::__naga::CooperativeSize::Sixteen,
            rows: crate::__naga::CooperativeSize::Sixteen,
        }.into(),
        _ => return Ok(None),
    };

    // Check for the enable extension required to use this type, if any.
    // Slice should be at least len one otherwise extension_needed should be None.
    let extensions_needed: Option<&[_]> = match ty {
        PredeclaredType::TypeInner(ref ty) if ty.scalar() == Some(Sc::F16) => {
            Some(&[ImplementedEnableExtension::F16])
        }
        PredeclaredType::TypeInner(ref ty) if matches!(ty.scalar(), Some(s) if s == Sc::I16 || s == Sc::U16) => {
            Some(&[ImplementedEnableExtension::WgpuInt16])
        }
        PredeclaredType::RayDesc
        | PredeclaredType::RayIntersection
        | PredeclaredType::TypeGenerator(TypeGenerator::AccelerationStructure)
        | PredeclaredType::TypeGenerator(TypeGenerator::RayQuery) => Some(&[
            ImplementedEnableExtension::WgpuRayQuery,
            ImplementedEnableExtension::WgpuRayTracingPipeline,
        ]),
        PredeclaredType::TypeGenerator(TypeGenerator::CooperativeMatrix { .. }) => {
            Some(&[ImplementedEnableExtension::WgpuCooperativeMatrix])
        }
        PredeclaredType::TypeGenerator(TypeGenerator::BindingArray) => {
            Some(&[ImplementedEnableExtension::WgpuBindingArray])
        }
        _ => None,
    };
    if let Some(extensions_needed) = extensions_needed {
        let mut any_extension_enabled = false;
        for extension_needed in extensions_needed {
            if enable_extensions.contains(*extension_needed) {
                any_extension_enabled = true;
            }
        }
        if !any_extension_enabled {
            return Err(Box::new(Error::EnableExtensionNotEnabled {
                span,
                kind: extensions_needed[0].into(),
            }));
        }
    }

    Ok(Some(ty))
}

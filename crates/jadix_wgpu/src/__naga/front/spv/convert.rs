use core::convert::TryInto;

use super::error::Error;

pub(super) const fn map_binary_operator(
    word: spirv::Op,
) -> Result<crate::__naga::BinaryOperator, Error> {
    use crate::__naga::BinaryOperator;
    use spirv::Op;

    match word {
        // Arithmetic Instructions +, -, *, /, %
        Op::IAdd | Op::FAdd => Ok(BinaryOperator::Add),
        Op::ISub | Op::FSub => Ok(BinaryOperator::Subtract),
        Op::IMul | Op::FMul => Ok(BinaryOperator::Multiply),
        Op::UDiv | Op::SDiv | Op::FDiv => Ok(BinaryOperator::Divide),
        Op::SRem => Ok(BinaryOperator::Modulo),
        // Relational and Logical Instructions
        Op::IEqual | Op::FOrdEqual | Op::FUnordEqual | Op::LogicalEqual => {
            Ok(BinaryOperator::Equal)
        }
        Op::INotEqual | Op::FOrdNotEqual | Op::FUnordNotEqual | Op::LogicalNotEqual => {
            Ok(BinaryOperator::NotEqual)
        }
        Op::ULessThan | Op::SLessThan | Op::FOrdLessThan | Op::FUnordLessThan => {
            Ok(BinaryOperator::Less)
        }
        Op::ULessThanEqual
        | Op::SLessThanEqual
        | Op::FOrdLessThanEqual
        | Op::FUnordLessThanEqual => Ok(BinaryOperator::LessEqual),
        Op::UGreaterThan | Op::SGreaterThan | Op::FOrdGreaterThan | Op::FUnordGreaterThan => {
            Ok(BinaryOperator::Greater)
        }
        Op::UGreaterThanEqual
        | Op::SGreaterThanEqual
        | Op::FOrdGreaterThanEqual
        | Op::FUnordGreaterThanEqual => Ok(BinaryOperator::GreaterEqual),
        Op::BitwiseOr => Ok(BinaryOperator::InclusiveOr),
        Op::BitwiseXor => Ok(BinaryOperator::ExclusiveOr),
        Op::BitwiseAnd => Ok(BinaryOperator::And),
        _ => Err(Error::UnknownBinaryOperator(word)),
    }
}

pub(super) const fn map_relational_fun(
    word: spirv::Op,
) -> Result<crate::__naga::RelationalFunction, Error> {
    use crate::__naga::RelationalFunction as Rf;
    use spirv::Op;

    match word {
        Op::All => Ok(Rf::All),
        Op::Any => Ok(Rf::Any),
        Op::IsNan => Ok(Rf::IsNan),
        Op::IsInf => Ok(Rf::IsInf),
        _ => Err(Error::UnknownRelationalFunction(word)),
    }
}

pub(super) const fn map_vector_size(word: spirv::Word) -> Result<crate::__naga::VectorSize, Error> {
    match word {
        2 => Ok(crate::__naga::VectorSize::Bi),
        3 => Ok(crate::__naga::VectorSize::Tri),
        4 => Ok(crate::__naga::VectorSize::Quad),
        _ => Err(Error::InvalidVectorSize(word)),
    }
}

pub(super) fn map_image_dim(word: spirv::Word) -> Result<crate::__naga::ImageDimension, Error> {
    use spirv::Dim as D;
    match D::from_u32(word) {
        Some(D::Dim1D) => Ok(crate::__naga::ImageDimension::D1),
        Some(D::Dim2D) => Ok(crate::__naga::ImageDimension::D2),
        Some(D::Dim3D) => Ok(crate::__naga::ImageDimension::D3),
        Some(D::DimCube) => Ok(crate::__naga::ImageDimension::Cube),
        _ => Err(Error::UnsupportedImageDim(word)),
    }
}

pub(super) fn map_image_format(word: spirv::Word) -> Result<crate::__naga::StorageFormat, Error> {
    match spirv::ImageFormat::from_u32(word) {
        Some(spirv::ImageFormat::R8) => Ok(crate::__naga::StorageFormat::R8Unorm),
        Some(spirv::ImageFormat::R8Snorm) => Ok(crate::__naga::StorageFormat::R8Snorm),
        Some(spirv::ImageFormat::R8ui) => Ok(crate::__naga::StorageFormat::R8Uint),
        Some(spirv::ImageFormat::R8i) => Ok(crate::__naga::StorageFormat::R8Sint),
        Some(spirv::ImageFormat::R16) => Ok(crate::__naga::StorageFormat::R16Unorm),
        Some(spirv::ImageFormat::R16Snorm) => Ok(crate::__naga::StorageFormat::R16Snorm),
        Some(spirv::ImageFormat::R16ui) => Ok(crate::__naga::StorageFormat::R16Uint),
        Some(spirv::ImageFormat::R16i) => Ok(crate::__naga::StorageFormat::R16Sint),
        Some(spirv::ImageFormat::R16f) => Ok(crate::__naga::StorageFormat::R16Float),
        Some(spirv::ImageFormat::Rg8) => Ok(crate::__naga::StorageFormat::Rg8Unorm),
        Some(spirv::ImageFormat::Rg8Snorm) => Ok(crate::__naga::StorageFormat::Rg8Snorm),
        Some(spirv::ImageFormat::Rg8ui) => Ok(crate::__naga::StorageFormat::Rg8Uint),
        Some(spirv::ImageFormat::Rg8i) => Ok(crate::__naga::StorageFormat::Rg8Sint),
        Some(spirv::ImageFormat::R32ui) => Ok(crate::__naga::StorageFormat::R32Uint),
        Some(spirv::ImageFormat::R32i) => Ok(crate::__naga::StorageFormat::R32Sint),
        Some(spirv::ImageFormat::R32f) => Ok(crate::__naga::StorageFormat::R32Float),
        Some(spirv::ImageFormat::Rg16) => Ok(crate::__naga::StorageFormat::Rg16Unorm),
        Some(spirv::ImageFormat::Rg16Snorm) => Ok(crate::__naga::StorageFormat::Rg16Snorm),
        Some(spirv::ImageFormat::Rg16ui) => Ok(crate::__naga::StorageFormat::Rg16Uint),
        Some(spirv::ImageFormat::Rg16i) => Ok(crate::__naga::StorageFormat::Rg16Sint),
        Some(spirv::ImageFormat::Rg16f) => Ok(crate::__naga::StorageFormat::Rg16Float),
        Some(spirv::ImageFormat::Rgba8) => Ok(crate::__naga::StorageFormat::Rgba8Unorm),
        Some(spirv::ImageFormat::Rgba8Snorm) => Ok(crate::__naga::StorageFormat::Rgba8Snorm),
        Some(spirv::ImageFormat::Rgba8ui) => Ok(crate::__naga::StorageFormat::Rgba8Uint),
        Some(spirv::ImageFormat::Rgba8i) => Ok(crate::__naga::StorageFormat::Rgba8Sint),
        Some(spirv::ImageFormat::Rgb10a2ui) => Ok(crate::__naga::StorageFormat::Rgb10a2Uint),
        Some(spirv::ImageFormat::Rgb10A2) => Ok(crate::__naga::StorageFormat::Rgb10a2Unorm),
        Some(spirv::ImageFormat::R11fG11fB10f) => Ok(crate::__naga::StorageFormat::Rg11b10Ufloat),
        Some(spirv::ImageFormat::R64ui) => Ok(crate::__naga::StorageFormat::R64Uint),
        Some(spirv::ImageFormat::Rg32ui) => Ok(crate::__naga::StorageFormat::Rg32Uint),
        Some(spirv::ImageFormat::Rg32i) => Ok(crate::__naga::StorageFormat::Rg32Sint),
        Some(spirv::ImageFormat::Rg32f) => Ok(crate::__naga::StorageFormat::Rg32Float),
        Some(spirv::ImageFormat::Rgba16) => Ok(crate::__naga::StorageFormat::Rgba16Unorm),
        Some(spirv::ImageFormat::Rgba16Snorm) => Ok(crate::__naga::StorageFormat::Rgba16Snorm),
        Some(spirv::ImageFormat::Rgba16ui) => Ok(crate::__naga::StorageFormat::Rgba16Uint),
        Some(spirv::ImageFormat::Rgba16i) => Ok(crate::__naga::StorageFormat::Rgba16Sint),
        Some(spirv::ImageFormat::Rgba16f) => Ok(crate::__naga::StorageFormat::Rgba16Float),
        Some(spirv::ImageFormat::Rgba32ui) => Ok(crate::__naga::StorageFormat::Rgba32Uint),
        Some(spirv::ImageFormat::Rgba32i) => Ok(crate::__naga::StorageFormat::Rgba32Sint),
        Some(spirv::ImageFormat::Rgba32f) => Ok(crate::__naga::StorageFormat::Rgba32Float),
        _ => Err(Error::UnsupportedImageFormat(word)),
    }
}

pub(super) fn map_width(word: spirv::Word) -> Result<crate::__naga::Bytes, Error> {
    (word >> 3) // bits to bytes
        .try_into()
        .map_err(|_| Error::InvalidTypeWidth(word))
}

pub(super) fn map_builtin(
    word: spirv::Word,
    invariant: bool,
) -> Result<crate::__naga::BuiltIn, Error> {
    use spirv::BuiltIn as Bi;
    Ok(match spirv::BuiltIn::from_u32(word) {
        Some(Bi::Position | Bi::FragCoord) => crate::__naga::BuiltIn::Position { invariant },
        Some(Bi::ViewIndex) => crate::__naga::BuiltIn::ViewIndex,
        // vertex
        Some(Bi::BaseInstance) => crate::__naga::BuiltIn::BaseInstance,
        Some(Bi::BaseVertex) => crate::__naga::BuiltIn::BaseVertex,
        Some(Bi::ClipDistance) => crate::__naga::BuiltIn::ClipDistances,
        Some(Bi::CullDistance) => crate::__naga::BuiltIn::CullDistance,
        Some(Bi::InstanceIndex) => crate::__naga::BuiltIn::InstanceIndex,
        Some(Bi::PointSize) => crate::__naga::BuiltIn::PointSize,
        Some(Bi::VertexIndex) => crate::__naga::BuiltIn::VertexIndex,
        Some(Bi::DrawIndex) => crate::__naga::BuiltIn::DrawIndex,
        // fragment
        Some(Bi::FragDepth) => crate::__naga::BuiltIn::FragDepth,
        Some(Bi::PointCoord) => crate::__naga::BuiltIn::PointCoord,
        Some(Bi::FrontFacing) => crate::__naga::BuiltIn::FrontFacing,
        Some(Bi::PrimitiveId) => crate::__naga::BuiltIn::PrimitiveIndex,
        Some(Bi::BaryCoordKHR) => crate::__naga::BuiltIn::Barycentric { perspective: true },
        Some(Bi::BaryCoordNoPerspKHR) => crate::__naga::BuiltIn::Barycentric { perspective: false },
        Some(Bi::SampleId) => crate::__naga::BuiltIn::SampleIndex,
        Some(Bi::SampleMask) => crate::__naga::BuiltIn::SampleMask,
        // compute
        Some(Bi::GlobalInvocationId) => crate::__naga::BuiltIn::GlobalInvocationId,
        Some(Bi::LocalInvocationId) => crate::__naga::BuiltIn::LocalInvocationId,
        Some(Bi::LocalInvocationIndex) => crate::__naga::BuiltIn::LocalInvocationIndex,
        Some(Bi::WorkgroupId) => crate::__naga::BuiltIn::WorkGroupId,
        Some(Bi::WorkgroupSize) => crate::__naga::BuiltIn::WorkGroupSize,
        Some(Bi::NumWorkgroups) => crate::__naga::BuiltIn::NumWorkGroups,
        // subgroup
        Some(Bi::NumSubgroups) => crate::__naga::BuiltIn::NumSubgroups,
        Some(Bi::SubgroupId) => crate::__naga::BuiltIn::SubgroupId,
        Some(Bi::SubgroupSize) => crate::__naga::BuiltIn::SubgroupSize,
        Some(Bi::SubgroupLocalInvocationId) => crate::__naga::BuiltIn::SubgroupInvocationId,
        _ => return Err(Error::UnsupportedBuiltIn(word)),
    })
}

pub(super) fn map_storage_class(word: spirv::Word) -> Result<super::ExtendedClass, Error> {
    use super::ExtendedClass as Ec;
    use spirv::StorageClass as Sc;
    Ok(match Sc::from_u32(word) {
        Some(Sc::Function) => Ec::Global(crate::__naga::AddressSpace::Function),
        Some(Sc::Input) => Ec::Input,
        Some(Sc::Output) => Ec::Output,
        Some(Sc::Private) => Ec::Global(crate::__naga::AddressSpace::Private),
        Some(Sc::UniformConstant) => Ec::Global(crate::__naga::AddressSpace::Handle),
        Some(Sc::StorageBuffer) => Ec::Global(crate::__naga::AddressSpace::Storage {
            //Note: this is restricted by decorations later
            access: crate::__naga::StorageAccess::LOAD | crate::__naga::StorageAccess::STORE,
        }),
        // we expect the `Storage` case to be filtered out before calling this function.
        Some(Sc::Uniform) => Ec::Global(crate::__naga::AddressSpace::Uniform),
        Some(Sc::Workgroup) => Ec::Global(crate::__naga::AddressSpace::WorkGroup),
        Some(Sc::PushConstant) => Ec::Global(crate::__naga::AddressSpace::Immediate),
        _ => return Err(Error::UnsupportedStorageClass(word)),
    })
}

/*!
[`Module`](super::Module) processing functionality.
*/

mod constant_evaluator;
mod emitter;
pub mod index;
mod keyword_set;
mod layouter;
mod namer;
mod overloads;
mod terminator;
mod type_methods;
mod typifier;

pub use constant_evaluator::{
    ConstantEvaluator, ConstantEvaluatorError, ExpressionKind, ExpressionKindTracker,
};
pub use emitter::Emitter;
pub use index::{BoundsCheckPolicies, BoundsCheckPolicy, IndexableLength, IndexableLengthError};
pub use keyword_set::{CaseInsensitiveKeywordSet, KeywordSet};
pub use layouter::{Alignment, LayoutError, LayoutErrorInner, Layouter, TypeLayout};
pub use namer::{EntryPointIndex, ExternalTextureNameKey, NameKey, Namer};
pub use overloads::{Conclusion, MissingSpecialType, OverloadSet, Rule};
pub use terminator::ensure_block_returns;
use thiserror::Error;
pub use type_methods::{
    concrete_int_scalars, min_max_float_representable_by, vector_size_str, vector_sizes,
};
pub use typifier::{compare_types, ResolveContext, ResolveError, TypeResolution};

use crate::__naga::non_max_u32::NonMaxU32;

impl From<super::StorageFormat> for super::Scalar {
    fn from(format: super::StorageFormat) -> Self {
        use super::{ScalarKind as Sk, StorageFormat as Sf};
        let kind = match format {
            Sf::R8Unorm => Sk::Float,
            Sf::R8Snorm => Sk::Float,
            Sf::R8Uint => Sk::Uint,
            Sf::R8Sint => Sk::Sint,
            Sf::R16Uint => Sk::Uint,
            Sf::R16Sint => Sk::Sint,
            Sf::R16Float => Sk::Float,
            Sf::Rg8Unorm => Sk::Float,
            Sf::Rg8Snorm => Sk::Float,
            Sf::Rg8Uint => Sk::Uint,
            Sf::Rg8Sint => Sk::Sint,
            Sf::R32Uint => Sk::Uint,
            Sf::R32Sint => Sk::Sint,
            Sf::R32Float => Sk::Float,
            Sf::Rg16Uint => Sk::Uint,
            Sf::Rg16Sint => Sk::Sint,
            Sf::Rg16Float => Sk::Float,
            Sf::Rgba8Unorm => Sk::Float,
            Sf::Rgba8Snorm => Sk::Float,
            Sf::Rgba8Uint => Sk::Uint,
            Sf::Rgba8Sint => Sk::Sint,
            Sf::Bgra8Unorm => Sk::Float,
            Sf::Rgb10a2Uint => Sk::Uint,
            Sf::Rgb10a2Unorm => Sk::Float,
            Sf::Rg11b10Ufloat => Sk::Float,
            Sf::R64Uint => Sk::Uint,
            Sf::Rg32Uint => Sk::Uint,
            Sf::Rg32Sint => Sk::Sint,
            Sf::Rg32Float => Sk::Float,
            Sf::Rgba16Uint => Sk::Uint,
            Sf::Rgba16Sint => Sk::Sint,
            Sf::Rgba16Float => Sk::Float,
            Sf::Rgba32Uint => Sk::Uint,
            Sf::Rgba32Sint => Sk::Sint,
            Sf::Rgba32Float => Sk::Float,
            Sf::R16Unorm => Sk::Float,
            Sf::R16Snorm => Sk::Float,
            Sf::Rg16Unorm => Sk::Float,
            Sf::Rg16Snorm => Sk::Float,
            Sf::Rgba16Unorm => Sk::Float,
            Sf::Rgba16Snorm => Sk::Float,
        };
        let width = match format {
            Sf::R64Uint => 8,
            _ => 4,
        };
        super::Scalar { kind, width }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HashableLiteral {
    F64(u64),
    F32(u32),
    F16(u16),
    U16(u16),
    I16(i16),
    U32(u32),
    I32(i32),
    U64(u64),
    I64(i64),
    Bool(bool),
    AbstractInt(i64),
    AbstractFloat(u64),
}

impl From<crate::__naga::Literal> for HashableLiteral {
    fn from(l: crate::__naga::Literal) -> Self {
        match l {
            crate::__naga::Literal::F64(v) => Self::F64(v.to_bits()),
            crate::__naga::Literal::F32(v) => Self::F32(v.to_bits()),
            crate::__naga::Literal::F16(v) => Self::F16(v.to_bits()),
            crate::__naga::Literal::U16(v) => Self::U16(v),
            crate::__naga::Literal::I16(v) => Self::I16(v),
            crate::__naga::Literal::U32(v) => Self::U32(v),
            crate::__naga::Literal::I32(v) => Self::I32(v),
            crate::__naga::Literal::U64(v) => Self::U64(v),
            crate::__naga::Literal::I64(v) => Self::I64(v),
            crate::__naga::Literal::Bool(v) => Self::Bool(v),
            crate::__naga::Literal::AbstractInt(v) => Self::AbstractInt(v),
            crate::__naga::Literal::AbstractFloat(v) => Self::AbstractFloat(v.to_bits()),
        }
    }
}

impl crate::__naga::Literal {
    pub const fn new(value: u8, scalar: crate::__naga::Scalar) -> Option<Self> {
        match (value, scalar.kind, scalar.width) {
            (value, crate::__naga::ScalarKind::Float, 8) => Some(Self::F64(value as _)),
            (value, crate::__naga::ScalarKind::Float, 4) => Some(Self::F32(value as _)),
            (value, crate::__naga::ScalarKind::Float, 2) => {
                Some(Self::F16(half::f16::from_f32_const(value as _)))
            }
            (value, crate::__naga::ScalarKind::Uint, 2) => Some(Self::U16(value as _)),
            (value, crate::__naga::ScalarKind::Sint, 2) => Some(Self::I16(value as _)),
            (value, crate::__naga::ScalarKind::Uint, 4) => Some(Self::U32(value as _)),
            (value, crate::__naga::ScalarKind::Sint, 4) => Some(Self::I32(value as _)),
            (value, crate::__naga::ScalarKind::Uint, 8) => Some(Self::U64(value as _)),
            (value, crate::__naga::ScalarKind::Sint, 8) => Some(Self::I64(value as _)),
            (1, crate::__naga::ScalarKind::Bool, crate::__naga::BOOL_WIDTH) => {
                Some(Self::Bool(true))
            }
            (0, crate::__naga::ScalarKind::Bool, crate::__naga::BOOL_WIDTH) => {
                Some(Self::Bool(false))
            }
            (value, crate::__naga::ScalarKind::AbstractInt, 8) => {
                Some(Self::AbstractInt(value as _))
            }
            (value, crate::__naga::ScalarKind::AbstractFloat, 8) => {
                Some(Self::AbstractFloat(value as _))
            }
            _ => None,
        }
    }

    pub const fn zero(scalar: crate::__naga::Scalar) -> Option<Self> {
        Self::new(0, scalar)
    }

    pub const fn one(scalar: crate::__naga::Scalar) -> Option<Self> {
        Self::new(1, scalar)
    }

    pub const fn minus_one(scalar: crate::__naga::Scalar) -> Option<Self> {
        match (scalar.kind, scalar.width) {
            (crate::__naga::ScalarKind::Float, 8) => Some(Self::F64(-1.0)),
            (crate::__naga::ScalarKind::Float, 4) => Some(Self::F32(-1.0)),
            (crate::__naga::ScalarKind::Float, 2) => {
                Some(Self::F16(half::f16::from_f32_const(-1.0)))
            }
            (crate::__naga::ScalarKind::Sint, 8) => Some(Self::I64(-1)),
            (crate::__naga::ScalarKind::Sint, 4) => Some(Self::I32(-1)),
            (crate::__naga::ScalarKind::Sint, 2) => Some(Self::I16(-1)),
            (crate::__naga::ScalarKind::AbstractInt, 8) => Some(Self::AbstractInt(-1)),
            _ => None,
        }
    }

    pub const fn width(&self) -> crate::__naga::Bytes {
        match *self {
            Self::F64(_) | Self::I64(_) | Self::U64(_) => 8,
            Self::F32(_) | Self::U32(_) | Self::I32(_) => 4,
            Self::F16(_) | Self::U16(_) | Self::I16(_) => 2,
            Self::Bool(_) => crate::__naga::BOOL_WIDTH,
            Self::AbstractInt(_) | Self::AbstractFloat(_) => crate::__naga::ABSTRACT_WIDTH,
        }
    }
    pub const fn scalar(&self) -> crate::__naga::Scalar {
        match *self {
            Self::F64(_) => crate::__naga::Scalar::F64,
            Self::F32(_) => crate::__naga::Scalar::F32,
            Self::F16(_) => crate::__naga::Scalar::F16,
            Self::U16(_) => crate::__naga::Scalar::U16,
            Self::I16(_) => crate::__naga::Scalar::I16,
            Self::U32(_) => crate::__naga::Scalar::U32,
            Self::I32(_) => crate::__naga::Scalar::I32,
            Self::U64(_) => crate::__naga::Scalar::U64,
            Self::I64(_) => crate::__naga::Scalar::I64,
            Self::Bool(_) => crate::__naga::Scalar::BOOL,
            Self::AbstractInt(_) => crate::__naga::Scalar::ABSTRACT_INT,
            Self::AbstractFloat(_) => crate::__naga::Scalar::ABSTRACT_FLOAT,
        }
    }
    pub const fn scalar_kind(&self) -> crate::__naga::ScalarKind {
        self.scalar().kind
    }
    pub const fn ty_inner(&self) -> crate::__naga::TypeInner {
        crate::__naga::TypeInner::Scalar(self.scalar())
    }
}

impl TryFrom<crate::__naga::Literal> for u32 {
    type Error = ConstValueError;

    fn try_from(value: crate::__naga::Literal) -> Result<Self, Self::Error> {
        match value {
            crate::__naga::Literal::U16(value) => Ok(value as u32),
            crate::__naga::Literal::I16(value) => {
                value.try_into().map_err(|_| ConstValueError::Negative)
            }
            crate::__naga::Literal::U32(value) => Ok(value),
            crate::__naga::Literal::I32(value) => {
                value.try_into().map_err(|_| ConstValueError::Negative)
            }
            _ => Err(ConstValueError::InvalidType),
        }
    }
}

impl TryFrom<crate::__naga::Literal> for bool {
    type Error = ConstValueError;

    fn try_from(value: crate::__naga::Literal) -> Result<Self, Self::Error> {
        match value {
            crate::__naga::Literal::Bool(value) => Ok(value),
            _ => Err(ConstValueError::InvalidType),
        }
    }
}

impl super::AddressSpace {
    pub fn access(self) -> crate::__naga::StorageAccess {
        use crate::__naga::StorageAccess as Sa;
        match self {
            crate::__naga::AddressSpace::Function
            | crate::__naga::AddressSpace::Private
            | crate::__naga::AddressSpace::WorkGroup => Sa::LOAD | Sa::STORE,
            crate::__naga::AddressSpace::Uniform => Sa::LOAD,
            crate::__naga::AddressSpace::Storage { access } => access,
            crate::__naga::AddressSpace::Handle => Sa::LOAD,
            crate::__naga::AddressSpace::Immediate => Sa::LOAD,
            // TaskPayload isn't always writable, but this is checked for elsewhere,
            // when not using multiple payloads and matching the entry payload is checked.
            crate::__naga::AddressSpace::TaskPayload => Sa::LOAD | Sa::STORE,
            crate::__naga::AddressSpace::RayPayload
            | crate::__naga::AddressSpace::IncomingRayPayload => Sa::LOAD | Sa::STORE,
        }
    }
}

impl super::MathFunction {
    pub const fn argument_count(&self) -> usize {
        match *self {
            // comparison
            Self::Abs => 1,
            Self::Min => 2,
            Self::Max => 2,
            Self::Clamp => 3,
            Self::Saturate => 1,
            // trigonometry
            Self::Cos => 1,
            Self::Cosh => 1,
            Self::Sin => 1,
            Self::Sinh => 1,
            Self::Tan => 1,
            Self::Tanh => 1,
            Self::Acos => 1,
            Self::Asin => 1,
            Self::Atan => 1,
            Self::Atan2 => 2,
            Self::Asinh => 1,
            Self::Acosh => 1,
            Self::Atanh => 1,
            Self::Radians => 1,
            Self::Degrees => 1,
            // decomposition
            Self::Ceil => 1,
            Self::Floor => 1,
            Self::Round => 1,
            Self::Fract => 1,
            Self::Trunc => 1,
            Self::Modf => 1,
            Self::Frexp => 1,
            Self::Ldexp => 2,
            // exponent
            Self::Exp => 1,
            Self::Exp2 => 1,
            Self::Log => 1,
            Self::Log2 => 1,
            Self::Pow => 2,
            // geometry
            Self::Dot => 2,
            Self::Dot4I8Packed => 2,
            Self::Dot4U8Packed => 2,
            Self::Outer => 2,
            Self::Cross => 2,
            Self::Distance => 2,
            Self::Length => 1,
            Self::Normalize => 1,
            Self::FaceForward => 3,
            Self::Reflect => 2,
            Self::Refract => 3,
            // computational
            Self::Sign => 1,
            Self::Fma => 3,
            Self::Mix => 3,
            Self::Step => 2,
            Self::SmoothStep => 3,
            Self::Sqrt => 1,
            Self::InverseSqrt => 1,
            Self::Inverse => 1,
            Self::Transpose => 1,
            Self::Determinant => 1,
            Self::QuantizeToF16 => 1,
            // bits
            Self::CountTrailingZeros => 1,
            Self::CountLeadingZeros => 1,
            Self::CountOneBits => 1,
            Self::ReverseBits => 1,
            Self::ExtractBits => 3,
            Self::InsertBits => 4,
            Self::FirstTrailingBit => 1,
            Self::FirstLeadingBit => 1,
            // data packing
            Self::Pack4x8snorm => 1,
            Self::Pack4x8unorm => 1,
            Self::Pack2x16snorm => 1,
            Self::Pack2x16unorm => 1,
            Self::Pack2x16float => 1,
            Self::Pack4xI8 => 1,
            Self::Pack4xU8 => 1,
            Self::Pack4xI8Clamp => 1,
            Self::Pack4xU8Clamp => 1,
            // data unpacking
            Self::Unpack4x8snorm => 1,
            Self::Unpack4x8unorm => 1,
            Self::Unpack2x16snorm => 1,
            Self::Unpack2x16unorm => 1,
            Self::Unpack2x16float => 1,
            Self::Unpack4xI8 => 1,
            Self::Unpack4xU8 => 1,
        }
    }
}

impl crate::__naga::Expression {
    /// Returns true if the expression is considered emitted at the start of a function.
    pub const fn needs_pre_emit(&self) -> bool {
        match *self {
            Self::Literal(_)
            | Self::Constant(_)
            | Self::Override(_)
            | Self::ZeroValue(_)
            | Self::FunctionArgument(_)
            | Self::GlobalVariable(_)
            | Self::LocalVariable(_) => true,
            _ => false,
        }
    }

    /// Return true if this expression is a dynamic array/vector/matrix index,
    /// for [`Access`].
    ///
    /// This method returns true if this expression is a dynamically computed
    /// index, and as such can only be used to index matrices when they appear
    /// behind a pointer. See the documentation for [`Access`] for details.
    ///
    /// Note, this does not check the _type_ of the given expression. It's up to
    /// the caller to establish that the `Access` expression is well-typed
    /// through other means, like [`ResolveContext`].
    ///
    /// [`Access`]: crate::Expression::Access
    /// [`ResolveContext`]: crate::proc::ResolveContext
    pub const fn is_dynamic_index(&self) -> bool {
        match *self {
            Self::Literal(_) | Self::ZeroValue(_) | Self::Constant(_) => false,
            _ => true,
        }
    }
}

impl crate::__naga::Function {
    /// Return the global variable being accessed by the expression `pointer`.
    ///
    /// Assuming that `pointer` is a series of `Access` and `AccessIndex`
    /// expressions that ultimately access some part of a `GlobalVariable`,
    /// return a handle for that global.
    ///
    /// If the expression does not ultimately access a global variable, return
    /// `None`.
    pub fn originating_global(
        &self,
        mut pointer: crate::__naga::Handle<crate::__naga::Expression>,
    ) -> Option<crate::__naga::Handle<crate::__naga::GlobalVariable>> {
        loop {
            pointer = match self.expressions[pointer] {
                crate::__naga::Expression::Access { base, .. } => base,
                crate::__naga::Expression::AccessIndex { base, .. } => base,
                crate::__naga::Expression::GlobalVariable(handle) => return Some(handle),
                // Other expressions are not on this path to a global.
                _ => return None,
            }
        }
    }
}

impl crate::__naga::SampleLevel {
    pub const fn implicit_derivatives(&self) -> bool {
        match *self {
            Self::Auto | Self::Bias(_) => true,
            Self::Zero | Self::Exact(_) | Self::Gradient { .. } => false,
        }
    }
}

impl crate::__naga::Binding {
    pub const fn to_built_in(&self) -> Option<crate::__naga::BuiltIn> {
        match *self {
            crate::__naga::Binding::BuiltIn(built_in) => Some(built_in),
            Self::Location { .. } => None,
        }
    }
}

impl super::SwizzleComponent {
    pub const XYZW: [Self; 4] = [Self::X, Self::Y, Self::Z, Self::W];

    pub const fn index(&self) -> u32 {
        match *self {
            Self::X => 0,
            Self::Y => 1,
            Self::Z => 2,
            Self::W => 3,
        }
    }
    pub const fn from_index(idx: u32) -> Self {
        match idx {
            0 => Self::X,
            1 => Self::Y,
            2 => Self::Z,
            _ => Self::W,
        }
    }
}

impl super::ImageClass {
    pub const fn is_multisampled(self) -> bool {
        match self {
            crate::__naga::ImageClass::Sampled { multi, .. }
            | crate::__naga::ImageClass::Depth { multi } => multi,
            crate::__naga::ImageClass::Storage { .. } => false,
            crate::__naga::ImageClass::External => false,
        }
    }

    pub const fn is_mipmapped(self) -> bool {
        match self {
            crate::__naga::ImageClass::Sampled { multi, .. }
            | crate::__naga::ImageClass::Depth { multi } => !multi,
            crate::__naga::ImageClass::Storage { .. } => false,
            crate::__naga::ImageClass::External => false,
        }
    }

    pub const fn is_depth(self) -> bool {
        matches!(self, crate::__naga::ImageClass::Depth { .. })
    }
}

impl crate::__naga::Module {
    pub const fn to_ctx(&self) -> GlobalCtx<'_> {
        GlobalCtx {
            types: &self.types,
            constants: &self.constants,
            overrides: &self.overrides,
            global_expressions: &self.global_expressions,
        }
    }

    pub fn compare_types(&self, lhs: &TypeResolution, rhs: &TypeResolution) -> bool {
        compare_types(lhs, rhs, &self.types)
    }
}

#[derive(Debug)]
pub enum ConstValueError {
    NonConst,
    Negative,
    InvalidType,
}

impl From<core::convert::Infallible> for ConstValueError {
    fn from(_: core::convert::Infallible) -> Self {
        unreachable!()
    }
}

#[derive(Clone, Copy, Debug)]
pub struct GlobalCtx<'a> {
    pub types: &'a crate::__naga::UniqueArena<crate::__naga::Type>,
    pub constants: &'a crate::__naga::Arena<crate::__naga::Constant>,
    pub overrides: &'a crate::__naga::Arena<crate::__naga::Override>,
    pub global_expressions: &'a crate::__naga::Arena<crate::__naga::Expression>,
}

impl GlobalCtx<'_> {
    /// Try to evaluate the expression in `self.global_expressions` using its `handle`
    /// and return it as a `T: TryFrom<ir::Literal>`.
    ///
    /// This currently only evaluates scalar expressions. If adding support for vectors,
    /// consider changing `valid::expression::validate_constant_shift_amounts` to use that
    /// support.
    #[cfg_attr(
        not(any(
            jadix_naga_feature_glsl_in,
            jadix_naga_feature_spv_in,
            jadix_naga_feature_wgsl_in,
            jadix_naga_glsl_out,
            jadix_naga_hlsl_out,
            jadix_naga_msl_out,
            jadix_naga_wgsl_out
        )),
        allow(dead_code)
    )]
    pub(super) fn get_const_val<T, E>(
        &self,
        handle: crate::__naga::Handle<crate::__naga::Expression>,
    ) -> Result<T, ConstValueError>
    where
        T: TryFrom<crate::__naga::Literal, Error = E>,
        E: Into<ConstValueError>,
    {
        self.get_const_val_from(handle, self.global_expressions)
    }

    pub(super) fn get_const_val_from<T, E>(
        &self,
        handle: crate::__naga::Handle<crate::__naga::Expression>,
        arena: &crate::__naga::Arena<crate::__naga::Expression>,
    ) -> Result<T, ConstValueError>
    where
        T: TryFrom<crate::__naga::Literal, Error = E>,
        E: Into<ConstValueError>,
    {
        fn get(
            gctx: GlobalCtx,
            handle: crate::__naga::Handle<crate::__naga::Expression>,
            arena: &crate::__naga::Arena<crate::__naga::Expression>,
        ) -> Option<crate::__naga::Literal> {
            match arena[handle] {
                crate::__naga::Expression::Literal(literal) => Some(literal),
                crate::__naga::Expression::ZeroValue(ty) => match gctx.types[ty].inner {
                    crate::__naga::TypeInner::Scalar(scalar) => {
                        crate::__naga::Literal::zero(scalar)
                    }
                    _ => None,
                },
                _ => None,
            }
        }
        let value = match arena[handle] {
            crate::__naga::Expression::Constant(c) => {
                get(*self, self.constants[c].init, self.global_expressions)
            }
            _ => get(*self, handle, arena),
        };
        match value {
            Some(v) => v.try_into().map_err(Into::into),
            None => Err(ConstValueError::NonConst),
        }
    }

    pub fn compare_types(&self, lhs: &TypeResolution, rhs: &TypeResolution) -> bool {
        compare_types(lhs, rhs, self.types)
    }
}

#[derive(Error, Debug, Clone, Copy, PartialEq)]
pub enum ResolveArraySizeError {
    #[error("array element count must be positive (> 0)")]
    ExpectedPositiveArrayLength,
    #[error("internal: array size override has not been resolved")]
    NonConstArrayLength,
}

impl crate::__naga::ArraySize {
    /// Return the number of elements that `size` represents, if known at code generation time.
    ///
    /// If `size` is override-based, return an error unless the override's
    /// initializer is a fully evaluated constant expression. You can call
    /// [`pipeline_constants::process_overrides`] to supply values for a
    /// module's overrides and ensure their initializers are fully evaluated, as
    /// this function expects.
    ///
    /// [`pipeline_constants::process_overrides`]: crate::back::pipeline_constants::process_overrides
    pub fn resolve(&self, gctx: GlobalCtx) -> Result<IndexableLength, ResolveArraySizeError> {
        match *self {
            crate::__naga::ArraySize::Constant(length) => Ok(IndexableLength::Known(length.get())),
            crate::__naga::ArraySize::Pending(handle) => {
                let Some(expr) = gctx.overrides[handle].init else {
                    return Err(ResolveArraySizeError::NonConstArrayLength);
                };
                let length = gctx.get_const_val(expr).map_err(|err| match err {
                    ConstValueError::NonConst => ResolveArraySizeError::NonConstArrayLength,
                    ConstValueError::Negative | ConstValueError::InvalidType => {
                        ResolveArraySizeError::ExpectedPositiveArrayLength
                    }
                })?;

                if length == 0 {
                    return Err(ResolveArraySizeError::ExpectedPositiveArrayLength);
                }

                Ok(IndexableLength::Known(length))
            }
            crate::__naga::ArraySize::Dynamic => Ok(IndexableLength::Dynamic),
        }
    }
}

/// Return an iterator over the individual components assembled by a
/// `Compose` expression.
///
/// Given `ty` and `components` from an `Expression::Compose`, return an
/// iterator over the components of the resulting value.
///
/// Normally, this would just be an iterator over `components`. However,
/// `Compose` expressions can concatenate vectors, in which case the i'th
/// value being composed is not generally the i'th element of `components`.
/// This function consults `ty` to decide if this concatenation is occurring,
/// and returns an iterator that produces the components of the result of
/// the `Compose` expression in either case.
pub fn flatten_compose<'arenas>(
    ty: crate::__naga::Handle<crate::__naga::Type>,
    components: &'arenas [crate::__naga::Handle<crate::__naga::Expression>],
    expressions: &'arenas crate::__naga::Arena<crate::__naga::Expression>,
    types: &'arenas crate::__naga::UniqueArena<crate::__naga::Type>,
) -> impl Iterator<Item = crate::__naga::Handle<crate::__naga::Expression>> + 'arenas {
    // Returning `impl Iterator` is a bit tricky. We may or may not
    // want to flatten the components, but we have to settle on a
    // single concrete type to return. This function returns a single
    // iterator chain that handles both the flattening and
    // non-flattening cases.
    let (size, is_vector) = if let crate::__naga::TypeInner::Vector { size, .. } = types[ty].inner {
        (size as usize, true)
    } else {
        (components.len(), false)
    };

    /// Flatten `Compose` expressions if `is_vector` is true.
    fn flatten_compose<'c>(
        component: &'c crate::__naga::Handle<crate::__naga::Expression>,
        is_vector: bool,
        expressions: &'c crate::__naga::Arena<crate::__naga::Expression>,
    ) -> &'c [crate::__naga::Handle<crate::__naga::Expression>] {
        if is_vector {
            if let crate::__naga::Expression::Compose {
                ty: _,
                components: ref subcomponents,
            } = expressions[*component]
            {
                return subcomponents;
            }
        }
        core::slice::from_ref(component)
    }

    /// Flatten `Splat` expressions if `is_vector` is true.
    fn flatten_splat<'c>(
        component: &'c crate::__naga::Handle<crate::__naga::Expression>,
        is_vector: bool,
        expressions: &'c crate::__naga::Arena<crate::__naga::Expression>,
    ) -> impl Iterator<Item = crate::__naga::Handle<crate::__naga::Expression>> {
        let mut expr = *component;
        let mut count = 1;
        if is_vector {
            if let crate::__naga::Expression::Splat { size, value } = expressions[expr] {
                expr = value;
                count = size as usize;
            }
        }
        core::iter::repeat_n(expr, count)
    }

    // Expressions like `vec4(vec3(vec2(6, 7), 8), 9)` require us to
    // flatten up to two levels of `Compose` expressions.
    //
    // Expressions like `vec4(vec3(1.0), 1.0)` require us to flatten
    // `Splat` expressions. Fortunately, the operand of a `Splat` must
    // be a scalar, so we can stop there.
    components
        .iter()
        .flat_map(move |component| flatten_compose(component, is_vector, expressions))
        .flat_map(move |component| flatten_compose(component, is_vector, expressions))
        .flat_map(move |component| flatten_splat(component, is_vector, expressions))
        .take(size)
}

#[test]
fn test_matrix_size() {
    let module = crate::__naga::Module::default();
    assert_eq!(
        crate::__naga::TypeInner::Matrix {
            columns: crate::__naga::VectorSize::Tri,
            rows: crate::__naga::VectorSize::Tri,
            scalar: crate::__naga::Scalar::F32,
        }
        .size(module.to_ctx()),
        48,
    );
}

impl crate::__naga::Module {
    /// Extracts mesh shader info from a mesh output global variable. Used in frontends
    /// and by validators. This only validates the output variable itself, and not the
    /// vertex and primitive output types.
    ///
    /// The output contains the extracted mesh stage info, with overrides unset,
    /// and then the overrides separately. This is because the overrides should be
    /// treated as expressions elsewhere, but that requires mutably modifying the
    /// module and the expressions should only be created at parse time, not validation
    /// time.
    #[allow(clippy::type_complexity)]
    pub fn analyze_mesh_shader_info(
        &self,
        gv: crate::__naga::Handle<crate::__naga::GlobalVariable>,
    ) -> (
        crate::__naga::MeshStageInfo,
        [Option<crate::__naga::Handle<crate::__naga::Override>>; 2],
        Option<crate::__naga::WithSpan<crate::__naga::valid::EntryPointError>>,
    ) {
        use crate::__naga::span::AddSpan;
        use crate::__naga::valid::EntryPointError;
        #[derive(Default)]
        struct OutError {
            pub inner: Option<EntryPointError>,
        }
        impl OutError {
            pub fn set(&mut self, err: EntryPointError) {
                if self.inner.is_none() {
                    self.inner = Some(err);
                }
            }
        }

        // Used to temporarily initialize stuff
        let null_type = crate::__naga::Handle::new(NonMaxU32::new(0).unwrap());
        let mut output = crate::__naga::MeshStageInfo {
            topology: crate::__naga::MeshOutputTopology::Triangles,
            max_vertices: 0,
            max_vertices_override: None,
            max_primitives: 0,
            max_primitives_override: None,
            vertex_output_type: null_type,
            primitive_output_type: null_type,
            output_variable: gv,
        };
        // Stores the error to output, if any.
        let mut error = OutError::default();
        let r#type = &self.types[self.global_variables[gv].ty].inner;

        let mut topology = output.topology;
        // Max, max override, type
        let mut vertex_info = (0, None, null_type);
        let mut primitive_info = (0, None, null_type);

        match r#type {
            &crate::__naga::TypeInner::Struct { ref members, .. } => {
                let mut builtins = crate::__naga::FastHashSet::default();
                for member in members {
                    match member.binding {
                        Some(crate::__naga::Binding::BuiltIn(
                            crate::__naga::BuiltIn::VertexCount,
                        )) => {
                            // Must have type u32
                            if self.types[member.ty].inner.scalar()
                                != Some(crate::__naga::Scalar::U32)
                            {
                                error.set(EntryPointError::BadMeshOutputVariableField);
                            }
                            // Each builtin should only occur once
                            if builtins.contains(&crate::__naga::BuiltIn::VertexCount) {
                                error.set(EntryPointError::BadMeshOutputVariableType);
                            }
                            builtins.insert(crate::__naga::BuiltIn::VertexCount);
                        }
                        Some(crate::__naga::Binding::BuiltIn(
                            crate::__naga::BuiltIn::PrimitiveCount,
                        )) => {
                            // Must have type u32
                            if self.types[member.ty].inner.scalar()
                                != Some(crate::__naga::Scalar::U32)
                            {
                                error.set(EntryPointError::BadMeshOutputVariableField);
                            }
                            // Each builtin should only occur once
                            if builtins.contains(&crate::__naga::BuiltIn::PrimitiveCount) {
                                error.set(EntryPointError::BadMeshOutputVariableType);
                            }
                            builtins.insert(crate::__naga::BuiltIn::PrimitiveCount);
                        }
                        Some(crate::__naga::Binding::BuiltIn(
                            crate::__naga::BuiltIn::Vertices | crate::__naga::BuiltIn::Primitives,
                        )) => {
                            let ty = &self.types[member.ty].inner;
                            // Analyze the array type to determine size and vertex/primitive type
                            let (a, b, c) = match ty {
                                &crate::__naga::TypeInner::Array { base, size, .. } => {
                                    let ty = base;
                                    let (max, max_override) = match size {
                                        crate::__naga::ArraySize::Constant(a) => (a.get(), None),
                                        crate::__naga::ArraySize::Pending(o) => (0, Some(o)),
                                        crate::__naga::ArraySize::Dynamic => {
                                            error.set(EntryPointError::BadMeshOutputVariableField);
                                            (0, None)
                                        }
                                    };
                                    (max, max_override, ty)
                                }
                                _ => {
                                    error.set(EntryPointError::BadMeshOutputVariableField);
                                    (0, None, null_type)
                                }
                            };
                            if matches!(
                                member.binding,
                                Some(crate::__naga::Binding::BuiltIn(
                                    crate::__naga::BuiltIn::Primitives
                                ))
                            ) {
                                // Primitives require special analysis to determine topology
                                primitive_info = (a, b, c);
                                match self.types[c].inner {
                                    crate::__naga::TypeInner::Struct { ref members, .. } => {
                                        for member in members {
                                            match member.binding {
                                                Some(crate::__naga::Binding::BuiltIn(
                                                    crate::__naga::BuiltIn::PointIndex,
                                                )) => {
                                                    topology =
                                                        crate::__naga::MeshOutputTopology::Points;
                                                }
                                                Some(crate::__naga::Binding::BuiltIn(
                                                    crate::__naga::BuiltIn::LineIndices,
                                                )) => {
                                                    topology =
                                                        crate::__naga::MeshOutputTopology::Lines;
                                                }
                                                Some(crate::__naga::Binding::BuiltIn(
                                                    crate::__naga::BuiltIn::TriangleIndices,
                                                )) => {
                                                    topology = crate::__naga::MeshOutputTopology::Triangles;
                                                }
                                                _ => (),
                                            }
                                        }
                                    }
                                    _ => (),
                                }
                                // Each builtin should only occur once
                                if builtins.contains(&crate::__naga::BuiltIn::Primitives) {
                                    error.set(EntryPointError::BadMeshOutputVariableType);
                                }
                                builtins.insert(crate::__naga::BuiltIn::Primitives);
                            } else {
                                vertex_info = (a, b, c);
                                // Each builtin should only occur once
                                if builtins.contains(&crate::__naga::BuiltIn::Vertices) {
                                    error.set(EntryPointError::BadMeshOutputVariableType);
                                }
                                builtins.insert(crate::__naga::BuiltIn::Vertices);
                            }
                        }
                        _ => error.set(EntryPointError::BadMeshOutputVariableType),
                    }
                }
                output = crate::__naga::MeshStageInfo {
                    topology,
                    max_vertices: vertex_info.0,
                    max_vertices_override: None,
                    vertex_output_type: vertex_info.2,
                    max_primitives: primitive_info.0,
                    max_primitives_override: None,
                    primitive_output_type: primitive_info.2,
                    ..output
                }
            }
            _ => error.set(EntryPointError::BadMeshOutputVariableType),
        }
        (
            output,
            [vertex_info.1, primitive_info.1],
            error
                .inner
                .map(|a| a.with_span_handle(self.global_variables[gv].ty, &self.types)),
        )
    }

    pub fn uses_mesh_shaders(&self) -> bool {
        let binding_uses_mesh = |b: &crate::__naga::Binding| {
            matches!(
                b,
                crate::__naga::Binding::BuiltIn(
                    crate::__naga::BuiltIn::MeshTaskSize
                        | crate::__naga::BuiltIn::CullPrimitive
                        | crate::__naga::BuiltIn::PointIndex
                        | crate::__naga::BuiltIn::LineIndices
                        | crate::__naga::BuiltIn::TriangleIndices
                        | crate::__naga::BuiltIn::VertexCount
                        | crate::__naga::BuiltIn::Vertices
                        | crate::__naga::BuiltIn::PrimitiveCount
                        | crate::__naga::BuiltIn::Primitives,
                ) | crate::__naga::Binding::Location {
                    per_primitive: true,
                    ..
                }
            )
        };
        for (_, ty) in self.types.iter() {
            match ty.inner {
                crate::__naga::TypeInner::Struct { ref members, .. } => {
                    for binding in members.iter().filter_map(|m| m.binding.as_ref()) {
                        if binding_uses_mesh(binding) {
                            return true;
                        }
                    }
                }
                _ => (),
            }
        }
        for ep in &self.entry_points {
            if matches!(
                ep.stage,
                crate::__naga::ShaderStage::Mesh | crate::__naga::ShaderStage::Task
            ) {
                return true;
            }
            for binding in ep
                .function
                .arguments
                .iter()
                .filter_map(|arg| arg.binding.as_ref())
                .chain(
                    ep.function
                        .result
                        .iter()
                        .filter_map(|res| res.binding.as_ref()),
                )
            {
                if binding_uses_mesh(binding) {
                    return true;
                }
            }
        }
        if self
            .global_variables
            .iter()
            .any(|gv| gv.1.space == crate::__naga::AddressSpace::TaskPayload)
        {
            return true;
        }
        false
    }

    pub fn uses_ray_tracing(&self, ep_index: Option<usize>) -> RayTracingUses {
        let mut uses = RayTracingUses::default();
        // Whether this uses ray tracing (unknown whether the usage is pipelines or ray queries).
        let mut uses_ray_tracing = self.special_types.ray_desc.is_some();

        uses.queries |= self.special_types.ray_intersection.is_some();

        for (_, &crate::__naga::Type { ref inner, .. }) in self.types.iter() {
            // Backends do not know whether these have vertex return - that is done by us
            match *inner {
                crate::__naga::TypeInner::AccelerationStructure { .. } => {
                    uses_ray_tracing = true;
                }
                crate::__naga::TypeInner::RayQuery { .. } => uses.queries = true,
                _ => {}
            }
        }

        for (index, ep) in self.entry_points.iter().enumerate() {
            if ep_index.is_some() && ep_index != Some(index) {
                continue;
            }

            // if we have a ray tracing pipeline shader we are definitely using
            // pipelines, otherwise, if we have a ray tracing type, we might
            // be using it in the shader (which would require ray queries),
            // so we should use queries.
            if matches!(
                ep.stage,
                crate::__naga::ShaderStage::RayGeneration
                    | crate::__naga::ShaderStage::AnyHit
                    | crate::__naga::ShaderStage::ClosestHit
                    | crate::__naga::ShaderStage::Miss
            ) {
                uses.pipelines = true;
            } else {
                uses.queries |= uses_ray_tracing;
            }
        }

        uses
    }
}

#[derive(Copy, Clone, Debug, Default)]
pub struct RayTracingUses {
    pub pipelines: bool,
    pub queries: bool,
}

impl crate::__naga::MeshOutputTopology {
    pub const fn to_builtin(self) -> crate::__naga::BuiltIn {
        match self {
            Self::Points => crate::__naga::BuiltIn::PointIndex,
            Self::Lines => crate::__naga::BuiltIn::LineIndices,
            Self::Triangles => crate::__naga::BuiltIn::TriangleIndices,
        }
    }
}

impl crate::__naga::AddressSpace {
    pub const fn is_workgroup_like(self) -> bool {
        matches!(self, Self::WorkGroup | Self::TaskPayload)
    }
}

impl TryFrom<crate::__naga::ScalarKind> for naga_types::glsl::GlslScalarKind {
    type Error = ();

    fn try_from(value: crate::__naga::ScalarKind) -> Result<Self, Self::Error> {
        Ok(match value {
            crate::__naga::ScalarKind::Sint => naga_types::glsl::GlslScalarKind::Sint,
            crate::__naga::ScalarKind::Uint => naga_types::glsl::GlslScalarKind::Uint,
            crate::__naga::ScalarKind::Float => naga_types::glsl::GlslScalarKind::Float,
            _ => return Err(()),
        })
    }
}

impl From<crate::__naga::VectorSize> for naga_types::glsl::GlslVectorSize {
    fn from(val: crate::__naga::VectorSize) -> Self {
        match val {
            crate::__naga::VectorSize::Bi => naga_types::glsl::GlslVectorSize::Bi,
            crate::__naga::VectorSize::Tri => naga_types::glsl::GlslVectorSize::Tri,
            crate::__naga::VectorSize::Quad => naga_types::glsl::GlslVectorSize::Quad,
        }
    }
}

impl TryFrom<crate::__naga::Scalar> for naga_types::glsl::GlslScalar {
    type Error = ();

    fn try_from(value: crate::__naga::Scalar) -> Result<Self, Self::Error> {
        Ok(naga_types::glsl::GlslScalar {
            kind: value.kind.try_into()?,
            width: value.width,
        })
    }
}

impl TryFrom<&crate::__naga::TypeInner> for naga_types::glsl::GlslUniformType {
    type Error = ();
    fn try_from(value: &crate::__naga::TypeInner) -> Result<Self, Self::Error> {
        match *value {
            crate::__naga::TypeInner::Scalar(scalar) => Ok(
                naga_types::glsl::GlslUniformType::Scalar(scalar.try_into()?),
            ),
            crate::__naga::TypeInner::Vector { size, scalar } => {
                Ok(naga_types::glsl::GlslUniformType::Vector {
                    size: size.into(),
                    scalar: scalar.try_into()?,
                })
            }
            crate::__naga::TypeInner::Matrix {
                columns,
                rows,
                scalar,
            } => Ok(naga_types::glsl::GlslUniformType::Matrix {
                columns: columns.into(),
                rows: rows.into(),
                scalar: scalar.try_into()?,
            }),
            _ => Err(()),
        }
    }
}

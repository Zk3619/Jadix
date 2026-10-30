use alloc::{vec, vec::Vec};

use arrayvec::ArrayVec;
use spirv::Word;

use crate::__naga::{Handle, UniqueArena};

pub(super) fn bytes_to_words(bytes: &[u8]) -> Vec<Word> {
    bytes
        .chunks(4)
        .map(|chars| chars.iter().rev().fold(0u32, |u, c| (u << 8) | *c as u32))
        .collect()
}

pub(super) fn string_to_words(input: &str) -> Vec<Word> {
    let bytes = input.as_bytes();

    str_bytes_to_words(bytes)
}

pub(super) fn str_bytes_to_words(bytes: &[u8]) -> Vec<Word> {
    let mut words = bytes_to_words(bytes);
    if bytes.len().is_multiple_of(4) {
        // nul-termination
        words.push(0x0u32);
    }

    words
}

/// split a string into chunks and keep utf8 valid
#[allow(unstable_name_collisions)]
pub(super) fn string_to_byte_chunks(input: &str, limit: usize) -> Vec<&[u8]> {
    let mut offset: usize = 0;
    let mut start: usize = 0;
    let mut words = vec![];
    while offset < input.len() {
        offset = input.floor_char_boundary_polyfill(offset + limit);
        // Clippy wants us to call as_bytes() first to avoid the UTF-8 check,
        // but we want to assert the output is valid UTF-8.
        #[allow(clippy::sliced_string_as_bytes)]
        words.push(input[start..offset].as_bytes());
        start = offset;
    }

    words
}

pub(super) const fn map_storage_class(space: crate::__naga::AddressSpace) -> spirv::StorageClass {
    match space {
        crate::__naga::AddressSpace::Handle => spirv::StorageClass::UniformConstant,
        crate::__naga::AddressSpace::Function => spirv::StorageClass::Function,
        crate::__naga::AddressSpace::Private => spirv::StorageClass::Private,
        crate::__naga::AddressSpace::Storage { .. } => spirv::StorageClass::StorageBuffer,
        crate::__naga::AddressSpace::Uniform => spirv::StorageClass::Uniform,
        crate::__naga::AddressSpace::WorkGroup => spirv::StorageClass::Workgroup,
        crate::__naga::AddressSpace::Immediate => spirv::StorageClass::PushConstant,
        crate::__naga::AddressSpace::TaskPayload => spirv::StorageClass::TaskPayloadWorkgroupEXT,
        // We can't require capabilities here but we request capabilities on the ray pipeline stages
        // and when writing global variables - global variables because we may be writing an
        // uncompacted module and pipeline stages for all other cases because these can only be
        //accessed in a ray tracing pipeline stage.
        crate::__naga::AddressSpace::RayPayload => spirv::StorageClass::RayPayloadKHR,
        crate::__naga::AddressSpace::IncomingRayPayload => {
            spirv::StorageClass::IncomingRayPayloadKHR
        }
    }
}

pub(super) fn contains_builtin(
    binding: Option<&crate::__naga::Binding>,
    ty: Handle<crate::__naga::Type>,
    arena: &UniqueArena<crate::__naga::Type>,
    built_in: crate::__naga::BuiltIn,
) -> bool {
    if let Some(&crate::__naga::Binding::BuiltIn(bi)) = binding {
        bi == built_in
    } else if let crate::__naga::TypeInner::Struct { ref members, .. } = arena[ty].inner {
        members
            .iter()
            .any(|member| contains_builtin(member.binding.as_ref(), member.ty, arena, built_in))
    } else {
        false // unreachable
    }
}

impl crate::__naga::AddressSpace {
    pub(super) const fn to_spirv_semantics_and_scope(
        self,
    ) -> (spirv::MemorySemantics, spirv::Scope) {
        match self {
            Self::Storage { .. } => (spirv::MemorySemantics::empty(), spirv::Scope::Device),
            Self::WorkGroup => (spirv::MemorySemantics::empty(), spirv::Scope::Workgroup),
            Self::Uniform => (spirv::MemorySemantics::empty(), spirv::Scope::Device),
            Self::Handle => (spirv::MemorySemantics::empty(), spirv::Scope::Device),
            _ => (spirv::MemorySemantics::empty(), spirv::Scope::Invocation),
        }
    }
}

/// Return true if the global requires a type decorated with `Block`.
///
/// See [`back::spv::GlobalVariable`] for details.
///
/// [`back::spv::GlobalVariable`]: super::GlobalVariable
pub fn global_needs_wrapper(
    ir_module: &crate::__naga::Module,
    var: &crate::__naga::GlobalVariable,
) -> bool {
    match var.space {
        crate::__naga::AddressSpace::Uniform
        | crate::__naga::AddressSpace::Storage { .. }
        | crate::__naga::AddressSpace::Immediate => {}
        _ => return false,
    };
    match ir_module.types[var.ty].inner {
        crate::__naga::TypeInner::Struct {
            ref members,
            span: _,
        } => match members.last() {
            Some(member) => match ir_module.types[member.ty].inner {
                // Structs with dynamically sized arrays can't be copied and can't be wrapped.
                crate::__naga::TypeInner::Array {
                    size: crate::__naga::ArraySize::Dynamic,
                    ..
                } => false,
                _ => true,
            },
            None => false,
        },
        crate::__naga::TypeInner::BindingArray { .. } => false,
        // if it's not a structure or a binding array, let's wrap it to be able to put "Block"
        _ => true,
    }
}

/// Returns true if `pointer` refers to two-row matrix which is a member of a
/// struct in the [`crate::AddressSpace::Uniform`] address space.
pub fn is_uniform_matcx2_struct_member_access(
    ir_function: &crate::__naga::Function,
    fun_info: &crate::__naga::valid::FunctionInfo,
    ir_module: &crate::__naga::Module,
    pointer: Handle<crate::__naga::Expression>,
) -> bool {
    if let crate::__naga::TypeInner::Pointer {
        base: pointer_base_type,
        space: crate::__naga::AddressSpace::Uniform,
    } = *fun_info[pointer].ty.inner_with(&ir_module.types)
    {
        if let crate::__naga::TypeInner::Matrix {
            rows: crate::__naga::VectorSize::Bi,
            ..
        } = ir_module.types[pointer_base_type].inner
        {
            if let crate::__naga::Expression::AccessIndex {
                base: parent_pointer,
                ..
            } = ir_function.expressions[pointer]
            {
                if let crate::__naga::TypeInner::Pointer {
                    base: parent_type, ..
                } = *fun_info[parent_pointer].ty.inner_with(&ir_module.types)
                {
                    if let crate::__naga::TypeInner::Struct { .. } =
                        ir_module.types[parent_type].inner
                    {
                        return true;
                    }
                }
            }
        }
    }

    false
}

///HACK: this is taken from std unstable, remove it when std's floor_char_boundary is stable
/// and available in our msrv.
trait U8Internal {
    fn is_utf8_char_boundary_polyfill(&self) -> bool;
}

impl U8Internal for u8 {
    fn is_utf8_char_boundary_polyfill(&self) -> bool {
        // This is bit magic equivalent to: b < 128 || b >= 192
        (*self as i8) >= -0x40
    }
}

trait StrUnstable {
    fn floor_char_boundary_polyfill(&self, index: usize) -> usize;
}

impl StrUnstable for str {
    fn floor_char_boundary_polyfill(&self, index: usize) -> usize {
        if index >= self.len() {
            self.len()
        } else {
            let lower_bound = index.saturating_sub(3);
            let new_index = self.as_bytes()[lower_bound..=index]
                .iter()
                .rposition(|b| b.is_utf8_char_boundary_polyfill());

            // We know that the character boundary will be within four bytes.
            lower_bound + new_index.unwrap()
        }
    }
}

#[derive(Debug)]
pub enum BindingDecorations {
    BuiltIn(spirv::BuiltIn, ArrayVec<spirv::Decoration, 2>),
    Location {
        location: u32,
        others: ArrayVec<spirv::Decoration, 5>,
        /// If this is `Some`, use Decoration::Index with blend_src as an operand
        blend_src: Option<Word>,
    },
    None,
}

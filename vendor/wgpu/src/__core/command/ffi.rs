//! Types that are useful for FFI bindings to `wgpu`.

use crate::__core::{command::IdReferences, id};

pub type TexelCopyBufferInfo = crate::__types::TexelCopyBufferInfo<id::BufferId>;
pub type TexelCopyTextureInfo = crate::__types::TexelCopyTextureInfo<id::TextureId>;
pub type CopyExternalImageDestInfo = crate::__types::CopyExternalImageDestInfo<id::TextureId>;

pub type Command = super::Command<IdReferences>;

use alloc::vec::Vec;

#[cfg(jadix_wgpu_core_feature_serde)]
use crate::__core::command::serde_object_reference_struct;
use crate::__core::command::{ArcReferences, ReferenceType};

#[cfg(jadix_wgpu_core_feature_serde)]
use macro_rules_attribute::apply;

/// cbindgen:ignore
#[derive(Clone, Debug)]
#[cfg_attr(jadix_wgpu_core_feature_serde, apply(serde_object_reference_struct))]
pub enum ComputeCommand<R: ReferenceType> {
    SetBindGroup {
        index: u32,
        num_dynamic_offsets: usize,
        bind_group: Option<R::BindGroup>,
    },

    SetPipeline(R::ComputePipeline),

    /// Set a range of immediates to values stored in `immediates_data`.
    SetImmediate {
        /// The byte offset within the immediate data storage to write to. This
        /// must be a multiple of four.
        offset: u32,

        /// The number of bytes to write. This must be a multiple of four.
        size_bytes: u32,

        /// Index in `immediates_data` of the start of the data
        /// to be written.
        ///
        /// Note: this is not a byte offset like `offset`. Rather, it is the
        /// index of the first `u32` element in `immediates_data` to read.
        values_offset: u32,
    },

    DispatchWorkgroups([u32; 3]),

    DispatchWorkgroupsIndirect {
        buffer: R::Buffer,
        offset: crate::__types::BufferAddress,
    },

    PushDebugGroup {
        color: u32,
        len: usize,
    },

    PopDebugGroup,

    InsertDebugMarker {
        color: u32,
        len: usize,
    },

    WriteTimestamp {
        query_set: R::QuerySet,
        query_index: u32,
    },

    BeginPipelineStatisticsQuery {
        query_set: R::QuerySet,
        query_index: u32,
    },

    EndPipelineStatisticsQuery,

    TransitionResources {
        buffer_transitions: Vec<crate::__types::BufferTransition<R::Buffer>>,
        texture_transitions: Vec<crate::__types::TextureTransition<R::TextureView>>,
    },
}

/// cbindgen:ignore
pub type ArcComputeCommand = ComputeCommand<ArcReferences>;

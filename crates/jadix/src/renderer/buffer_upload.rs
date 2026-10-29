//! Persistent scene streams. Compare bytes, upload one aligned changed span,
//! and keep spare GPU capacity across frames. No hashes participate in correctness.

use std::ops::Range;

use crate::app::frame_stats::{note_scene_upload, SceneUploads};

pub(crate) struct RetainedBuffer {
    pub buffer: wgpu::Buffer,
    bytes: Vec<u8>,
    label: &'static str,
    usage: wgpu::BufferUsages,
}

pub(crate) fn retained_uploads_enabled() -> bool {
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ENABLED.get_or_init(|| std::env::var("JADIX_RETAINED_UPLOADS").as_deref() != Ok("0"))
}

impl RetainedBuffer {
    pub fn new(
        device: &wgpu::Device,
        label: &'static str,
        usage: wgpu::BufferUsages,
        bytes: &[u8],
    ) -> Self {
        use wgpu::util::DeviceExt;
        let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some(label),
            contents: if bytes.is_empty() { &[0; 4] } else { bytes },
            usage: usage | wgpu::BufferUsages::COPY_DST,
        });
        Self {
            buffer,
            bytes: bytes.to_vec(),
            label,
            usage,
        }
    }

    /// Returns whether the buffer identity changed (storage bind groups must rebind).
    pub fn upload(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, bytes: &[u8]) -> bool {
        let stats = self.upload_with_mode(device, queue, bytes, retained_uploads_enabled());
        note_scene_upload(stats);
        stats.reallocations != 0
    }

    fn upload_with_mode(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        bytes: &[u8],
        retained: bool,
    ) -> SceneUploads {
        assert_eq!(bytes.len() % wgpu::COPY_BUFFER_ALIGNMENT as usize, 0);
        let mut stats = SceneUploads {
            source_bytes: bytes.len() as u64,
            ..Default::default()
        };
        let grew = bytes.len() as u64 > self.buffer.size();
        if grew {
            // Cap geometric growth at the device's binding limit. The caller's
            // requested size still goes through wgpu validation if it exceeds it.
            let limit = if self.usage.contains(wgpu::BufferUsages::STORAGE) {
                device.limits().max_storage_buffer_binding_size
            } else {
                device.limits().max_buffer_size
            };
            let size = (bytes.len() as u64)
                .next_power_of_two()
                .min(limit)
                .max(bytes.len() as u64);
            self.buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(self.label),
                size,
                usage: self.usage | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            stats.reallocations = 1;
        }
        let range = if grew || !retained {
            0..bytes.len()
        } else {
            changed_range(&self.bytes, bytes)
        };
        if !range.is_empty() {
            queue.write_buffer(&self.buffer, range.start as u64, &bytes[range.clone()]);
            stats.uploaded_bytes = range.len() as u64;
            stats.write_calls = 1;
        }
        // A shrink changes the active draw length, not the old GPU tail. Future
        // growth must rewrite that tail, even if it matches a much older frame.
        if retained {
            self.bytes.resize(bytes.len(), 0);
            self.bytes[range.clone()].copy_from_slice(&bytes[range]);
        } else {
            self.bytes.clear();
        }
        stats
    }
}

fn changed_range(old: &[u8], new: &[u8]) -> Range<usize> {
    const BLOCK: usize = 4096;
    let shared = old.len().min(new.len());
    if old[..shared] == new[..shared] {
        return if shared == new.len() {
            0..0
        } else {
            shared..new.len()
        };
    }
    // Compare blocks with slice equality so idle scans use the standard library's
    // bulk comparison. Refine only the first/last differing blocks to 4-byte writes.
    let start = old[..shared]
        .chunks(BLOCK)
        .zip(new[..shared].chunks(BLOCK))
        .position(|(a, b)| a != b)
        .map(|block| {
            let lo = block * BLOCK;
            lo + old[lo..shared.min(lo + BLOCK)]
                .iter()
                .zip(&new[lo..shared.min(lo + BLOCK)])
                .position(|(a, b)| a != b)
                .unwrap()
        })
        .unwrap_or(shared);
    if start == new.len() {
        return 0..0;
    }
    let end = if old.len() < new.len() {
        new.len()
    } else {
        let tail = &new[start..];
        let block = tail
            .rchunks(BLOCK)
            .zip(old[start..new.len()].rchunks(BLOCK))
            .position(|(a, b)| a != b)
            .unwrap();
        let hi = new.len() - block * BLOCK;
        let lo = hi.saturating_sub(BLOCK).max(start);
        hi - new[lo..hi]
            .iter()
            .rev()
            .zip(old[lo..hi].iter().rev())
            .position(|(a, b)| a != b)
            .unwrap()
    };
    (start / 4 * 4)..end.div_ceil(4) * 4
}

#[cfg(test)]
#[path = "tests/buffer_upload.rs"]
mod tests;

//! Typed, ordered compute/render recordings. This is a Jadix resource ABI;
//! Vello's encoded scene buffers and built-in coarse/fine shaders are not accepted.
//! Resources persist across submissions until explicitly freed. Encoding validates
//! the complete recording before allocating or submitting anything.
use crate::wgpu;
use std::{
    collections::HashMap,
    ops::Range,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
};
mod engine;
mod render;
pub use engine::{Download, Engine, Execution, ExecutionStats, RecordedExecution};
pub use render::{CapturedScene, Render};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ResourceId(pub u64);
impl ResourceId {
    pub fn next() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        Self(
            NEXT.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
                .expect("resource IDs exhausted"),
        )
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BufferProxy {
    pub id: ResourceId,
    pub size: u64,
    pub name: &'static str,
}
impl BufferProxy {
    pub fn new(size: u64, name: &'static str) -> Self {
        Self {
            id: ResourceId::next(),
            size,
            name,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImageFormat {
    Rgba8,
    Bgra8,
    Rgba16Float,
}
impl ImageFormat {
    pub fn to_wgpu(self) -> wgpu::TextureFormat {
        match self {
            Self::Rgba8 => wgpu::TextureFormat::Rgba8Unorm,
            Self::Bgra8 => wgpu::TextureFormat::Bgra8Unorm,
            Self::Rgba16Float => wgpu::TextureFormat::Rgba16Float,
        }
    }
    pub fn from_wgpu(format: wgpu::TextureFormat) -> Option<Self> {
        match format {
            wgpu::TextureFormat::Rgba8Unorm => Some(Self::Rgba8),
            wgpu::TextureFormat::Bgra8Unorm => Some(Self::Bgra8),
            wgpu::TextureFormat::Rgba16Float => Some(Self::Rgba16Float),
            _ => None,
        }
    }
    pub fn bytes_per_pixel(self) -> u32 {
        if self == Self::Rgba16Float {
            8
        } else {
            4
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ImageProxy {
    pub id: ResourceId,
    pub width: u32,
    pub height: u32,
    pub format: ImageFormat,
}
impl ImageProxy {
    pub fn new(width: u32, height: u32, format: ImageFormat) -> Self {
        Self {
            id: ResourceId::next(),
            width,
            height,
            format,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResourceProxy {
    Buffer(BufferProxy),
    BufferRange {
        proxy: BufferProxy,
        offset: u64,
        size: u64,
    },
    Image(ImageProxy),
}
impl From<BufferProxy> for ResourceProxy {
    fn from(v: BufferProxy) -> Self {
        Self::Buffer(v)
    }
}
impl From<ImageProxy> for ResourceProxy {
    fn from(v: ImageProxy) -> Self {
        Self::Image(v)
    }
}
impl ResourceProxy {
    pub fn new_buf(size: u64, name: &'static str) -> Self {
        BufferProxy::new(size, name).into()
    }
    pub fn new_image(w: u32, h: u32, f: ImageFormat) -> Self {
        ImageProxy::new(w, h, f).into()
    }
    pub fn as_buf(&self) -> Option<&BufferProxy> {
        match self {
            Self::Buffer(b) | Self::BufferRange { proxy: b, .. } => Some(b),
            _ => None,
        }
    }
    pub fn as_image(&self) -> Option<&ImageProxy> {
        if let Self::Image(i) = self {
            Some(i)
        } else {
            None
        }
    }
    fn id(self) -> ResourceId {
        match self {
            Self::Buffer(b) | Self::BufferRange { proxy: b, .. } => b.id,
            Self::Image(i) => i.id,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BindType {
    Buffer,
    BufReadOnly,
    Uniform,
    Image(ImageFormat),
    ImageRead(ImageFormat),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ShaderId(pub u64);
#[derive(Clone, Debug)]
pub struct DrawParams {
    pub shader: ShaderId,
    pub target: ImageProxy,
    pub resources: Vec<ResourceProxy>,
    pub vertices: Range<u32>,
    pub instances: Range<u32>,
    pub clear: Option<wgpu::Color>,
}
#[derive(Clone, Debug)]
pub enum Command {
    Upload(BufferProxy, Arc<[u8]>),
    UploadUniform(BufferProxy, Arc<[u8]>),
    UploadImage(ImageProxy, Arc<[u8]>),
    WriteImage(ImageProxy, [u32; 2], [u32; 2], Arc<[u8]>),
    Clear(BufferProxy, u64, Option<u64>),
    Dispatch(ShaderId, [u32; 3], Vec<ResourceProxy>),
    DispatchIndirect(ShaderId, BufferProxy, u64, Vec<ResourceProxy>),
    Draw(DrawParams),
    Download(BufferProxy),
    DownloadImage(ImageProxy),
    FreeBuffer(BufferProxy),
    FreeImage(ImageProxy),
}
#[derive(Clone, Debug, Default)]
pub struct Recording {
    pub commands: Vec<Command>,
}
impl Recording {
    pub fn push(&mut self, cmd: Command) {
        self.commands.push(cmd);
    }
    pub fn into_commands(self) -> Vec<Command> {
        self.commands
    }
    pub fn upload(&mut self, name: &'static str, data: impl Into<Vec<u8>>) -> BufferProxy {
        let d = data.into();
        let b = BufferProxy::new(d.len() as u64, name);
        self.push(Command::Upload(b, d.into()));
        b
    }
    pub fn upload_uniform(&mut self, name: &'static str, data: impl Into<Vec<u8>>) -> BufferProxy {
        let d = data.into();
        let b = BufferProxy::new(d.len() as u64, name);
        self.push(Command::UploadUniform(b, d.into()));
        b
    }
    pub fn upload_image(&mut self, image: ImageProxy, data: impl Into<Vec<u8>>) {
        self.push(Command::UploadImage(image, data.into().into()));
    }
    pub fn write_image(
        &mut self,
        image: ImageProxy,
        origin: [u32; 2],
        size: [u32; 2],
        data: impl Into<Vec<u8>>,
    ) {
        self.push(Command::WriteImage(image, origin, size, data.into().into()));
    }
    pub fn clear_all(&mut self, b: BufferProxy) {
        self.push(Command::Clear(b, 0, None));
    }
    pub fn dispatch(
        &mut self,
        shader: ShaderId,
        groups: [u32; 3],
        resources: impl IntoIterator<Item = ResourceProxy>,
    ) {
        self.push(Command::Dispatch(
            shader,
            groups,
            resources.into_iter().collect(),
        ));
    }
    pub fn dispatch_indirect(
        &mut self,
        shader: ShaderId,
        b: BufferProxy,
        offset: u64,
        resources: impl IntoIterator<Item = ResourceProxy>,
    ) {
        self.push(Command::DispatchIndirect(
            shader,
            b,
            offset,
            resources.into_iter().collect(),
        ));
    }
    pub fn draw(&mut self, params: DrawParams) {
        self.push(Command::Draw(params));
    }
    pub fn download(&mut self, b: BufferProxy) {
        self.push(Command::Download(b));
    }
    pub fn download_image(&mut self, i: ImageProxy) {
        self.push(Command::DownloadImage(i));
    }
    pub fn free_buffer(&mut self, b: BufferProxy) {
        self.push(Command::FreeBuffer(b));
    }
    pub fn free_image(&mut self, i: ImageProxy) {
        self.push(Command::FreeImage(i));
    }
    pub fn free_resource(&mut self, r: ResourceProxy) {
        match r {
            ResourceProxy::Image(i) => self.free_image(i),
            _ => self.free_buffer(*r.as_buf().unwrap()),
        }
    }
}
/// `max_resources` also bounds proxy tombstones until `Engine::reset_resources`.
/// Payload and resource ceilings. These are requested GPU/staging bytes, not RSS/VRAM high water.
#[derive(Clone, Copy, Debug)]
pub struct EngineLimits {
    pub max_live_bytes: u64,
    pub max_staging_bytes: u64,
    pub max_resources: usize,
    pub max_commands: usize,
    pub max_shaders: usize,
}
impl Default for EngineLimits {
    fn default() -> Self {
        Self {
            max_live_bytes: 256 << 20,
            max_staging_bytes: 256 << 20,
            max_resources: 4096,
            max_commands: 65536,
            max_shaders: 1024,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    UnsupportedDevice,
    Budget,
    InvalidResource,
    DescriptorConflict,
    ReleasedResource,
    InvalidRange,
    InvalidBinding,
    InvalidShader,
    InvalidDispatch,
    InvalidImage,
    GpuValidation,
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "compute recording: {self:?}")
    }
}
impl std::error::Error for Error {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Descriptor {
    Buffer(BufferProxy),
    Image(ImageProxy),
}
impl Descriptor {
    fn id(self) -> ResourceId {
        match self {
            Self::Buffer(b) => b.id,
            Self::Image(i) => i.id,
        }
    }
    fn bytes(self) -> Option<u64> {
        match self {
            Self::Buffer(b) => b.size.checked_add(3).map(|n| n / 4 * 4),
            Self::Image(i) => (i.width as u64)
                .checked_mul(i.height as u64)?
                .checked_mul(i.format.bytes_per_pixel() as u64),
        }
    }
}

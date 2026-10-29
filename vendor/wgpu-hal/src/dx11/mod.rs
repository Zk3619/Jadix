//! Native D3D11 bring-up, kept separate from the GLES/ANGLE path.
//!
//! Exposes an experimental graphics `Api` through `Instance::from_hal`, with a
//! distinct Dx11 identity, serialized commands, SM5 resources and DXGI surfaces.
//! Compute, depth/stencil, MSAA and advanced binding types are not implemented.

mod backend;
mod resources;
pub use backend::{Api, Instance};
pub use resources::{RenderPipeline, RenderTarget, UniformBuffer};

use alloc::{format, string::String, vec::Vec};
use parking_lot::Mutex;
use windows::{
    core::{s, PCSTR},
    Win32::{
        Foundation::HMODULE,
        Graphics::{
            Direct3D::{
                Fxc::D3DCompile, D3D_DRIVER_TYPE_UNKNOWN, D3D_FEATURE_LEVEL,
                D3D_FEATURE_LEVEL_11_0, D3D_FEATURE_LEVEL_11_1,
            },
            Direct3D11::{
                D3D11CreateDevice, ID3D11Device, ID3D11DeviceContext,
                D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_SDK_VERSION,
            },
            Dxgi::{
                CreateDXGIFactory1, IDXGIAdapter1, IDXGIFactory1, DXGI_ADAPTER_DESC1,
                DXGI_ADAPTER_FLAG_SOFTWARE, DXGI_ERROR_NOT_FOUND,
            },
        },
    },
};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Windows(#[from] windows::core::Error),
    #[error("D3D11: {0}")]
    Unsupported(&'static str),
    #[error("D3D11 shader: {0}")]
    Shader(String),
}

#[derive(Debug)]
pub struct Adapter {
    raw: IDXGIAdapter1,
    pub description: DXGI_ADAPTER_DESC1,
}

/// Enumerate hardware adapters in DXGI order, preserving their LUIDs.
pub fn enumerate_adapters() -> Result<Vec<Adapter>, Error> {
    let factory: IDXGIFactory1 = unsafe { CreateDXGIFactory1()? };
    let mut adapters = Vec::new();
    for index in 0.. {
        let raw = match unsafe { factory.EnumAdapters1(index) } {
            Ok(adapter) => adapter,
            Err(error) if error.code() == DXGI_ERROR_NOT_FOUND => break,
            Err(error) => return Err(error.into()),
        };
        let description = unsafe { raw.GetDesc1()? };
        if description.Flags & DXGI_ADAPTER_FLAG_SOFTWARE.0 as u32 == 0 {
            adapters.push(Adapter { raw, description });
        }
    }
    Ok(adapters)
}

#[derive(Debug)]
pub struct Device {
    raw: ID3D11Device,
    context: Mutex<ID3D11DeviceContext>,
    pub feature_level: D3D_FEATURE_LEVEL,
}

impl Adapter {
    /// Create a device on this exact adapter; never use WARP or a default adapter.
    pub fn open(&self) -> Result<Device, Error> {
        let (mut raw, mut context, mut feature_level) = (None, None, D3D_FEATURE_LEVEL::default());
        unsafe {
            D3D11CreateDevice(
                &self.raw,
                D3D_DRIVER_TYPE_UNKNOWN,
                HMODULE::default(),
                D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                Some(&[D3D_FEATURE_LEVEL_11_1, D3D_FEATURE_LEVEL_11_0]),
                D3D11_SDK_VERSION,
                Some(&mut raw),
                Some(&mut feature_level),
                Some(&mut context),
            )?;
        }
        Ok(Device {
            raw: raw.ok_or(Error::Unsupported("device creation returned no device"))?,
            context: Mutex::new(
                context.ok_or(Error::Unsupported("device creation returned no context"))?,
            ),
            feature_level,
        })
    }
}

impl Device {
    pub fn raw_device(&self) -> &ID3D11Device {
        &self.raw
    }

    /// Serialize all immediate-context operations, including map/unmap and submit.
    ///
    /// # Safety
    /// The closure must obey D3D11 resource lifetime, mapping and device rules and
    /// must not retain or clone the context beyond the closure.
    pub unsafe fn with_immediate_context<T>(&self, f: impl FnOnce(&ID3D11DeviceContext) -> T) -> T {
        f(&self.context.lock())
    }
}

/// Translate one validated, resource-free Naga entry point to SM5.0 bytecode.
/// The caller retains the original validation authority and shader source.
pub fn compile_shader(
    module: &naga::Module,
    info: &naga::valid::ModuleInfo,
    stage: naga::ShaderStage,
    entry: &str,
) -> Result<Vec<u8>, Error> {
    compile_shader_with_uniforms(module, info, stage, entry, &[])
}

/// Compile with an explicit WGSL binding to D3D11 constant-buffer slot mapping.
/// Only uniform buffers are supported; missing, duplicate or out-of-range slots
/// fail before FXC. The same layout must be used when creating a render pipeline.
pub fn compile_shader_with_uniforms(
    module: &naga::Module,
    info: &naga::valid::ModuleInfo,
    stage: naga::ShaderStage,
    entry: &str,
    uniforms: &[(naga::ResourceBinding, u32)],
) -> Result<Vec<u8>, Error> {
    let profile = match stage {
        naga::ShaderStage::Vertex => s!("vs_5_0"),
        naga::ShaderStage::Fragment => s!("ps_5_0"),
        naga::ShaderStage::Compute => s!("cs_5_0"),
        _ => return Err(Error::Unsupported("shader stage")),
    };
    let mut binding_map = naga::back::hlsl::BindingMap::default();
    let mut slots = 0u32;
    for &(binding, slot) in uniforms {
        if slot >= 14 || slots & (1 << slot) != 0 || binding_map.contains_key(&binding) {
            return Err(Error::Unsupported("invalid or duplicate uniform binding"));
        }
        slots |= 1 << slot;
        binding_map.insert(
            binding,
            naga::back::hlsl::BindTarget {
                register: slot,
                ..Default::default()
            },
        );
    }
    for (_, global) in module.global_variables.iter() {
        if let Some(binding) = global.binding {
            if global.space != naga::AddressSpace::Uniform || !binding_map.contains_key(&binding) {
                return Err(Error::Unsupported(
                    "shader resource register layout is not implemented",
                ));
            }
        }
    }
    if uniforms.iter().any(|(binding, _)| {
        !module
            .global_variables
            .iter()
            .any(|(_, global)| global.binding == Some(*binding))
    }) {
        return Err(Error::Unsupported(
            "uniform binding does not exist in shader",
        ));
    }
    let options = naga::back::hlsl::Options {
        shader_model: naga::back::hlsl::ShaderModel::V5_0,
        fake_missing_bindings: false,
        binding_map,
        ..Default::default()
    };
    let pipeline = naga::back::hlsl::PipelineOptions {
        entry_point: Some((stage, entry.into())),
    };
    let mut source = String::new();
    let reflection = naga::back::hlsl::Writer::new(&mut source, &options, &pipeline)
        .write(module, info, None)
        .map_err(|error| Error::Shader(format!("{error}")))?;
    // PipelineOptions selects exactly one entry; reflection is filtered as well.
    let name = reflection
        .entry_point_names
        .first()
        .ok_or(Error::Unsupported("entry point reflection missing"))?
        .as_ref()
        .map_err(|error| Error::Shader(format!("{error:?}")))?;
    let name = std::ffi::CString::new(name.as_bytes())
        .map_err(|_| Error::Unsupported("invalid entry point name"))?;
    let (mut code, mut errors) = (None, None);
    unsafe {
        if let Err(error) = D3DCompile(
            source.as_ptr().cast(),
            source.len(),
            PCSTR::null(),
            None,
            None,
            PCSTR(name.as_ptr().cast()),
            profile,
            0,
            0,
            &mut code,
            Some(&mut errors),
        ) {
            let diagnostic = errors
                .as_ref()
                .map(|blob| {
                    let bytes = core::slice::from_raw_parts(
                        blob.GetBufferPointer().cast(),
                        blob.GetBufferSize(),
                    );
                    String::from_utf8_lossy(bytes).into_owned()
                })
                .unwrap_or_else(|| format!("{error}"));
            return Err(Error::Shader(diagnostic));
        }
        let code = code.ok_or(Error::Unsupported("FXC returned no bytecode"))?;
        Ok(
            core::slice::from_raw_parts(code.GetBufferPointer().cast(), code.GetBufferSize())
                .to_vec(),
        )
    }
}

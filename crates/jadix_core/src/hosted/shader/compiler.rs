//! CPU-only WGSL validation and reflection. No native view or GPU ownership lives here.
use super::super::{HostedContent, ShaderLanguage, ShaderUniform};
use serde::Serialize;
mod glsl_source;
use std::collections::BTreeMap;
use wgpu::naga::{self, back::glsl, valid, TypeInner};

#[derive(Debug, Serialize)]
pub struct ShaderBlock {
    pub name: String,
    pub builtin: bool,
    pub size: u32,
    pub offsets: BTreeMap<String, u32>,
}
#[derive(Debug, Serialize)]
pub struct CompiledShader {
    pub source: String,
    pub blocks: Vec<ShaderBlock>,
    /// Authored image name to the actual GLES sampler name.
    pub textures: BTreeMap<String, String>,
}

/// Values supplied by a native frame, in logical and physical pixels respectively.
pub struct ShaderFrame {
    pub size: [f32; 2], pub pixels: [f32; 2], pub time: f32,
    pub scale: f32, pub progress: f32, pub overflow: f32,
}
/// Pack reflected std140 inputs once for every GLES adapter. Never trust offsets
/// supplied across a bridge: an out-of-bounds reflection is an explicit error.
pub fn pack_shader_block(block: &ShaderBlock, parameters: &BTreeMap<String, ShaderUniform>, frame: &ShaderFrame) -> Result<Vec<u8>, String> {
    if block.size > 16384 { return Err("shader block exceeds portable budget".into()); }
    let mut bytes = vec![0; block.size as usize];
    for (name, offset) in &block.offsets {
        let words: Vec<[u8; 4]> = if block.builtin {
            let values: &[f32] = match name.as_str() {
                "size" => &frame.size, "pixels" => &frame.pixels, "time" => std::slice::from_ref(&frame.time),
                "scale" => std::slice::from_ref(&frame.scale), "progress" => std::slice::from_ref(&frame.progress),
                "overflow" => std::slice::from_ref(&frame.overflow), _ => return Err("unknown shader frame field".into()),
            }; values.iter().map(|value| value.to_ne_bytes()).collect()
        } else {
            match parameters.get(name).ok_or("missing shader parameter")? {
                ShaderUniform::Int(value) => vec![value.to_ne_bytes()], ShaderUniform::Uint(value) => vec![value.to_ne_bytes()],
                ShaderUniform::Float(value) => vec![value.to_ne_bytes()],
                ShaderUniform::Vec2(values) => values.iter().map(|v| v.to_ne_bytes()).collect(),
                ShaderUniform::Vec3(values) => values.iter().map(|v| v.to_ne_bytes()).collect(),
                ShaderUniform::Vec4(values) => values.iter().map(|v| v.to_ne_bytes()).collect(),
                ShaderUniform::Mat4(values) => values.iter().map(|v| v.to_ne_bytes()).collect(),
                ShaderUniform::Vec4Array(values) => values.iter().flatten().map(|v| v.to_ne_bytes()).collect(),
            }
        };
        let start = *offset as usize;
        let end = start.checked_add(words.len() * 4).ok_or("shader offset overflow")?;
        let output = bytes.get_mut(start..end).ok_or("shader offset outside input block")?;
        for (chunk, word) in output.chunks_exact_mut(4).zip(words) { chunk.copy_from_slice(&word); }
    }
    Ok(bytes)
}
fn parameter_type(value: &ShaderUniform) -> String {
    match value {
        ShaderUniform::Float(_) => "f32".into(), ShaderUniform::Int(_) => "i32".into(),
        ShaderUniform::Uint(_) => "u32".into(), ShaderUniform::Vec2(_) => "vec2<f32>".into(),
        ShaderUniform::Vec3(_) => "vec3<f32>".into(), ShaderUniform::Vec4(_) => "vec4<f32>".into(),
        ShaderUniform::Mat4(_) => "mat4x4<f32>".into(),
        ShaderUniform::Vec4Array(values) => format!("array<vec4<f32>, {}>", values.len()),
    }
}
/// Generate and validate portable bindings before any graphics API sees the source.
/// Direct3D/WebGPU use top-left fragment coordinates; GLES flips the viewport Y.
fn validated_shader(specification: &HostedContent, flip_y: bool) -> Result<(String, naga::Module, valid::ModuleInfo), String> {
    specification.validate().map_err(str::to_owned)?;
    let HostedContent::Shader { source, function, language, parameters, textures, composition, .. } = specification
        else { return Err("expected a shader".into()) };
    if *language == ShaderLanguage::Glsl { return glsl_source::validated(specification, flip_y); }
    let mut text = String::from("struct JadixFrame { size: vec2<f32>, time: f32, scale: f32, progress: f32, overflow: f32, pixels: vec2<f32> };\n@group(0) @binding(0) var<uniform> jadix: JadixFrame;\n");
    if !parameters.is_empty() {
        text.push_str("struct JadixParameters {\n");
        // Sixteen-byte alignment makes the WGSL layout identical to GLES std140, including scalars.
        for (name, value) in parameters { text.push_str(&format!("@align(16) {name}: {},\n", parameter_type(value))); }
        text.push_str("};\n@group(0) @binding(1) var<uniform> uniforms: JadixParameters;\n");
    }
    let texture_names: Vec<&str> = textures.keys().map(String::as_str).chain(composition.inputs().iter().copied()).collect();
    for (index, name) in texture_names.iter().enumerate() {
        let coordinates = if composition.inputs().contains(name) {
            "(uv * jadix.size + vec2<f32>(jadix.overflow)) / (jadix.size + vec2<f32>(2.0 * jadix.overflow))"
        } else { "uv" };
        text.push_str(&format!("@group(1) @binding({}) var jadix_texture_{name}: texture_2d<f32>;\n@group(1) @binding({}) var jadix_sampler_{name}: sampler;\nfn sample_{name}(uv: vec2<f32>) -> vec4<f32> {{ let c = textureSample(jadix_texture_{name}, jadix_sampler_{name}, {coordinates}); return vec4<f32>(select(vec3<f32>(0.0), c.rgb / max(c.a, 0.000001), c.a > 0.0), c.a); }}\n", index * 2, index * 2 + 1));
    }
    let prelude_lines = text.lines().count();
    text.push_str(source);
    let y = if flip_y { "1.0 - p.y / jadix.pixels.y" } else { "p.y / jadix.pixels.y" };
    text.push_str(&format!("\n@fragment fn jadix_fragment(@builtin(position) p: vec4<f32>) -> @location(0) vec4<f32> {{ let uv = vec2<f32>(p.x / jadix.pixels.x, {y}); let c = {function}(uv); return vec4<f32>(c.rgb * c.a, c.a); }}"));
    let module = naga::front::wgsl::parse_str(&text).map_err(|error|
        format!("{}\nGenerated prelude: {prelude_lines} lines; subtract it for authored source positions.", error.emit_to_string(&text)))?;
    if module.entry_points.len() != 1 { return Err("shader must define shade, not additional entry points".into()); }
    let mut uniform_count = 0; let mut image_count = 0; let mut sampler_count = 0;
    for (_, global) in module.global_variables.iter() {
        match global.space {
            naga::AddressSpace::Uniform => { uniform_count += 1; if !matches!(global.name.as_deref(), Some("jadix" | "uniforms")) { return Err("custom resource bindings are not allowed".into()); } },
            naga::AddressSpace::Handle => match module.types[global.ty].inner {
                TypeInner::Image { .. } => image_count += 1, TypeInner::Sampler { .. } => sampler_count += 1,
                _ => return Err("unsupported shader resource".into()),
            },
            naga::AddressSpace::Private => {},
            _ => return Err("storage, workgroup and immediate resources are not allowed".into()),
        }
    }
    if uniform_count != 1 + usize::from(!parameters.is_empty()) || image_count != texture_names.len() || sampler_count != texture_names.len() {
        return Err("shader declares resources outside its portable inputs".into());
    }
    let info = valid::Validator::new(valid::ValidationFlags::all(), valid::Capabilities::empty())
        .validate(&module).map_err(|error| error.emit_to_string(&text))?;
    Ok((text, module, info))
}

fn uniform_block(module: &naga::Module, global: &naga::GlobalVariable, name: String) -> Result<ShaderBlock, String> {
    let TypeInner::Struct { members, span } = &module.types[global.ty].inner else { return Err("invalid input block".into()) };
    let size = (*span + 15) & !15;
    if size > 16384 { return Err("shader input block exceeds portable 16 KiB budget".into()); }
    Ok(ShaderBlock { name, builtin: global.name.as_deref() == Some("jadix"), size,
        offsets: members.iter().filter_map(|member| Some((member.name.clone()?, member.offset))).collect() })
}

/// Validated WGSL and reflected uniform blocks for native WebGPU/Direct3D adapters.
/// Blocks retain `jadix`/`uniforms` names and texture names retain authored identity.
pub fn compile_shader_wgsl(specification: &HostedContent) -> Result<CompiledShader, String> {
    let (text, module, info) = validated_shader(specification, false)?;
    let source = if matches!(specification, HostedContent::Shader { language: ShaderLanguage::Glsl, .. }) {
        naga::back::wgsl::write_string(&module, &info, naga::back::wgsl::WriterFlags::EXPLICIT_TYPES).map_err(|error| error.to_string())?
    } else { text };
    let mut blocks = Vec::new();
    let mut textures = BTreeMap::new();
    for (_, global) in module.global_variables.iter() {
        if global.space == naga::AddressSpace::Uniform {
            blocks.push(uniform_block(&module, global, global.name.clone().ok_or("unnamed input block")?)?);
        } else if let Some(authored) = global.name.as_deref().and_then(|name| name.strip_prefix("jadix_texture_")) {
            textures.insert(authored.to_owned(), global.name.clone().unwrap());
        }
    }
    Ok(CompiledShader { source, blocks, textures })
}

/// Generated bindings are the only permitted resources; validation precedes GLES emission.
pub fn compile_shader_gles(specification: &HostedContent) -> Result<CompiledShader, String> {
    let (_, module, info) = validated_shader(specification, true)?;
    let options = glsl::Options { version: glsl::Version::new_gles(300), writer_flags: glsl::WriterFlags::empty(), ..Default::default() };
    let pipeline = glsl::PipelineOptions { shader_stage: naga::ShaderStage::Fragment, entry_point: "jadix_fragment".into(), multiview: None };
    let mut output = String::new();
    let reflection = glsl::Writer::new(&mut output, &module, &info, &options, &pipeline, Default::default())
        .map_err(|e| e.to_string())?.write().map_err(|e| e.to_string())?;
    let mut blocks = Vec::new();
    for (handle, global) in module.global_variables.iter() {
        if global.space != naga::AddressSpace::Uniform { continue; }
        if let Some(name) = reflection.uniforms.get(&handle) {
            blocks.push(uniform_block(&module, global, name.clone())?);
        }
    }
    let mut images = BTreeMap::new();
    for (gl_name, mapping) in reflection.texture_mapping {
        let authored = module.global_variables[mapping.texture].name.as_deref().and_then(|n| n.strip_prefix("jadix_texture_"))
            .ok_or("unrecognized texture binding")?;
        images.insert(authored.into(), gl_name);
    }
    Ok(CompiledShader { source: output, blocks, textures: images })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reflected_packing_preserves_integer_bits_arrays_and_std140_padding() {
        let frame = ShaderFrame { size: [10.,20.], pixels: [20.,40.], time: 1.5, scale: 2., progress: 0.25, overflow: 8. };
        let parameters = BTreeMap::from([
            ("signed".into(), ShaderUniform::Int(-23)), ("unsigned".into(), ShaderUniform::Uint(u32::MAX)),
            ("vectors".into(), ShaderUniform::Vec4Array(vec![[1.,2.,3.,4.], [5.,6.,7.,8.]])),
        ]);
        let mut block = ShaderBlock { name: "test".into(), builtin: false, size: 64,
            offsets: BTreeMap::from([("signed".into(),0), ("unsigned".into(),16), ("vectors".into(),32)]) };
        let bytes = pack_shader_block(&block, &parameters, &frame).unwrap();
        assert_eq!(&bytes[..4], &(-23i32).to_ne_bytes());
        assert_eq!(&bytes[4..16], &[0;12]);
        assert_eq!(&bytes[16..20], &u32::MAX.to_ne_bytes());
        assert_eq!(&bytes[60..64], &8f32.to_ne_bytes());
        block.offsets.insert("vectors".into(), 48);
        assert!(pack_shader_block(&block, &parameters, &frame).is_err());
        block.builtin = true; block.size = 32;
        block.offsets = BTreeMap::from([("size".into(),0), ("time".into(),8), ("pixels".into(),24)]);
        let bytes = pack_shader_block(&block, &parameters, &frame).unwrap();
        assert_eq!(&bytes[8..12], &1.5f32.to_ne_bytes());
        assert_eq!(&bytes[24..28], &20f32.to_ne_bytes());
    }
    fn specification(source: &str) -> HostedContent {
        serde_json::from_value(serde_json::json!({"type":"shader","language":"wgsl","source":source,
            "function":"shade","uniforms":[],"frames_per_second":60,
            "parameters":{"amount":{"type":"float","value":0.5},"color":{"type":"vec4","value":[1,0,0,1]}},
            "textures":{"image":"https://example.com/image.png"}})).unwrap()
    }
    #[test]
    fn portable_wgsl_compiles_with_reflected_inputs_and_top_left_coordinates() {
        let compiled = compile_shader_gles(&specification("fn shade(uv: vec2<f32>) -> vec4<f32> { return sample_image(uv) * uniforms.color * uniforms.amount + vec4<f32>(jadix.time * 0.01); }")).unwrap();
        assert!(compiled.source.starts_with("#version 300 es"));
        assert!(compiled.source.contains("1.0 -"));
        assert_eq!(compiled.blocks.len(), 2);
        let parameters = compiled.blocks.iter().find(|b| !b.builtin).unwrap();
        assert_eq!(parameters.offsets["amount"], 0);
        assert_eq!(parameters.offsets["color"], 16);
        assert_eq!(parameters.size, 32);
        assert!(!compiled.textures["image"].is_empty());
    }
    #[test]
    fn wgsl_and_gles_share_parameter_layout_and_texture_identity() {
        let spec = specification("fn shade(uv: vec2<f32>) -> vec4<f32> { return sample_image(uv) * uniforms.color * uniforms.amount; }");
        let native = compile_shader_wgsl(&spec).unwrap();
        let gles = compile_shader_gles(&spec).unwrap();
        assert!(native.source.contains("p.y / jadix.pixels.y"));
        assert!(!native.source.contains("1.0 - p.y"));
        assert_eq!(native.textures["image"], "jadix_texture_image");
        for block in &native.blocks {
            let other = gles.blocks.iter().find(|other| other.builtin == block.builtin).unwrap();
            assert_eq!(block.size, other.size); assert_eq!(block.offsets, other.offsets);
        }
        assert!(compile_shader_wgsl(&specification("@compute @workgroup_size(1) fn extra() {} fn shade(uv: vec2<f32>) -> vec4<f32> { return vec4<f32>(1.); }")).is_err());
    }
    #[test]
    fn invalid_source_and_unowned_resources_fail_before_gpu_submission() {
        assert!(compile_shader_gles(&specification("fn shade(" )).unwrap_err().contains("Generated prelude"));
        assert!(compile_shader_gles(&specification("@group(2) @binding(0) var<storage, read> illegal: array<f32>; fn shade(uv: vec2<f32>) -> vec4<f32> { return vec4<f32>(illegal[0]); }")).is_err());
    }
    #[test]
    fn composition_inputs_share_texture_limits_and_preserve_padded_coordinates() {
        let mut spec = specification("fn shade(uv: vec2<f32>) -> vec4<f32> { return mix(sample_from(uv), sample_to(uv), jadix.progress) + sample_backdrop(uv) * 0.0; }");
        let HostedContent::Shader { composition, max_sample_offset, .. } = &mut spec else { panic!() };
        *composition = crate::hosted::ShaderComposition::Transition {
            value: "next".into(), duration_ms: 300, fallback: Default::default(),
        };
        *max_sample_offset = 12.;
        let compiled = compile_shader_gles(&spec).unwrap();
        for input in ["from", "to", "backdrop"] { assert!(compiled.textures.contains_key(input)); }
        let frame = compiled.blocks.iter().find(|b| b.builtin).unwrap();
        assert!(frame.offsets.contains_key("progress"));
        assert!(frame.offsets.contains_key("overflow"));
        let HostedContent::Shader { textures, .. } = &mut spec else { panic!() };
        textures.insert("content".into(), "https://example.com/image.png".into());
        assert!(compile_shader_gles(&spec).is_err());
    }
}

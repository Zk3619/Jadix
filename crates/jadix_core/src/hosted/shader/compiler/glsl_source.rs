//! GLSL function shaders use the same reflected inputs as WGSL, without a GPU.
use super::*;

pub(super) fn validated(specification: &HostedContent, flip_y: bool) -> Result<(String, naga::Module, valid::ModuleInfo), String> {
    let HostedContent::Shader { source, function, parameters, textures, composition, uniforms, .. } = specification else { unreachable!() };
    let mut text = String::from("#version 450\nlayout(set=0,binding=0,std140) uniform JadixFrame { vec2 size; float time; float scale; float progress; float overflow; vec2 pixels; } jadix;\n#define uResolution jadix.pixels\n#define uTime jadix.time\n#define uScale jadix.scale\n#define uProgress jadix.progress\n#define uOverflow jadix.overflow\nlayout(location=0) out vec4 jadix_color;\n");
    if !parameters.is_empty() {
        text.push_str("layout(set=0,binding=1,std140) uniform JadixParameters {\n");
        for (name, value) in parameters {
            let (kind, suffix) = match value {
                ShaderUniform::Float(_) => ("float", String::new()),
                ShaderUniform::Int(_) => ("int", String::new()),
                ShaderUniform::Uint(_) => ("uint", String::new()),
                ShaderUniform::Vec2(_) => ("vec2", String::new()),
                ShaderUniform::Vec3(_) => ("vec3", String::new()),
                ShaderUniform::Vec4(_) => ("vec4", String::new()),
                ShaderUniform::Mat4(_) => ("mat4", String::new()),
                ShaderUniform::Vec4Array(values) => ("vec4", format!("[{}]", values.len())),
            };
            text.push_str(&format!("{kind} {name}{suffix};\n"));
        }
        text.push_str("};\n");
    }
    // Legacy values are specialization constants. Updating them invalidates the
    // compiled source; typed parameters above remain dynamic uniform buffers.
    text.push_str("const float uValues[256] = float[256](");
    for index in 0..256 {
        if index != 0 { text.push(','); }
        text.push_str(&format!("{:?}", uniforms.get(index).copied().unwrap_or(0.)));
    }
    text.push_str(");\n");
    let names: Vec<_> = textures.keys().map(String::as_str).chain(composition.inputs().iter().copied()).collect();
    for (index, name) in names.iter().enumerate() {
        text.push_str(&format!("layout(set=1,binding={}) uniform texture2D jadix_texture_{name};\nlayout(set=1,binding={}) uniform sampler jadix_sampler_{name};\n",index*2,index*2+1));
        let coordinates = if composition.inputs().contains(name) { "(uv*jadix.size+vec2(jadix.overflow))/(jadix.size+vec2(2.0*jadix.overflow))" } else { "uv" };
        text.push_str(&format!("vec4 sample_{name}(vec2 uv) {{ vec4 c=texture(sampler2D(jadix_texture_{name},jadix_sampler_{name}),{coordinates}); return vec4(c.a>0.0?c.rgb/max(c.a,0.000001):vec3(0.0),c.a); }}\n"));
    }
    for name in &names { text.push_str(&format!("#define {name} sampler2D(jadix_texture_{name},jadix_sampler_{name})\n")); }
    let prelude_lines = text.lines().count();
    text.push_str(source);
    let coordinates = if flip_y { "gl_FragCoord.xy" } else { "vec2(gl_FragCoord.x,uResolution.y-gl_FragCoord.y)" };
    text.push_str(&format!("\nvoid main() {{ vec4 c={function}({coordinates}); jadix_color=vec4(c.rgb*c.a,c.a); }}"));
    let mut module = naga::front::glsl::Frontend::default().parse(&naga::front::glsl::Options::from(naga::ShaderStage::Fragment), &text)
        .map_err(|error| format!("{}\nGenerated prelude: {prelude_lines} lines; subtract it for authored source positions.",error.emit_to_string(&text)))?;
    if module.entry_points.len() != 1 { return Err("shader must declare exactly one fragment entry point".into()); }
    module.entry_points[0].name = "jadix_fragment".into();
    let mut uniform_count=0; let mut images=0; let mut samplers=0;
    for (_, global) in module.global_variables.iter_mut() {
        match global.space {
            naga::AddressSpace::Uniform => {
                let binding=global.binding.as_ref().ok_or("unbound shader resource")?;
                if binding.group != 0 || binding.binding > 1 { return Err("custom resource bindings are not allowed".into()); }
                global.name=Some(if binding.binding==0 {"jadix"} else {"uniforms"}.into()); uniform_count+=1;
            }
            naga::AddressSpace::Handle => match module.types[global.ty].inner { TypeInner::Image { .. } => images+=1, TypeInner::Sampler { .. } => samplers+=1, _ => return Err("unsupported shader resource".into()) },
            naga::AddressSpace::Private => {},
            _ => return Err("storage, workgroup and immediate resources are not allowed".into()),
        }
    }
    if module.entry_points.len()!=1 || uniform_count!=1+usize::from(!parameters.is_empty()) || images!=names.len() || samplers!=names.len() { return Err("shader declares resources outside its portable inputs".into()); }
    let info=valid::Validator::new(valid::ValidationFlags::all(),valid::Capabilities::empty()).validate(&module).map_err(|error|error.emit_to_string(&text))?;
    Ok((text,module,info))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn glsl_inputs_translate_to_valid_wgsl_and_gles_with_matching_layout() {
        let spec: HostedContent=serde_json::from_value(serde_json::json!({"type":"shader","source":"vec4 shade(vec2 p) { return sample_image(p / uResolution) * tint * (uValues[0] + uTime * 0.0); }","function":"shade","uniforms":[0.75],"frames_per_second":60,"parameters":{"tint":{"type":"vec4","value":[1,0,0,1]}},"textures":{"image":"https://example.com/image.png"}})).unwrap();
        let native=compile_shader_wgsl(&spec).unwrap();
        let module=naga::front::wgsl::parse_str(&native.source).unwrap();
        valid::Validator::new(valid::ValidationFlags::all(),valid::Capabilities::empty()).validate(&module).unwrap();
        let gles=compile_shader_gles(&spec).unwrap();
        assert_eq!(native.blocks.len(),2);
        for block in &native.blocks { let other=gles.blocks.iter().find(|other|other.builtin==block.builtin).unwrap(); assert_eq!(block.size,other.size); assert_eq!(block.offsets,other.offsets); }
        assert!(native.source.contains("jadix_fragment"));
        assert!(native.textures.contains_key("image"));
        let mut invalid=spec;
        if let HostedContent::Shader { source, .. }=&mut invalid { *source="layout(set=2,binding=0) buffer Bad { float bad[]; }; vec4 shade(vec2 p) { return vec4(bad[0]); }".into(); }
        assert!(compile_shader_wgsl(&invalid).is_err());
    }
}

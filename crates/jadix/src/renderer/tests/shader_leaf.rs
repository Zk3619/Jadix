use super::*;

#[test]
fn uniforms_match_wgsl_layout() {
    assert_eq!(std::mem::size_of::<ShaderUniforms>(), 96);
    assert_eq!(std::mem::offset_of!(ShaderUniforms, resolution), 8);
    assert_eq!(std::mem::offset_of!(ShaderUniforms, pointer), 16);
    assert_eq!(std::mem::offset_of!(ShaderUniforms, custom), 32);
}

#[test]
fn compose_injects_vs_and_uniforms() {
    let src = compose_user_wgsl(
        r#"
            @fragment
            fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
                return vec4<f32>(in.uv, u.time, 1.0);
            }
        "#,
    );
    assert!(src.contains("fn vs_main"));
    assert!(src.contains("var<uniform> u"));
    assert!(src.contains("var t_src"));
    assert!(src.contains("var t_src1"));
    assert!(src.contains("var t_src2"));
    assert!(src.contains("fn fs_main"));
}

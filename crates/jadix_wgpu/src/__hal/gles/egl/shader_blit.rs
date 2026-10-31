//! ANGLE D3D11's default-framebuffer blit decodes sRGB without re-encoding.
//! Present encoded bytes to an explicitly linear EGL window surface.
// ponytail: one full-screen sRGB re-encode per present; remove when ANGLE's
// native sRGB window writes can preserve the same color and orientation oracle.
use glow::HasContext;

#[derive(Debug)]
pub(super) struct ShaderBlit {
    pub texture: glow::Texture,
    program: glow::Program,
    vao: glow::VertexArray,
}

impl ShaderBlit {
    pub unsafe fn new(
        gl: &glow::Context,
        width: i32,
        height: i32,
    ) -> Result<Self, crate::__hal::SurfaceError> {
        unsafe {
            let program = gl
                .create_program()
                .map_err(|_| crate::__hal::DeviceError::OutOfMemory)?;
            let mut shaders = alloc::vec::Vec::new();
            let result = (|| {
                for (stage, source) in [
                    (glow::VERTEX_SHADER, "#version 300 es\nvoid main() { vec2 p = vec2((gl_VertexID << 1) & 2, gl_VertexID & 2); gl_Position = vec4(p * 2.0 - 1.0, 0.0, 1.0); }"),
                    (glow::FRAGMENT_SHADER, "#version 300 es\nprecision highp float; uniform highp sampler2D image; out vec4 color; void main() { ivec2 size = textureSize(image, 0); vec4 c = texelFetch(image, ivec2(int(gl_FragCoord.x), size.y - 1 - int(gl_FragCoord.y)), 0); vec3 encoded = mix(c.rgb * 12.92, 1.055 * pow(c.rgb, vec3(1.0 / 2.4)) - 0.055, step(vec3(0.0031308), c.rgb)); color = vec4(encoded, c.a); }"),
                ] {
                    let shader = gl.create_shader(stage).map_err(|_| crate::__hal::DeviceError::OutOfMemory)?;
                    shaders.push(shader);
                    gl.shader_source(shader, source);
                    gl.compile_shader(shader);
                    if !gl.get_shader_compile_status(shader) {
                        log::error!("ANGLE presentation shader: {}", gl.get_shader_info_log(shader));
                        return Err(crate::__hal::SurfaceError::Other("ANGLE presentation shader compilation"));
                    }
                    gl.attach_shader(program, shader);
                }
                gl.link_program(program);
                if !gl.get_program_link_status(program) {
                    log::error!(
                        "ANGLE presentation program: {}",
                        gl.get_program_info_log(program)
                    );
                    return Err(crate::__hal::SurfaceError::Other(
                        "ANGLE presentation shader linking",
                    ));
                }
                Ok(())
            })();
            for shader in shaders {
                gl.delete_shader(shader);
            }
            if let Err(error) = result {
                gl.delete_program(program);
                return Err(error);
            }
            let texture = match gl.create_texture() {
                Ok(value) => value,
                Err(_) => {
                    gl.delete_program(program);
                    return Err(crate::__hal::DeviceError::OutOfMemory.into());
                }
            };
            let vao = match gl.create_vertex_array() {
                Ok(value) => value,
                Err(_) => {
                    gl.delete_texture(texture);
                    gl.delete_program(program);
                    return Err(crate::__hal::DeviceError::OutOfMemory.into());
                }
            };
            let previous_texture = gl.get_parameter_texture(glow::TEXTURE_BINDING_2D);
            let previous_program = gl.get_parameter_program(glow::CURRENT_PROGRAM);
            gl.bind_texture(glow::TEXTURE_2D, Some(texture));
            gl.tex_storage_2d(glow::TEXTURE_2D, 1, glow::SRGB8_ALPHA8, width, height);
            gl.bind_texture(glow::TEXTURE_2D, previous_texture);
            gl.use_program(Some(program));
            gl.uniform_1_i32(gl.get_uniform_location(program, "image").as_ref(), 0);
            gl.use_program(previous_program);
            Ok(Self {
                texture,
                program,
                vao,
            })
        }
    }

    pub unsafe fn draw(&self, gl: &glow::Context, width: i32, height: i32) {
        unsafe {
            let previous_vao = gl.get_parameter_vertex_array(glow::VERTEX_ARRAY_BINDING);
            let previous_program = gl.get_parameter_program(glow::CURRENT_PROGRAM);
            let previous_unit = gl.get_parameter_i32(glow::ACTIVE_TEXTURE) as u32;
            gl.active_texture(glow::TEXTURE0);
            let previous_texture = gl.get_parameter_texture(glow::TEXTURE_BINDING_2D);
            let previous_sampler = gl.get_parameter_sampler(glow::SAMPLER_BINDING);
            for state in [
                glow::BLEND,
                glow::DEPTH_TEST,
                glow::STENCIL_TEST,
                glow::CULL_FACE,
                glow::SCISSOR_TEST,
                glow::RASTERIZER_DISCARD,
            ] {
                gl.disable(state);
            }
            gl.viewport(0, 0, width, height);
            gl.use_program(Some(self.program));
            gl.bind_vertex_array(Some(self.vao));
            gl.active_texture(glow::TEXTURE0);
            gl.bind_sampler(0, None);
            gl.bind_texture(glow::TEXTURE_2D, Some(self.texture));
            gl.draw_arrays(glow::TRIANGLES, 0, 3);
            gl.bind_texture(glow::TEXTURE_2D, previous_texture);
            gl.bind_sampler(0, previous_sampler);
            gl.active_texture(previous_unit);
            gl.bind_vertex_array(previous_vao);
            gl.use_program(previous_program);
        }
    }

    pub unsafe fn destroy(self, gl: &glow::Context) {
        unsafe {
            gl.delete_texture(self.texture);
            gl.delete_program(self.program);
            gl.delete_vertex_array(self.vao);
        }
    }
}

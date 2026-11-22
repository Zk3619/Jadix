//! Actual GLES readback, without GTK initialization, a window or DISPLAY.
use super::*;
type Handle = *mut c_void;
#[link(name = "EGL")]
unsafe extern "C" {
    fn eglGetPlatformDisplay(platform: u32, display: Handle, attributes: *const isize) -> Handle;
    fn eglInitialize(display: Handle, major: *mut i32, minor: *mut i32) -> u32;
    fn eglBindAPI(api: u32) -> u32;
    fn eglChooseConfig(
        display: Handle,
        attributes: *const i32,
        configs: *mut Handle,
        count: i32,
        found: *mut i32,
    ) -> u32;
    fn eglCreateContext(
        display: Handle,
        config: Handle,
        shared: Handle,
        attributes: *const i32,
    ) -> Handle;
    fn eglCreatePbufferSurface(display: Handle, config: Handle, attributes: *const i32) -> Handle;
    fn eglMakeCurrent(display: Handle, draw: Handle, read: Handle, context: Handle) -> u32;
    fn eglDestroySurface(display: Handle, surface: Handle) -> u32;
    fn eglDestroyContext(display: Handle, context: Handle) -> u32;
    fn eglTerminate(display: Handle) -> u32;
    fn eglGetError() -> u32;
}
struct Offscreen {
    display: Handle,
    context: Handle,
    surface: Handle,
}
impl Offscreen {
    fn new() -> Self {
        let null = std::ptr::null_mut();
        let mut this = Self {
            display: null,
            context: null,
            surface: null,
        };
        unsafe {
            this.display = eglGetPlatformDisplay(0x31DD, null, std::ptr::null()); // Mesa surfaceless
            assert!(
                !this.display.is_null(),
                "surfaceless EGL unavailable: {:x}",
                eglGetError()
            );
            assert_ne!(
                eglInitialize(this.display, std::ptr::null_mut(), std::ptr::null_mut()),
                0,
                "EGL initialization: {:x}",
                eglGetError()
            );
            assert_ne!(eglBindAPI(0x30A0), 0); // OpenGL ES
            let attributes = [
                0x3033, 1, 0x3040, 0x0040, 0x3024, 8, 0x3023, 8, 0x3022, 8, 0x3021, 8, 0x3038,
            ];
            let mut config = null;
            let mut found = 0;
            assert_ne!(
                eglChooseConfig(
                    this.display,
                    attributes.as_ptr(),
                    &mut config,
                    1,
                    &mut found
                ),
                0
            );
            assert_eq!(found, 1, "EGL has no RGBA GLES3 pbuffer configuration");
            this.context =
                eglCreateContext(this.display, config, null, [0x3098, 3, 0x3038].as_ptr());
            assert!(
                !this.context.is_null(),
                "EGL GLES3 context: {:x}",
                eglGetError()
            );
            this.surface = eglCreatePbufferSurface(
                this.display,
                config,
                [0x3057, 8, 0x3056, 8, 0x3038].as_ptr(),
            );
            assert!(!this.surface.is_null());
            assert_ne!(
                eglMakeCurrent(this.display, this.surface, this.surface, this.context),
                0
            );
        }
        this
    }
}
impl Drop for Offscreen {
    fn drop(&mut self) {
        unsafe {
            if !self.display.is_null() {
                eglMakeCurrent(
                    self.display,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                );
                if !self.surface.is_null() {
                    eglDestroySurface(self.display, self.surface);
                }
                if !self.context.is_null() {
                    eglDestroyContext(self.display, self.context);
                }
                eglTerminate(self.display);
            }
        }
    }
}
fn specification(language: &str, source: &str) -> HostedContent {
    HostedContent::from_json(&serde_json::json!({"type":"shader","language":language,"source":source,"function":"shade","uniforms":[],"frames_per_second":60}).to_string()).unwrap()
}
fn pixels(
    spec: &HostedContent,
    frame: &ShaderFrame,
    images: &BTreeMap<String, Rc<Image>>,
) -> Vec<u8> {
    let HostedContent::Shader {
        parameters,
        uniforms,
        ..
    } = spec
    else {
        panic!()
    };
    let mut gpu = unsafe { Gpu::new(spec) }.unwrap();
    unsafe { gpu.draw(parameters, uniforms, frame, images) }.unwrap();
    let mut pixels = vec![0; 8 * 8 * 4];
    unsafe {
        gpu.gl.read_pixels(
            0,
            0,
            8,
            8,
            glow::RGBA,
            glow::UNSIGNED_BYTE,
            glow::PixelPackData::Slice(Some(&mut pixels)),
        );
        assert_eq!(gpu.gl.get_error(), glow::NO_ERROR);
        gpu.destroy();
    }
    pixels
}
#[test]
fn surfaceless_gles_renders_wgsl_parameters_textures_coordinates_and_glsl() {
    let _context = Offscreen::new();
    let frame = ShaderFrame {
        size: [4., 4.],
        pixels: [8., 8.],
        scale: 2.,
        time: 3.,
        progress: 0.25,
        overflow: 0.,
    };
    let mut spec = specification("wgsl", "fn shade(uv: vec2<f32>) -> vec4<f32> { return sample_image(uv) * uniforms.color * uniforms.amount; }");
    let HostedContent::Shader {
        parameters,
        textures,
        ..
    } = &mut spec
    else {
        panic!()
    };
    parameters.insert("amount".into(), ShaderUniform::Float(0.5));
    parameters.insert("color".into(), ShaderUniform::Vec4([1., 1., 1., 1.]));
    textures.insert("image".into(), "https://example.test/texture.png".into());
    let images = BTreeMap::from([(
        "image".into(),
        Rc::new(Image {
            width: 1,
            height: 1,
            pixels: vec![128, 0, 0, 128],
        }),
    )]);
    let result = pixels(&spec, &frame, &images);
    for pixel in result.chunks_exact(4) {
        assert!((pixel[0] as i32 - 32).abs() <= 1);
        assert_eq!(pixel[1], 0);
        assert!((pixel[3] as i32 - 64).abs() <= 1);
    }
    let gradient = pixels(
        &specification(
            "wgsl",
            "fn shade(uv: vec2<f32>) -> vec4<f32> { return vec4<f32>(uv, jadix.progress, 1.0); }",
        ),
        &frame,
        &BTreeMap::new(),
    );
    assert!(
        gradient[1] > gradient[7 * 8 * 4 + 1],
        "WGSL coordinates must use the top-left origin"
    );
    assert!((gradient[2] as i32 - 64).abs() <= 1);
    let glsl = pixels(
        &specification(
            "glsl",
            "vec4 shade(vec2 p) { return vec4(uTime/3.0, uScale/2.0, uProgress, 0.5); }",
        ),
        &frame,
        &BTreeMap::new(),
    );
    assert!((glsl[0] as i32 - 128).abs() <= 1);
    assert!((glsl[1] as i32 - 128).abs() <= 1);
    assert!((glsl[2] as i32 - 32).abs() <= 1);
    assert!(unsafe {
        Gpu::new(&specification(
            "glsl",
            "vec4 shade(vec2 p) { this is invalid }",
        ))
    }
    .is_err());
    // A failed program must release its partial objects without poisoning the context.
    assert_eq!(
        pixels(
            &specification("glsl", "vec4 shade(vec2 p) { return vec4(1.0); }"),
            &frame,
            &BTreeMap::new()
        )[0],
        255
    );
}

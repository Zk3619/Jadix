//! WP-3.12：`<canvas>` / `canvas()` 双书写面可编译。

#[cfg(feature = "macros")]
use jadix::prelude::*;

#[cfg(feature = "macros")]
#[test]
fn canvas_tag_and_builder_compile() {
    let _b: Element = canvas()
        .w(80.0)
        .h(40.0)
        .paint(|ctx, size| {
            ctx.clear("secondary");
            ctx.set_fill("chart-1");
            ctx.fill_rect(4.0, 4.0, size.w - 8.0, size.h - 8.0);
            ctx.set_stroke("foreground");
            ctx.set_line_width(2.0);
            ctx.stroke_rect(4.0, 4.0, size.w - 8.0, size.h - 8.0);
            LayoutStatus::Clean
        })
        .into();
    let el = ui! {
        <canvas w={80.0} h={40.0} paint={move |ctx: &mut dyn Canvas2D, size: CanvasSize| {
            ctx.set_fill("chart-1");
            ctx.fill_rect(4.0, 4.0, size.w - 8.0, size.h - 8.0);
            LayoutStatus::Clean
        }} />
    };
    let _: Element = el;
}

#[cfg(feature = "macros")]
#[test]
fn scene3d_tag_and_builder_compile() {
    let _b: Element = scene3d().w(80.0).h(60.0).into();
    let el = ui! {
        <scene3d w={80.0} h={60.0} />
    };
    let _: Element = el;
}

#[cfg(feature = "macros")]
#[test]
fn shader_tag_and_builder_compile() {
    const WGSL: &str = r#"
        @fragment
        fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
            return vec4<f32>(in.uv, 0.3, 1.0);
        }
    "#;
    let _b: Element = shader()
        .w(80.0)
        .h(60.0)
        .wgsl(WGSL)
        .texture("src", "jadix-gallery-stream")
        .into();
    let el = ui! {
        <shader w={80.0} h={60.0} wgsl={WGSL} texture={"jadix-gallery-stream"} />
    };
    assert_eq!(
        el.get_attr(jadix::primitives::shader::ATTR_SHADER_TEX)
            .map(|s| s.as_str()),
        Some("jadix-gallery-stream")
    );
    let _: Element = el;
}

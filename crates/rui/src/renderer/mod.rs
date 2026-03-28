//! GPU 与文本渲染：管线、实例数据、glyphon 封装及每帧文本准备。

pub mod pipeline;
pub mod prepare_text;
pub(crate) mod text_prepare_debug;
pub mod text;
pub mod text_hit;
pub mod textured_pipeline;

pub use pipeline::{Globals, InstanceInput, RuiPipeline, VertexInput};
pub use textured_pipeline::{TexturedInstanceInput, TexturedQuadPipeline};
pub use prepare_text::prepare_text_frame;
pub(crate) use prepare_text::caret_blink_phase;
pub use text::RuiTextRenderer;
pub use text_hit::{best_font_family, hit_test_input_text_index, measure_prefix_width};

/// 将逻辑空间实例缩放到物理像素（DPI）。
pub fn scale_instances(instances: &[InstanceInput], scale: f32) -> Vec<InstanceInput> {
    instances
        .iter()
        .map(|i| {
            let mut out = *i;
            out.rect_pos[0] *= scale;
            out.rect_pos[1] *= scale;
            out.rect_size[0] *= scale;
            out.rect_size[1] *= scale;
            out.radii = [
                out.radii[0] * scale,
                out.radii[1] * scale,
                out.radii[2] * scale,
                out.radii[3] * scale,
            ];
            out.border_width *= scale;
            out.shadow_blur *= scale;
            out
        })
        .collect()
}

/// 布局收集阶段产出的纹理绘制实例（逻辑像素坐标）。
#[derive(Clone, Debug)]
pub struct TexturedDraw {
    pub rect: [f32; 4],
    pub texture_key: String,
}

/// 缩放至物理像素后的纹理绘制矩形。
#[derive(Clone, Debug)]
pub struct ScaledTexturedDraw {
    pub rect: [f32; 4],
    pub texture_key: String,
}

pub fn scale_textured_draws(draws: &[TexturedDraw], scale: f32) -> Vec<ScaledTexturedDraw> {
    draws
        .iter()
        .map(|d| ScaledTexturedDraw {
            rect: [
                d.rect[0] * scale,
                d.rect[1] * scale,
                d.rect[2] * scale,
                d.rect[3] * scale,
            ],
            texture_key: d.texture_key.clone(),
        })
        .collect()
}

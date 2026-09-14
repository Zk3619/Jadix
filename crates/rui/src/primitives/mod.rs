//! 原子原语层：声明式 UI 的底座。
//!
//! - **[`r#box`] / [`boxed`] / [`hstack`] / [`vstack`] / [`gstack`]**：布局与视觉容器。
//! - **[`interactive`]**：点击、悬停、键盘与焦点挂载。
//! - **[`overlay`]**：全屏模态遮罩（色 / 透明度 / backdrop 模糊 / 点击关闭）。
//! - **[`text`] / [`icon`]**：纯文本与图标叶子。
//! - **[`path`]**：折线/多边形/扇区（WP-3.2 / 3.11 共用核；CPU 覆盖率光栅贴图，tessellate 回退）。
//! - **[`svg`]**：公开 `<svg>` 叶（场景图；描边/填充；子形状不占 taffy）。
//! - **[`canvas`]**：公开 `<canvas>` 叶（paint / 2D context MVP；同一 path 核）。
//! - **[`scene3d`]**：公开 `<scene3d>` 叶（WP-3.13 实验 feature；camera/light/material）。
//! - **[`shader`]**：公开 `<shader>` 叶（experimental；用户 WGSL fragment → 离屏贴图）。
//! - **[`effect_wrap`]**：B16 子树特效 wrap（离屏捕获 + shader pass）。
//! - **[`Portal`]**：树外挂载（Solid `<Portal>`）；定位 / Esc 栈仍在 [`crate::utils::popup`] /
//!   [`crate::app::portal_overlay`]。
//!
//! 拖拽、浮层几何、虚拟列表、内嵌文本等能力在 [`crate::utils`] / [`crate::app`]；
//! 布尔开关状态在 [`crate::utils::toggle_state`]（WP-3.7）；Tabs / Toggle 外观在 [`crate::components`]。

pub mod r#box;
pub mod canvas;
pub mod effect_wrap;
pub mod interactive;
pub mod overlay;
pub mod path;
pub mod portal;
pub mod scene3d;
pub mod shader;
pub mod svg;
pub mod svg_smil;
pub mod text;

pub use canvas::{canvas, Canvas2D, CanvasBuilder, CanvasSize, ATTR_CANVAS_PAINT};
pub use effect_wrap::{effect_wrap, EffectWrapBuilder};
pub use interactive::{interactive, InteractiveBuilder};
pub use overlay::{overlay, OverlayBuilder};
pub use path::{
    attach_path, attach_polyline, decode_path_d, draw_polyline_batch, draw_polyline_batch_stub,
    element_has_path_geometry, encode_path_d, encode_points, ensure_raster_texture, filled_circle,
    filled_ellipse, flatten_path, path_gpu_from_element_attrs, path_pointer_hit,
    path_quads_from_element_attrs, path_raster_from_element_attrs, path_target_leaf_size, pie_ring,
    pie_sector, polygon, polyline, rasterize_layers_rgba, rasterize_path_rgba, tessellate_path,
    tessellate_path_mesh, PathCommand, PathQuad, PathStyle, PathTri, Point2, ATTR_PATH_D,
    ATTR_PATH_FILL, ATTR_PATH_FIT_LEAF, ATTR_PATH_KIND, ATTR_PATH_MARKERS, ATTR_PATH_POINTS,
    ATTR_PATH_STROKE, ATTR_PATH_STROKE_WIDTH, ATTR_PATH_VIEWBOX,
};
pub use portal::{Portal, PortalProps};
pub use r#box::{boxed, gstack, hstack, r#box, vstack, BoxBuilder};
pub use scene3d::{
    scene3d, Scene3dBuilder, Scene3dCamera, Scene3dLight, Scene3dMaterial, ATTR_SCENE3D,
};
pub use shader::{
    shader, ShaderBuilder, ShaderUniform, ATTR_SHADER, ATTR_SHADER_TEX, ATTR_SHADER_TEX1,
};
pub use svg::{
    animate, animate_motion, animate_transform, circle, clip_path, defs, desc, ellipse, fe_blend,
    fe_color_matrix, fe_drop_shadow, fe_flood, fe_gaussian_blur, fe_merge, fe_merge_node,
    fe_offset, filter, g, image, line, linear_gradient, marker, mask, mpath, path as svg_path,
    pattern, polygon as svg_polygon, polyline as svg_polyline, radial_gradient, rect, set_el, stop,
    svg, svg_el, svg_text, svg_use, symbol, text_path, title, tspan, AnimateBuilder, CircleBuilder,
    ClipPathBuilder, DefsBuilder, EllipseBuilder, GroupBuilder, LineBuilder, LinearGradientBuilder,
    PathBuilder, PolygonBuilder, PolylineBuilder, RadialGradientBuilder, RectBuilder, SetBuilder,
    StopBuilder, SvgBuilder, SvgShapeBuilder, SymbolBuilder, UseBuilder, ATTR_SVG_HAS_SMIL,
    ATTR_SVG_KIND, ATTR_SVG_SCENE, ATTR_SVG_TEXT,
};
pub use text::{icon, text, TextBuilder};

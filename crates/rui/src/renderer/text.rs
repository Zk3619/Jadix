use glyphon::{
    FontSystem, SwashCache, TextArea,
    TextAtlas, TextRenderer, Viewport,
};
use wgpu::{Device, Queue, SurfaceConfiguration, RenderPass};

pub(crate) struct ShapedTextEntry {
    pub key_hash: u64,
    pub buffer: glyphon::Buffer,
    pub measured_text_w: f32,
}

pub struct RuiTextRenderer {
    pub font_system: FontSystem,
    pub swash_cache: SwashCache,
    pub text_atlas: TextAtlas,
    pub text_renderer: TextRenderer,
    pub viewport: Viewport,
    pub(crate) shaped_cache: Vec<ShapedTextEntry>,
    pub(crate) text_area_capacity_hint: usize,
}

impl RuiTextRenderer {
    pub fn new(device: &Device, queue: &Queue, config: &SurfaceConfiguration) -> Self {
        let mut font_system = FontSystem::new();
        font_system.db_mut().load_font_data(include_bytes!("../../assets/lucide.ttf").to_vec());

        let swash_cache = SwashCache::new();
        let cache = glyphon::Cache::new(device);
        let mut text_atlas = TextAtlas::new(device, queue, &cache, config.format);
        let text_renderer =
            TextRenderer::new(&mut text_atlas, device, wgpu::MultisampleState::default(), None);
        let viewport = Viewport::new(device, &cache);

        Self {
            font_system,
            swash_cache,
            text_atlas,
            text_renderer,
            viewport,
            shaped_cache: Vec::new(),
            text_area_capacity_hint: 0,
        }
    }

    pub fn prepare<'a>(
        &mut self,
        device: &Device,
        queue: &Queue,
        config: &SurfaceConfiguration,
        text_areas: impl Iterator<Item = TextArea<'a>>,
    ) {
        self.viewport.update(
            queue,
            glyphon::Resolution {
                width: config.width,
                height: config.height,
            },
        );

        self.text_renderer
            .prepare(
                device,
                queue,
                &mut self.font_system,
                &mut self.text_atlas,
                &self.viewport,
                text_areas,
                &mut self.swash_cache,
            )
            .unwrap();
    }

    pub fn render<'pass>(&'pass self, render_pass: &mut RenderPass<'pass>) {
        self.text_renderer.render(&self.text_atlas, &self.viewport, render_pass).unwrap();
    }
}

//! GTK owns the GL context and native child tree; the shared compiler owns WGSL.
use super::*;
use glow::HasContext;
use gtk::subclass::prelude::*;
use jadix_core::hosted::{
    compile_shader_gles, pack_shader_block, CompiledShader, HostedContent, ShaderComposition,
    ShaderFrame, ShaderLanguage, ShaderUniform,
};
use jadix_net::service::{FetchEvent, FetchHandle, FetchRequest};
use std::{
    ffi::{c_char, c_void, CString},
    time::Instant,
};
#[cfg(test)]
mod tests;

#[link(name = "EGL")]
unsafe extern "C" {
    fn eglGetProcAddress(name: *const c_char) -> *const c_void;
}

mod surface {
    use super::*;
    #[derive(Default)]
    pub struct Surface {
        pub(super) owner: RefCell<std::rc::Weak<Shader>>,
    }
    #[glib::object_subclass]
    impl ObjectSubclass for Surface {
        const NAME: &'static str = "JadixNativeShader";
        type Type = super::Surface;
        type ParentType = gtk::Widget;
    }
    impl ObjectImpl for Surface {
        fn dispose(&self) {
            while let Some(child) = self.obj().first_child() {
                child.unparent();
            }
        }
    }
    impl WidgetImpl for Surface {
        fn measure(&self, _: gtk::Orientation, _: i32) -> (i32, i32, i32, i32) {
            (0, 0, -1, -1)
        }
        fn size_allocate(&self, width: i32, height: i32, baseline: i32) {
            let mut child = self.obj().first_child();
            while let Some(widget) = child {
                widget.allocate(width, height, baseline, None);
                child = widget.next_sibling();
            }
        }
        fn snapshot(&self, snapshot: &gtk::Snapshot) {
            if let Some(owner) = self.owner.borrow().upgrade() {
                owner.snapshot(snapshot);
            }
        }
    }
}
glib::wrapper! {
    pub struct Surface(ObjectSubclass<surface::Surface>) @extends gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

struct Capture {
    node: gtk::gsk::RenderNode,
    signature: glib::Bytes,
    geometry: [f32; 4],
    image: Rc<Image>,
}
struct Image {
    width: i32,
    height: i32,
    pixels: Vec<u8>,
}
struct Texture {
    handle: glow::NativeTexture,
    image: Option<Rc<Image>>,
}
struct Remote {
    name: String,
    handle: FetchHandle,
    bytes: Vec<u8>,
    valid: bool,
}
struct Gpu {
    gl: glow::Context,
    program: glow::NativeProgram,
    vao: glow::NativeVertexArray,
    blocks: Vec<(glow::NativeBuffer, u32)>,
    textures: BTreeMap<String, Texture>,
    compiled: CompiledShader,
}

pub(super) struct Shader {
    pub widget: Surface,
    pub children: gtk::Fixed,
    area: gtk::GLArea,
    spec: RefCell<HostedContent>,
    gpu: RefCell<Option<Gpu>>,
    rebuild: Cell<bool>,
    ready: Cell<bool>,
    failed: Cell<bool>,
    events: Rc<Events>,
    control: Rc<Control>,
    clock: RefCell<(Instant, f64, bool)>,
    tick: RefCell<Option<gtk::TickCallbackId>>,
    last_frame: Cell<i64>,
    content: RefCell<Option<Capture>>,
    backdrop: RefCell<Option<Capture>>,
    from: RefCell<Option<Capture>>,
    transition: RefCell<Option<Instant>>,
    textures: RefCell<BTreeMap<String, Rc<Image>>>,
    runtime: Rc<web::Runtime>,
    generation: Cell<u64>,
    remote: RefCell<Vec<Remote>>,
    reads: RefCell<Vec<gio::Cancellable>>,
}
impl Shader {
    pub fn new(
        spec: &HostedContent,
        events: Rc<Events>,
        control: Rc<Control>,
        runtime: Rc<web::Runtime>,
    ) -> Result<Rc<Self>, String> {
        spec.validate().map_err(str::to_owned)?;
        let widget: Surface = glib::Object::new();
        let area = gtk::GLArea::new();
        area.set_allowed_apis(gdk::GLAPI::GLES);
        area.set_required_version(3, 0);
        area.set_has_depth_buffer(false);
        area.set_has_stencil_buffer(false);
        area.set_auto_render(false);
        area.set_can_target(false);
        area.set_parent(&widget);
        let children = gtk::Fixed::new();
        children.set_parent(&widget);
        let this = Rc::new(Self {
            widget,
            children,
            area,
            spec: RefCell::new(spec.clone()),
            gpu: Default::default(),
            rebuild: Cell::new(true),
            ready: Cell::new(false),
            failed: Cell::new(false),
            events,
            control,
            clock: RefCell::new((Instant::now(), 0., false)),
            tick: Default::default(),
            last_frame: Cell::new(0),
            content: Default::default(),
            backdrop: Default::default(),
            from: Default::default(),
            transition: Default::default(),
            textures: Default::default(),
            runtime,
            generation: Cell::new(0),
            remote: Default::default(),
            reads: Default::default(),
        });
        *this.widget.imp().owner.borrow_mut() = Rc::downgrade(&this);
        let weak = Rc::downgrade(&this);
        this.area.connect_render(move |_, _| {
            if let Some(this) = weak.upgrade() {
                match this.render() {
                    Ok(()) => {
                        if !this.ready.replace(true) {
                            this.widget.queue_draw();
                        }
                    }
                    Err(error) => this.error(error),
                }
            }
            glib::Propagation::Stop
        });
        let weak = Rc::downgrade(&this);
        this.area.connect_unrealize(move |area| {
            if let Some(this) = weak.upgrade() {
                area.make_current();
                if let Some(gpu) = this.gpu.borrow_mut().take() {
                    if area.error().is_none() {
                        unsafe {
                            gpu.destroy();
                        }
                    }
                }
                this.rebuild.set(true);
                this.ready.set(false);
            }
        });
        let weak = Rc::downgrade(&this);
        this.area.connect_realize(move |area| {
            if let Some(this) = weak.upgrade() {
                if let Some(error) = area.error() {
                    this.error(error.to_string());
                }
            }
        });
        let weak = Rc::downgrade(&this);
        this.widget.connect_map(move |_| {
            if let Some(this) = weak.upgrade() {
                this.animate();
                this.area.queue_render();
            }
        });
        this.update(spec);
        Ok(this)
    }
    fn error(&self, message: String) {
        if !self.failed.replace(true) {
            hosted::emit(
                &self.events,
                &self.control,
                "error",
                message.chars().take(4096).collect::<String>().into(),
            );
        }
        self.ready.set(false);
        self.widget.queue_draw();
    }
    pub fn update(self: &Rc<Self>, spec: &HostedContent) {
        let HostedContent::Shader {
            paused, textures, ..
        } = spec
        else {
            return;
        };
        let previous = self.spec.borrow().clone();
        let HostedContent::Shader {
            source: old_source,
            function: old_function,
            language: old_language,
            parameters: old_parameters,
            textures: old_textures,
            composition: old_composition,
            ..
        } = &previous
        else {
            return;
        };
        let HostedContent::Shader {
            source,
            function,
            language,
            parameters,
            composition,
            ..
        } = spec
        else {
            return;
        };
        let changed_program = source != old_source
            || function != old_function
            || language != old_language
            || parameters
                .iter()
                .map(|(n, v)| (n, std::mem::discriminant(v), array_length(v)))
                .ne(old_parameters
                    .iter()
                    .map(|(n, v)| (n, std::mem::discriminant(v), array_length(v))))
            || textures.keys().ne(old_textures.keys())
            || composition.inputs() != old_composition.inputs();
        if changed_program {
            self.rebuild.set(true);
            self.failed.set(false);
        }
        if let ShaderComposition::Transition { value, .. } = composition {
            if !matches!(old_composition, ShaderComposition::Transition { value: old, .. } if old == value)
            {
                *self.from.borrow_mut() = self.content.borrow_mut().take();
                *self.transition.borrow_mut() = self.from.borrow().as_ref().map(|_| Instant::now());
            }
        } else {
            self.from.borrow_mut().take();
            self.transition.borrow_mut().take();
        }
        let mut clock = self.clock.borrow_mut();
        if clock.2 {
            clock.1 += clock.0.elapsed().as_secs_f64();
        }
        clock.0 = Instant::now();
        clock.2 = !paused;
        drop(clock);
        if textures != old_textures || self.generation.get() == 0 {
            self.generation.set(self.generation.get().wrapping_add(1));
            self.remote.borrow_mut().clear();
            for read in self.reads.borrow_mut().drain(..) {
                read.cancel();
            }
            let mut images = BTreeMap::new();
            for (name, source) in textures {
                if source.starts_with("data:image/") {
                    match controls::texture(source).and_then(|texture| download(&texture)) {
                        Ok(image) => {
                            let total: usize = images
                                .values()
                                .map(|image: &Rc<Image>| image.pixels.len())
                                .sum();
                            if total + image.pixels.len() <= 32 * 1024 * 1024 {
                                images.insert(name.clone(), Rc::new(image));
                            } else {
                                hosted::emit(
                                    &self.events,
                                    &self.control,
                                    "error",
                                    "shader image aggregate budget exceeded".into(),
                                );
                            }
                        }
                        Err(error) => {
                            hosted::emit(&self.events, &self.control, "error", error.into())
                        }
                    }
                } else {
                    self.load_image(name, source);
                }
            }
            *self.textures.borrow_mut() = images;
        }
        *self.spec.borrow_mut() = spec.clone();
        self.animate();
        self.area.queue_render();
        self.widget.queue_draw();
    }
    fn load_image(self: &Rc<Self>, name: &str, source: &str) {
        if source.starts_with("https://") {
            match self.runtime.fetch.send(FetchRequest {
                url: source.into(),
                method: "GET".into(),
                headers: vec![],
                body: vec![],
                timeout_ms: 10_000,
                max_response_bytes: 768 * 1024,
            }) {
                Ok(handle) => self.remote.borrow_mut().push(Remote {
                    name: name.into(),
                    handle,
                    bytes: vec![],
                    valid: false,
                }),
                Err(error) => self.image(name.into(), Err(error.to_string())),
            }
            return;
        }
        let root = self.runtime.asset_root();
        let path = if let Some(relative) = jadix_core::web_resource::bundle_path(source) {
            root.join(relative)
                .canonicalize()
                .ok()
                .filter(|p| p.starts_with(&root))
        } else if let Some(relative) = source.strip_prefix("asset:/") {
            root.join(relative.trim_start_matches('/'))
                .canonicalize()
                .ok()
                .filter(|p| p.starts_with(&root))
        } else if source.starts_with("file:///") {
            gio::File::for_uri(source)
                .path()
                .and_then(|path| path.canonicalize().ok())
        } else {
            None
        };
        let Some(path) = path
            .filter(|path| path.is_file() && path.metadata().is_ok_and(|m| m.len() <= 768 * 1024))
        else {
            self.image(
                name.into(),
                Err("invalid or oversized shader image source".into()),
            );
            return;
        };
        let cancel = gio::Cancellable::new();
        self.reads.borrow_mut().push(cancel.clone());
        let weak = Rc::downgrade(self);
        let generation = self.generation.get();
        let name = name.to_owned();
        gio::File::for_path(path).read_async(
            glib::Priority::DEFAULT,
            Some(&cancel.clone()),
            move |result| {
                let Some(this) = weak
                    .upgrade()
                    .filter(|this| this.generation.get() == generation)
                else {
                    return;
                };
                match result {
                    Ok(stream) => read_image(
                        Rc::downgrade(&this),
                        generation,
                        name,
                        stream,
                        cancel,
                        vec![],
                    ),
                    Err(error) => this.finish_read(name, &cancel, Err(error.to_string())),
                }
            },
        );
    }
    fn image(&self, name: String, bytes: Result<Vec<u8>, String>) {
        use base64::Engine;
        let image = bytes.and_then(|bytes| {
            let mime = if bytes.windows(4).take(1024).any(|v| v == b"<svg") {
                "svg+xml"
            } else {
                "png"
            };
            controls::texture(&format!(
                "data:image/{mime};base64,{}",
                base64::engine::general_purpose::STANDARD.encode(bytes)
            ))
            .and_then(|texture| download(&texture))
        });
        match image {
            Ok(image) => {
                let total: usize = self
                    .textures
                    .borrow()
                    .iter()
                    .filter(|(key, _)| **key != name)
                    .map(|(_, image)| image.pixels.len())
                    .sum();
                if total + image.pixels.len() > 32 * 1024 * 1024 {
                    hosted::emit(
                        &self.events,
                        &self.control,
                        "error",
                        "shader image aggregate budget exceeded".into(),
                    );
                    return;
                }
                self.textures.borrow_mut().insert(name, Rc::new(image));
                self.area.queue_render();
                self.widget.queue_draw();
            }
            Err(error) => hosted::emit(&self.events, &self.control, "error", error.into()),
        }
    }
    fn finish_read(
        &self,
        name: String,
        cancel: &gio::Cancellable,
        result: Result<Vec<u8>, String>,
    ) {
        self.reads.borrow_mut().retain(|read| read != cancel);
        self.image(name, result);
        self.events.wake.wake();
    }
    pub fn pump(&self) {
        let mut complete = vec![];
        self.remote.borrow_mut().retain_mut(|remote| {
            for event in remote.handle.events() {
                match event {
                    FetchEvent::Headers { status, .. } => remote.valid = status == 200,
                    FetchEvent::Chunk { bytes } => remote.bytes.extend(bytes),
                    FetchEvent::Complete => {
                        complete.push((
                            remote.name.clone(),
                            if remote.valid {
                                Ok(std::mem::take(&mut remote.bytes))
                            } else {
                                Err("shader image HTTP status rejected".into())
                            },
                        ));
                        return false;
                    }
                    FetchEvent::Failed { message } => {
                        complete.push((remote.name.clone(), Err(message)));
                        return false;
                    }
                    FetchEvent::Cancelled => return false,
                }
            }
            true
        });
        for (name, result) in complete {
            self.image(name, result);
        }
    }
    fn progress(&self) -> f32 {
        let spec = self.spec.borrow();
        let HostedContent::Shader {
            progress,
            composition,
            ..
        } = &*spec
        else {
            return 1.;
        };
        if let Some(value) = progress {
            return *value;
        }
        if let ShaderComposition::Transition { duration_ms, .. } = composition {
            if let Some(start) = *self.transition.borrow() {
                return (start.elapsed().as_secs_f32() / (*duration_ms as f32 / 1000.).max(0.001))
                    .min(1.);
            }
        }
        1.
    }
    fn animate(self: &Rc<Self>) {
        if let Some(tick) = self.tick.borrow_mut().take() {
            tick.remove();
        }
        if !self.needs_animation() {
            return;
        }
        let weak = Rc::downgrade(self);
        let tick = self.widget.add_tick_callback(move |_, clock| {
            let Some(this) = weak.upgrade() else {
                return glib::ControlFlow::Break;
            };
            if !this.needs_animation() || !this.in_viewport() {
                this.tick.borrow_mut().take();
                return glib::ControlFlow::Break;
            }
            let fps = match &*this.spec.borrow() {
                HostedContent::Shader {
                    frames_per_second, ..
                } => *frames_per_second,
                _ => 60,
            };
            if clock.frame_time() - this.last_frame.get() >= 1_000_000 / fps as i64 {
                this.last_frame.set(clock.frame_time());
                this.area.queue_render();
                this.widget.queue_draw();
            }
            glib::ControlFlow::Continue
        });
        *self.tick.borrow_mut() = Some(tick);
    }
    fn needs_animation(&self) -> bool {
        if self.failed.get() {
            return self.transition.borrow().is_some() && self.progress() < 1.;
        }
        matches!(
            &*self.spec.borrow(),
            HostedContent::Shader {
                paused: false,
                animated: true,
                time_seconds: None,
                ..
            }
        ) || (self.transition.borrow().is_some() && self.progress() < 1.)
    }
    fn in_viewport(&self) -> bool {
        let Some(root) = self
            .widget
            .root()
            .and_then(|root| root.dynamic_cast::<gtk::Widget>().ok())
        else {
            return false;
        };
        self.widget.compute_bounds(&root).is_some_and(|bounds| {
            bounds
                .intersection(&gtk::graphene::Rect::new(
                    0.,
                    0.,
                    root.width() as f32,
                    root.height() as f32,
                ))
                .is_some()
        })
    }
    fn snapshot(&self, snapshot: &gtk::Snapshot) {
        let (composition, overflow) = match &*self.spec.borrow() {
            HostedContent::Shader {
                composition,
                max_sample_offset,
                ..
            } => (composition.clone(), *max_sample_offset),
            _ => return,
        };
        if !matches!(composition, ShaderComposition::Background) {
            let capture = gtk::Snapshot::new();
            self.widget.snapshot_child(&self.children, &capture);
            if let Some(node) = capture.to_node() {
                self.capture(node, overflow, &self.content);
            } else if self.content.borrow_mut().take().is_some() {
                self.area.queue_render();
            }
            if let Some(node) = self.backdrop_node() {
                self.capture(node, overflow, &self.backdrop);
            } else if self.backdrop.borrow_mut().take().is_some() {
                self.area.queue_render();
            }
        }
        if !self.failed.get() {
            self.widget.snapshot_child(&self.area, snapshot);
        }
        if matches!(composition, ShaderComposition::Background) || !self.ready.get() {
            if let ShaderComposition::Transition {
                fallback: jadix_core::hosted::ShaderFallback::Fade,
                ..
            } = composition
            {
                let progress = self.progress() as f64;
                if let Some(from) = self.from.borrow().as_ref().filter(|_| progress < 1.) {
                    snapshot.push_opacity(1. - progress);
                    snapshot.append_node(&from.node);
                    snapshot.pop();
                    snapshot.push_opacity(progress);
                    self.widget.snapshot_child(&self.children, snapshot);
                    snapshot.pop();
                    return;
                }
            }
            self.widget.snapshot_child(&self.children, snapshot);
        }
        if self.progress() >= 1. {
            self.from.borrow_mut().take();
            self.transition.borrow_mut().take();
        }
    }
    fn capture(&self, node: gtk::gsk::RenderNode, overflow: f32, slot: &RefCell<Option<Capture>>) {
        let signature = node.serialize();
        let scale = self.widget.scale_factor().max(1) as f32;
        let geometry = [
            self.widget.width() as f32,
            self.widget.height() as f32,
            scale,
            overflow,
        ];
        if slot
            .borrow()
            .as_ref()
            .is_some_and(|old| old.signature == signature && old.geometry == geometry)
        {
            return;
        }
        let width = (self.widget.width() as f32 + 2. * overflow) * scale;
        let height = (self.widget.height() as f32 + 2. * overflow) * scale;
        if !valid_size(width.ceil() as i32, height.ceil() as i32) {
            self.error("shader capture exceeds pixel budget".into());
            return;
        }
        let Some(renderer) = self.widget.native().and_then(|native| native.renderer()) else {
            return;
        };
        let scaled = gtk::Snapshot::new();
        scaled.scale(scale, scale);
        scaled.translate(&gtk::graphene::Point::new(overflow, overflow));
        scaled.append_node(&node);
        let Some(scaled) = scaled.to_node() else {
            return;
        };
        let texture = renderer.render_texture(
            &scaled,
            Some(&gtk::graphene::Rect::new(0., 0., width, height)),
        );
        match download(&texture) {
            Ok(image) => {
                *slot.borrow_mut() = Some(Capture {
                    node,
                    signature,
                    geometry,
                    image: Rc::new(image),
                });
                self.area.queue_render();
            }
            Err(error) => self.error(error),
        }
    }
    fn backdrop_node(&self) -> Option<gtk::gsk::RenderNode> {
        let snapshot = gtk::Snapshot::new();
        let mut ancestors = Vec::new();
        let mut child: gtk::Widget = self.widget.clone().upcast();
        while let Some(parent) = child.parent() {
            ancestors.push((parent.clone(), child));
            child = parent;
        }
        for (parent, owner) in ancestors.iter().rev() {
            let point = parent.compute_point(&self.widget, &gtk::graphene::Point::zero())?;
            snapshot.save();
            snapshot.translate(&point);
            #[allow(deprecated)]
            snapshot.render_background(
                &parent.style_context(),
                0.,
                0.,
                parent.width() as f64,
                parent.height() as f64,
            );
            let mut sibling = parent.first_child();
            while let Some(widget) = sibling {
                if widget == *owner {
                    break;
                }
                if widget.is_visible() {
                    parent.snapshot_child(&widget, &snapshot);
                }
                sibling = widget.next_sibling();
            }
            snapshot.restore();
        }
        snapshot.to_node()
    }
    fn render(&self) -> Result<(), String> {
        if let Some(error) = self.area.error() {
            return Err(error.to_string());
        }
        let width = self.area.width() * self.area.scale_factor();
        let height = self.area.height() * self.area.scale_factor();
        if !valid_size(width, height) {
            return Err("shader viewport exceeds pixel budget".into());
        }
        let spec = self.spec.borrow();
        if self.rebuild.replace(false) {
            if let Some(gpu) = self.gpu.borrow_mut().take() {
                unsafe {
                    gpu.destroy();
                }
            }
            *self.gpu.borrow_mut() = Some(unsafe { Gpu::new(&spec)? });
        }
        let mut gpu = self.gpu.borrow_mut();
        let Some(gpu) = gpu.as_mut() else {
            return Err("shader GPU unavailable".into());
        };
        let HostedContent::Shader {
            parameters,
            uniforms,
            time_seconds,
            max_sample_offset,
            ..
        } = &*spec
        else {
            return Ok(());
        };
        let clock = self.clock.borrow();
        let time = time_seconds.unwrap_or(
            clock.1
                + if clock.2 {
                    clock.0.elapsed().as_secs_f64()
                } else {
                    0.
                },
        );
        let frame = ShaderFrame {
            size: [self.area.width() as f32, self.area.height() as f32],
            pixels: [width as f32, height as f32],
            scale: self.area.scale_factor() as f32,
            time: time as f32,
            progress: self.progress(),
            overflow: *max_sample_offset,
        };
        let mut images = self.textures.borrow().clone();
        for (name, slot) in [
            ("content", &self.content),
            ("to", &self.content),
            ("from", &self.from),
            ("backdrop", &self.backdrop),
        ] {
            if let Some(capture) = slot.borrow().as_ref() {
                images.insert(name.into(), capture.image.clone());
            }
        }
        if !images.contains_key("from") {
            if let Some(image) = images.get("to").cloned() {
                images.insert("from".into(), image);
            }
        }
        unsafe {
            gpu.draw(parameters, uniforms, &frame, &images)?;
        }
        Ok(())
    }
}
impl Drop for Shader {
    fn drop(&mut self) {
        self.remote.borrow_mut().clear();
        for read in self.reads.borrow_mut().drain(..) {
            read.cancel();
        }
        if let Some(tick) = self.tick.borrow_mut().take() {
            tick.remove();
        }
        if self.area.is_realized() {
            self.area.make_current();
            if self.area.error().is_none() {
                if let Some(gpu) = self.gpu.borrow_mut().take() {
                    unsafe {
                        gpu.destroy();
                    }
                }
            }
        }
    }
}
fn read_image(
    weak: std::rc::Weak<Shader>,
    generation: u64,
    name: String,
    stream: gio::FileInputStream,
    cancel: gio::Cancellable,
    mut bytes: Vec<u8>,
) {
    stream.clone().read_bytes_async(
        64 * 1024,
        glib::Priority::DEFAULT,
        Some(&cancel.clone()),
        move |result| {
            let Some(this) = weak
                .upgrade()
                .filter(|this| this.generation.get() == generation)
            else {
                return;
            };
            match result {
                Ok(chunk) if chunk.is_empty() => this.finish_read(name, &cancel, Ok(bytes)),
                Ok(chunk) if bytes.len() + chunk.len() <= 768 * 1024 => {
                    bytes.extend_from_slice(&chunk);
                    read_image(
                        Rc::downgrade(&this),
                        generation,
                        name,
                        stream,
                        cancel,
                        bytes,
                    );
                }
                Ok(_) => this.finish_read(
                    name,
                    &cancel,
                    Err("shader image source exceeds budget".into()),
                ),
                Err(error) => this.finish_read(name, &cancel, Err(error.to_string())),
            }
        },
    );
}
fn array_length(value: &ShaderUniform) -> usize {
    if let ShaderUniform::Vec4Array(values) = value {
        values.len()
    } else {
        0
    }
}
fn valid_size(width: i32, height: i32) -> bool {
    width > 0
        && height > 0
        && width <= 4096
        && height <= 4096
        && width as usize * height as usize <= 4 * 1024 * 1024
}
fn download(texture: &gdk::Texture) -> Result<Image, String> {
    let (width, height) = (texture.width(), texture.height());
    if !valid_size(width, height) {
        return Err("shader texture exceeds pixel budget".into());
    }
    let mut downloader = gdk::TextureDownloader::new(texture);
    downloader.set_format(gdk::MemoryFormat::R8g8b8a8Premultiplied);
    let (bytes, stride) = downloader.download_bytes();
    let row = width as usize * 4;
    if stride < row || bytes.len() < stride * height as usize {
        return Err("invalid shader image stride".into());
    }
    let pixels = bytes
        .chunks(stride)
        .take(height as usize)
        .flat_map(|chunk| chunk[..row].iter().copied())
        .collect();
    Ok(Image {
        width,
        height,
        pixels,
    })
}

impl Gpu {
    unsafe fn new(spec: &HostedContent) -> Result<Self, String> {
        let gl = unsafe {
            glow::Context::from_loader_function(|name| {
                let name = CString::new(name).unwrap();
                eglGetProcAddress(name.as_ptr())
            })
        };
        let compiled = program_source(spec)?;
        // Retain every allocation immediately so all compilation failure paths clean up.
        let program = unsafe { gl.create_program()? };
        let vao = match unsafe { gl.create_vertex_array() } {
            Ok(vao) => vao,
            Err(error) => {
                unsafe {
                    gl.delete_program(program);
                }
                return Err(error);
            }
        };
        let mut this = Self {
            gl,
            program,
            vao,
            blocks: vec![],
            textures: BTreeMap::new(),
            compiled,
        };
        let result = unsafe { this.initialize() };
        if let Err(error) = result {
            unsafe {
                this.destroy();
            }
            return Err(error);
        }
        Ok(this)
    }
    unsafe fn initialize(&mut self) -> Result<(), String> {
        let gl = &self.gl;
        let vertex = "#version 300 es\nvoid main(){ vec2 p=vec2(float((gl_VertexID<<1)&2),float(gl_VertexID&2)); gl_Position=vec4(p*2.0-1.0,0.0,1.0); }";
        for (kind, source) in [
            (glow::VERTEX_SHADER, vertex),
            (glow::FRAGMENT_SHADER, self.compiled.source.as_str()),
        ] {
            let shader = unsafe { gl.create_shader(kind)? };
            unsafe {
                gl.shader_source(shader, source);
                gl.compile_shader(shader);
            }
            let success = unsafe { gl.get_shader_compile_status(shader) };
            if !success {
                let error = unsafe { gl.get_shader_info_log(shader) };
                unsafe {
                    gl.delete_shader(shader);
                }
                return Err(error);
            }
            unsafe {
                gl.attach_shader(self.program, shader);
                gl.delete_shader(shader);
            }
        }
        unsafe {
            gl.link_program(self.program);
        }
        if !unsafe { gl.get_program_link_status(self.program) } {
            return Err(unsafe { gl.get_program_info_log(self.program) });
        }
        for (index, block) in self.compiled.blocks.iter().enumerate() {
            if let Some(binding) = unsafe { gl.get_uniform_block_index(self.program, &block.name) }
            {
                let buffer = unsafe { gl.create_buffer()? };
                self.blocks.push((buffer, index as u32));
                unsafe {
                    gl.uniform_block_binding(self.program, binding, index as u32);
                }
            } else {
                self.blocks
                    .push((unsafe { gl.create_buffer()? }, index as u32));
            }
        }
        for name in self.compiled.textures.keys() {
            self.textures.insert(
                name.clone(),
                Texture {
                    handle: unsafe { gl.create_texture()? },
                    image: None,
                },
            );
        }
        Ok(())
    }
    unsafe fn draw(
        &mut self,
        parameters: &BTreeMap<String, ShaderUniform>,
        uniforms: &[f32],
        frame: &ShaderFrame,
        images: &BTreeMap<String, Rc<Image>>,
    ) -> Result<(), String> {
        let gl = &self.gl;
        unsafe {
            gl.viewport(0, 0, frame.pixels[0] as i32, frame.pixels[1] as i32);
            gl.disable(glow::DEPTH_TEST);
            gl.disable(glow::BLEND);
            gl.disable(glow::SCISSOR_TEST);
            gl.clear_color(0., 0., 0., 0.);
            gl.clear(glow::COLOR_BUFFER_BIT);
            gl.use_program(Some(self.program));
            gl.bind_vertex_array(Some(self.vao));
        }
        for (block, (buffer, index)) in self.compiled.blocks.iter().zip(&self.blocks) {
            let bytes = pack_shader_block(block, parameters, frame)?;
            unsafe {
                gl.bind_buffer(glow::UNIFORM_BUFFER, Some(*buffer));
                gl.buffer_data_u8_slice(glow::UNIFORM_BUFFER, &bytes, glow::DYNAMIC_DRAW);
                gl.bind_buffer_base(glow::UNIFORM_BUFFER, *index, Some(*buffer));
            }
        }
        unsafe {
            gl.uniform_2_f32(
                gl.get_uniform_location(self.program, "uResolution")
                    .as_ref(),
                frame.pixels[0],
                frame.pixels[1],
            );
            gl.uniform_1_f32(
                gl.get_uniform_location(self.program, "uTime").as_ref(),
                frame.time,
            );
            gl.uniform_1_f32(
                gl.get_uniform_location(self.program, "uScale").as_ref(),
                frame.scale,
            );
            gl.uniform_1_f32(
                gl.get_uniform_location(self.program, "uProgress").as_ref(),
                frame.progress,
            );
            gl.uniform_1_f32(
                gl.get_uniform_location(self.program, "uOverflow").as_ref(),
                frame.overflow,
            );
            gl.uniform_1_f32_slice(
                gl.get_uniform_location(self.program, "uValues").as_ref(),
                uniforms,
            );
        }
        for (name, value) in parameters {
            let location = unsafe { gl.get_uniform_location(self.program, name) };
            unsafe {
                match value {
                    ShaderUniform::Float(v) => gl.uniform_1_f32(location.as_ref(), *v),
                    ShaderUniform::Int(v) => gl.uniform_1_i32(location.as_ref(), *v),
                    ShaderUniform::Uint(v) => gl.uniform_1_u32(location.as_ref(), *v),
                    ShaderUniform::Vec2(v) => gl.uniform_2_f32_slice(location.as_ref(), v),
                    ShaderUniform::Vec3(v) => gl.uniform_3_f32_slice(location.as_ref(), v),
                    ShaderUniform::Vec4(v) => gl.uniform_4_f32_slice(location.as_ref(), v),
                    ShaderUniform::Mat4(v) => {
                        gl.uniform_matrix_4_f32_slice(location.as_ref(), false, v)
                    }
                    ShaderUniform::Vec4Array(v) => gl.uniform_4_f32_slice(
                        location.as_ref(),
                        &v.iter().flatten().copied().collect::<Vec<_>>(),
                    ),
                }
            }
        }
        for (unit, (name, texture)) in self.textures.iter_mut().enumerate() {
            let image = images.get(name);
            unsafe {
                gl.active_texture(glow::TEXTURE0 + unit as u32);
                gl.bind_texture(glow::TEXTURE_2D, Some(texture.handle));
                let changed = match (&texture.image, image) {
                    (Some(old), Some(new)) => !Rc::ptr_eq(old, new),
                    (None, None) => true,
                    _ => true,
                };
                if changed {
                    let fallback =
                        if matches!(name.as_str(), "from" | "to" | "content" | "backdrop") {
                            [0; 4]
                        } else {
                            [255; 4]
                        };
                    let (width, height, pixels) = image
                        .map_or((1, 1, fallback.as_slice()), |image| {
                            (image.width, image.height, image.pixels.as_slice())
                        });
                    gl.tex_image_2d(
                        glow::TEXTURE_2D,
                        0,
                        glow::RGBA as i32,
                        width,
                        height,
                        0,
                        glow::RGBA,
                        glow::UNSIGNED_BYTE,
                        glow::PixelUnpackData::Slice(Some(pixels)),
                    );
                    texture.image = image.cloned();
                    for (key, value) in [
                        (glow::TEXTURE_MIN_FILTER, glow::LINEAR),
                        (glow::TEXTURE_MAG_FILTER, glow::LINEAR),
                        (glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE),
                        (glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE),
                    ] {
                        gl.tex_parameter_i32(glow::TEXTURE_2D, key, value as i32);
                    }
                }
                gl.uniform_1_i32(
                    gl.get_uniform_location(self.program, &self.compiled.textures[name])
                        .as_ref(),
                    unit as i32,
                );
            }
        }
        unsafe {
            gl.draw_arrays(glow::TRIANGLES, 0, 3);
            gl.bind_vertex_array(None);
            gl.use_program(None);
            gl.bind_buffer(glow::UNIFORM_BUFFER, None);
        }
        Ok(())
    }
    unsafe fn destroy(self) {
        unsafe {
            for (_, texture) in self.textures {
                self.gl.delete_texture(texture.handle);
            }
            for (buffer, _) in self.blocks {
                self.gl.delete_buffer(buffer);
            }
            self.gl.delete_vertex_array(self.vao);
            self.gl.delete_program(self.program);
        }
    }
}

fn program_source(spec: &HostedContent) -> Result<CompiledShader, String> {
    let HostedContent::Shader {
        source,
        function,
        language,
        parameters,
        textures,
        composition,
        ..
    } = spec
    else {
        return Err("expected shader".into());
    };
    if *language == ShaderLanguage::Wgsl {
        return compile_shader_gles(spec);
    }
    let textures: BTreeMap<String, String> = textures
        .keys()
        .map(|name| (name.clone(), name.clone()))
        .chain(
            composition
                .inputs()
                .iter()
                .map(|name| (name.to_string(), format!("jadix_texture_{name}"))),
        )
        .collect();
    if source.contains("void main(") {
        return Ok(CompiledShader {
            source: source.clone(),
            blocks: vec![],
            textures,
        });
    }
    let mut code = String::from("#version 300 es\nprecision highp float; precision highp int;\nout vec4 jadix_color;\nuniform vec2 uResolution; uniform float uTime; uniform float uScale; uniform float uProgress; uniform float uOverflow; uniform float uValues[256];\n");
    for (name, value) in parameters {
        let kind = match value {
            ShaderUniform::Float(_) => "float",
            ShaderUniform::Int(_) => "int",
            ShaderUniform::Uint(_) => "uint",
            ShaderUniform::Vec2(_) => "vec2",
            ShaderUniform::Vec3(_) => "vec3",
            ShaderUniform::Vec4(_) | ShaderUniform::Vec4Array(_) => "vec4",
            ShaderUniform::Mat4(_) => "mat4",
        };
        let suffix = if let ShaderUniform::Vec4Array(values) = value {
            format!("[{}]", values.len())
        } else {
            String::new()
        };
        code.push_str(&format!("uniform {kind} {name}{suffix};\n"));
    }
    for (name, binding) in &textures {
        code.push_str(&format!("uniform sampler2D {binding};\n"));
        let coordinates = if composition.inputs().contains(&name.as_str()) {
            "(uv*(uResolution/uScale)+vec2(uOverflow))/(uResolution/uScale+vec2(2.0*uOverflow))"
        } else {
            "uv"
        };
        code.push_str(&format!("vec4 sample_{name}(vec2 uv) {{ vec4 c=texture({binding},{coordinates}); return vec4(c.a>0.0?c.rgb/max(c.a,0.000001):vec3(0.0),c.a); }}\n"));
    }
    code.push_str(source);
    code.push_str(&format!(
        "\nvoid main(){{ vec4 c={function}(gl_FragCoord.xy); jadix_color=vec4(c.rgb*c.a,c.a); }}"
    ));
    Ok(CompiledShader {
        source: code,
        blocks: vec![],
        textures,
    })
}

//! GSK composition around real GTK controls. The Fixed child transform is also
//! used by GTK picking, so painted and interactive coordinates stay together.
use super::*;
use gtk::{graphene, gsk, subclass::prelude::*};
use jadix_core::presentation::style::*;
use std::time::Instant;

struct Transition {
    start: Instant,
    delay: f32,
    duration: f32,
    curve: Vec<f32>,
    channels: Vec<AppearanceChannel>,
    target: serde_json::Value,
}
struct Animation {
    start: Instant,
    spec: ViewAnimation,
    curve: Vec<[f32; 4]>,
}

mod imp {
    use super::*;
    #[derive(Default)]
    pub struct Surface {
        pub child: RefCell<Option<gtk::Widget>>,
        pub style: RefCell<NodeStyle>,
        pub target: RefCell<Option<NodeStyle>>,
        pub css: RefCell<Option<gtk::CssProvider>>,
        pub position: Cell<[f64; 2]>,
        pub size: Cell<[f64; 2]>,
        pub(super) transition: RefCell<Option<Transition>>,
        pub(super) animation: RefCell<Option<Animation>>,
        pub last_frame: Cell<Option<Instant>>,
        pub tick: RefCell<Option<gtk::TickCallbackId>>,
        pub paused: Cell<Option<Instant>>,
    }
    #[glib::object_subclass]
    impl ObjectSubclass for Surface {
        const NAME: &'static str = "JadixNativeVisual";
        type Type = super::Surface;
        type ParentType = gtk::Fixed;
        fn class_init(class: &mut Self::Class) {
            class.set_accessible_role(gtk::AccessibleRole::Presentation);
        }
    }
    impl ObjectImpl for Surface {}
    impl FixedImpl for Surface {}
    impl WidgetImpl for Surface {
        fn snapshot(&self, snapshot: &gtk::Snapshot) {
            let obj = self.obj();
            let style = self.style.borrow();
            let view = style.view.clone().unwrap_or_default();
            let bounds = graphene::Rect::new(0., 0., obj.width() as f32, obj.height() as f32);
            if bounds.width() <= 0. || bounds.height() <= 0. {
                return;
            }
            if let Err(error) =
                validate_effect_size(&view, f64::from(bounds.width()), f64::from(bounds.height()))
            {
                log::error!("Linux visual allocation: {error}");
                return;
            }
            let rounded = rounded(&bounds, view.corners);
            let content = gtk::Snapshot::new();
            if !view.backdrop_filters.is_empty() {
                if let Some(back) = backdrop(obj.upcast_ref()) {
                    let filtered = if let Some(progressive) = view.backdrop_progressive {
                        progressive_node(&back, &view.backdrop_filters, progressive, &bounds)
                    } else {
                        filters(back, &view.backdrop_filters)
                    };
                    content.push_rounded_clip(&rounded);
                    content.append_node(&filtered);
                    content.pop();
                }
            }
            append_background(&content, &style, &bounds);
            if view.clip {
                content.push_rounded_clip(&rounded);
            }
            if let Some(child) = self.child.borrow().as_ref() {
                obj.snapshot_child(child, &content);
            }
            if view.clip {
                content.pop();
            }
            let Some(mut node) = content.to_node() else {
                return;
            };
            node = filters(node, &view.filters);
            for gradient in &view.masks {
                if let Some(mask) = gradient_node(gradient, &bounds) {
                    node = gsk::MaskNode::new(&node, &mask, gsk::MaskMode::Alpha).upcast();
                }
            }
            if view.blend_mode != BlendMode::Normal {
                if let Some(back) = backdrop(obj.upcast_ref()) {
                    node = match view.blend_mode {
                        BlendMode::PlusLighter | BlendMode::PlusDarker => {
                            match arithmetic_blend(&back, &node, &bounds, view.blend_mode) {
                                Ok(result) => result,
                                Err(error) => {
                                    log::error!("Linux blend: {error}");
                                    return;
                                }
                            }
                        }
                        _ => gsk::BlendNode::new(&back, &node, blend(&view)).upcast(),
                    };
                    node = gsk::RoundedClipNode::new(&node, &rounded).upcast();
                }
            }
            snapshot.append_node(&node);
        }
    }
}
glib::wrapper! {
    pub struct Surface(ObjectSubclass<imp::Surface>) @extends gtk::Fixed, gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}
impl Surface {
    pub fn new(child: &gtk::Widget, css: &gtk::CssProvider) -> Self {
        let obj: Self = glib::Object::new();
        obj.put(child, 0., 0.);
        *obj.imp().child.borrow_mut() = Some(child.clone());
        *obj.imp().css.borrow_mut() = Some(css.clone());
        obj.connect_unmap(|obj| {
            obj.imp().paused.set(Some(Instant::now()));
        });
        obj.connect_map(|obj| {
            if let Some(paused) = obj.imp().paused.take() {
                let elapsed = paused.elapsed();
                if let Some(t) = obj.imp().transition.borrow_mut().as_mut() {
                    t.start += elapsed;
                }
                if let Some(a) = obj.imp().animation.borrow_mut().as_mut() {
                    a.start += elapsed;
                }
            }
        });
        obj
    }
    pub fn update(&self, descriptor: &NodeDescriptor) {
        let imp = self.imp();
        let now = Instant::now();
        let animate = self.settings().is_gtk_enable_animations();
        let previous = imp.target.borrow_mut().replace(descriptor.style.clone());
        let previous_view = previous.as_ref().and_then(|s| s.view.as_ref());
        let target_view = descriptor.style.view.as_ref();
        if previous_view != target_view {
            let transition = target_view.and_then(|v| v.transition).filter(|_| animate);
            *imp.transition.borrow_mut() = transition.and_then(|spec| {
                let current = imp.style.borrow().view.clone().unwrap_or_default();
                let target = target_view.cloned().unwrap_or_default();
                let channels = current.transition_channels(&target, spec.properties);
                let (duration, curve) = jadix_animation::presentation::transition_curve(&spec)?;
                if previous.is_none() || channels.is_empty() || duration <= 0. {
                    return None;
                }
                Some(Transition {
                    start: now,
                    delay: spec.delay,
                    duration,
                    curve,
                    channels,
                    target: serde_json::to_value(target).ok()?,
                })
            });
        }
        if previous_view.and_then(|v| v.animation) != target_view.and_then(|v| v.animation)
            || !animate
        {
            *imp.animation.borrow_mut() = target_view
                .and_then(|v| v.animation)
                .filter(|a| animate && a.iterations != Some(0))
                .and_then(|spec| {
                    Some(Animation {
                        start: now,
                        spec,
                        curve: jadix_animation::presentation::keyframe_curve(&spec)?,
                    })
                });
        }
        if let Some(parent) = self.parent().and_downcast::<gtk::Fixed>() {
            let (x, y) = parent.child_position(self);
            imp.position.set([x, y]);
        }
        *imp.style.borrow_mut() = descriptor.style.clone();
        imp.size
            .set([descriptor.frame.width, descriptor.frame.height]);
        self.set_size_request(
            pixel(descriptor.frame.width),
            pixel(descriptor.frame.height),
        );
        self.frame();
        if (imp.transition.borrow().is_some() || imp.animation.borrow().is_some())
            && imp.tick.borrow().is_none()
        {
            *imp.tick.borrow_mut() = Some(self.add_tick_callback(|obj, _| {
                let rate = obj
                    .imp()
                    .animation
                    .borrow()
                    .as_ref()
                    .and_then(|a| a.spec.frame_rate)
                    .unwrap_or(60.)
                    .min(120.);
                if obj
                    .imp()
                    .last_frame
                    .get()
                    .is_some_and(|t| t.elapsed().as_secs_f32() < 1. / rate)
                {
                    return glib::ControlFlow::Continue;
                }
                obj.imp().last_frame.set(Some(Instant::now()));
                if obj.frame() {
                    glib::ControlFlow::Continue
                } else {
                    obj.imp().tick.borrow_mut().take();
                    glib::ControlFlow::Break
                }
            }));
        }
    }
    fn frame(&self) -> bool {
        let imp = self.imp();
        let mut style = imp.target.borrow().clone().unwrap_or_default();
        let animations = self.settings().is_gtk_enable_animations();
        if !animations {
            imp.transition.borrow_mut().take();
            imp.animation.borrow_mut().take();
        }
        let mut finished = false;
        if let Some(t) = imp.transition.borrow().as_ref() {
            let elapsed = t.start.elapsed().as_secs_f32() * 1000. - t.delay;
            let phase = (elapsed / t.duration).clamp(0., 1.);
            let value = sample(&t.curve, phase);
            let mut view = t.target.clone();
            for channel in &t.channels {
                let path = format!("/{}", channel.path.join("/"));
                if let Some(slot) = view.pointer_mut(&path) {
                    let v = (channel.from + (channel.to - channel.from) * f64::from(value))
                        .clamp(channel.minimum, channel.maximum);
                    *slot = if channel.integral {
                        serde_json::json!(v.round() as i64)
                    } else {
                        serde_json::json!(v)
                    };
                }
            }
            style.view = serde_json::from_value(view).ok();
            finished = phase >= 1.;
        }
        if finished {
            imp.transition.borrow_mut().take();
        }
        let mut motion = [0., 1., 1., 0.];
        let mut animation_done = false;
        if let Some(a) = imp.animation.borrow().as_ref() {
            let elapsed = (a.start.elapsed().as_secs_f32() * 1000. - a.spec.delay).max(0.);
            let cycles = elapsed / a.spec.duration;
            animation_done = a.spec.iterations.is_some_and(|n| cycles >= n as f32);
            if !animation_done {
                let phase = cycles.fract();
                for i in 0..4 {
                    motion[i] = f64::from(sample_track(&a.curve, phase, i));
                }
            }
        }
        if animation_done {
            imp.animation.borrow_mut().take();
        }
        if let Some(css) = imp.css.borrow().as_ref() {
            css.load_from_string(&super::style::stylesheet(&style));
        }
        *imp.style.borrow_mut() = style.clone();
        let view = style.view.clone().unwrap_or_default();
        let size = imp.size.get();
        let projected = surface_transform(&view, size, motion);
        self.set_visible(
            !view.hidden
                && !(view.backface_hidden && projected.as_ref().is_some_and(|t| t.backface)),
        );
        self.set_opacity((f64::from(view.opacity) * motion[2]).clamp(0., 1.));
        self.set_cursor_from_name(cursor(view.cursor));
        if let Some(parent) = self.parent().and_downcast::<gtk::Fixed>() {
            let [x, y] = imp.position.get();
            let local = projected
                .map(|t| projection_matrix(t.matrix))
                .unwrap_or_else(|| projection_matrix([1., 0., 0., 0., 1., 0., 0., 0., 1.]));
            let transform = gsk::Transform::new()
                .translate(&graphene::Point::new(x as f32, y as f32))
                .matrix(&local);
            parent.set_child_transform(self, Some(&transform));
        }
        self.queue_draw();
        imp.transition.borrow().is_some() || imp.animation.borrow().is_some()
    }
}

fn cursor(value: ViewCursor) -> Option<&'static str> {
    Some(match value {
        ViewCursor::Auto => return None,
        ViewCursor::Default => "default",
        ViewCursor::Pointer => "pointer",
        ViewCursor::Text => "text",
        ViewCursor::Crosshair => "crosshair",
        ViewCursor::Grab => "grab",
        ViewCursor::Grabbing => "grabbing",
        ViewCursor::Wait => "wait",
        ViewCursor::Progress => "progress",
        ViewCursor::Help => "help",
        ViewCursor::Move => "move",
        ViewCursor::ZoomIn => "zoom-in",
        ViewCursor::ZoomOut => "zoom-out",
        ViewCursor::NotAllowed => "not-allowed",
        ViewCursor::ResizeHorizontal => "ew-resize",
        ViewCursor::ResizeVertical => "ns-resize",
        ViewCursor::ResizeNorthEastSouthWest => "nesw-resize",
        ViewCursor::ResizeNorthWestSouthEast => "nwse-resize",
        ViewCursor::None => "none",
    })
}

fn sample(values: &[f32], phase: f32) -> f32 {
    let x = phase * (values.len() - 1) as f32;
    let i = (x.floor() as usize).min(values.len() - 1);
    values[i] + (values[(i + 1).min(values.len() - 1)] - values[i]) * (x - i as f32)
}
fn sample_track(values: &[[f32; 4]], phase: f32, track: usize) -> f32 {
    let x = phase * (values.len() - 1) as f32;
    let i = (x.floor() as usize).min(values.len() - 1);
    values[i][track]
        + (values[(i + 1).min(values.len() - 1)][track] - values[i][track]) * (x - i as f32)
}

pub(super) fn color(v: [u8; 4]) -> gdk::RGBA {
    gdk::RGBA::new(
        v[0] as f32 / 255.,
        v[1] as f32 / 255.,
        v[2] as f32 / 255.,
        v[3] as f32 / 255.,
    )
}
fn rounded(bounds: &graphene::Rect, corners: [f32; 4]) -> gsk::RoundedRect {
    let c = corners.map(|r| graphene::Size::new(r, r));
    gsk::RoundedRect::new(*bounds, c[0], c[1], c[2], c[3])
}
fn projection_values(m: [f64; 9]) -> [f32; 16] {
    [
        m[0] as f32,
        m[3] as f32,
        0.,
        m[6] as f32,
        m[1] as f32,
        m[4] as f32,
        0.,
        m[7] as f32,
        0.,
        0.,
        1.,
        0.,
        m[2] as f32,
        m[5] as f32,
        0.,
        m[8] as f32,
    ]
}
fn projection_matrix(m: [f64; 9]) -> graphene::Matrix {
    graphene::Matrix::from_float(projection_values(m))
}
fn matrix_values(m: [f32; 20]) -> [f32; 16] {
    std::array::from_fn(|i| m[(i % 4) * 5 + i / 4])
}
fn filters(mut node: gsk::RenderNode, list: &[ViewFilter]) -> gsk::RenderNode {
    for filter in list {
        node = match *filter {
            ViewFilter::Blur(radius) if radius > 0. => gsk::BlurNode::new(&node, radius).upcast(),
            ViewFilter::DropShadow(s) => gsk::ShadowNode::new(
                &node,
                &[gsk::Shadow::new(
                    color(s.color),
                    s.offset[0],
                    s.offset[1],
                    s.blur,
                )],
            )
            .upcast(),
            _ => {
                if let Some(m) = filter.color_matrix() {
                    gsk::ColorMatrixNode::new(
                        &node,
                        &graphene::Matrix::from_float(matrix_values(m)),
                        &graphene::Vec4::new(m[4], m[9], m[14], m[19]),
                    )
                    .upcast()
                } else {
                    node
                }
            }
        };
    }
    node
}

/// Keep software composition and blur intermediates bounded before publication.
pub(super) fn validate(descriptor: &NodeDescriptor) -> Result<(), String> {
    let Some(view) = descriptor.style.view.as_ref() else {
        return Ok(());
    };
    validate_effect_size(view, descriptor.frame.width, descriptor.frame.height)
}

fn validate_effect_size(view: &ViewAppearance, width: f64, height: f64) -> Result<(), String> {
    let offscreen = !view.filters.is_empty()
        || !view.backdrop_filters.is_empty()
        || !view.masks.is_empty()
        || view.blend_mode != BlendMode::Normal;
    if !offscreen {
        return Ok(());
    }
    let padding = view
        .filters
        .iter()
        .chain(&view.backdrop_filters)
        .map(|f| match f {
            ViewFilter::Blur(r) => 3. * f64::from(*r),
            ViewFilter::DropShadow(s) => {
                3. * f64::from(s.blur) + f64::from(s.offset[0].abs().max(s.offset[1].abs()))
            }
            _ => 0.,
        })
        .sum::<f64>();
    let width = width + 2. * padding;
    let height = height + 2. * padding;
    if !width.is_finite()
        || !height.is_finite()
        || width > 4096.
        || height > 4096.
        || width.ceil() * height.ceil() > (4 * 1024 * 1024) as f64
    {
        return Err("Linux visual effect exceeds 4096-edge / 4-Mpixel intermediate budget".into());
    }
    Ok(())
}

fn arithmetic_blend(
    back: &gsk::RenderNode,
    front: &gsk::RenderNode,
    bounds: &graphene::Rect,
    mode: BlendMode,
) -> Result<gsk::RenderNode, String> {
    use gtk::cairo::{Context, Format, ImageSurface};
    let width = bounds.width().ceil() as i32;
    let height = bounds.height().ceil() as i32;
    if width <= 0
        || height <= 0
        || width > 4096
        || height > 4096
        || u64::from(width as u32) * u64::from(height as u32) > 4 * 1024 * 1024
    {
        return Err("blend surface exceeds pixel budget".into());
    }
    let render = |node: &gsk::RenderNode| -> Result<ImageSurface, String> {
        let surface =
            ImageSurface::create(Format::ARgb32, width, height).map_err(|e| e.to_string())?;
        let context = Context::new(&surface).map_err(|e| e.to_string())?;
        context.translate(-f64::from(bounds.x()), -f64::from(bounds.y()));
        node.draw(&context);
        Ok(surface)
    };
    let mut bottom = render(back)?;
    let mut top = render(front)?;
    let top_stride = top.stride() as usize;
    let bottom_stride = bottom.stride() as usize;
    {
        let mut output = bottom.data().map_err(|e| e.to_string())?;
        let source = top.data().map_err(|e| e.to_string())?;
        for y in 0..height as usize {
            for x in 0..width as usize {
                let a = y * bottom_stride + x * 4;
                let b = y * top_stride + x * 4;
                let result = blend_pixel(
                    u32::from_ne_bytes(output[a..a + 4].try_into().unwrap()),
                    u32::from_ne_bytes(source[b..b + 4].try_into().unwrap()),
                    mode,
                );
                output[a..a + 4].copy_from_slice(&result.to_ne_bytes());
            }
        }
    }
    bottom.mark_dirty();
    let node = gsk::CairoNode::new(bounds);
    let context = node.draw_context();
    context
        .set_source_surface(&bottom, f64::from(bounds.x()), f64::from(bounds.y()))
        .map_err(|e| e.to_string())?;
    context.paint().map_err(|e| e.to_string())?;
    Ok(node.upcast())
}

fn blend_pixel(back: u32, front: u32, mode: BlendMode) -> u32 {
    if mode == BlendMode::PlusLighter {
        return (0..4).fold(0, |v, c| {
            v | ((((back >> (c * 8)) & 255) + ((front >> (c * 8)) & 255)).min(255) << (c * 8))
        });
    }
    // Linear burn, matching the shared desktop PlusDarker convention. Work on
    // premultiplied channels so transparent source/backdrop remain well-defined.
    let ab = (back >> 24) as f64 / 255.;
    let af = (front >> 24) as f64 / 255.;
    let alpha = ab + af - ab * af;
    let mut result = ((alpha * 255.).round() as u32) << 24;
    for c in 0..3 {
        let b = ((back >> (c * 8)) & 255) as f64 / 255.;
        let f = ((front >> (c * 8)) & 255) as f64 / 255.;
        let burn = if ab > 0. && af > 0. {
            (b / ab + f / af - 1.).max(0.)
        } else {
            0.
        };
        let value = (b * (1. - af) + f * (1. - ab) + ab * af * burn).clamp(0., alpha);
        result |= ((value * 255.).round() as u32) << (c * 8);
    }
    result
}
fn gradient_node(gradient: &ViewGradient, bounds: &graphene::Rect) -> Option<gsk::RenderNode> {
    let p = gradient.resolve([bounds.width() as f64, bounds.height() as f64])?;
    let stops: Vec<_> = p
        .positions
        .iter()
        .zip(&p.colors)
        .map(|(v, c)| gsk::ColorStop::new(*v as f32, color(*c)))
        .collect();
    let center = graphene::Point::new(p.start[0] as f32, p.start[1] as f32);
    Some(match p.kind {
        "radial" => gsk::RadialGradientNode::new(
            bounds,
            &center,
            p.radii[0] as f32,
            p.radii[1] as f32,
            0.,
            1.,
            &stops,
        )
        .upcast(),
        "conic" => {
            gsk::ConicGradientNode::new(bounds, &center, p.angle.to_degrees() as f32, &stops)
                .upcast()
        }
        _ => gsk::LinearGradientNode::new(
            bounds,
            &center,
            &graphene::Point::new(p.end[0] as f32, p.end[1] as f32),
            &stops,
        )
        .upcast(),
    })
}
fn progressive_node(
    back: &gsk::RenderNode,
    list: &[ViewFilter],
    progressive: ViewProgressiveBlur,
    bounds: &graphene::Rect,
) -> gsk::RenderNode {
    // GSK 4.14 has no additive blend node. Cairo's ADD operation preserves the
    // shared partition-of-unity masks, including translucent backgrounds.
    let composite = gsk::CairoNode::new(bounds);
    let context = composite.draw_context();
    context.set_operator(gtk::cairo::Operator::Add);
    for (strength, positions) in progressive.sample_masks().unwrap_or_default() {
        let stops: Vec<_> = positions
            .iter()
            .map(|p| gsk::ColorStop::new(p[0], gdk::RGBA::new(1., 1., 1., p[1])))
            .collect();
        let mask = gsk::LinearGradientNode::new(
            bounds,
            &graphene::Point::zero(),
            &graphene::Point::new(0., bounds.height()),
            &stops,
        );
        let list: Vec<_> = list.iter().map(|f| f.at_strength(strength)).collect();
        let node =
            gsk::MaskNode::new(filters(back.clone(), &list), &mask, gsk::MaskMode::Alpha).upcast();
        let node: gsk::RenderNode = node;
        node.draw(&context);
    }
    composite.upcast()
}
fn blend(view: &ViewAppearance) -> gsk::BlendMode {
    use gsk::BlendMode::*;
    match serde_json::to_value(view.blend_mode)
        .ok()
        .and_then(|s| s.as_str().map(str::to_owned))
        .as_deref()
    {
        Some("multiply") => Multiply,
        Some("screen") => Screen,
        Some("overlay") => Overlay,
        Some("darken") => Darken,
        Some("lighten") => Lighten,
        Some("color_dodge") => ColorDodge,
        Some("color_burn") => ColorBurn,
        Some("hard_light") => HardLight,
        Some("soft_light") => SoftLight,
        Some("difference") => Difference,
        Some("exclusion") => Exclusion,
        Some("hue") => Hue,
        Some("saturation") => Saturation,
        Some("color") => Color,
        Some("luminosity") => Luminosity,
        _ => Default,
    }
}
fn append_background(snapshot: &gtk::Snapshot, style: &NodeStyle, bounds: &graphene::Rect) {
    let Some(view) = style
        .view
        .as_ref()
        .filter(|view| !view.backgrounds.is_empty())
    else {
        return;
    };
    snapshot.push_rounded_clip(&rounded(bounds, view.corners));
    snapshot.append_color(&color(view.background.unwrap_or(style.background)), bounds);
    // CSS backgrounds put the first layer on top.
    for gradient in view.backgrounds.iter().rev() {
        if let Some(node) = gradient_node(gradient, bounds) {
            snapshot.append_node(&node);
        }
    }
    snapshot.pop();
}

pub(super) fn backdrop(widget: &gtk::Widget) -> Option<gsk::RenderNode> {
    let snapshot = gtk::Snapshot::new();
    let mut ancestors = Vec::new();
    let mut child = widget.clone();
    while let Some(parent) = child.parent() {
        ancestors.push((parent.clone(), child));
        child = parent;
    }
    for (parent, owner) in ancestors.iter().rev() {
        let point = parent.compute_point(widget, &graphene::Point::zero())?;
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
        if let Some(surface) = parent.downcast_ref::<Surface>() {
            append_background(
                &snapshot,
                &surface.imp().style.borrow(),
                &graphene::Rect::new(0., 0., parent.width() as f32, parent.height() as f32),
            );
        }
        let mut sibling = parent.first_child();
        while let Some(w) = sibling {
            if w == *owner {
                break;
            }
            if w.is_visible() {
                parent.snapshot_child(&w, &snapshot);
            }
            sibling = w.next_sibling();
        }
        snapshot.restore();
    }
    snapshot.to_node()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn arithmetic_modes_preserve_transparent_and_premultiplied_pixels() {
        assert_eq!(
            blend_pixel(0xff808080, 0xff808080, BlendMode::PlusLighter),
            0xffffffff
        );
        assert_eq!(
            blend_pixel(0xff808080, 0xff808080, BlendMode::PlusDarker),
            0xff010101
        );
        for mode in [BlendMode::PlusLighter, BlendMode::PlusDarker] {
            assert_eq!(blend_pixel(0, 0x80402010, mode), 0x80402010);
            assert_eq!(blend_pixel(0x80402010, 0, mode), 0x80402010);
            let pixel = blend_pixel(0x80804020, 0x40402010, mode);
            for shift in [0, 8, 16] {
                assert!(((pixel >> shift) & 255) <= pixel >> 24);
            }
        }
    }
    #[test]
    fn offscreen_budget_includes_blur_padding_before_publication() {
        let mut descriptor = NodeDescriptor {
            content: NodeContent::Container,
            frame: Rect {
                x: 0.,
                y: 0.,
                width: 2000.,
                height: 2000.,
            },
            interaction: Default::default(),
            keyboard: Default::default(),
            navigation_keys: vec![],
            accessible_description: None,
            accessible_name: None,
            accessible_role: None,
            accessible_selected: None,
            accessible_expanded: None,
            accessibility: Default::default(),
            accessibility_relations: Default::default(),
            style: NodeStyle {
                view: Some(ViewAppearance {
                    filters: vec![ViewFilter::Blur(1.)],
                    ..Default::default()
                }),
                ..Default::default()
            },
        };
        assert!(validate(&descriptor).is_ok());
        descriptor.style.view.as_mut().unwrap().filters = vec![ViewFilter::Blur(20.)];
        assert!(validate(&descriptor).is_err());
        descriptor.style.view = None;
        descriptor.frame.height = 100_000.;
        assert!(validate(&descriptor).is_ok()); // Long documents do not allocate effect surfaces.
    }
    #[test]
    fn matrices_preserve_perspective_offsets_and_channel_order() {
        let m = [1., 2., 3., 4., 5., 6., 0.001, 0.002, 1.];
        let g = projection_values(m);
        let point = [8., 9., 0., 1.];
        let projected: [f32; 4] =
            std::array::from_fn(|row| (0..4).map(|col| g[col * 4 + row] * point[col]).sum());
        assert!((projected[0] - 29.).abs() < 0.0001);
        assert!((projected[1] - 83.).abs() < 0.0001);
        assert!((projected[3] - 1.026).abs() < 0.0001);
        let filter = ViewFilter::Sepia(1.).color_matrix().unwrap();
        let matrix = matrix_values(filter);
        assert_eq!(matrix[4], filter[1]);
        assert_eq!(matrix[1], filter[5]);
    }
}

//! Cross-frame fingerprint display list: skip collect of clean subtrees.
//!
//! Analog of GPUI `Entity::cached` without Entity: paint-hash the retained tree, replay
//! last frame's ops when a sibling subtree is unchanged. FullClear GPU present still
//! receives a complete list (replay, not omit) — see [`crate::app::damage`].

use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};

use taffy::prelude::NodeId;

use crate::core::tree::{Element, LayoutStatus};
use crate::layout::dimension::{CssAngle, CssDim};
use crate::layout::layout_tree::{
    PaintOp, TextLayoutInfo, ATTR_ESCAPE_OVERFLOW_CLIP, ATTR_HOISTED_PORTAL, PORTAL_LAYER_ID,
};
use crate::renderer::{InstanceInput, TexturedDraw};
use crate::style::{
    BoxShadow, ClipCoord, ClipPath, ColorStop, FilterFunction, GpuClipPoly, Gradient, OffsetPath,
    RadialExtent, TransformFunction, TransformOrigin, TransformOriginX, TransformOriginY,
};

/// Indices into collect buffers at the start of a subtree visit.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct DlCursor {
    pub instances: usize,
    pub textured: usize,
    pub text: usize,
    pub chrome: usize,
    pub clip_polys: usize,
    pub paint_ops: usize,
}

impl DlCursor {
    pub fn capture(
        instances: &[InstanceInput],
        textured: &[TexturedDraw],
        text: &[TextLayoutInfo],
        chrome: &[InstanceInput],
        clip_polys: &[GpuClipPoly],
        paint_ops: &[PaintOp],
    ) -> Self {
        Self {
            instances: instances.len(),
            textured: textured.len(),
            text: text.len(),
            chrome: chrome.len(),
            clip_polys: clip_polys.len(),
            paint_ops: paint_ops.len(),
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct GeomKey {
    abs: [u32; 2],
    size: [u32; 2],
    scroll: [u32; 2],
    clip: [u32; 4],
    has_clip: bool,
    opacity: u32,
    scale: [u32; 2],
    /// Ancestor 2×2 (`a,b,c,d`). Centered children keep the same Taffy origin when
    /// a parent rotates; skip must still bust so the VS linear is not the rest pose.
    linear: [u32; 4],
}

impl GeomKey {
    fn new(
        abs: [f32; 2],
        size: [f32; 2],
        scroll: [f32; 2],
        clip: Option<[f32; 4]>,
        opacity: f32,
        scale: [f32; 2],
        linear: &crate::style::Mat2d,
    ) -> Self {
        let (has_clip, clip_bits) = match clip {
            Some(c) => (
                true,
                [
                    c[0].to_bits(),
                    c[1].to_bits(),
                    c[2].to_bits(),
                    c[3].to_bits(),
                ],
            ),
            None => (false, [0; 4]),
        };
        Self {
            abs: [abs[0].to_bits(), abs[1].to_bits()],
            size: [size[0].to_bits(), size[1].to_bits()],
            scroll: [scroll[0].to_bits(), scroll[1].to_bits()],
            clip: clip_bits,
            has_clip,
            opacity: opacity.to_bits(),
            scale: [scale[0].to_bits(), scale[1].to_bits()],
            linear: [
                linear[0].to_bits(),
                linear[1].to_bits(),
                linear[2].to_bits(),
                linear[3].to_bits(),
            ],
        }
    }
}

impl Hash for GeomKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.abs.hash(state);
        self.size.hash(state);
        self.scroll.hash(state);
        self.has_clip.hash(state);
        self.clip.hash(state);
        self.opacity.hash(state);
        self.scale.hash(state);
        self.linear.hash(state);
    }
}

pub(crate) struct CachedSubtree {
    paint_hash: u64,
    geom: GeomKey,
    instances: Vec<InstanceInput>,
    textured: Vec<TexturedDraw>,
    text: Vec<TextLayoutInfo>,
    chrome: Vec<InstanceInput>,
    clip_polys: Vec<GpuClipPoly>,
    paint_ops: Vec<PaintOp>,
    extent: [f32; 2],
}

#[derive(Default)]
pub(crate) struct CollectSkipState {
    /// When false (scroll / cold FullClear), never replay and do not clone the DL.
    pub allowed: bool,
    prev: HashMap<NodeId, CachedSubtree>,
    next: HashMap<NodeId, CachedSubtree>,
    pub skipped: u32,
    pub patched: u32,
}

impl CollectSkipState {
    pub fn begin_frame(&mut self) {
        self.next.clear();
        self.skipped = 0;
        self.patched = 0;
    }

    /// Drop fingerprint entries for a Taffy id (node recycled after unmount).
    pub fn evict(&mut self, node: NodeId) {
        self.prev.remove(&node);
        self.next.remove(&node);
    }

    /// Full Taffy rebuild may reuse `NodeId`s — drop fingerprint entries.
    pub fn invalidate(&mut self) {
        self.prev.clear();
        self.next.clear();
        self.skipped = 0;
        self.patched = 0;
    }

    pub fn end_frame(&mut self) {
        if self.allowed {
            // Skipped subtrees never `store` into `next`; keep last-frame entries
            // so a later dirty ancestor can still replay a clean descendant.
            for (k, v) in std::mem::take(&mut self.next) {
                self.prev.insert(k, v);
            }
        } else {
            // Layout / scroll / resize: absolute DLs are stale. Keeping `prev` across
            // a skip-off frame is what let maximize replay the pre-grow list.
            self.next.clear();
            self.prev.clear();
        }
    }

    pub fn try_replay(
        &mut self,
        node: NodeId,
        el: &Element,
        geom: GeomKey,
        instances: &mut Vec<InstanceInput>,
        textured: &mut Vec<TexturedDraw>,
        text: &mut Vec<TextLayoutInfo>,
        chrome: &mut Vec<InstanceInput>,
        clip_polys: &mut Vec<GpuClipPoly>,
        paint_ops: &mut Vec<PaintOp>,
    ) -> Option<[f32; 2]> {
        if !self.allowed || !subtree_is_skippable(el) {
            return None;
        }
        let cached = self.prev.get(&node)?;
        if cached.paint_hash != el.collect_paint_hash || !geom_eq(&cached.geom, &geom) {
            if cached.paint_hash != 0 {
                self.patched = self.patched.saturating_add(1);
            }
            return None;
        }
        splice_cached(
            cached, instances, textured, text, chrome, clip_polys, paint_ops,
        );
        self.skipped = self.skipped.saturating_add(1);
        Some(cached.extent)
    }

    pub fn store(
        &mut self,
        node: NodeId,
        el: &Element,
        geom: GeomKey,
        start: DlCursor,
        instances: &[InstanceInput],
        textured: &[TexturedDraw],
        text: &[TextLayoutInfo],
        chrome: &[InstanceInput],
        clip_polys: &[GpuClipPoly],
        paint_ops: &[PaintOp],
        extent: [f32; 2],
    ) {
        if !self.allowed {
            return;
        }
        if !subtree_is_skippable(el) {
            return;
        }
        let cached = capture_slice(
            el.collect_paint_hash,
            geom,
            start,
            instances,
            textured,
            text,
            chrome,
            clip_polys,
            paint_ops,
            extent,
        );
        self.next.insert(node, cached);
    }
}

fn geom_eq(a: &GeomKey, b: &GeomKey) -> bool {
    a.abs == b.abs
        && a.size == b.size
        && a.scroll == b.scroll
        && a.has_clip == b.has_clip
        && a.clip == b.clip
        && a.opacity == b.opacity
        && a.scale == b.scale
        && a.linear == b.linear
}

/// Fingerprint of a subtree's **paint** geometry.
///
/// `abs` is the paint origin (`parent abs_offset + taffy.location`), not the
/// parent origin alone — a flex Footer keeps the same size while `location.y`
/// moves on maximize.
pub(crate) fn geom_key(
    abs: [f32; 2],
    size: [f32; 2],
    scroll: [f32; 2],
    clip: Option<[f32; 4]>,
    opacity: f32,
    scale: [f32; 2],
    linear: crate::style::Mat2d,
) -> GeomKey {
    GeomKey::new(abs, size, scroll, clip, opacity, scale, &linear)
}

/// Collect walk has side channels (portal `overlay_split`, in-page
/// `page_stack_bands`, drag overlay) that are not stored in the fingerprint DL.
/// Skipping an **ancestor** splices rects/glyphs but never records those cursors
/// — present then draws all rects before all text (glyph bleed through the top
/// card / empty overlay suffix).
fn node_has_collect_side_channel(el: &Element) -> bool {
    el.template_id.as_deref() == Some(PORTAL_LAYER_ID)
        || el.attrs.get(ATTR_HOISTED_PORTAL).map(String::as_str) == Some("1")
        || el.attrs.get("rui_portal").map(String::as_str) == Some("1")
        || el.attrs.contains_key(crate::utils::drag::ATTR_DRAG_OVERLAY)
        || crate::utils::anchored_overlay::point_overlay_drag_collect_live(el)
        || node_records_page_stack_bands(el)
}

/// Same opt-in as [`crate::layout::layout_tree`] `collect_begin_walk`: outermost
/// `data-stacking-context=1` or any direct child `z-index != 0` records
/// `page_stack_bands`. Nested z-index without the attr does not (MessageScroller).
/// Stamping those parents live still lets clean **siblings** fingerprint-replay.
fn node_records_page_stack_bands(el: &Element) -> bool {
    if el.attrs.get("data-stacking-context").map(String::as_str) == Some("1") {
        return true;
    }
    el.children.iter().any(|c| c.style.z_index != 0)
}

/// This node opens a later page-stack band (`layout_tree` `child_opens_page_stack_band`).
/// Skipping it records one parent NodePaint and drops descendant slots that
/// dest-reset / scroll-blit pins key by (absolute jump chrome).
fn node_opens_stacking_band(el: &Element) -> bool {
    el.style.z_index != 0
        || el.style.position == taffy::style::Position::Absolute
        || el.style.position_fixed
        || el.attrs.get("data-stacking-context").map(String::as_str) == Some("1")
}

pub(crate) fn subtree_is_skippable(el: &Element) -> bool {
    if el.children.is_empty() {
        return false;
    }
    if el.collect_skip_live {
        return false;
    }
    if node_has_collect_side_channel(el) {
        return false;
    }
    if node_opens_stacking_band(el) {
        return false;
    }
    true
}

fn node_collect_is_live(el: &Element) -> bool {
    // Caret on/off is a global clock, not a hash field. Skip would freeze the
    // last hairline (solid or hidden) until the next typed character.
    if el.text_style.input_caret.is_some() {
        return true;
    }
    if el.attrs.get("rui_animate").is_some_and(|v| {
        matches!(
            v.as_str(),
            "spin" | "ping" | "pulse" | "bounce" | "tw-shimmer"
        )
    }) {
        return true;
    }
    if el
        .attrs
        .get(crate::renderer::ATTR_SHIMMER_BOX)
        .map(String::as_str)
        == Some("1")
    {
        return true;
    }
    if el
        .attrs
        .get(crate::renderer::ATTR_GLYPH_FX)
        .map(String::as_str)
        == Some("shimmer")
    {
        return true;
    }
    if el.attrs.contains_key(crate::utils::ATTR_ANIMATED_ICON) {
        return true;
    }
    #[cfg(feature = "svg")]
    if el.attrs.contains_key(crate::utils::ATTR_MORPH_ICON) {
        return true;
    }
    if crate::style::css_animation_needs_frame(&el.style, &el.animation_snapshot) {
        return true;
    }
    if el
        .attrs
        .contains_key(crate::primitives::shader::ATTR_SHADER)
    {
        return true;
    }
    el.transition_snapshot.has_active_tweens()
}

/// Bottom-up paint hash so a dirty leaf invalidates ancestors but not siblings.
pub(crate) fn stamp_subtree_paint_hashes(root: &mut Element) {
    fn rec(el: &mut Element) -> u64 {
        let mut h = DefaultHasher::new();
        hash_paint_self(el, &mut h);
        // Side-channel nodes stamp `live` so ancestors walk into them. Skipping the
        // root while a portal layer sits underneath parks `overlay_split` at `len`.
        let mut live = node_collect_is_live(el) || node_has_collect_side_channel(el);
        for child in &mut el.children {
            rec(child).hash(&mut h);
            live |= child.collect_skip_live;
        }
        let hash = h.finish();
        el.collect_paint_hash = hash;
        el.collect_skip_live = live;
        hash
    }
    rec(root);
}

fn hash_paint_self(el: &Element, h: &mut impl Hasher) {
    // Do not hash `layout_bounds`: collect stamps them after this pass, so
    // frame N's fingerprint would never match frame N+1. Position/size live in GeomKey.
    hash_f32s(h, &el.scroll_offset);
    el.text.hash(h);
    el.template_id.hash(h);
    el.image_texture.hash(h);
    el.children.len().hash(h);
    hash_f32s(h, &el.style.bg_color);
    hash_f32s(h, &el.style.border_color);
    el.style.border_width.to_bits().hash(h);
    el.style.opacity.to_bits().hash(h);
    // Auto scrollbar cubic is not baked into `bg_color`. Hash the live peek so
    // fingerprint skip busts the rail without disabling collect-skip for the page.
    if let Some(v) = crate::components::scrollbar::auto_scrollbar_live_paint_alpha(el) {
        v.to_bits().hash(h);
    }
    // Overlay thumbs are signal-driven (ViewportsOnly does not write Taffy margin).
    // Hash live offset / metrics or skip replays a frozen rail while the list scrolls.
    if let Some(bits) = crate::components::scrollbar::scrollbar_live_geometry_bits(el) {
        bits.hash(h);
    }
    hash_f32s(h, &el.style.radii);
    for s in el.style.resolved_corner_shapes() {
        std::mem::discriminant(&s).hash(h);
        s.to_shader_f32().to_bits().hash(h);
        s.shader_param().to_bits().hash(h);
    }
    hash_f32s(h, &el.style.shadow_color);
    hash_f32s(h, &el.style.shadow_offset);
    el.style.shadow_blur.to_bits().hash(h);
    el.style.shadow_spread.to_bits().hash(h);
    el.style.shadow_inset.hash(h);
    hash_f32s(h, &el.style.translate);
    // Combined transform samples `translate` px, not `%` of the box. Hashing
    // `translate_pct` here is required: fingerprint runs before collect stamps
    // `layout_bounds`, so the composed matrix would see a 0×0 box.
    hash_f32s(h, &el.style.translate_pct);
    // Independent transform props are sampled every CSS tick. Paint-only skip
    // must bust when they change or idle spin replays the rest-pose DL.
    hash_f32s(h, &el.style.scale);
    el.style.rotate_deg.to_bits().hash(h);
    // `transform-origin` keywords / % resolve against the border box. At
    // fingerprint time `layout_bounds` are often still 0×0, so Left/Center/Right
    // collapse to the same combined matrix. Hash the specified origin, not the
    // resolved px.
    hash_transform_origin(h, &el.style.transform_origin);
    crate::layout::lpa_length_px(el.style.inset.left)
        .unwrap_or(0.0)
        .to_bits()
        .hash(h);
    crate::layout::lpa_length_px(el.style.inset.top)
        .unwrap_or(0.0)
        .to_bits()
        .hash(h);
    el.style.z_index.hash(h);
    // Values, not list shape: bind_prop / CSS ticks can keep `len` / `Some` while
    // changing color, stops, or blur — skip would splice the previous DL.
    for sh in &el.style.extra_shadows {
        hash_box_shadow(h, sh);
    }
    match el.style.bg_gradient.as_ref() {
        Some(g) => {
            true.hash(h);
            hash_gradient(h, g);
        }
        None => false.hash(h),
    }
    hash_f32s(h, &el.style.bg_position);
    el.style.backdrop_blur.to_bits().hash(h);
    match el.style.transform.as_ref() {
        Some(list) => {
            true.hash(h);
            list.len().hash(h);
            for f in list {
                hash_transform_function(h, f);
            }
        }
        None => false.hash(h),
    }
    // CSS `animate-spin` writes the angle into the transform list, not `rotate_deg`.
    // Discriminant-only hashing above would keep the rest-pose fingerprint after maximize.
    let xf = crate::style::compute_combined_transform(
        &el.style.translate,
        el.style.rotate_deg,
        &el.style.scale,
        el.style.transform.as_ref(),
        &el.style.transform_origin,
        el.layout_bounds[2],
        el.layout_bounds[3],
    );
    for v in xf {
        v.to_bits().hash(h);
    }
    match el.style.filter.as_ref() {
        Some(list) => {
            true.hash(h);
            hash_filter_list(h, list);
        }
        None => false.hash(h),
    }
    hash_clip_path(h, &el.style.clip_path);
    el.style.clip_path_name.hash(h);
    hash_f32s(h, &el.style.outline_color);
    el.style.outline_width.to_bits().hash(h);
    el.style.outline_offset.to_bits().hash(h);
    std::mem::discriminant(&el.style.outline_style).hash(h);
    for c in el.style.resolved_border_colors() {
        hash_f32s(h, &c);
    }
    for w in el.style.resolved_border_widths() {
        w.to_bits().hash(h);
    }
    for s in el.style.resolved_border_styles() {
        std::mem::discriminant(&s).hash(h);
    }
    el.style.border_image_source.hash(h);
    hash_f32s(h, &el.style.border_image_slice);
    el.style.border_image_slice_fill.hash(h);
    match el.style.border_image_width {
        Some(w) => {
            true.hash(h);
            hash_f32s(h, &w);
        }
        None => false.hash(h),
    }
    hash_f32s(h, &el.style.border_image_outset);
    for r in &el.style.border_image_repeat {
        std::mem::discriminant(r).hash(h);
    }
    el.text_style.letter_spacing.to_bits().hash(h);
    el.text_style.underline.hash(h);
    el.text_style.overline.hash(h);
    el.text_style.line_through.hash(h);
    el.text_style.decoration_color.hash(h);
    el.text_style
        .decoration_thickness
        .map(|t| t.to_bits())
        .hash(h);
    el.text_style.underline_offset.map(|t| t.to_bits()).hash(h);
    match el.text_style.decoration_style {
        Some(s) => {
            true.hash(h);
            std::mem::discriminant(&s).hash(h);
        }
        None => false.hash(h),
    }
    el.text_style.shadow_color.hash(h);
    hash_f32s(h, &el.text_style.shadow_offset);
    el.text_style.shadow_blur.to_bits().hash(h);
    if let Some(v) = el.attrs.get("rui_animate") {
        v.hash(h);
    }
    el.text_style.color.hash(h);
    el.text_style.size.map(|s| s.to_bits()).hash(h);
    #[cfg(feature = "svg")]
    crate::utils::morph_icon::hash_morph_icon_paint(el, h);
    el.text_style.weight.hash(h);
    el.text_style.font_family.hash(h);
    match el.text_style.align {
        Some(a) => {
            true.hash(h);
            std::mem::discriminant(&a).hash(h);
        }
        None => false.hash(h),
    }
    std::mem::discriminant(&el.text_style.white_space).hash(h);
    std::mem::discriminant(&el.text_style.wrap_mode).hash(h);
    std::mem::discriminant(&el.text_style.overflow).hash(h);
    std::mem::discriminant(&el.text_style.direction).hash(h);
    el.text_style.line_height_px.map(|v| v.to_bits()).hash(h);
    el.text_style.line_height_scale.map(|v| v.to_bits()).hash(h);
    std::mem::discriminant(&el.style.writing_mode).hash(h);
    // Focus / caret / selection are paint-only. Omitting them lets a parent
    // replay the unfocused DL: dest-reset wipes the field and the hairline
    // never comes back (Gallery Kbd Search).
    match el.text_style.input_caret.as_ref() {
        Some(c) => {
            true.hash(h);
            c.caret_byte_index.hash(h);
            c.selection.hash(h);
            c.extra_carets.hash(h);
            c.extra_selections.hash(h);
            c.overwrite_highlight.hash(h);
            c.preedit_underline.hash(h);
            c.extra_preedit_underlines.hash(h);
            c.drop_caret.hash(h);
            c.caret_color.hash(h);
            c.selection_bg.hash(h);
            c.selection_foreground.hash(h);
        }
        None => false.hash(h),
    }
    el.text_style.scroll_x.to_bits().hash(h);
    // Motion path is sampled at paint (`offset_path::sample`). Animation writes
    // `offset_distance` without LayoutChanged; skip would splice the rest pose.
    match &el.style.offset_path {
        OffsetPath::None => 0u8.hash(h),
        OffsetPath::Path(s) => {
            1u8.hash(h);
            s.hash(h);
        }
        OffsetPath::Url(s) => {
            2u8.hash(h);
            s.hash(h);
        }
    }
    el.style.offset_distance.to_bits().hash(h);
    match el.style.offset_rotate_deg {
        Some(d) => {
            true.hash(h);
            d.to_bits().hash(h);
        }
        None => false.hash(h),
    }
    if let Some(class) = el.attrs.get("class").or_else(|| el.attrs.get("className")) {
        class.hash(h);
    }
    // Overflow clip is baked into instance `clip_*` at collect (layout border box,
    // not rotated vis). Skipping a parent after visible→hidden replays unclipped
    // children. Discriminant only: Visible vs Hidden vs Scroll, not Taffy geometry.
    std::mem::discriminant(&el.style.overflow.x).hash(h);
    std::mem::discriminant(&el.style.overflow.y).hash(h);
    if let Some(v) = el.attrs.get(ATTR_ESCAPE_OVERFLOW_CLIP) {
        v.hash(h);
    }
}

/// Fingerprint skip is a **paint-only** cache of absolute display-list ops.
///
/// `Clean` / `StyleChanged`: geometry matches the previous frame; replay is valid
/// for subtrees that are not live. `LayoutChanged` / `StructureChanged` (maximize,
/// text-fit settle, wrap) move flex boxes — replaying pre-reflow ops ghosts glyphs
/// and freezes time-sampled paint. Smooth scroll and live resize are the same class.
pub(crate) fn collect_skip_allowed_for_frame(
    layout: LayoutStatus,
    scroll_paint_only: bool,
    full_rebuild: bool,
    viewport_resizing: bool,
    text_settling: bool,
) -> bool {
    matches!(layout, LayoutStatus::Clean | LayoutStatus::StyleChanged)
        && !scroll_paint_only
        && !full_rebuild
        && !viewport_resizing
        && !text_settling
}

fn hash_f32s(h: &mut impl Hasher, xs: &[f32]) {
    for x in xs {
        x.to_bits().hash(h);
    }
}

fn hash_transform_origin(h: &mut impl Hasher, origin: &TransformOrigin) {
    hash_origin_x(h, &origin.x);
    hash_origin_y(h, &origin.y);
    origin.z.to_bits().hash(h);
}

fn hash_transform_function(h: &mut impl Hasher, f: &TransformFunction) {
    std::mem::discriminant(f).hash(h);
    match f {
        TransformFunction::Translate(x, y) => {
            hash_css_dim_paint(h, x);
            hash_css_dim_paint(h, y);
        }
        TransformFunction::TranslateX(x) | TransformFunction::TranslateY(x) => {
            hash_css_dim_paint(h, x)
        }
        TransformFunction::TranslateZ(d) => hash_css_dim_paint(h, d),
        TransformFunction::Translate3d(x, y, z) => {
            hash_css_dim_paint(h, x);
            hash_css_dim_paint(h, y);
            hash_css_dim_paint(h, z);
        }
        TransformFunction::Scale(a, b) => {
            a.to_bits().hash(h);
            b.is_some().hash(h);
            (*b).unwrap_or(*a).to_bits().hash(h);
        }
        TransformFunction::ScaleX(v)
        | TransformFunction::ScaleY(v)
        | TransformFunction::ScaleZ(v) => {
            v.to_bits().hash(h);
        }
        TransformFunction::Scale3d(x, y, z) => {
            x.to_bits().hash(h);
            y.to_bits().hash(h);
            z.to_bits().hash(h);
        }
        TransformFunction::Rotate(a)
        | TransformFunction::SkewX(a)
        | TransformFunction::SkewY(a)
        | TransformFunction::RotateX(a)
        | TransformFunction::RotateY(a)
        | TransformFunction::RotateZ(a) => hash_css_angle(h, a),
        TransformFunction::Skew(a, b) => {
            hash_css_angle(h, a);
            b.is_some().hash(h);
            if let Some(b) = b {
                hash_css_angle(h, b);
            }
        }
        TransformFunction::Rotate3d(x, y, z, a) => {
            x.to_bits().hash(h);
            y.to_bits().hash(h);
            z.to_bits().hash(h);
            hash_css_angle(h, a);
        }
        TransformFunction::Matrix(m) => hash_f32s(h, m),
        TransformFunction::Matrix3d(m) => hash_f32s(h, m),
        TransformFunction::Perspective(d) => hash_css_dim_paint(h, d),
    }
}

fn hash_origin_x(h: &mut impl Hasher, ox: &TransformOriginX) {
    match ox {
        TransformOriginX::Left => 0u8.hash(h),
        TransformOriginX::Center => 1u8.hash(h),
        TransformOriginX::Right => 2u8.hash(h),
        TransformOriginX::Length(d) => {
            3u8.hash(h);
            hash_css_dim_paint(h, d);
        }
    }
}

fn hash_origin_y(h: &mut impl Hasher, oy: &TransformOriginY) {
    match oy {
        TransformOriginY::Top => 0u8.hash(h),
        TransformOriginY::Center => 1u8.hash(h),
        TransformOriginY::Bottom => 2u8.hash(h),
        TransformOriginY::Length(d) => {
            3u8.hash(h);
            hash_css_dim_paint(h, d);
        }
    }
}

fn hash_css_dim_paint(h: &mut impl Hasher, d: &CssDim) {
    std::mem::discriminant(d).hash(h);
    match d {
        CssDim::Px(v)
        | CssDim::Percent(v)
        | CssDim::Em(v)
        | CssDim::Rem(v)
        | CssDim::Vw(v)
        | CssDim::Vh(v)
        | CssDim::Number(v) => v.to_bits().hash(h),
        CssDim::CalcAdd(a, b)
        | CssDim::CalcSub(a, b)
        | CssDim::CalcMul(a, b)
        | CssDim::CalcDiv(a, b) => {
            hash_css_dim_paint(h, a);
            hash_css_dim_paint(h, b);
        }
        CssDim::CalcSize { basis, calc } => {
            hash_css_dim_paint(h, basis);
            hash_css_dim_paint(h, calc);
        }
        CssDim::Auto
        | CssDim::FitContent
        | CssDim::MinContent
        | CssDim::MaxContent
        | CssDim::Size => {}
    }
}

fn hash_opt_f32(h: &mut impl Hasher, v: &Option<f32>) {
    match v {
        Some(x) => {
            true.hash(h);
            x.to_bits().hash(h);
        }
        None => false.hash(h),
    }
}

fn hash_box_shadow(h: &mut impl Hasher, sh: &BoxShadow) {
    hash_f32s(h, &sh.color);
    hash_f32s(h, &sh.offset);
    sh.blur.to_bits().hash(h);
    sh.spread.to_bits().hash(h);
    sh.inset.hash(h);
}

fn hash_color_stop(h: &mut impl Hasher, s: &ColorStop) {
    hash_f32s(h, &s.color);
    hash_opt_f32(h, &s.position);
    hash_opt_f32(h, &s.position_end);
    s.position_px.hash(h);
    s.position_end_px.hash(h);
}

fn hash_gradient_stops_and_hints(
    h: &mut impl Hasher,
    stops: &[ColorStop],
    hints: &[Option<crate::style::ColorHint>],
    repeating: bool,
) {
    repeating.hash(h);
    stops.len().hash(h);
    for s in stops {
        hash_color_stop(h, s);
    }
    hints.len().hash(h);
    for hint in hints {
        match hint {
            Some(c) => {
                true.hash(h);
                c.position.to_bits().hash(h);
            }
            None => false.hash(h),
        }
    }
}

fn hash_radial_extent(h: &mut impl Hasher, extent: &RadialExtent) {
    std::mem::discriminant(extent).hash(h);
    match *extent {
        RadialExtent::Keyword(k) => {
            std::mem::discriminant(&k).hash(h);
        }
        RadialExtent::Ellipse { rx, ry } => {
            rx.to_bits().hash(h);
            ry.to_bits().hash(h);
        }
        RadialExtent::Circle(r) => r.to_bits().hash(h),
    }
}

fn hash_gradient(h: &mut impl Hasher, g: &Gradient) {
    std::mem::discriminant(g).hash(h);
    match g {
        Gradient::Linear(lg) => {
            lg.angle_deg.to_bits().hash(h);
            hash_gradient_stops_and_hints(h, &lg.stops, &lg.hints, lg.repeating);
        }
        Gradient::Radial(rg) => {
            std::mem::discriminant(&rg.shape).hash(h);
            hash_radial_extent(h, &rg.extent);
            rg.position.x.to_bits().hash(h);
            rg.position.y.to_bits().hash(h);
            hash_gradient_stops_and_hints(h, &rg.stops, &rg.hints, rg.repeating);
        }
        Gradient::Conic(cg) => {
            cg.from_deg.to_bits().hash(h);
            cg.position.x.to_bits().hash(h);
            cg.position.y.to_bits().hash(h);
            hash_gradient_stops_and_hints(h, &cg.stops, &cg.hints, cg.repeating);
        }
    }
}

fn hash_css_dim(h: &mut impl Hasher, d: &CssDim) {
    std::mem::discriminant(d).hash(h);
    match d {
        CssDim::Auto
        | CssDim::FitContent
        | CssDim::MinContent
        | CssDim::MaxContent
        | CssDim::Size => {}
        CssDim::Px(v)
        | CssDim::Percent(v)
        | CssDim::Em(v)
        | CssDim::Rem(v)
        | CssDim::Vw(v)
        | CssDim::Vh(v)
        | CssDim::Number(v) => v.to_bits().hash(h),
        CssDim::CalcAdd(a, b)
        | CssDim::CalcSub(a, b)
        | CssDim::CalcMul(a, b)
        | CssDim::CalcDiv(a, b) => {
            hash_css_dim(h, a);
            hash_css_dim(h, b);
        }
        CssDim::CalcSize { basis, calc } => {
            hash_css_dim(h, basis);
            hash_css_dim(h, calc);
        }
    }
}

fn hash_opt_css_dim(h: &mut impl Hasher, d: &Option<CssDim>) {
    match d {
        Some(v) => {
            true.hash(h);
            hash_css_dim(h, v);
        }
        None => false.hash(h),
    }
}

fn hash_css_angle(h: &mut impl Hasher, a: &CssAngle) {
    std::mem::discriminant(a).hash(h);
    match *a {
        CssAngle::Deg(v) | CssAngle::Rad(v) | CssAngle::Grad(v) | CssAngle::Turn(v) => {
            v.to_bits().hash(h);
        }
    }
}

fn hash_filter_fn(h: &mut impl Hasher, f: &FilterFunction) {
    std::mem::discriminant(f).hash(h);
    match f {
        FilterFunction::Blur(d) => hash_opt_css_dim(h, d),
        FilterFunction::Brightness(v)
        | FilterFunction::Contrast(v)
        | FilterFunction::Grayscale(v)
        | FilterFunction::Invert(v)
        | FilterFunction::Opacity(v)
        | FilterFunction::Saturate(v)
        | FilterFunction::Sepia(v) => hash_opt_f32(h, v),
        FilterFunction::HueRotate(a) => match a {
            Some(ang) => {
                true.hash(h);
                hash_css_angle(h, ang);
            }
            None => false.hash(h),
        },
        FilterFunction::DropShadow {
            color,
            offset_x,
            offset_y,
            blur,
        } => {
            match color {
                Some(c) => {
                    true.hash(h);
                    hash_f32s(h, c);
                }
                None => false.hash(h),
            }
            hash_css_dim(h, offset_x);
            hash_css_dim(h, offset_y);
            hash_opt_css_dim(h, blur);
        }
        FilterFunction::Url(s) => s.hash(h),
    }
}

fn hash_filter_list(h: &mut impl Hasher, list: &[FilterFunction]) {
    list.len().hash(h);
    for f in list {
        hash_filter_fn(h, f);
    }
}

fn hash_clip_coord(h: &mut impl Hasher, c: &ClipCoord) {
    std::mem::discriminant(c).hash(h);
    match *c {
        ClipCoord::Px(v) | ClipCoord::Percent(v) => v.to_bits().hash(h),
    }
}

fn hash_clip_path(h: &mut impl Hasher, path: &ClipPath) {
    std::mem::discriminant(path).hash(h);
    match path {
        ClipPath::None => {}
        ClipPath::Inset { trbl } => {
            for c in trbl {
                hash_clip_coord(h, c);
            }
        }
        ClipPath::Polygon { fill, points } => {
            std::mem::discriminant(fill).hash(h);
            points.len().hash(h);
            for p in points {
                hash_clip_coord(h, &p[0]);
                hash_clip_coord(h, &p[1]);
            }
        }
        ClipPath::Circle { r, cx, cy } => {
            hash_clip_coord(h, r);
            hash_clip_coord(h, cx);
            hash_clip_coord(h, cy);
        }
        ClipPath::Ellipse { rx, ry, cx, cy } => {
            hash_clip_coord(h, rx);
            hash_clip_coord(h, ry);
            hash_clip_coord(h, cx);
            hash_clip_coord(h, cy);
        }
        ClipPath::Url(s) => s.hash(h),
    }
}

fn capture_slice(
    paint_hash: u64,
    geom: GeomKey,
    start: DlCursor,
    instances: &[InstanceInput],
    textured: &[TexturedDraw],
    text: &[TextLayoutInfo],
    chrome: &[InstanceInput],
    clip_polys: &[GpuClipPoly],
    paint_ops: &[PaintOp],
    extent: [f32; 2],
) -> CachedSubtree {
    let inst = instances.get(start.instances..).unwrap_or(&[]).to_vec();
    let tex = textured.get(start.textured..).unwrap_or(&[]).to_vec();
    let txt = text.get(start.text..).unwrap_or(&[]).to_vec();
    let chr = chrome.get(start.chrome..).unwrap_or(&[]).to_vec();
    let clips = clip_polys.get(start.clip_polys..).unwrap_or(&[]).to_vec();
    let ops = remap_ops(
        paint_ops.get(start.paint_ops..).unwrap_or(&[]),
        start.instances as u32,
        start.textured as u32,
    );
    CachedSubtree {
        paint_hash,
        geom,
        instances: inst,
        textured: tex,
        text: txt,
        chrome: chr,
        clip_polys: clips,
        paint_ops: ops,
        extent,
    }
}

fn remap_ops(ops: &[PaintOp], inst_base: u32, tex_base: u32) -> Vec<PaintOp> {
    ops.iter()
        .map(|op| match *op {
            PaintOp::Rects { start, count } => PaintOp::Rects {
                start: start.saturating_sub(inst_base),
                count,
            },
            PaintOp::Textured { start, count } => PaintOp::Textured {
                start: start.saturating_sub(tex_base),
                count,
            },
        })
        .collect()
}

fn splice_cached(
    cached: &CachedSubtree,
    instances: &mut Vec<InstanceInput>,
    textured: &mut Vec<TexturedDraw>,
    text: &mut Vec<TextLayoutInfo>,
    chrome: &mut Vec<InstanceInput>,
    clip_polys: &mut Vec<GpuClipPoly>,
    paint_ops: &mut Vec<PaintOp>,
) {
    let inst_base = instances.len() as u32;
    let tex_base = textured.len() as u32;
    instances.extend_from_slice(&cached.instances);
    textured.extend_from_slice(&cached.textured);
    text.extend_from_slice(&cached.text);
    chrome.extend_from_slice(&cached.chrome);
    clip_polys.extend_from_slice(&cached.clip_polys);
    while clip_polys.len() < instances.len() {
        clip_polys.push(GpuClipPoly::NONE);
    }
    for op in &cached.paint_ops {
        match *op {
            PaintOp::Rects { start, count } => {
                paint_ops.push(PaintOp::Rects {
                    start: start + inst_base,
                    count,
                });
            }
            PaintOp::Textured { start, count } => {
                paint_ops.push(PaintOp::Textured {
                    start: start + tex_base,
                    count,
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::drag::ATTR_DRAG_OVERLAY;

    fn el_with_child() -> Element {
        let mut el = Element::new();
        el.children.push(Element::new().text("x"));
        el
    }

    #[test]
    fn portal_layer_is_not_skippable() {
        let mut el = el_with_child();
        el.template_id = Some(PORTAL_LAYER_ID.to_string());
        assert!(!subtree_is_skippable(&el));
    }

    #[test]
    fn hoisted_portal_is_not_skippable() {
        let mut el = el_with_child();
        el.set_attr(ATTR_HOISTED_PORTAL, "1");
        assert!(!subtree_is_skippable(&el));
    }

    #[test]
    fn rui_portal_is_not_skippable() {
        let mut el = el_with_child();
        el.set_attr("rui_portal", "1");
        assert!(!subtree_is_skippable(&el));
    }

    #[test]
    fn drag_overlay_is_not_skippable() {
        let mut el = el_with_child();
        el.set_attr(ATTR_DRAG_OVERLAY, "1");
        assert!(!subtree_is_skippable(&el));
    }

    #[test]
    fn ordinary_subtree_with_children_is_skippable() {
        assert!(subtree_is_skippable(&el_with_child()));
    }

    #[test]
    fn stacking_band_node_is_not_skippable() {
        let mut abs = el_with_child();
        abs.style.position = taffy::style::Position::Absolute;
        assert!(
            !subtree_is_skippable(&abs),
            "absolute dock skip would drop descendant dest-reset slots"
        );
        let mut zed = el_with_child();
        zed.style.z_index = 20;
        assert!(!subtree_is_skippable(&zed));
    }

    #[test]
    fn empty_children_is_not_skippable() {
        assert!(!subtree_is_skippable(&Element::new()));
    }

    #[test]
    fn geom_key_includes_opacity() {
        let a = geom_key(
            [0.0, 0.0],
            [10.0, 10.0],
            [0.0, 0.0],
            None,
            1.0,
            [1.0, 1.0],
            crate::style::MAT2D_IDENTITY,
        );
        let b = geom_key(
            [0.0, 0.0],
            [10.0, 10.0],
            [0.0, 0.0],
            None,
            0.4,
            [1.0, 1.0],
            crate::style::MAT2D_IDENTITY,
        );
        let c = geom_key(
            [0.0, 0.0],
            [10.0, 10.0],
            [0.0, 0.0],
            None,
            1.0,
            [1.0, 1.0],
            crate::style::MAT2D_IDENTITY,
        );
        assert!(
            !geom_eq(&a, &b),
            "opacity change must bust skip geom, not replay the previous fade"
        );
        assert!(geom_eq(&a, &c));
    }

    #[test]
    fn geom_key_includes_ancestor_linear() {
        let id = geom_key(
            [0.0, 0.0],
            [10.0, 10.0],
            [0.0, 0.0],
            None,
            1.0,
            [1.0, 1.0],
            crate::style::MAT2D_IDENTITY,
        );
        let rot = geom_key(
            [0.0, 0.0],
            [10.0, 10.0],
            [0.0, 0.0],
            None,
            1.0,
            [1.0, 1.0],
            crate::style::mat2d_rotate(90.0f32.to_radians()),
        );
        assert!(
            !geom_eq(&id, &rot),
            "ancestor rotate must bust skip geom even when Taffy origin is unchanged"
        );
    }

    fn self_paint_hash(el: &Element) -> u64 {
        let mut h = DefaultHasher::new();
        hash_paint_self(el, &mut h);
        h.finish()
    }

    #[test]
    fn paint_hash_includes_extra_shadow_values() {
        let mut el = Element::new().bg([1.0, 0.0, 0.0, 1.0]);
        el.style.extra_shadows.push(BoxShadow {
            color: [1.0, 0.0, 0.0, 0.8],
            offset: [2.0, 2.0],
            blur: 4.0,
            spread: 0.0,
            inset: false,
        });
        let before = self_paint_hash(&el);
        el.style.extra_shadows[0].color = [0.0, 1.0, 0.0, 0.8];
        assert_ne!(
            before,
            self_paint_hash(&el),
            "same-length extra_shadows color change must bust paint-hash skip"
        );
    }

    #[test]
    fn paint_hash_includes_gradient_stop_values() {
        use crate::style::LinearGradient;
        let mut el = Element::new();
        el.style.bg_gradient = Some(Gradient::Linear(LinearGradient::two_stop(
            [1.0, 0.0, 0.0, 1.0],
            [0.0, 0.0, 0.0, 1.0],
            90.0,
        )));
        let before = self_paint_hash(&el);
        if let Some(Gradient::Linear(g)) = el.style.bg_gradient.as_mut() {
            g.stops[0].color = [0.0, 0.0, 1.0, 1.0];
        }
        assert_ne!(
            before,
            self_paint_hash(&el),
            "same-shape gradient stop color must bust paint-hash skip"
        );
    }

    #[test]
    fn paint_hash_includes_filter_blur_amount() {
        let mut el = Element::new().bg([1.0, 0.0, 0.0, 1.0]);
        el.style.filter = crate::style::parse_filter("blur(4px)");
        let before = self_paint_hash(&el);
        el.style.filter = crate::style::parse_filter("blur(12px)");
        assert_ne!(
            before,
            self_paint_hash(&el),
            "filter blur amount (not just discriminant) must bust paint-hash skip"
        );
    }

    #[test]
    fn paint_hash_includes_backdrop_blur() {
        let mut el = Element::new().bg([1.0, 0.0, 0.0, 1.0]);
        el.style.backdrop_blur = 0.0;
        let before = self_paint_hash(&el);
        el.style.backdrop_blur = 8.0;
        assert_ne!(
            before,
            self_paint_hash(&el),
            "backdrop_blur must bust paint-hash skip"
        );
    }

    #[test]
    fn paint_hash_includes_clip_path_values() {
        let mut el = Element::new().bg([1.0, 0.0, 0.0, 1.0]);
        el.style.clip_path = crate::style::parse_clip_path("circle(40% at 50% 50%)").unwrap();
        let before = self_paint_hash(&el);
        el.style.clip_path = crate::style::parse_clip_path("circle(20% at 50% 50%)").unwrap();
        assert_ne!(
            before,
            self_paint_hash(&el),
            "clip-path circle radius must bust paint-hash skip"
        );
    }

    #[test]
    fn paint_hash_includes_outline_values() {
        use crate::style::LineStyle;
        let mut el = Element::new().bg([1.0, 0.0, 0.0, 1.0]);
        el.style.outline_style = LineStyle::Solid;
        el.style.outline_width = 2.0;
        el.style.outline_color = [0.0, 0.0, 1.0, 1.0];
        let before = self_paint_hash(&el);
        el.style.outline_color = [0.0, 1.0, 0.0, 1.0];
        assert_ne!(
            before,
            self_paint_hash(&el),
            "outline color must bust paint-hash skip"
        );
        let after_color = self_paint_hash(&el);
        el.style.outline_width = 6.0;
        assert_ne!(
            after_color,
            self_paint_hash(&el),
            "outline width must bust paint-hash skip"
        );
    }

    #[test]
    fn paint_hash_includes_per_side_border_color() {
        let mut el = Element::new().bg([1.0, 0.0, 0.0, 1.0]);
        el.style.set_uniform_border_width(2.0);
        el.style.set_border_side_color(0, [0.0, 0.0, 1.0, 1.0]);
        let before = self_paint_hash(&el);
        el.style.set_border_side_color(0, [0.0, 1.0, 0.0, 1.0]);
        assert_ne!(
            before,
            self_paint_hash(&el),
            "per-side border color must bust paint-hash skip"
        );
    }

    #[test]
    fn paint_hash_includes_border_image_and_text_chrome() {
        let mut el = Element::new().bg([1.0, 0.0, 0.0, 1.0]).text("hi");
        el.style.border_image_source = Some("tex:a".into());
        el.text_style.letter_spacing = 0.0;
        el.text_style.underline = Some(false);
        let before = self_paint_hash(&el);
        el.style.border_image_source = Some("tex:b".into());
        assert_ne!(
            before,
            self_paint_hash(&el),
            "border-image source must bust paint-hash skip"
        );
        el.style.border_image_source = Some("tex:a".into());
        assert_eq!(before, self_paint_hash(&el));
        el.text_style.letter_spacing = 2.0;
        assert_ne!(
            before,
            self_paint_hash(&el),
            "letter-spacing must bust paint-hash skip"
        );
        el.text_style.letter_spacing = 0.0;
        el.text_style.underline = Some(true);
        assert_ne!(
            before,
            self_paint_hash(&el),
            "underline must bust paint-hash skip"
        );
    }

    #[test]
    fn paint_hash_includes_corner_shape_values() {
        use crate::style::CornerShape;
        let mut el = Element::new().bg([1.0, 0.0, 0.0, 1.0]);
        el.style.corner_shape = CornerShape::Round;
        let before = self_paint_hash(&el);
        el.style.corner_shape = CornerShape::Notch;
        assert_ne!(
            before,
            self_paint_hash(&el),
            "corner-shape keyword must bust paint-hash skip"
        );
        el.style.corner_shape = CornerShape::Superellipse(2.0);
        let se = self_paint_hash(&el);
        el.style.corner_shape = CornerShape::Superellipse(8.0);
        assert_ne!(
            se,
            self_paint_hash(&el),
            "superellipse exponent (not just discriminant) must bust paint-hash skip"
        );
    }

    #[test]
    fn paint_hash_includes_translate_pct() {
        let mut el = Element::new().bg([1.0, 0.0, 0.0, 1.0]);
        el.style.translate_pct = [0.0, 0.0];
        let before = self_paint_hash(&el);
        el.style.translate_pct = [0.5, 0.0];
        assert_ne!(
            before,
            self_paint_hash(&el),
            "translate_pct must bust paint-hash skip even when layout_bounds are still 0"
        );
    }

    #[test]
    fn paint_hash_includes_transform_origin() {
        let mut el = Element::new().bg([1.0, 0.0, 0.0, 1.0]);
        el.style.rotate_deg = 45.0;
        assert!(
            el.layout_bounds[2] == 0.0 && el.layout_bounds[3] == 0.0,
            "fingerprint-time box must still be 0×0"
        );
        el.style.transform_origin = crate::style::parse_transform_origin("center center");
        let center = self_paint_hash(&el);
        el.style.transform_origin = crate::style::parse_transform_origin("left top");
        assert_ne!(
            center,
            self_paint_hash(&el),
            "transform-origin keywords must bust paint-hash skip when the combined matrix sees a 0×0 box"
        );
        el.style.transform_origin = crate::style::parse_transform_origin("10px 10px");
        let px = self_paint_hash(&el);
        el.style.transform_origin = crate::style::parse_transform_origin("20px 10px");
        assert_ne!(
            px,
            self_paint_hash(&el),
            "transform-origin length px must bust paint-hash skip (origin_frac collapses px to 0.5)"
        );
    }

    #[test]
    fn paint_hash_includes_transform_list_percent_translate() {
        let mut el = Element::new().bg([1.0, 0.0, 0.0, 1.0]);
        assert!(
            el.layout_bounds[2] == 0.0 && el.layout_bounds[3] == 0.0,
            "fingerprint-time box must still be 0×0"
        );
        el.style.transform = crate::style::parse_transform("translate(50%)");
        let a = self_paint_hash(&el);
        el.style.transform = crate::style::parse_transform("translate(10%)");
        assert_ne!(
            a,
            self_paint_hash(&el),
            "transform list % translate must bust skip when the combined matrix sees a 0×0 box"
        );
    }

    #[test]
    fn paint_hash_includes_text_layout_fields() {
        use crate::style::{TextAlign, TextDirection, TextOverflow, WhiteSpace, WritingMode};
        let mut el = Element::new().text("hi");
        let before = self_paint_hash(&el);
        el.text_style.font_family = Some("Inter".into());
        assert_ne!(
            before,
            self_paint_hash(&el),
            "font_family must bust paint-hash skip"
        );
        el.text_style.font_family = None;
        assert_eq!(before, self_paint_hash(&el));
        el.text_style.align = Some(TextAlign::Center);
        assert_ne!(
            before,
            self_paint_hash(&el),
            "align must bust paint-hash skip"
        );
        el.text_style.align = None;
        el.text_style.white_space = WhiteSpace::Nowrap;
        assert_ne!(
            before,
            self_paint_hash(&el),
            "white_space must bust paint-hash skip"
        );
        el.text_style.white_space = WhiteSpace::Normal;
        el.text_style.wrap_mode = crate::text::WrapMode::Char;
        assert_ne!(
            before,
            self_paint_hash(&el),
            "wrap_mode must bust paint-hash skip"
        );
        el.text_style.wrap_mode = crate::text::WrapMode::Word;
        el.text_style.overflow = TextOverflow::Ellipsis;
        assert_ne!(
            before,
            self_paint_hash(&el),
            "text overflow must bust paint-hash skip"
        );
        el.text_style.overflow = TextOverflow::Clip;
        el.text_style.direction = TextDirection::Rtl;
        assert_ne!(
            before,
            self_paint_hash(&el),
            "direction must bust paint-hash skip"
        );
        el.text_style.direction = TextDirection::Auto;
        el.text_style.line_height_px = Some(20.0);
        assert_ne!(
            before,
            self_paint_hash(&el),
            "line_height_px must bust paint-hash skip"
        );
        el.text_style.line_height_px = None;
        el.text_style.line_height_scale = Some(1.5);
        assert_ne!(
            before,
            self_paint_hash(&el),
            "line_height_scale must bust paint-hash skip"
        );
        el.text_style.line_height_scale = None;
        el.style.writing_mode = WritingMode::VerticalRl;
        assert_ne!(
            before,
            self_paint_hash(&el),
            "writing_mode must bust paint-hash skip"
        );
    }

    #[test]
    fn paint_hash_includes_offset_distance() {
        let mut el = Element::new().bg([1.0, 0.0, 0.0, 1.0]);
        el.style.offset_path = crate::style::parse_offset_path(r#"path("M 0 0 L 40 0")"#).unwrap();
        el.style.offset_distance = 0.0;
        let before = self_paint_hash(&el);
        el.style.offset_distance = 1.0;
        assert_ne!(
            before,
            self_paint_hash(&el),
            "offset_distance must bust paint-hash skip (paint-only motion path)"
        );
        el.style.offset_distance = 0.0;
        el.style.offset_rotate_deg = Some(15.0);
        assert_ne!(
            before,
            self_paint_hash(&el),
            "offset_rotate_deg must bust paint-hash skip"
        );
        el.style.offset_rotate_deg = None;
        el.style.offset_path = crate::style::parse_offset_path(r#"path("M 0 0 L 80 0")"#).unwrap();
        assert_ne!(
            before,
            self_paint_hash(&el),
            "offset_path d-string must bust paint-hash skip"
        );
    }

    #[test]
    fn paint_hash_includes_overflow_clip_mode() {
        let mut el = Element::new().bg([1.0, 0.0, 0.0, 1.0]);
        el.style.overflow.y = taffy::style::Overflow::Visible;
        let before = self_paint_hash(&el);
        el.style.overflow.y = taffy::style::Overflow::Hidden;
        assert_ne!(
            before,
            self_paint_hash(&el),
            "overflow:hidden must bust paint-hash skip (instance clip is a collect side effect)"
        );
        el.style.overflow.y = taffy::style::Overflow::Visible;
        el.set_attr(crate::layout::layout_tree::ATTR_ESCAPE_OVERFLOW_CLIP, "1");
        assert_ne!(
            before,
            self_paint_hash(&el),
            "rui_escape_overflow_clip must bust paint-hash skip"
        );
    }
}

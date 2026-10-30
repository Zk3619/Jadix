use super::*;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CarouselWheelMode {
    #[default]
    Discrete,
    Continuous,
}

impl CarouselWheelMode {
    pub fn parse(value: &str) -> Self {
        if value.eq_ignore_ascii_case("continuous") {
            Self::Continuous
        } else {
            Self::Discrete
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct CarouselWheelOptions {
    pub enabled: bool,
    pub mode: CarouselWheelMode,
    pub interval_ms: u64,
    pub speed: f32,
}

impl Default for CarouselWheelOptions {
    fn default() -> Self {
        Self {
            enabled: false,
            mode: CarouselWheelMode::Discrete,
            interval_ms: 180,
            speed: 1.0,
        }
    }
}

pub(super) fn release_free_offset(ctx: &CarouselCtx) {
    use_signal(&format!("{}__wheel_free", ctx.id), || false).set_silent(false);
}

pub(super) fn owns_offset(ctx: &CarouselCtx) -> bool {
    use_signal(&format!("{}__wheel_free", ctx.id), || false).peek()
}

fn apply(
    ctx: &CarouselCtx,
    options: CarouselWheelOptions,
    delta: [f32; 2],
    last: &mut Option<(Instant, bool)>,
    now: Instant,
) -> bool {
    let amount =
        if ctx.orientation == CarouselOrientation::Vertical || delta[1].abs() > delta[0].abs() {
            delta[1]
        } else {
            delta[0]
        };
    if !options.enabled
        || !amount.is_finite()
        || amount.abs() < 0.001
        || ctx.count.peek() <= 1
        || ctx.viewport_len.peek() <= 1.0
    {
        return false;
    }
    let previous = amount > 0.0;
    if options.mode == CarouselWheelMode::Discrete {
        if last.is_some_and(|(time, direction)| {
            direction == previous
                && now.saturating_duration_since(time) < Duration::from_millis(options.interval_ms)
        }) {
            return true;
        }
        if previous {
            scroll_prev(ctx);
        } else {
            scroll_next(ctx);
        }
        *last = Some((now, previous));
    } else {
        if loop_anim(&ctx.id).is_some() {
            resolve_loop_anim_keep_offset(&ctx.id, ctx.index);
        }
        let speed = if options.speed.is_finite() {
            options.speed.max(0.0)
        } else {
            1.0
        };
        let raw = offset_target(&ctx.id).peek() + amount * speed;
        let next = if track_loops(ctx) {
            wrap_loop_offset(raw, ctx.count.peek(), loop_clone_count(ctx), step_px(ctx))
        } else if ctx.loop_slides && ctx.variant == CarouselVariant::Stack {
            -(-raw).rem_euclid(ctx.count.peek() as f32 * step_px(ctx))
        } else {
            clamp_offset(ctx, raw)
        };
        use_signal(&format!("{}__wheel_free", ctx.id), || false).set_silent(true);
        let index = if ctx.loop_slides && ctx.variant == CarouselVariant::Stack {
            ((-next / step_px(ctx)).round() as usize) % ctx.count.peek()
        } else {
            index_from_offset(ctx, next)
        };
        ctx.index.set(index);
        snap_offset_to(&ctx.id, next);
        bump_autoplay(ctx);
    }
    true
}

pub(super) fn attach(root: Element, ctx: CarouselCtx, options: CarouselWheelOptions) -> Element {
    if !options.enabled {
        return root;
    }
    let mut last = None;
    root.on_wheel(move |delta| apply(&ctx, options, delta, &mut last, Instant::now()))
}

#[cfg(all(test, feature = "product-tests"))]
#[path = "tests/wheel.rs"]
mod tests;

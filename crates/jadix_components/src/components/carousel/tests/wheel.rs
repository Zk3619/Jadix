use super::*;
use crate::components::{CarouselContent, CarouselContentProps, CarouselItem, CarouselItemProps};

fn context(id: &str) -> (Element, CarouselCtx) {
    let root: Element = carousel(id)
        .loop_slides(false)
        .slide(|| Element::new().h(40.0))
        .slide(|| Element::new().h(40.0))
        .slide(|| Element::new().h(40.0))
        .into();
    let ctx = use_carousel().unwrap();
    ctx.count.set_silent(3);
    ctx.viewport_len.set_silent(300.0);
    (root, ctx)
}

#[test]
fn discrete_wheel_throttles_same_direction_and_allows_immediate_reversal() {
    crate::core::reactivity::create_root(|| {
        let (_root, ctx) = context("wheel-discrete-test");
        let options = CarouselWheelOptions {
            enabled: true,
            interval_ms: 200,
            ..Default::default()
        };
        let mut last = None;
        let now = Instant::now();
        assert!(apply(&ctx, options, [0.0, -60.0], &mut last, now));
        assert_eq!(ctx.index.peek(), 1);
        assert!(apply(
            &ctx,
            options,
            [0.0, -60.0],
            &mut last,
            now + Duration::from_millis(20)
        ));
        assert_eq!(ctx.index.peek(), 1);
        assert!(apply(
            &ctx,
            options,
            [0.0, 60.0],
            &mut last,
            now + Duration::from_millis(30)
        ));
        assert_eq!(ctx.index.peek(), 0);
        assert!(apply(
            &ctx,
            options,
            [0.0, -60.0],
            &mut last,
            now + Duration::from_millis(250)
        ));
        assert_eq!(ctx.index.peek(), 1);
    });
}

#[test]
fn continuous_wheel_preserves_sub_slide_offsets_speed_and_bounds() {
    crate::core::reactivity::create_root(|| {
        let (_root, ctx) = context("wheel-continuous-test");
        let options = CarouselWheelOptions {
            enabled: true,
            mode: CarouselWheelMode::Continuous,
            speed: 2.0,
            ..Default::default()
        };
        let mut last = None;
        let now = Instant::now();
        apply(&ctx, options, [-15.0, 0.0], &mut last, now);
        assert_eq!(offset_target(&ctx.id).peek(), -30.0);
        auto_sync_offset_if_snapping(&ctx);
        assert_eq!(offset_target(&ctx.id).peek(), -30.0);
        apply(&ctx, options, [0.0, -1000.0], &mut last, now);
        assert_eq!(offset_target(&ctx.id).peek(), -600.0);
        assert_eq!(ctx.index.peek(), 2);
        apply(&ctx, options, [0.0, 1000.0], &mut last, now);
        assert_eq!(offset_target(&ctx.id).peek(), 0.0);
        let mut looping = ctx.clone();
        looping.loop_slides = true;
        looping.seamless = true;
        snap_offset_to(&ctx.id, ideal_offset(&looping));
        apply(&looping, options, [0.0, -10000.0], &mut last, now);
        let offset = offset_target(&ctx.id).peek();
        let upper = -(loop_clone_count(&looping) as f32) * step_px(&looping);
        assert!(offset <= upper && offset > upper - 3.0 * step_px(&looping));
        reset_carousel_ephemeral(&ctx.id);
        assert!(!owns_offset(&ctx));
    });
}

#[test]
fn wheel_is_opt_in_on_builder_and_ui_surfaces() {
    crate::core::reactivity::create_root(|| {
        let disabled: Element = carousel("wheel-off").into();
        assert!(disabled.on_wheel.is_none());
        let enabled: Element = carousel("wheel-builder")
            .wheel(true)
            .wheel_mode(CarouselWheelMode::Continuous)
            .wheel_speed(1.5)
            .into();
        assert!(enabled.on_wheel.is_some());
        let rsx = crate::ui! {
            <Carousel id="wheel-ui" wheel={true} wheelMode="continuous" wheelSpeed={1.5} wheelIntervalMs={200}>
                <CarouselContent><CarouselItem><Box h="40px" /></CarouselItem></CarouselContent>
            </Carousel>
        };
        assert!(rsx.on_wheel.is_some());
    });
}

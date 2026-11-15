use jadix_2d::*;
fn close(a: [f32; 4], b: [f32; 4]) {
    assert!(
        a.iter().zip(b).all(|(a, b)| (a - b).abs() < 0.0001),
        "{a:?} != {b:?}"
    );
}
#[test]
fn mix_respects_alpha_missing_polar_hue_and_weight_normalization() {
    let c = ColorContext2D::default();
    close(
        c.resolve("color-mix(in srgb-linear, red, blue)")
            .unwrap()
            .linear_rgba(),
        [0.5, 0., 0.5, 1.],
    );
    close(
        c.resolve("color-mix(in srgb, red 20%, blue 20%)")
            .unwrap()
            .0
            .components,
        [0.5, 0., 0.5, 0.4],
    );
    close(
        c.resolve("color-mix(in srgb, rgb(255 0 0 / 0), blue)")
            .unwrap()
            .0
            .components,
        [0., 0., 1., 0.5],
    );
    let a = c
        .resolve("color-mix(in oklch longer hue, oklch(0.6 0.1 20), oklch(0.6 0.1 40))")
        .unwrap();
    assert!((a.0.components[2].rem_euclid(360.) - 210.).abs() < 0.0001);
    close(
        c.resolve("color-mix(in srgb, rgb(none 0 0), rgb(255 0 0))")
            .unwrap()
            .0
            .components,
        [1., 0., 0., 1.],
    );
    assert!(c.resolve("color-mix(in unknown, red, blue)").is_err());
    assert!(c.resolve("color-mix(in srgb, red 0%, blue 0%)").is_err());
}
#[test]
fn context_variables_relative_channels_math_and_wide_gamut() {
    let mut c = ColorContext2D {
        current_color: Color2D::parse("color(display-p3 1 0 0 / .8)").unwrap(),
        ..Default::default()
    };
    c.variables.insert("--base".into(), "currentColor".into());
    c.variables.insert("--channels".into(), "255 0 0".into());
    close(
        c.resolve("rgb(var(--channels) / calc(1 / 2))")
            .unwrap()
            .0
            .components,
        [1., 0., 0., 0.5],
    );
    close(
        c.resolve("color(from var(--base) display-p3 calc(r / 2) g b / calc(alpha * .5))")
            .unwrap()
            .0
            .components,
        [0.5, 0., 0., 0.4],
    );
    close(
        c.resolve("color(from color(srgb-linear 2 -.1 .5) srgb-linear r g b)")
            .unwrap()
            .linear_rgba(),
        [2., -0.1, 0.5, 1.],
    );
    close(
        c.resolve("rgb(from red calc(r / 2) g b / 25%)")
            .unwrap()
            .0
            .components,
        [0.5, 0., 0., 0.25],
    );
    close(
        c.resolve("oklch(from oklch(.5 .2 60) calc(l + .1) calc(c / 2) calc(h + 1turn))")
            .unwrap()
            .0
            .components,
        [0.6, 0.1, 420., 1.],
    );
    close(
        c.resolve("lab(calc(25% * 2) calc(20% * 2) calc(-10% * 2))")
            .unwrap()
            .0
            .components,
        [50., 50., -25., 1.],
    );
    close(
        c.resolve("rgb(clamp(0, 300, 255) min(10,20) max(1,2))")
            .unwrap()
            .0
            .components,
        [1., 10. / 255., 2. / 255., 1.],
    );
    assert_eq!(
        c.resolve("var(--unknown, red)").unwrap(),
        Color2D::parse("red").unwrap()
    );
    assert_eq!(
        c.resolve("light-dark(red, blue)").unwrap(),
        Color2D::parse("red").unwrap()
    );
    c.dark = true;
    assert_eq!(
        c.resolve("light-dark(red, blue)").unwrap(),
        Color2D::parse("blue").unwrap()
    );
    c.variables.insert("--unicode".into(), "é".into());
    assert_eq!(c.resolve("var(--unicode)"), Err(CssColorError::Unsupported));
    c.variables
        .insert("--comment".into(), "/* color */ red".into());
    assert_eq!(c.resolve("var(--comment)"), Err(CssColorError::Unsupported));
    c.variables.insert("--cycle".into(), "var(--cycle)".into());
    assert_eq!(c.resolve("var(--cycle)"), Err(CssColorError::Limit));
    for bad in [
        "rgb(calc(1 / 0) 0 0)",
        "rgb(calc(1deg) 0 0)",
        "rgb(r g b)",
        "color(lab calc(1) 0 0)",
        "color-mix(in srgb longer hue,red,blue)",
        "rgb(from red r g)",
        "rgb(1px 0 0)",
        "var(--missing)",
        "red trailing",
        "color(unsupported 1 0 0)",
        "device-cmyk(0 1 1 0)",
    ] {
        assert!(c.resolve(bad).is_err(), "{bad}");
    }
}
#[test]
fn adversarial_inputs_are_bounded_and_never_panic() {
    let c = ColorContext2D::default();
    let mut state = 7u64;
    for n in 0..2000 {
        let text = (0..(n % 96))
            .map(|_| {
                state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
                b"rgb(from calc(var(-+*/.01234% ,)none"
                    [(state >> 32) as usize % b"rgb(from calc(var(-+*/.01234% ,)none".len()]
                    as char
            })
            .collect::<String>();
        let _ = c.resolve(&text);
    }
    assert_eq!(c.resolve(&" ".repeat(4097)), Err(CssColorError::Limit));
}

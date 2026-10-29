#[test]
fn gradient_accepts_functional_colors() {
    use crate::style::gradient::parse_gradient;
    let g =
        parse_gradient("linear-gradient(to right, rgba(255,0,0,0.8), hsl(210 100% 50%), #00ff00)")
            .unwrap();
    assert_eq!(g.stops().len(), 3);

    let g2 = parse_gradient(
            "linear-gradient(90deg, color-mix(in oklab, red, blue), hwb(40 10% 0%), rgb(from pink r g b))",
        )
        .unwrap();
    assert_eq!(g2.stops().len(), 3);
}

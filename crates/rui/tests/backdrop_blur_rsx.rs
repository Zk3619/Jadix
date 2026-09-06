//! RSX unitless lengths are rem; builder numeric lengths are px.

use rui::prelude::*;

#[test]
fn ui_unitless_backdrop_blur_is_rem() {
    let rem = ui! {
        <Box backdrop_blur="1" />
    };
    assert!(
        (rem.style.backdrop_blur - 16.0).abs() < 0.01,
        "ui! backdrop_blur=\"1\" is 1rem = 16px, got {}",
        rem.style.backdrop_blur
    );
    let sixteen = ui! {
        <Box backdrop_blur="16" />
    };
    assert!(
        (sixteen.style.backdrop_blur - 256.0).abs() < 0.01,
        "ui! backdrop_blur=\"16\" is 16rem, got {}",
        sixteen.style.backdrop_blur
    );
    let px = ui! {
        <Box backdrop_blur="16px" />
    };
    assert!((px.style.backdrop_blur - 16.0).abs() < 0.01);
    let named = ui! {
        <Box backdrop_blur="1rem" />
    };
    assert!((named.style.backdrop_blur - 16.0).abs() < 0.01);
}

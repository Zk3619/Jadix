use super::*;

#[test]
fn builder_src_and_style() {
    let el: Element = image()
        .src("assets/photo.jpg")
        .class("rounded-md")
        .style(|b| b.w("8rem").h("8rem"))
        .into();
    assert_eq!(el.image_texture.as_deref().unwrap().contains("photo"), true);
    assert_eq!(
        crate::layout::dim_length_px(el.style.size.width),
        crate::core::tree::IntoCssDim::into_css_dim("8rem")
            .as_immediate_dimension()
            .and_then(crate::layout::dim_length_px)
    );
}

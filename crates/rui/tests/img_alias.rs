//! `<Img>` / `<img>` 宏别名 → `Image`。

use rui::prelude::*;

#[test]
fn img_and_img_lower_tags_attach_texture() {
    let from_img = ui! {
        <Img src="data:image/png;base64,aaaa" w="2rem" h="2rem" />
    };
    let from_lower = ui! {
        <img src="data:image/png;base64,bbbb" w="2rem" h="2rem" />
    };
    let from_image = ui! {
        <Image src="data:image/png;base64,cccc" w="2rem" h="2rem" />
    };
    assert!(from_img.image_texture.is_some());
    assert!(from_lower.image_texture.is_some());
    assert!(from_image.image_texture.is_some());
    assert!(from_img.image_texture.as_ref().unwrap().contains("data:"));
    assert!(from_lower.image_texture.as_ref().unwrap().contains("data:"));
    assert!(from_image.image_texture.as_ref().unwrap().contains("data:"));
}

#[test]
fn img_props_alias_matches_image() {
    let via_alias = Img(ImgProps {
        id: "alias".into(),
        src: Some("data:image/png;base64,dddd".into()),
        texture: None,
        base_builder: None,
        children: None,
        base: None,
    });
    assert!(via_alias.image_texture.is_some());
}

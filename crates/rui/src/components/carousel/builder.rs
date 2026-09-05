//! Carousel product-layer submodule — shares parent symbols via `use super::*`.

use super::content::*;
use super::*;
use crate::components::base::SlotChrome;
use crate::impl_slot_chrome_methods;

// ── Builder ──────────────────────────────────────────────────────────────────

pub struct CarouselSlotBuilder {
    chrome: SlotChrome,
}

fn carousel_slot() -> CarouselSlotBuilder {
    CarouselSlotBuilder {
        chrome: SlotChrome::default(),
    }
}

impl CarouselSlotBuilder {
    impl_slot_chrome_methods!();
}

pub struct CarouselBuilder {
    id: String,
    slides: Vec<Rc<dyn Fn() -> Element>>,
    index: Option<Signal<usize>>,
    orientation: CarouselOrientation,
    loop_slides: bool,
    seamless: Option<bool>,
    snap: bool,
    per_view: f32,
    autoplay_ms: u64,
    controls: CarouselControls,
    scroll_by: CarouselScrollBy,
    variant: CarouselVariant,
    end_align: CarouselEndAlign,
    page_snap: CarouselPageSnap,
    item_sizing: CarouselItemSizing,
    item_width: String,
    dots: Option<bool>,
    stack_sides: u32,
    chrome: SlotChrome,
    content_chrome: SlotChrome,
    item_chrome: SlotChrome,
    previous_chrome: SlotChrome,
    next_chrome: SlotChrome,
    dots_chrome: SlotChrome,
}

/// Builder 入口：默认单页 `Carousel`。
pub fn carousel(id: impl Into<String>) -> CarouselBuilder {
    CarouselBuilder {
        id: id.into(),
        slides: Vec::new(),
        index: None,
        orientation: CarouselOrientation::Horizontal,
        loop_slides: true,
        seamless: None,
        snap: true,
        per_view: 1.0,
        autoplay_ms: 0,
        controls: CarouselControls::Outside,
        scroll_by: CarouselScrollBy::Slide,
        variant: CarouselVariant::Single,
        end_align: CarouselEndAlign::Flush,
        page_snap: CarouselPageSnap::Peek,
        item_sizing: CarouselItemSizing::Fraction,
        item_width: "12rem".into(),
        dots: None,
        stack_sides: 1,
        chrome: SlotChrome::default(),
        content_chrome: SlotChrome::default(),
        item_chrome: SlotChrome::default(),
        previous_chrome: SlotChrome::default(),
        next_chrome: SlotChrome::default(),
        dots_chrome: SlotChrome::default(),
    }
}

/// Single-page full-width Builder (`CarouselSingle` / `variant=single`).
pub fn carousel_single(id: impl Into<String>) -> CarouselBuilder {
    carousel(id).variant(CarouselVariant::Single).per_view(1.0)
}

/// 多列列表 Builder（`CarouselMulti` / `variant=multi`）。
pub fn carousel_multi(id: impl Into<String>) -> CarouselBuilder {
    carousel(id)
        .variant(CarouselVariant::Multi)
        .per_view(2.2)
        .scroll_by(CarouselScrollBy::Slide)
}

/// Stack Builder(`CarouselStack` / `variant=stack`) on both sides.
pub fn carousel_stack(id: impl Into<String>) -> CarouselBuilder {
    carousel(id).variant(CarouselVariant::Stack).per_view(1.0)
}

impl CarouselBuilder {
    pub fn slide(mut self, build: impl Fn() -> Element + 'static) -> Self {
        self.slides.push(Rc::new(build));
        self
    }

    pub fn index_signal(mut self, s: Signal<usize>) -> Self {
        self.index = Some(s);
        self
    }

    pub fn orientation(mut self, o: CarouselOrientation) -> Self {
        self.orientation = o;
        self
    }

    pub fn loop_slides(mut self, yes: bool) -> Self {
        self.loop_slides = yes;
        self
    }

    pub fn seamless(mut self, yes: bool) -> Self {
        self.seamless = Some(yes);
        self
    }

    pub fn snap(mut self, yes: bool) -> Self {
        self.snap = yes;
        self
    }

    pub fn per_view(mut self, n: f32) -> Self {
        self.per_view = n;
        self
    }

    pub fn autoplay_ms(mut self, ms: u64) -> Self {
        self.autoplay_ms = ms;
        self
    }

    pub fn controls(mut self, c: CarouselControls) -> Self {
        self.controls = c;
        self
    }

    pub fn scroll_by(mut self, s: CarouselScrollBy) -> Self {
        self.scroll_by = s;
        self
    }

    /// 设置轮播变体（single / multi / stack）。
    pub fn variant(mut self, v: CarouselVariant) -> Self {
        self.variant = v;
        self
    }

    /// Old name; please use [`Self::variant`] first.
    pub fn kind(self, k: CarouselKind) -> Self {
        self.variant(k)
    }

    pub fn end_align(mut self, a: CarouselEndAlign) -> Self {
        self.end_align = a;
        self
    }

    pub fn page_snap(mut self, s: CarouselPageSnap) -> Self {
        self.page_snap = s;
        self
    }

    pub fn item_sizing(mut self, s: CarouselItemSizing) -> Self {
        self.item_sizing = s;
        self
    }

    pub fn item_width(mut self, w: impl Into<String>) -> Self {
        self.item_width = w.into();
        self
    }

    pub fn dots(mut self, yes: bool) -> Self {
        self.dots = Some(yes);
        self
    }

    pub fn stack_sides(mut self, n: u32) -> Self {
        self.stack_sides = n.max(1);
        self
    }

    impl_slot_chrome_methods!();

    pub fn content_with(
        mut self,
        f: impl FnOnce(CarouselSlotBuilder) -> CarouselSlotBuilder,
    ) -> Self {
        self.content_chrome = f(carousel_slot()).chrome;
        self
    }

    pub fn item_with(mut self, f: impl FnOnce(CarouselSlotBuilder) -> CarouselSlotBuilder) -> Self {
        self.item_chrome = f(carousel_slot()).chrome;
        self
    }

    pub fn previous_with(
        mut self,
        f: impl FnOnce(CarouselSlotBuilder) -> CarouselSlotBuilder,
    ) -> Self {
        self.previous_chrome = f(carousel_slot()).chrome;
        self
    }

    pub fn next_with(mut self, f: impl FnOnce(CarouselSlotBuilder) -> CarouselSlotBuilder) -> Self {
        self.next_chrome = f(carousel_slot()).chrome;
        self
    }

    pub fn dots_with(mut self, f: impl FnOnce(CarouselSlotBuilder) -> CarouselSlotBuilder) -> Self {
        self.dots_chrome = f(carousel_slot()).chrome;
        self
    }

    pub fn child(self, body: impl FnOnce() -> Element) -> Element {
        let chrome = self.chrome.clone();
        let el = carousel_scope(
            self.id,
            self.index,
            self.orientation.as_str(),
            self.loop_slides,
            self.seamless,
            self.snap,
            self.per_view,
            self.autoplay_ms,
            self.controls.as_str(),
            self.scroll_by.as_str(),
            self.variant.as_str(),
            self.end_align.as_str(),
            self.page_snap.as_str(),
            self.item_sizing.as_str(),
            self.item_width,
            self.dots,
            self.stack_sides,
            body,
        );
        chrome.apply(el)
    }
}

impl From<CarouselBuilder> for Element {
    fn from(builder: CarouselBuilder) -> Self {
        let CarouselBuilder {
            id,
            slides,
            index,
            orientation,
            loop_slides,
            seamless,
            snap,
            per_view,
            autoplay_ms,
            controls,
            scroll_by,
            variant,
            end_align,
            page_snap,
            item_sizing,
            item_width,
            dots,
            stack_sides,
            chrome,
            content_chrome,
            item_chrome,
            previous_chrome,
            next_chrome,
            dots_chrome,
        } = builder;
        let slides_for_content = slides.clone();
        let inject_dots = dots.unwrap_or_else(|| variant.default_dots());
        let el = carousel_scope(
            id,
            index,
            orientation.as_str(),
            loop_slides,
            seamless,
            snap,
            per_view,
            autoplay_ms,
            controls.as_str(),
            scroll_by.as_str(),
            variant.as_str(),
            end_align.as_str(),
            page_snap.as_str(),
            item_sizing.as_str(),
            item_width,
            dots,
            stack_sides,
            move || {
                let mut items = Vec::new();
                for build in &slides_for_content {
                    let body = build();
                    let mut item: Element = r#box()
                        .min_w(0.0)
                        .w("100%")
                        .flex_basis("100%")
                        .flex_shrink(0.0)
                        .flex_grow(0.0)
                        .px("0.375rem")
                        .child(body)
                        .into();
                    item.attrs
                        .insert(ATTR_SLOT.to_string(), SLOT_ITEM.to_string());
                    item = item.accesskit_role("group");
                    item.set_attr("aria-roledescription", "slide");
                    items.push(item_chrome.apply(item));
                }
                let mut frag = Element::new();
                frag.set_attr("rui_fragment", "1");
                frag = frag.children(items);
                let content = content_chrome.apply(CarouselContent(CarouselContentProps {
                    children: Some(frag),
                    ..Default::default()
                }));
                let prev =
                    previous_chrome.apply(CarouselPrevious(CarouselPreviousProps::default()));
                let next = next_chrome.apply(CarouselNext(CarouselNextProps::default()));
                let mut kids = vec![content, prev, next];
                if inject_dots {
                    kids.push(dots_chrome.apply(CarouselDots(CarouselDotsProps::default())));
                }
                let mut root = Element::new();
                root.set_attr("rui_fragment", "1");
                root.children(kids)
            },
        );
        chrome.apply(el)
    }
}

//! 文本分区。

use crate::imports::*;
use crate::shared::*;

const FONT_SAMPLE: &str = "The quick brown fox 中文字重 Ag 0123";

/// Constrained sample leaf. `<Box>` is flex-row; percent-width labels in a row
/// become N× parent width (gallery inherit demo overflow). Always wrap in VStack.
fn font_sample_label(id: &str, text: &str, class: &str) -> Element {
    let el: Element = label(id).text(text).w("100%").class(class).into();
    el.min_w("0")
}

fn font_family_tile(id: &str, caption: &str, class: &str) -> Element {
    let sample = font_sample_label(id, FONT_SAMPLE, class);
    ui! {
        <VStack gap="0.25rem" min_w="0" w="100%" shrink="1">
            <Label font_weight="700" text_size="0.75rem" leading="none" w="100%" min_w="0">{ caption }</Label>
            <VStack bg="muted" rounded="0.375rem" p="0.5rem" w="100%" min_w="0">
                { sample }
            </VStack>
        </VStack>
    }
}

fn font_weight_chip(token: &str) -> Element {
    let sample = font_sample_label(
        &format!("gal-{token}"),
        "Ag 字",
        &format!("font-sans {token} leading-normal"),
    );
    ui! {
        <VStack gap="0.15rem" items="start" min_w="0" shrink="1">
            <Label text_size="0.6875rem" color="muted-foreground" leading="none">{ token }</Label>
            { sample }
        </VStack>
    }
}

fn font_size_chip(token: &str) -> Element {
    let sample = font_sample_label(
        &format!("gal-{token}"),
        FONT_SAMPLE,
        &format!("font-sans font-normal leading-normal {token}"),
    );
    ui! {
        <HStack gap="0.5rem" items="start" w="100%" min_w="0">
            <Label text_size="0.6875rem" color="muted-foreground" w="4.5rem" leading="none">{ token }</Label>
            <VStack grow="1" min_w="0" shrink="1">
                { sample }
            </VStack>
        </HStack>
    }
}

fn font_leading_tile(id: &str, caption: &str, class: &str) -> Element {
    let sample = font_sample_label(id, "中文顶底笔画 gyp 行盒裁切对照", class);
    ui! {
        <VStack gap="0.25rem" min_w="0" w="100%" shrink="1">
            <Label font_weight="700" text_size="0.75rem" leading="none" w="100%" min_w="0">{ caption }</Label>
            <VStack bg="muted" rounded="0.375rem" p="0.5rem" w="100%" min_w="0" overflow="hidden">
                { sample }
            </VStack>
        </VStack>
    }
}

fn font_rsx_family_tile(_id: &str, caption: &str, sample: Element) -> Element {
    ui! {
        <VStack gap="0.25rem" min_w="0" w="100%" shrink="1">
            <Label font_weight="700" text_size="0.75rem" leading="none" w="100%" min_w="0">{ caption }</Label>
            <VStack bg="muted" rounded="0.375rem" p="0.5rem" w="100%" min_w="0">
                { sample }
            </VStack>
        </VStack>
    }
}

/// 族 / 字重 / 字号 / 行高覆盖验收（对照 `docs/engine/font.md`）。
#[component]
pub(crate) fn FontOverrideShowcase() -> jadix::core::tree::Element {
    let family_sans = font_family_tile(
        "gal-font-sans",
        "class=\"font-sans\"",
        "font-sans font-normal",
    );
    let family_serif = font_family_tile(
        "gal-font-serif",
        "class=\"font-serif\"",
        "font-serif font-normal",
    );
    let family_mono = font_family_tile(
        "gal-font-mono",
        "class=\"font-mono\"",
        "font-mono font-normal",
    );
    let weights: Vec<Element> = [
        "font-100",
        "font-200",
        "font-300",
        "font-normal",
        "font-medium",
        "font-semibold",
        "font-bold",
        "font-extrabold",
        "font-black",
    ]
    .into_iter()
    .map(font_weight_chip)
    .collect();
    let sizes: Vec<Element> = [
        "text-xs",
        "text-sm",
        "text-base",
        "text-lg",
        "text-xl",
        "text-2xl",
    ]
    .into_iter()
    .map(font_size_chip)
    .collect();
    let lead_none = font_leading_tile(
        "gal-font-leading-none",
        "leading-none（1.0 · Label 默认）",
        "font-sans font-normal leading-none",
    );
    let lead_tight = font_leading_tile(
        "gal-font-leading-tight",
        "leading-tight",
        "font-sans font-normal leading-tight",
    );
    let lead_normal = font_leading_tile(
        "gal-font-leading-normal",
        "leading-normal",
        "font-sans font-normal leading-normal",
    );
    let lead_arb = font_leading_tile(
        "gal-font-leading-arb",
        "leading-[1.15]",
        "font-sans font-normal leading-[1.15]",
    );
    let rsx_segoe = font_rsx_family_tile(
        "gal-font-family-segoe",
        "font_family=\"Segoe UI, sans-serif\"",
        label("gal-font-family-segoe")
            .text(FONT_SAMPLE)
            .font_family("Segoe UI, sans-serif")
            .font_weight(400.0)
            .leading(1.5)
            .w("100%")
            .into(),
    );
    let rsx_consolas = font_rsx_family_tile(
        "gal-font-family-consolas",
        "font_family=\"Consolas, ui-monospace\"",
        label("gal-font-family-consolas")
            .text(FONT_SAMPLE)
            .font_family("Consolas, Cascadia Mono, ui-monospace, monospace")
            .font_weight(400.0)
            .leading(1.5)
            .w("100%")
            .into(),
    );
    let rsx_times = font_rsx_family_tile(
        "gal-font-family-times",
        "text_font=\"Times New Roman, Georgia, serif\"",
        label("gal-font-family-times")
            .text(FONT_SAMPLE)
            .font_family("Times New Roman, Georgia, serif")
            .font_weight(400.0)
            .leading(1.5)
            .w("100%")
            .into(),
    );

    ui! {
        <MyCard title="7) 字体族 / 字重 / 行高覆盖（验收）">
            <VStack w="100%" min_w="0" gap="0.75rem">
                <Label w="100%" text_size="0.75rem" color="muted-foreground" leading="normal">
                    "对照 docs/engine/font.md · font_strategy.md。默认栈 Inter → Inter Variable → Segoe/SF → CJK 后段。进程级 JADIX_FONT_FAMILY / WindowOptions::default_font* 不在本卡切换。"
                </Label>
                <Label w="100%" text_size="0.75rem" color="muted-foreground" leading="normal">
                    "勾选：① sans/serif/mono 字形可辨 ② RSX font_family 覆盖 class ③ 400 < 500 ≤ 700（无 Medium 面时 500 为 Regular+伪粗）④ 字号阶梯 ⑤ leading-none 比 normal 更易裁切 CJK ⑥ 子级可覆盖父级族"
                </Label>

                <Label font_weight="700" text_size="0.875rem" leading="none">"默认 cascade（无 class）"</Label>
                <VStack bg="muted" rounded="0.375rem" p="0.5rem" w="100%" min_w="0">
                    <Label id="gal-font-default" font_weight="400" leading="normal" w="100%">
                        { FONT_SAMPLE }
                    </Label>
                </VStack>

                <Label font_weight="700" text_size="0.875rem" leading="none">"泛型族 class"</Label>
                <VStack w="100%" min_w="0" gap="0.75rem">
                    { family_sans }
                    { family_serif }
                    { family_mono }
                </VStack>

                <Label font_weight="700" text_size="0.875rem" leading="none">"RSX font_family / text_font"</Label>
                <VStack w="100%" min_w="0" gap="0.75rem">
                    { rsx_segoe }
                    { rsx_consolas }
                    { rsx_times }
                </VStack>

                <Label font_weight="700" text_size="0.875rem" leading="none">"字重阶梯（class font-* · 同族 sans）"</Label>
                <Label w="100%" text_size="0.75rem" color="muted-foreground" leading="normal">
                    "可变字体：500 为真 wght 轴。静态 Segoe：500 匹配 Regular 400 + synthetic bold（略重于 font-normal，轻于 font-bold）。"
                </Label>
                <HStack w="100%" min_w="0" gap="0.75rem" items="end" wrap="wrap">
                    { weights }
                </HStack>
                <HStack w="100%" min_w="0" gap="1rem" items="end" wrap="wrap">
                    <Label id="gal-font-w400" class="font-sans font-normal" leading="normal" text_size="1.25rem">
                        "400 Regular"
                    </Label>
                    <Label id="gal-font-w500" class="font-sans font-medium" leading="normal" text_size="1.25rem">
                        "500 Medium"
                    </Label>
                    <Label id="gal-font-w700" class="font-sans font-bold" leading="normal" text_size="1.25rem">
                        "700 Bold"
                    </Label>
                </HStack>

                <Label font_weight="700" text_size="0.875rem" leading="none">"字号 class text-*"</Label>
                <VStack w="100%" min_w="0" gap="0.35rem">
                    { sizes }
                </VStack>

                <Label font_weight="700" text_size="0.875rem" leading="none">"行高 leading-*（CJK 裁切对照）"</Label>
                <Label w="100%" text_size="0.75rem" color="muted-foreground" leading="normal">
                    "Label 默认 leading-none。盒外墨水会被 prepare_text / overflow 裁掉；验收时看「字」顶底是否缺笔，而不是和 Chrome 像素对齐。"
                </Label>
                <VStack w="100%" min_w="0" gap="0.75rem">
                    { lead_none }
                    { lead_tight }
                    { lead_normal }
                    { lead_arb }
                </VStack>

                <Label font_weight="700" text_size="0.875rem" leading="none">"子树继承与局部覆盖"</Label>
                <VStack class="font-mono text-sm" p="0.75rem" bg="muted" rounded="0.5rem" gap="0.35rem" w="100%" min_w="0">
                    <Label id="gal-font-inherit-mono" leading="normal" font_weight="400" w="100%">
                        "父级 font-mono text-sm：code 继承 代码"
                    </Label>
                    <Label id="gal-font-inherit-sans" class="font-sans" leading="normal" font_weight="400" w="100%">
                        "子级 class=\"font-sans\" 覆盖族"
                    </Label>
                    <Label
                        id="gal-font-inherit-serif"
                        font_family="Georgia, Times New Roman, serif"
                        leading="normal"
                        font_weight="400"
                        w="100%"
                    >
                        "子级 font_family=Georgia, serif"
                    </Label>
                </VStack>
            </VStack>
        </MyCard>
    }
}

pub(crate) fn typography_showcase_buttons() -> Element {
    ui! {
        <VStack w="100%" min_w="0" gap="0.75rem">
            <HStack w="100%" min_w="0" gap="0.5rem" items="center" wrap="wrap">
                <Button variant="outline" size="xs">"XS"</Button>
                <Button variant="outline" size="sm">"SM"</Button>
                <Button variant="outline" size="default">"Default"</Button>
                <Button variant="outline" size="lg">"LG"</Button>
                <Button variant="default" size="icon-xs"><Icon name="star" color="primary-foreground" size="0.75rem" /></Button>
                <Button variant="default" size="icon-sm"><Icon name="heart" color="primary-foreground" size="0.875rem" /></Button>
                <Button variant="default" size="icon"><Icon name="rocket" color="primary-foreground" size="1rem" /></Button>
                <Button variant="default" size="icon-lg"><Icon name="sparkles" color="primary-foreground" size="1.125rem" /></Button>
            </HStack>
            <HStack w="100%" min_w="0" gap="0.5rem" items="center" wrap="wrap">
                <Button variant="default" leading="none">"Leading None"</Button>
                <Button variant="default" leading="tight">"Leading Tight"</Button>
                <Button variant="default" leading="normal">"Leading Normal"</Button>
                <Button variant="default" leading="relaxed">"Leading Relaxed"</Button>
                <Button variant="default" line_height="1.5rem">"LineHeight\n1.5rem (24px)"</Button>
            </HStack>
        </VStack>
    }
}

pub(crate) fn typography_showcase_decoration() -> Element {
    ui! {
        <VStack w="100%" min_w="0" gap="0.5rem">
            <Label w="100%" text_size="1rem" font_weight="700">"Label Decoration"</Label>
            <HStack w="100%" min_w="0" gap="1rem" items="center" wrap="wrap">
                <Label text_decoration="underline" text_decoration_style="solid">"Solid"</Label>
                <Label text_decoration="underline" text_decoration_style="double">"Double"</Label>
                <Label text_decoration="underline" text_decoration_style="dotted">"Dotted"</Label>
                <Label text_decoration="underline" text_decoration_style="dashed">"Dashed"</Label>
                <Label text_decoration="underline" text_decoration_style="wavy">"Wavy"</Label>
            </HStack>
            <Label w="100%" text_size="0.875rem" font_weight="700">"Decoration Line"</Label>
            <HStack w="100%" min_w="0" gap="1rem" items="center" wrap="wrap">
                <Label text_decoration_line="underline">"Underline"</Label>
                <Label text_decoration_line="overline">"Overline"</Label>
                <Label text_decoration_line="line-through">"Line Through"</Label>
                <Label text_decoration_line="underline line-through">"Underline + Line Through"</Label>
            </HStack>
            <Label w="100%" text_size="0.875rem" font_weight="700">"Decoration Color / Thickness / Style"</Label>
            <HStack w="100%" min_w="0" gap="1rem" items="center" wrap="wrap">
                <Label text_decoration_line="underline" text_decoration_color="red-500">"Red"</Label>
                <Label text_decoration_line="underline" text_decoration_color="emerald-500">"Green"</Label>
                <Label text_decoration_line="underline" text_decoration_thickness="1px">"Thin"</Label>
                <Label text_decoration_line="underline" text_decoration_thickness="2px">"Medium"</Label>
                <Label text_decoration_line="underline" text_decoration_thickness="3px" text_decoration_style="dashed">"Thick Dashed"</Label>
                <Label text_decoration_line="underline" text_decoration_style="wavy" text_decoration_color="violet-500">"Wavy Violet"</Label>
            </HStack>
        </VStack>
    }
}

#[component]
pub(crate) fn TypographyShowcase() -> jadix::core::tree::Element {
    let buttons = typography_showcase_buttons();
    let decoration = typography_showcase_decoration();
    ui! {
        <MyCard title="3) Button Sizes + Icon + Custom Text Leading">
            <VStack w="100%" min_w="0" gap="0.75rem">
                { buttons }
                { decoration }
            </VStack>
        </MyCard>
    }
}

pub(crate) fn gal_wp21_chip_mention() -> Element {
    Element::new()
        .text_spans(vec![
            TextSpan::new("prefix "),
            TextSpan::new("Chip Label")
                .chip()
                .padding_x(4.0)
                .background([100, 116, 139, 56]),
            TextSpan::new(" and "),
            TextSpan::new("@alice").mention(),
            TextSpan::new(" wrap after atomic spans"),
        ])
        .text_size(14.0)
        .text_leading(1.5)
        .text_color(parse_text_color("foreground"))
        .w("10rem")
        .min_w("0")
}

#[component]
pub(crate) fn TextAlignShowcase() -> jadix::core::tree::Element {
    let row_h = jadix::item_height_for_text(14.0, 1.5);
    let chip_mention = gal_wp21_chip_mention();
    // Always-on scrollbar: idle `auto` chrome hides and a mid-list offset looks "broken".
    let virt: Element = virtual_list(TEXT_ALIGN_WP21_VIRT)
        .item_count(120)
        .item_height_from_font(14.0, 1.5)
        .height(168.0)
        .overscan(4)
        .scrollbar_visibility("always")
        .render_item(|i| {
            label(&format!("gal-wp21-virt-{i}"))
                .text(format!("virtual row {i:03}"))
                .size(14.0)
                .leading(1.5)
                .into()
        })
        .into();

    ui! {
        <MyCard title="WP-2.1) 测宽 ≡ glyphon 分行">
            <VStack w="100%" gap="0.75rem">
                <Label w="100%" text_size="0.75rem" color="muted-foreground" leading="normal">
                    "Gallery 验收（对照 docs/engine/text_align.md）：窄框多行软折不横溢；Chip/Mention 整段原子；VirtualList 行高 = item_height_for_text(14, 1.5)。"
                </Label>
                <Label w="100%" text_size="0.75rem" color="muted-foreground" leading="normal">
                    { format!("勾选：① ASCII 窄折 ② CJK 窄折 ③ Chip/Mention 不中段断开 ④ VirtualList 行高 {row_h}px、滚动只挂可见行") }
                </Label>
                <HStack w="100%" min_w="0" gap="1rem" items="start" wrap="wrap">
                    <VStack gap="0.35rem" min_w="0">
                        <Label font_weight="700" text_size="0.75rem">"ASCII soft-wrap（~5rem）"</Label>
                        <Box bg="muted" rounded="0.375rem" p="0.5rem" w="5rem" borderStyle="solid" borderColor="border" borderWidth="0.0625">
                            <Label w="100%" text_size="0.875rem" leading="normal">
                                "hello world from jadix text measure alignment"
                            </Label>
                        </Box>
                    </VStack>
                    <VStack gap="0.35rem" min_w="0">
                        <Label font_weight="700" text_size="0.75rem">"CJK soft-wrap（~6rem）"</Label>
                        <Box bg="muted" rounded="0.375rem" p="0.5rem" w="6rem" borderStyle="solid" borderColor="border" borderWidth="0.0625">
                            <Label w="100%" text_size="1rem" leading="normal">
                                "中文日文测试测宽分行对齐用例足够长"
                            </Label>
                        </Box>
                    </VStack>
                    <VStack gap="0.35rem" min_w="0">
                        <Label font_weight="700" text_size="0.75rem">"Chip + Mention（10rem）"</Label>
                        <Box bg="muted" rounded="0.375rem" p="0.5rem" borderStyle="solid" borderColor="border" borderWidth="0.0625">
                            { chip_mention }
                        </Box>
                    </VStack>
                </HStack>
                <VStack gap="0.35rem" w="100%" min_w="0">
                    <Label font_weight="700" text_size="0.875rem">
                        { format!("VirtualList · item_height_from_font(14, 1.5) = {row_h}px") }
                    </Label>
                    <Label w="100%" text_size="0.75rem" color="muted-foreground" leading="normal">
                        "首屏应从 virtual row 000 起；滚动条常显。行高 21px，仅挂载可见区间，无重叠/空隙跳变。"
                    </Label>
                    <Box bg="muted" rounded="0.375rem" p="0.25rem" w="100%" borderStyle="solid" borderColor="border" borderWidth="0.0625">
                        { virt }
                    </Box>
                </VStack>
            </VStack>
        </MyCard>
    }
}

#[component]
pub(crate) fn TextCompatibilityShowcase() -> jadix::core::tree::Element {
    ui! {
        <MyCard title="6) Text Compatibility Stress Test">
            <VStack w="100%" gap="0.75rem">
                <Label w="100%" text_size="0.75rem" color="muted-foreground" leading="normal">
                    "Use this section to verify multilingual shaping, emoji fallback, combining marks, punctuation width, and bidirectional text."
                </Label>
                <Box bg="background" rounded="0.5rem" p="0.75rem" w="100%" min_w="0">
                    <VStack gap="0.5rem" w="100%" min_w="0">
                        <Label w="100%" font_weight="700">"English / Latin"</Label>
                        <Label w="100%" leading="normal">
                            "The quick brown fox jumps over the lazy dog. Pack my box with five dozen liquor jugs. 0123456789"
                        </Label>
                        <Label w="100%">"Accents: cafe cafe\u{0301} naive na\u{00EF}ve resume re\u{0301}sume\u{0301} fiance fianc\u{00E9}"</Label>

                        <Label w="100%" font_weight="700">"Chinese / Japanese / Korean"</Label>
                        <Label w="100%" id="gal-s6-label-cjk" leading="normal">
                            "中文测试：输入法、标点符号、全角半角、换行和字距表现。"
                        </Label>
                        <Label w="100%" leading="normal">
                            "日本語テスト：こんにちは世界。漢字・かな・カタカナ・句読点「」『』の表示確認。"
                        </Label>
                        <Label w="100%" leading="normal">
                            "한국어 테스트: 안녕하세요 세계. 받침, 조사, 기호와 줄바꿈 렌더링 확인."
                        </Label>

                        <Label w="100%" font_weight="700">"Other Scripts"</Label>
                        <Label w="100%" leading="normal">
                            "Arabic: مرحبا بالعالم، هذا اختبار لاتجاه الكتابة والمحاذاة وعرض الحروف."
                        </Label>
                        <Label w="100%" leading="normal">
                            "Hindi: नमस्ते दुनिया। संयुक्ताक्षर, मात्रा, और देवनागरी रेंडरिंग परीक्षण।"
                        </Label>
                        <Label w="100%" leading="normal">
                            "Thai: สวัสดีชาวโลก ทดสอบการจัดวรรณยุกต์และระยะบรรทัด"
                        </Label>
                        <Label w="100%" leading="normal">
                            "Russian: Привет, мир! Проверка кириллицы, знаков препинания и переносов."
                        </Label>

                        <Label w="100%" font_weight="700">"Emoji / Symbols / Math"</Label>
                        <Label w="100%" leading="normal">
                            "Emoji: 😀 😁 😂 🤣 😅 🥲 🧠 🧪 🚀 🌍 🔥 ✅ ❌ 💡 🧩 👨‍👩‍👧‍👦 🧑‍💻 🏳️‍🌈"
                        </Label>
                        <Label w="100%" leading="normal">
                            "Currency & units: $ € £ ¥ ₹ ₽ ₩ ₫ ₺ 10km/h 23°C 5ms 1GiB"
                        </Label>
                        <Label w="100%" leading="normal">
                            "Math & operators: +-*/ = != <= >= ≈ ≠ ≤ ≥ ∑ ∏ √ ∞ ∫ ∂ ∇ → ← ↔"
                        </Label>
                        <Label w="100%" leading="normal">
                            "Box/shape symbols: ■ □ ▲ △ ● ○ ◆ ◇ ★ ☆ ※ ◎ ◉"
                        </Label>

                        <Label w="100%" font_weight="700">"Edge Cases"</Label>
                        <Label w="100%" leading="normal">
                            "Zero-width joiner: 👩‍🚀 👨‍🔬 👩‍💻 | Combining marks: A\u{0301} e\u{0301} o\u{0302} n\u{0303}"
                        </Label>
                        <Label w="100%" leading="normal">
                            "Bidirectional sample: English 123 - \u{2068}العربية\u{2060}٤٥٦\u{2069}\u{200B}- \u{2068}עברית\u{2060}789\u{2069}"
                        </Label>
                        <Label w="100%" leading="normal" id="gal-rtl-emoji-mixed">
                            "RTL+emoji+Latin: عربي 😀 English مرحبا 👍🏽 — 固定回归样例"
                        </Label>
                        <Label w="8rem" leading="normal" id="gal-narrow-multiline-emoji">
                            "Narrow wrap: Mixed: Rust 👩‍💻 + 中文 + عربي 🔥 second line 456"
                        </Label>
                        <Label w="100%" leading="normal">
                            "Punctuation width: ,.;:!? 。，；：！？ () [] {} <> 《》【】"
                        </Label>
                    </VStack>
                </Box>
            </VStack>
        </MyCard>
    }
}

#[component]
pub(crate) fn EmojiRealtimeRegressionShowcase() -> jadix::core::tree::Element {
    ui! {
        <MyCard title="6b) Windows Emoji Realtime Regression">
            <VStack w="100%" gap="0.75rem">
                <Label w="100%" text_size="0.75rem" color="muted-foreground" leading="normal">
                    "实时输入回归：输入法候选上屏、粘贴、退格与替换后，Emoji 应在当帧或下一帧可见。"
                </Label>
                <Input id="gal-emoji-regression-input" placeholder="输入或粘贴: 😀 👨‍👩‍👧‍👦 👍🏽 🇨🇳 1️⃣" w="100%" />
                <Box bg="background" rounded="0.5rem" p="0.75rem" w="100%">
                    <VStack gap="0.5rem" w="100%">
                        <Label w="100%" font_weight="700">"固定样例（视觉比对）"</Label>
                        <Label w="100%" leading="normal">"Family: 👨‍👩‍👧‍👦  Profession: 🧑‍💻"</Label>
                        <Label w="100%" leading="normal">"Skin tone: 👍🏽  Flags: 🇨🇳 🇺🇸"</Label>
                        <Label w="100%" leading="normal">
                            "Hello 😀 😁 😭  Keycap: 1️⃣ 2️⃣ 9️⃣  Mixed: Rust 👩‍💻 + 中文 + عربي"
                        </Label>
                    </VStack>
                </Box>
            </VStack>
        </MyCard>
    }
}

/// Coverage / Bake / 三通道 MSDF 对照（scale>2 走 Msdf；尖角看「永角」）。
#[component]
pub(crate) fn MsdfMultichannelShowcase() -> jadix::core::tree::Element {
    ui! {
        <MyCard title="三通道 MSDF（Coverage / Bake / Msdf）">
            <VStack w="100%" gap="0.75rem">
                <Label w="100%" text_size="0.75rem" color="muted-foreground" leading="normal">
                    "GlyphRasterMode：轴对齐小字 = Coverage（glyphon）；sticky/旋转且缩放约 0.5–2 = Bake RGBA；scale 越出该带 = 三通道 MSDF（RGB 边对 + FS median）。A 通道现为 255，未做 MTSDF。Emoji 强制 Bake。"
                </Label>
                <HStack gap="2rem" wrap="wrap" items="end" justify="start" p="1.25rem" bg="muted" rounded="0.5rem" overflow="visible">
                    <VStack gap="0.25rem" items="center">
                        <Box w="6rem" h="3.5rem" bg="secondary" rounded="0.375rem" class="flex items-center justify-center">
                            <Label id="gal-msdf-coverage" text_size="0.75rem" font_weight="600">"永角 Ag"</Label>
                        </Box>
                        <Label text_size="0.6rem" color="muted-foreground">"Coverage · 无变换"</Label>
                    </VStack>
                    <VStack gap="0.25rem" items="center">
                        <Box w="6rem" h="3.5rem" bg="secondary" rounded="0.375rem" class="rotate-12 flex items-center justify-center">
                            <Label id="gal-msdf-bake" text_size="0.75rem" font_weight="600">"永角 Ag"</Label>
                        </Box>
                        <Label text_size="0.6rem" color="muted-foreground">"Bake · rotate-12"</Label>
                    </VStack>
                    <VStack gap="0.25rem" items="center">
                        <Box w="6rem" h="3.5rem" bg="secondary" rounded="0.375rem" class="scale-[3] flex items-center justify-center">
                            <Label id="gal-msdf-msdf" text_size="0.75rem" font_weight="600">"永角 Ag"</Label>
                        </Box>
                        <Label text_size="0.6rem" color="muted-foreground">"Msdf · scale-3 尖角"</Label>
                    </VStack>
                </HStack>
            </VStack>
        </MyCard>
    }
}

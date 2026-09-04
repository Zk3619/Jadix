//! 文本分区。

use crate::imports::*;
use crate::shared::*;

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
pub(crate) fn TypographyShowcase() -> rui::core::tree::Element {
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
pub(crate) fn TextAlignShowcase() -> rui::core::tree::Element {
    let row_h = rui::item_height_for_text(14.0, 1.5);
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
                                "hello world from rui text measure alignment"
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
pub(crate) fn TextCompatibilityShowcase() -> rui::core::tree::Element {
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
pub(crate) fn EmojiRealtimeRegressionShowcase() -> rui::core::tree::Element {
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

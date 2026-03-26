#![allow(non_snake_case)]

use rui::component;
use rui::core::reactivity::use_signal;
use rui::ui;

#[component]
fn MyCard(title: String, children: Option<rui::core::tree::Element>) -> rui::core::tree::Element {
    ui! {
        <VStack p="1rem" bg="secondary" rounded="0.5rem" gap="0.5rem">
            <Label text_size="1.25rem" font_weight="700">{ title }</Label>
            { children.unwrap_or_else(|| rui::core::tree::Element::new()) }
        </VStack>
    }
}

#[component]
fn ButtonShowcase() -> rui::core::tree::Element {
    let count = use_signal("count", || 0);
    let selected = use_signal("selected_variant", || "default".to_string());

    ui! {
        <MyCard title="1) Button Variants + State">
            <VStack gap="0.75rem">
                <HStack gap="0.5rem" items="center">
                    <Button variant="default" on_click={move || count.set(count.get() + 1)}>
                        { format!("Default ({})", count.get()) }
                    </Button>
                    <Button variant="secondary" on_click={move || selected.set("secondary".into())}>
                        "Secondary"
                    </Button>
                    <Button variant="outline" on_click={move || selected.set("outline".into())}>
                        "Outline"
                    </Button>
                    <Button variant="ghost">"Ghost"</Button>
                    <Button variant="destructive">"Destructive"</Button>
                    <Button variant="link">"Link"</Button>
                    <Button variant="default" disabled=true>"Disabled"</Button>
                </HStack>
                <Label text_size="0.875rem" color="muted-foreground">
                    { format!("Selected variant: {}", selected.get()) }
                </Label>
            </VStack>
        </MyCard>
    }
}

#[component]
fn InputShowcase() -> rui::core::tree::Element {
    ui! {
        <MyCard title="2) Input (ShadCN-style)">
            <VStack gap="0.75rem">
                <Label text_size="0.75rem" color="muted-foreground" leading="normal">
                    "Click to focus, type text, Tab to move focus, Backspace to delete"
                </Label>
                <HStack gap="0.5rem" items="center" w="100%">
                    <Input placeholder="Placeholder…" w="20rem" />
                    <Input placeholder="Invalid" invalid=true w="12rem" />
                </HStack>
            </VStack>
        </MyCard>
    }
}

#[component]
fn TypographyShowcase() -> rui::core::tree::Element {
    ui! {
        <MyCard title="3) Button Sizes + Icon + Custom Text Leading">
            <VStack gap="0.75rem">
                <HStack gap="0.5rem" items="center">
                    <Button variant="outline" size="xs">"XS"</Button>
                    <Button variant="outline" size="sm">"SM"</Button>
                    <Button variant="outline" size="default">"Default"</Button>
                    <Button variant="outline" size="lg">"LG"</Button>
                    <Button variant="default" size="icon-xs"><Icon name="star" color="primary-foreground" size="0.75rem" /></Button>
                    <Button variant="default" size="icon-sm"><Icon name="heart" color="primary-foreground" size="0.875rem" /></Button>
                    <Button variant="default" size="icon"><Icon name="rocket" color="primary-foreground" size="1rem" /></Button>
                    <Button variant="default" size="icon-lg"><Icon name="sparkles" color="primary-foreground" size="1.125rem" /></Button>
                </HStack>
                <HStack gap="0.5rem" items="center">
                    <Button variant="default" leading="none">"Leading None"</Button>
                    <Button variant="default" leading="tight">"Leading Tight"</Button>
                    <Button variant="default" leading="normal">"Leading Normal"</Button>
                    <Button variant="default" leading="relaxed">"Leading Relaxed"</Button>
                    <Button variant="default" line_height="1.5rem">"LineHeight\n1.5rem (24px)"</Button>
                </HStack>
                <VStack gap="0.5rem">
                    <Label text_size="1rem" font_weight="700">"Label Decoration"</Label>
                    <HStack gap="1rem">
                        <Label text_decoration="underline" text_decoration_style="solid">"Solid"</Label>
                        <Label text_decoration="underline" text_decoration_style="double">"Double"</Label>
                        <Label text_decoration="underline" text_decoration_style="dotted">"Dotted"</Label>
                        <Label text_decoration="underline" text_decoration_style="dashed">"Dashed"</Label>
                        <Label text_decoration="underline" text_decoration_style="wavy">"Wavy"</Label>
                    </HStack>
                </VStack>
            </VStack>
        </MyCard>
    }
}

#[component]
fn ScrollShowcase() -> rui::core::tree::Element {
    ui! {
        <MyCard title="4) ScrollArea + Scrollbar">
            <VStack gap="1rem" w="100%">
                <Label text_size="0.75rem" color="muted-foreground" leading="normal">
                    "Defaults: inside overlay, vertical right / horizontal bottom, scrollbar_visibility auto, hover_mode scrollbar (track/thumb only)."
                </Label>
                <Rect w="28rem" h="15rem" bg="background" rounded="0.5rem" p="0.25rem" relative=true>
                    <ScrollArea id="demo-scroll" w="100%" h="100%" bg="background" rounded="0.375rem" p="0.5rem" overflow_y="auto" overflow_x="hidden" scroll_behavior="smooth">
                        <VStack gap="0.25rem" w="100%">
                            { (1..=20).map(|i| ui! { <Label>{ format!("Scroll row {:02}", i) }</Label> }).collect::<Vec<rui::core::tree::Element>>() }
                            <Rect bg="secondary" rounded="0.5rem" p="0.5rem" mt="0.5rem">
                                <VStack gap="0.25rem">
                                    <Label font_weight="700">"Nested ScrollArea"</Label>
                                    <Rect bg="background" rounded="0.5rem" w="100%" h="6rem" p="0.25rem" relative=true>
                                        <ScrollArea id="nested-scroll" w="100%" h="100%" p="0.5rem" overflow_y="auto" overflow_x="hidden">
                                            <VStack gap="0.25rem" w="100%">
                                                { (1..=12).map(|i| ui! { <Label>{ format!("Nested row {:02}", i) }</Label> }).collect::<Vec<rui::core::tree::Element>>() }
                                            </VStack>
                                        </ScrollArea>
                                    </Rect>
                                </VStack>
                            </Rect>
                        </VStack>
                    </ScrollArea>
                    { rui::core::tree::Element::new()
                        .absolute()
                        .top(8.0)
                        .right(8.0)
                        .z_index(2000)
                        .bg(rui::core::tree::parse_color("destructive"))
                        .rounded(10.0)
                        .px(10.0)
                        .py(6.0)
                        .child(rui::core::tree::Element::new()
                            .text("Overlay (z_index=2000)")
                            .text_color([255, 255, 255, 255])
                            .text_size(12.0)
                        )
                    }
                </Rect>

                <Label font_weight="700" text_size="0.875rem">"Outside + top bar + vertical left"</Label>
                <Rect w="28rem" h="10rem" bg="background" rounded="0.5rem" p="0.25rem">
                    <ScrollArea id="scroll-outside-sides" w="100%" h="100%" bg="background" rounded="0.375rem"
                        scrollbar_inset="outside" scrollbar_x_side="top" scrollbar_y_side="left"
                        overflow_y="auto" overflow_x="auto" p="0.5rem">
                        <VStack gap="0.25rem" min_w="48rem">
                            <Label>"Wide content → horizontal scroll; vertical track is outside on the left."</Label>
                            { (1..=14).map(|i| ui! { <Label>{ format!("Row {:02}", i) }</Label> }).collect::<Vec<rui::core::tree::Element>>() }
                        </VStack>
                    </ScrollArea>
                </Rect>

                <Label font_weight="700" text_size="0.875rem">"overflow_x hidden (no horizontal bar) + scrollbar_visibility always"</Label>
                <Rect w="28rem" h="6rem" bg="background" rounded="0.5rem" p="0.25rem">
                    <ScrollArea id="scroll-vertical-only" w="100%" h="100%" p="0.5rem"
                        overflow_y="auto" overflow_x="hidden" scrollbar_visibility="always">
                        <VStack gap="0.25rem" w="100%">
                            { (1..=18).map(|i| ui! { <Label>{ format!("Vertical-only {:02}", i) }</Label> }).collect::<Vec<rui::core::tree::Element>>() }
                        </VStack>
                    </ScrollArea>
                </Rect>

                <Label font_weight="700" text_size="0.875rem">"hover_mode area + scrollbar_visibility always (both axes)"</Label>
                <Rect w="28rem" h="6rem" bg="background" rounded="0.5rem" p="0.25rem">
                    <ScrollArea id="scroll-area-hover" w="100%" h="100%" p="0.5rem"
                        hover_mode="area" scrollbar_visibility="always"
                        overflow_y="auto" overflow_x="auto">
                        <HStack gap="2rem" items="start">
                            <VStack gap="0.25rem" w="12rem">
                                { (1..=10).map(|i| ui! { <Label>{ format!("Hover anywhere → bars stay on ({:02})", i) }</Label> }).collect::<Vec<rui::core::tree::Element>>() }
                            </VStack>
                            <Rect min_w="24rem"><Label>"Long horizontal line to enable horizontal scrollbar when combined with overflow."</Label></Rect>
                        </HStack>
                    </ScrollArea>
                </Rect>

                <Label font_weight="700" text_size="0.875rem">"overflow_x hidden + scrollbar_visibility always (vertical only)"</Label>
                <Rect w="28rem" h="5rem" bg="background" rounded="0.5rem" p="0.25rem">
                    <ScrollArea id="scroll-override-x" w="100%" h="100%" p="0.5rem"
                        scrollbar_visibility="always"
                        overflow_y="auto" overflow_x="hidden">
                        <VStack gap="0.25rem" min_w="40rem">
                            <Label>"No horizontal scrollbar widget; wide content is clipped along X."</Label>
                            { (1..=8).map(|i| ui! { <Label>{ format!("Line {:02}", i) }</Label> }).collect::<Vec<rui::core::tree::Element>>() }
                        </VStack>
                    </ScrollArea>
                </Rect>

                                <Label font_weight="700" text_size="0.875rem">"overflow_x hidden + scrollbar_visibility always (vertical only)"</Label>
                <Rect w="28rem" h="5rem" bg="background" rounded="0.5rem" p="0.25rem">
                    <ScrollArea id="scroll-override-x" w="100%" h="100%" p="0.5rem"
                        scrollbar_visibility="always"
                        overflow_y="auto" overflow_x="hidden">
                        <VStack gap="0.25rem" min_w="40rem">
                            <Label>"No horizontal scrollbar widget; wide content is clipped along X."</Label>
                            { (1..=8).map(|i| ui! { <Label>{ format!("Line {:02}", i) }</Label> }).collect::<Vec<rui::core::tree::Element>>() }
                        </VStack>
                    </ScrollArea>
                </Rect>

                                <Label font_weight="700" text_size="0.875rem">"overflow_x hidden + scrollbar_visibility always (vertical only)"</Label>
                <Rect w="28rem" h="5rem" bg="background" rounded="0.5rem" p="0.25rem">
                    <ScrollArea id="scroll-override-x" w="100%" h="100%" p="0.5rem"
                        scrollbar_visibility="always"
                        overflow_y="auto" overflow_x="hidden">
                        <VStack gap="0.25rem" min_w="40rem">
                            <Label>"No horizontal scrollbar widget; wide content is clipped along X."</Label>
                            { (1..=8).map(|i| ui! { <Label>{ format!("Line {:02}", i) }</Label> }).collect::<Vec<rui::core::tree::Element>>() }
                        </VStack>
                    </ScrollArea>
                </Rect>

                                <Label font_weight="700" text_size="0.875rem">"overflow_x hidden + scrollbar_visibility always (vertical only)"</Label>
                <Rect w="28rem" h="5rem" bg="background" rounded="0.5rem" p="0.25rem">
                    <ScrollArea id="scroll-override-x" w="100%" h="100%" p="0.5rem"
                        scrollbar_visibility="always"
                        overflow_y="auto" overflow_x="hidden">
                        <VStack gap="0.25rem" min_w="40rem">
                            <Label>"No horizontal scrollbar widget; wide content is clipped along X."</Label>
                            { (1..=8).map(|i| ui! { <Label>{ format!("Line {:02}", i) }</Label> }).collect::<Vec<rui::core::tree::Element>>() }
                        </VStack>
                    </ScrollArea>
                </Rect>

                                <Label font_weight="700" text_size="0.875rem">"overflow_x hidden + scrollbar_visibility always (vertical only)"</Label>
                <Rect w="28rem" h="5rem" bg="background" rounded="0.5rem" p="0.25rem">
                    <ScrollArea id="scroll-override-x" w="100%" h="100%" p="0.5rem"
                        scrollbar_visibility="always"
                        overflow_y="auto" overflow_x="hidden">
                        <VStack gap="0.25rem" min_w="40rem">
                            <Label>"No horizontal scrollbar widget; wide content is clipped along X."</Label>
                            { (1..=8).map(|i| ui! { <Label>{ format!("Line {:02}", i) }</Label> }).collect::<Vec<rui::core::tree::Element>>() }
                        </VStack>
                    </ScrollArea>
                </Rect>

                                <Label font_weight="700" text_size="0.875rem">"overflow_x hidden + scrollbar_visibility always (vertical only)"</Label>
                <Rect w="28rem" h="5rem" bg="background" rounded="0.5rem" p="0.25rem">
                    <ScrollArea id="scroll-override-x" w="100%" h="100%" p="0.5rem"
                        scrollbar_visibility="always"
                        overflow_y="auto" overflow_x="hidden">
                        <VStack gap="0.25rem" min_w="40rem">
                            <Label>"No horizontal scrollbar widget; wide content is clipped along X."</Label>
                            { (1..=8).map(|i| ui! { <Label>{ format!("Line {:02}", i) }</Label> }).collect::<Vec<rui::core::tree::Element>>() }
                        </VStack>
                    </ScrollArea>
                </Rect>

                                <Label font_weight="700" text_size="0.875rem">"overflow_x hidden + scrollbar_visibility always (vertical only)"</Label>
                <Rect w="28rem" h="5rem" bg="background" rounded="0.5rem" p="0.25rem">
                    <ScrollArea id="scroll-override-x" w="100%" h="100%" p="0.5rem"
                        scrollbar_visibility="always"
                        overflow_y="auto" overflow_x="hidden">
                        <VStack gap="0.25rem" min_w="40rem">
                            <Label>"No horizontal scrollbar widget; wide content is clipped along X."</Label>
                            { (1..=8).map(|i| ui! { <Label>{ format!("Line {:02}", i) }</Label> }).collect::<Vec<rui::core::tree::Element>>() }
                        </VStack>
                    </ScrollArea>
                </Rect>

                                <Label font_weight="700" text_size="0.875rem">"overflow_x hidden + scrollbar_visibility always (vertical only)"</Label>
                <Rect w="28rem" h="5rem" bg="background" rounded="0.5rem" p="0.25rem">
                    <ScrollArea id="scroll-override-x" w="100%" h="100%" p="0.5rem"
                        scrollbar_visibility="always"
                        overflow_y="auto" overflow_x="hidden">
                        <VStack gap="0.25rem" min_w="40rem">
                            <Label>"No horizontal scrollbar widget; wide content is clipped along X."</Label>
                            { (1..=8).map(|i| ui! { <Label>{ format!("Line {:02}", i) }</Label> }).collect::<Vec<rui::core::tree::Element>>() }
                        </VStack>
                    </ScrollArea>
                </Rect>

                                <Label font_weight="700" text_size="0.875rem">"overflow_x hidden + scrollbar_visibility always (vertical only)"</Label>
                <Rect w="28rem" h="5rem" bg="background" rounded="0.5rem" p="0.25rem">
                    <ScrollArea id="scroll-override-x" w="100%" h="100%" p="0.5rem"
                        scrollbar_visibility="always"
                        overflow_y="auto" overflow_x="hidden">
                        <VStack gap="0.25rem" min_w="40rem">
                            <Label>"No horizontal scrollbar widget; wide content is clipped along X."</Label>
                            { (1..=8).map(|i| ui! { <Label>{ format!("Line {:02}", i) }</Label> }).collect::<Vec<rui::core::tree::Element>>() }
                        </VStack>
                    </ScrollArea>
                </Rect>

                                <Label font_weight="700" text_size="0.875rem">"overflow_x hidden + scrollbar_visibility always (vertical only)"</Label>
                <Rect w="28rem" h="5rem" bg="background" rounded="0.5rem" p="0.25rem">
                    <ScrollArea id="scroll-override-x" w="100%" h="100%" p="0.5rem"
                        scrollbar_visibility="always"
                        overflow_y="auto" overflow_x="hidden">
                        <VStack gap="0.25rem" min_w="40rem">
                            <Label>"No horizontal scrollbar widget; wide content is clipped along X."</Label>
                            { (1..=8).map(|i| ui! { <Label>{ format!("Line {:02}", i) }</Label> }).collect::<Vec<rui::core::tree::Element>>() }
                        </VStack>
                    </ScrollArea>
                </Rect>

                                <Label font_weight="700" text_size="0.875rem">"overflow_x hidden + scrollbar_visibility always (vertical only)"</Label>
                <Rect w="28rem" h="5rem" bg="background" rounded="0.5rem" p="0.25rem">
                    <ScrollArea id="scroll-override-x" w="100%" h="100%" p="0.5rem"
                        scrollbar_visibility="always"
                        overflow_y="auto" overflow_x="hidden">
                        <VStack gap="0.25rem" min_w="40rem">
                            <Label>"No horizontal scrollbar widget; wide content is clipped along X."</Label>
                            { (1..=8).map(|i| ui! { <Label>{ format!("Line {:02}", i) }</Label> }).collect::<Vec<rui::core::tree::Element>>() }
                        </VStack>
                    </ScrollArea>
                </Rect>
            </VStack>
        </MyCard>
    }
}

#[component]
fn LogicalShowcase() -> rui::core::tree::Element {
    ui! {
        <MyCard title="5) Logical Properties & Layout">
            <VStack gap="0.75rem">
                <Rect
                    inline_size="24rem"
                    block_size="auto"
                    bg="background"
                    rounded="0.5rem"
                    px="1rem"
                    py="0.75rem"
                >
                    <Label text_size="0.875rem">
                        "px/py + inline-size + block-size + auto-height"
                    </Label>
                </Rect>
                <HStack gap="0.5rem">
                    <Rect bg="background" rounded="0.4rem" px="0.75rem" py="0.5rem"><Label>"px/py"</Label></Rect>
                    <Rect bg="background" rounded="0.4rem" pt="0.75rem" pr="1rem" pb="0.25rem" pl="0.5rem"><Label>"pt/pr/pb/pl"</Label></Rect>
                    <Rect bg="background" rounded="0.4rem" mx="0.75rem" my="0.25rem"><Label>"mx/my"</Label></Rect>
                </HStack>
            </VStack>
        </MyCard>
    }
}

#[component]
fn TextCompatibilityShowcase() -> rui::core::tree::Element {
    ui! {
        <MyCard title="6) Text Compatibility Stress Test">
            <VStack gap="0.75rem">
                <Label text_size="0.75rem" color="muted-foreground" leading="normal">
                    "Use this section to verify multilingual shaping, emoji fallback, combining marks, punctuation width, and bidirectional text."
                </Label>
                <Rect bg="background" rounded="0.5rem" p="0.75rem" w="100%">
                    <VStack gap="0.5rem" w="100%">
                        <Label font_weight="700">"English / Latin"</Label>
                        <Label leading="normal">
                            "The quick brown fox jumps over the lazy dog. Pack my box with five dozen liquor jugs. 0123456789"
                        </Label>
                        <Label>"Accents: cafe cafe\u{0301} naive na\u{00EF}ve resume re\u{0301}sume\u{0301} fiance fianc\u{00E9}"</Label>

                        <Label font_weight="700">"Chinese / Japanese / Korean"</Label>
                        <Label leading="normal">
                            "中文测试：输入法、标点符号、全角半角、换行和字距表现。"
                        </Label>
                        <Label leading="normal">
                            "日本語テスト：こんにちは世界。漢字・かな・カタカナ・句読点「」『』の表示確認。"
                        </Label>
                        <Label leading="normal">
                            "한국어 테스트: 안녕하세요 세계. 받침, 조사, 기호와 줄바꿈 렌더링 확인."
                        </Label>

                        <Label font_weight="700">"Other Scripts"</Label>
                        <Label leading="normal">
                            "Arabic: مرحبا بالعالم، هذا اختبار لاتجاه الكتابة والمحاذاة وعرض الحروف."
                        </Label>
                        <Label leading="normal">
                            "Hindi: नमस्ते दुनिया। संयुक्ताक्षर, मात्रा, और देवनागरी रेंडरिंग परीक्षण।"
                        </Label>
                        <Label leading="normal">
                            "Thai: สวัสดีชาวโลก ทดสอบการจัดวรรณยุกต์และระยะบรรทัด"
                        </Label>
                        <Label leading="normal">
                            "Russian: Привет, мир! Проверка кириллицы, знаков препинания и переносов."
                        </Label>

                        <Label font_weight="700">"Emoji / Symbols / Math"</Label>
                        <Label leading="normal">
                            "Emoji: 😀 😁 😂 🤣 😅 🥲 🧠 🧪 🚀 🌍 🔥 ✅ ❌ 💡 🧩 👨‍👩‍👧‍👦 🧑‍💻 🏳️‍🌈"
                        </Label>
                        <Label leading="normal">
                            "Currency & units: $ € £ ¥ ₹ ₽ ₩ ₫ ₺ 10km/h 23°C 5ms 1GiB"
                        </Label>
                        <Label leading="normal">
                            "Math & operators: +-*/ = != <= >= ≈ ≠ ≤ ≥ ∑ ∏ √ ∞ ∫ ∂ ∇ → ← ↔"
                        </Label>
                        <Label leading="normal">
                            "Box/shape symbols: ■ □ ▲ △ ● ○ ◆ ◇ ★ ☆ ※ ◎ ◉"
                        </Label>

                        <Label font_weight="700">"Edge Cases"</Label>
                        <Label leading="normal">
                            "Zero-width joiner: 👩‍🚀 👨‍🔬 👩‍💻 | Combining marks: A\u{0301} e\u{0301} o\u{0302} n\u{0303}"
                        </Label>
                        <Label leading="normal">
                            "Bidirectional sample: English 123 - العربية ٤٥٦ - עברית 789"
                        </Label>
                        <Label leading="normal">
                            "Punctuation width: ,.;:!? 。，；：！？ () [] {} <> 《》【】"
                        </Label>
                    </VStack>
                </Rect>
            </VStack>
        </MyCard>
    }
}

#[component]
fn UtilityClassShowcase() -> rui::core::tree::Element {
    ui! {
        <MyCard title="7) Utility Classes (class / tw / uno)">
            <VStack gap="1rem">
                <VStack gap="0.5rem">
                    <Label font_weight="700">"A) class=..."</Label>
                    <Rect class="p-4 rounded-lg bg-secondary">
                        <VStack class="gap-2">
                            <Label class="text-sm text-muted-foreground">"This block uses class=\"p-4 rounded-lg bg-secondary\""</Label>
                            <Label class="text-base font-semibold">"Spacing + radius + color + text size"</Label>
                            <Label class="mt-2 text-xs">"mt-2 applied here"</Label>
                        </VStack>
                    </Rect>
                </VStack>

                <VStack gap="0.5rem">
                    <Label font_weight="700">"B) tw=..."</Label>
                    <HStack tw="gap-3 items-center">
                        <Rect tw="w-24 h-12 rounded-md bg-secondary" />
                        <Rect tw="w-[120px] h-[48px] rounded-xl bg-muted" />
                        <Rect tw="w-20 h-12 rounded-full bg-accent" />
                    </HStack>
                    <Label tw="text-sm text-muted-foreground mt-2">
                        "tw supports numeric scale (e.g. mt-2 => 8px) and arbitrary values [120px]"
                    </Label>
                </VStack>

                <VStack gap="0.5rem">
                    <Label font_weight="700">"C) uno=..."</Label>
                    <Rect uno="p-3 rounded-md bg-secondary">
                        <VStack uno="gap-2">
                            <Label uno="text-sm">"uno=\"...\" works the same way as class/tw"</Label>
                            <HStack uno="gap-2">
                                <Button variant="outline" size="sm">"Button"</Button>
                                <Label uno="mt-1 text-xs text-muted-foreground">"Label with mt-1"</Label>
                            </HStack>
                        </VStack>
                    </Rect>
                </VStack>
            </VStack>
        </MyCard>
    }
}

fn main() {
    rui::app::run(move || {
        let show_extra = use_signal("show_extra", || false);

        ui! {
            <HStack w="100%" h="100%" bg="background" gap="0" relative=true>
                <ScrollArea id="root-scroll" h="100%" overflow_y="auto" overflow_x="auto" grow="1" scroll_behavior="smooth">
                    <VStack items="center" gap="1.5rem" p="2rem" w="100%">
                        <VStack items="center" gap="0.5rem">
                            <Label text_size="2.5rem" font_weight="800">"RUI Framework"</Label>
                            <Label text_size="1rem" color="muted-foreground">"Layered Architecture • Functional Components • Hot Reload"</Label>
                        </VStack>

                        <VStack max_w="70rem" w="100%" gap="1rem">
                            <ButtonShowcase />
                            <InputShowcase />
                            <TypographyShowcase />
                            <ScrollShowcase />
                            <LogicalShowcase />
                            <TextCompatibilityShowcase />
                            <UtilityClassShowcase />

                            <VStack items="center" gap="1rem" p="1rem">
                                <Button variant="secondary" on_click={move || show_extra.set(!show_extra.get())}>
                                    { if show_extra.get() { "Hide Technical Details" } else { "Show Technical Details" } }
                                </Button>

                                { if show_extra.get() {
                                    ui! {
                                        <Rect bg="secondary" p="1.5rem" rounded="0.75rem" w="100%">
                                            <VStack gap="0.5rem">
                                                <Label font_weight="700">"Technical Stack:"</Label>
                                                <Label>"• Layout: Taffy (Flexbox)"</Label>
                                                <Label>"• Text: Glyphon (Cosmic-text wrapper)"</Label>
                                                <Label>"• Rendering: WGPU (Direct GPU Instances)"</Label>
                                                <Label>"• Reactivity: Signal-based fine-grained updates"</Label>
                                            </VStack>
                                        </Rect>
                                    }
                                } else {
                                    rui::core::tree::Element::new()
                                } }
                            </VStack>
                        </VStack>
                    </VStack>
                </ScrollArea>
            </HStack>
        }
    });
}

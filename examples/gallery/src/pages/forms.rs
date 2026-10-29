//! 表单分区。

use crate::imports::*;
use crate::pages::basics::{ReactiveCountChip, ReactiveCountChipProps};
use crate::shared::*;

#[component]
pub(crate) fn InputShowcase() -> jadix::core::tree::Element {
    let mark_invalid = use_signal("gal_input_mark_invalid", || false);
    let badge_ok = use_signal("gal_status_badge_ok", || true);
    let badge_label = use_signal("gal_status_badge_label", || "Ready".to_string());
    let chip_count = use_signal("gal_status_chip_count", || 0_i32);

    ui! {
        <MyCard title="2) Input (ShadCN-style)">
            <VStack w="100%" gap="0.75rem">
                <Label w="100%" text_size="0.75rem" color="muted-foreground" leading="normal">
                    "text / password / number / email / tel / url / search — type 过滤与非空校验；clearable 叉号清除；password_toggle 睁眼/闭眼"
                </Label>
                <HStack gap="0.5rem" items="start" wrap="wrap" w="100%">
                <Input
                        id="gal-s2-input-a"
                        placeholder="Placeholder…"
                        aria_label="Gallery demo name"
                        clearable="true"
                        flex_grow="1"
                    />
                    <Input placeholder="Invalid" invalid=true clearable="true" flex_grow="1" />
                    <Input id="gal-s2-input-pw" type="password" password_toggle="true" clearable="true" flex_grow="1" />
                    <Input type="number" number_steppers=true clearable=true flex_grow="1" />
                </HStack>
                <HStack gap="0.5rem" items="start" wrap="wrap" w="100%">
                    <Input type="email" clearable=true flex_grow="1" />
                    <Input type="tel" clearable=true flex_grow="1" />
                    <Input type="url" clearable=true flex_grow="1" />
                    <Input type="search" clearable="true" flex_grow="1" />
                </HStack>
                <Label text_size="0.75rem" color="muted-foreground">
                    "Input / Combobox 默认 text-overflow:hidden（原生 input：裁切，光标或选区可滚出）。clip / ellipsis / newline 用于 Label、Button 等不可编辑文本。"
                </Label>
                <HStack gap="0.5rem" items="center" w="10rem">
                    <Input
                        id="gal-s2-input-hidden"
                        w="100%"
                        placeholder="hidden：超长裁切，→ 或向后选中可滚出"
                    />
                </HStack>
                <HStack gap="0.5rem" items="center" w="10rem">
                    <Button truncate=true w="100%" variant="outline" size="sm">
                        "Very long button caption that must not wrap"
                    </Button>
                </HStack>
                <HStack gap="0.5rem" items="start" w="10rem">
                    <VStack gap="0.25rem" w="100%">
                        <Label text_overflow="clip" w="100%">
                            "clip：超长单行会被裁切不会出现省略号"
                        </Label>
                        <Label text_overflow="ellipsis" w="100%">
                            "ellipsis：超长单行末尾显示省略号"
                        </Label>
                        <Label text_overflow="newline" w="100%">
                            "newline：允许软换行到下一行而不是裁切或省略"
                        </Label>
                    </VStack>
                </HStack>
                <HStack gap="0.5rem" items="center" wrap="wrap" w="100%">
                    <Input
                        placeholder="invalid={signal}"
                        invalid={mark_invalid.get()}
                        flex_grow="1"
                    />
                    <Button
                        id="gal-s2-mark-invalid"
                        variant="outline"
                        size="sm"
                        on_click={move || {
                            // Solid `batch`: one flush for caption bind_text + Input invalid bind_attr.
                            batch(|| {
                                mark_invalid.set(!mark_invalid.peek());
                            });
                        }}
                    >
                        { if mark_invalid.get() { "Clear invalid" } else { "Mark invalid" } }
                    </Button>
                </HStack>
                <HStack gap="0.5rem" items="center" wrap="wrap" w="100%">
                    <ReactiveStatusBadge
                        id="gal-status-badge"
                        label={badge_label}
                        ok={badge_ok}
                    />
                    <Button
                        variant="ghost"
                        size="sm"
                        on_click={move || {
                            let next = !badge_ok.peek();
                            badge_ok.set(next);
                            badge_label.set(if next { "Ready".into() } else { "Blocked".into() });
                        }}
                    >
                        "Toggle badge"
                    </Button>
                    <ReactiveCountChip id="gal-count-chip" count={chip_count} />
                    <Button
                        variant="ghost"
                        size="sm"
                        on_click={move || chip_count.set(chip_count.peek() + 1)}
                    >
                        "Bump count"
                    </Button>
                    <Show when={badge_ok.get()}>
                        <Label text_size="0.75rem" color="muted-foreground">"online"</Label>
                    </Show>
                </HStack>
                <Label w="100%" text_size="0.75rem" color="muted-foreground" leading="normal">
                    "ReactiveStatusBadge：底层 <Badge variant={…}> + bind 文案；完整变体矩阵见「基础 → Badge」。"
                </Label>
            </VStack>
        </MyCard>
    }
}

#[component]
pub(crate) fn InputAdornmentShowcase() -> jadix::core::tree::Element {
    ui! {
        <MyCard title="2b) InputAdornment（对位 ShadCN InputGroup）">
            <VStack w="100%" gap="0.75rem">
                <Label w="100%" text_size="0.75rem" color="muted-foreground" leading="normal">
                    "单字段外壳 + addon（≠ 多个 Input 成组）。双写法：标签 / input_adornment().addon().child()"
                </Label>
                <InputAdornment>
                    <InputAdornmentAddon align="inline-start">
                        <InputAdornmentText>
                            <Icon name="search" size="0.875rem" color="muted-foreground" />
                        </InputAdornmentText>
                    </InputAdornmentAddon>
                    <InputAdornmentInput id="gal-ia-search" placeholder="Search…" />
                    <InputAdornmentAddon align="inline-end">
                        <InputAdornmentText>"12 results"</InputAdornmentText>
                    </InputAdornmentAddon>
                </InputAdornment>
                <InputAdornment>
                    <InputAdornmentAddon align="inline-start">
                        <InputAdornmentText>"https://"</InputAdornmentText>
                    </InputAdornmentAddon>
                    <InputAdornmentInput id="gal-ia-url" placeholder="example.com" />
                    <InputAdornmentAddon align="inline-end">
                        <InputAdornmentButton size="icon-xs" variant="ghost">
                            <Icon name="info" size="0.75rem" />
                        </InputAdornmentButton>
                    </InputAdornmentAddon>
                </InputAdornment>
                <InputAdornment>
                    <InputAdornmentAddon align="block-start">
                        <InputAdornmentText>"Message"</InputAdornmentText>
                    </InputAdornmentAddon>
                    <InputAdornmentTextarea id="gal-ia-ta" placeholder="Ask, Search or Chat…" rows={3} />
                    <InputAdornmentAddon align="block-end">
                        <InputAdornmentButton size="xs" variant="default">"Send"</InputAdornmentButton>
                    </InputAdornmentAddon>
                </InputAdornment>
                {
                    Element::from(
                        input_adornment()
                            .addon(
                                "inline-start",
                                InputAdornmentText(InputAdornmentTextProps {
                                    children: Some(Element::new().text("$")),
                                    base: None,
                                    ..Default::default()
                                }),
                            )
                            .child(
                                input("gal-ia-builder")
                                    .placeholder("0.00")
                                    .input_type("number")
                                    .plain_chrome(true),
                            )
                            .addon(
                                "inline-end",
                                InputAdornmentText(InputAdornmentTextProps {
                                    children: Some(Element::new().text("USD")),
                                    base: None,
                                    ..Default::default()
                                }),
                            ),
                    )
                }
            </VStack>
        </MyCard>
    }
}

#[component]
pub(crate) fn InputOTPShowcase() -> jadix::core::tree::Element {
    let code = use_signal("gal_otp_code", String::new);
    ui! {
        <MyCard title="2c) InputOTP（对位 ShadCN）">
            <VStack w="100%" gap="0.75rem">
                <Label w="100%" text_size="0.75rem" color="muted-foreground" leading="normal">
                    "独立复合组件（≠ Input type）。连体分段条（border-y/r · first/last 圆角）；活动槽 ring；Builder separator_at 分段。"
                </Label>
                <Label font_weight="700" text_size="0.75rem">"RSX · 6 位分段"</Label>
                <InputOTP id="gal-otp-rsx" maxLength={6}>
                    <InputOTPGroup>
                        <InputOTPSlot index={0} />
                        <InputOTPSlot index={1} />
                        <InputOTPSlot index={2} />
                    </InputOTPGroup>
                    <InputOTPSeparator />
                    <InputOTPGroup>
                        <InputOTPSlot index={3} />
                        <InputOTPSlot index={4} />
                        <InputOTPSlot index={5} />
                    </InputOTPGroup>
                </InputOTP>
                <Label font_weight="700" text_size="0.75rem">"Builder · 受控 value"</Label>
                {
                    Element::from(
                        input_otp("gal-otp-b")
                            .max_length(4)
                            .value_signal(code)
                            .separator_at(2),
                    )
                }
                <Label text_size="0.75rem" color="muted-foreground">
                    { format!("value = {:?}", code.get()) }
                </Label>
            </VStack>
        </MyCard>
    }
}

pub(crate) fn wp39_form_demo() -> Element {
    form_provider("gal-wp39", || {
        let st = use_form().expect("form_provider");
        let st_submit = st.clone();
        let st_clear = st.clone();
        let field = form_field(
            "email",
            "邮箱",
            Element::from(input("gal-wp39-email").placeholder("you@example.com")),
        );
        let submit: Element = button("gal-wp39-submit")
            .text("提交")
            .on_click(move || {
                let _ = st_submit.validate(|| {
                    let key = input_value_signal_key("gal-wp39-email");
                    let v = use_signal(&key, String::new).peek();
                    if v.trim().is_empty() {
                        st_submit.set_field_error("email", "必填");
                    } else if !v.contains('@') {
                        st_submit.set_field_error("email", "需包含 @");
                    }
                });
            })
            .into();
        let clear: Element = button("gal-wp39-clear")
            .variant("outline")
            .text("清除错误")
            .on_click(move || st_clear.clear_all())
            .into();
        vstack()
            .w("100%")
            .gap("0.5rem")
            .child(field)
            .child(
                hstack()
                    .gap("0.5rem")
                    .wrap("wrap")
                    .items("center")
                    .child(submit)
                    .child(clear),
            )
            .into()
    })
}

#[component]
pub(crate) fn FormValidateShowcase() -> jadix::core::tree::Element {
    ui! {
        <MyCard title="Form 校验（WP-3.9）">
            <VStack w="100%" gap="0.75rem">
                <Label w="100%" text_size="0.75rem" color="muted-foreground" leading="normal">
                    "form_provider + form_field：错误文案与 Input invalid 环同源。空提交 →「必填」+ 红环；清除错误 → 文案与环一起消失。"
                </Label>
                { wp39_form_demo() }
            </VStack>
        </MyCard>
    }
}

#[component]
pub(crate) fn QuestionnaireShowcase() -> jadix::core::tree::Element {
    let summary = use_signal("gal-q-summary", String::new);
    let summary_submit = summary;
    let builder_el: Element = questionnaire("gal-q-builder")
        .class("gap-8")
        .parts(|p| p.choices("gap-2").actions("gap-3").title("text-lg"))
        .shortcuts("letters")
        .default_item("scope")
        .question("scope", "Which files are in scope?", |q| {
            q.required(true)
                .class("gap-4")
                .description("Pick one area. Letter shortcuts A–C are enabled.")
                .choice("component", "Component only")
                .choice("tests", "Component and tests")
                .choice("feature", "The complete feature area")
        })
        .question("timing", "When should this be revisited?", |q| {
            q.description("Optional — Skip if timing is undecided.")
                .choice("week", "This week")
                .choice("cycle", "Next cycle")
        })
        .on_submit(move |answers| {
            let scope = answers.get("scope").unwrap_or("none");
            let timing = match answers.status_of("timing") {
                QuestionnaireItemStatus::Skipped => "skipped",
                _ => answers.get("timing").unwrap_or("none"),
            };
            toast("Questionnaire saved")
                .description(format!("Scope: {scope} · Timing: {timing}"))
                .duration(4000)
                .show();
        })
        .into();

    ui! {
        <MyCard title="2d) Questionnaire（ShadCN one-question flow）">
            <VStack w="100%" min_w="0" gap="1rem" max_w="28rem">
                <Label w="100%" text_size="0.75rem" color="muted-foreground" leading="normal">
                    "Dual style: composite RSX (macro → questionnaire_scope / questionnaire_item_scope) and Builder. Builder `.class` is the root; `.parts` styles auto-assembled slots; `.child` composes parts yourself. shortcuts=\"letters\" assigns A, B, C."
                </Label>
                <Label font_weight="700" text_size="0.875rem">"A · RSX (letters + freeform + skip)"</Label>
                <Questionnaire
                    id="gal-q-rsx"
                    defaultItem="direction"
                    shortcuts="letters"
                    on_submit={move |answers: QuestionnaireAnswers| {
                        let direction = answers.get("direction").unwrap_or("none").to_string();
                        let signals = answers.get_all("signals").join(", ");
                        let timing = match answers.status_of("timing") {
                            QuestionnaireItemStatus::Skipped => "skipped".to_string(),
                            _ => answers.get("timing").unwrap_or("none").to_string(),
                        };
                        let line = format!(
                            "Direction: {direction} · Signals: {} · Timing: {timing}",
                            if signals.is_empty() { "none".to_string() } else { signals }
                        );
                        summary_submit.set(line.clone());
                        toast("Agent plan saved").description(line).duration(4000).show();
                    }}
                >
                    <QuestionnaireProgress />
                    <QuestionnaireItem name="direction" required={true}>
                        <QuestionnaireTitle>"What should the agent build next?"</QuestionnaireTitle>
                        <QuestionnaireDescription>
                            "Choose a direction or describe another task."
                        </QuestionnaireDescription>
                        <QuestionnaireChoices>
                            <QuestionnaireChoice value="tool-calls">
                                "Tool call timeline"
                                <QuestionnaireChoiceDescription>
                                    "Show what the agent ran and what came back."
                                </QuestionnaireChoiceDescription>
                            </QuestionnaireChoice>
                            <QuestionnaireChoice value="approvals">
                                "Approval checkpoints"
                                <QuestionnaireChoiceDescription>
                                    "Ask before sensitive or destructive actions."
                                </QuestionnaireChoiceDescription>
                            </QuestionnaireChoice>
                            <QuestionnaireChoice value="handoffs">
                                "Sub-agent handoffs"
                                <QuestionnaireChoiceDescription>
                                    "Make delegated work and results easier to follow."
                                </QuestionnaireChoiceDescription>
                            </QuestionnaireChoice>
                            <QuestionnaireInput placeholder="Describe another feature…" />
                        </QuestionnaireChoices>
                        <QuestionnaireError />
                    </QuestionnaireItem>
                    <QuestionnaireItem name="signals" multiple={true}>
                        <QuestionnaireTitle>"What should every progress update include?"</QuestionnaireTitle>
                        <QuestionnaireDescription>
                            "Select all that apply, or skip this question."
                        </QuestionnaireDescription>
                        <QuestionnaireChoices>
                            <QuestionnaireChoice value="progress">"Progress"</QuestionnaireChoice>
                            <QuestionnaireChoice value="decisions">"Decisions"</QuestionnaireChoice>
                            <QuestionnaireChoice value="risks">"Risks"</QuestionnaireChoice>
                            <QuestionnaireChoice value="next-step">"Next step"</QuestionnaireChoice>
                        </QuestionnaireChoices>
                        <QuestionnaireError />
                    </QuestionnaireItem>
                    <QuestionnaireItem name="timing" required={true}>
                        <QuestionnaireTitle>"When should work begin?"</QuestionnaireTitle>
                        <QuestionnaireDescription>
                            "Choose when the agent should begin the work."
                        </QuestionnaireDescription>
                        <QuestionnaireChoices>
                            <QuestionnaireChoice value="now">"Start now"</QuestionnaireChoice>
                            <QuestionnaireChoice value="next-cycle">"Next development cycle"</QuestionnaireChoice>
                            <QuestionnaireChoice value="backlog">"Add it to the backlog"</QuestionnaireChoice>
                        </QuestionnaireChoices>
                        <QuestionnaireError />
                    </QuestionnaireItem>
                    <QuestionnaireActions>
                        <QuestionnairePrevious />
                        <Box flex_grow="1" min_w="0" h="1px" />
                        <QuestionnaireSkip />
                        <QuestionnaireNext>"Next"</QuestionnaireNext>
                        <QuestionnaireSubmit>"Save plan"</QuestionnaireSubmit>
                    </QuestionnaireActions>
                </Questionnaire>
                <Label w="100%" text_size="0.75rem" color="muted-foreground" leading="normal">
                    { if summary.get().is_empty() {
                        "Submit to see the collected answers here.".to_string()
                    } else {
                        summary.get()
                    } }
                </Label>
                <Label font_weight="700" text_size="0.875rem">"B · Builder dual style"</Label>
                { builder_el }
            </VStack>
        </MyCard>
    }
}

#[component]
pub(crate) fn PickerPlaceholdersShowcase() -> jadix::core::tree::Element {
    ui! {
        <VStack w="100%" gap="1rem">
            <MyCard title="2b) Pickers (file · color · date/time)">
                <VStack w="100%" min_w="0" gap="0.75rem">
                    <Label w="100%" text_size="0.75rem" color="muted-foreground" leading="normal">
                        "WP-3.3：触发器行 + Date/Time Tabs（芯片按内容宽并折行；仅挂载当前项，避免 forms 滚入时一次冷启动整排 picker）。"
                    </Label>
                    <HStack gap="0.5rem" items="start" wrap="wrap" w="100%">
                        <Button
                            variant="outline"
                            size="sm"
                            flex_grow="1"
                            on_click={move || {
                                let _ = jadix::utils::pick_files_into("gal-file", "image/*", false);
                            }}
                        >
                            "Choose image…"
                        </Button>
                        <Button
                            variant="outline"
                            size="sm"
                            flex_grow="1"
                            on_click={move || {
                                let _ = jadix::utils::pick_folder_into("gal-dir");
                            }}
                        >
                            "Choose folder…"
                        </Button>
                        <ColorPicker id="gal-color" value="#3b82f6" flex_grow="1" />
                    </HStack>
                    <Tabs id="gal-dp-kinds" default_value="date">
                        <TabsList>
                            <TabsTrigger value="date">"Date"</TabsTrigger>
                            <TabsTrigger value="datetime">"DateTime"</TabsTrigger>
                            <TabsTrigger value="time">"Time"</TabsTrigger>
                            <TabsTrigger value="month">"Month"</TabsTrigger>
                            <TabsTrigger value="week">"Week"</TabsTrigger>
                        </TabsList>
                        <TabsContent value="date">
                            <DatePicker id="gal-date" kind="date" captionLayout="dropdown" />
                        </TabsContent>
                        <TabsContent value="datetime">
                            <DatePicker id="gal-dt" kind="datetime-local" />
                        </TabsContent>
                        <TabsContent value="time">
                            <TimePicker id="gal-time" />
                        </TabsContent>
                        <TabsContent value="month">
                            <DatePicker id="gal-month" kind="month" />
                        </TabsContent>
                        <TabsContent value="week">
                            <DatePicker id="gal-week" kind="week" />
                        </TabsContent>
                    </Tabs>
                </VStack>
            </MyCard>
            <MyCard title="2c) Inline Calendar · ColorPicker">
                <VStack w="100%" min_w="0" gap="0.75rem">
                    <Label w="100%" text_size="0.75rem" color="muted-foreground">
                        "独立卡片：滚入 2b 上半时不连带着 Calendar / 内联 HSV。"
                    </Label>
                    <HStack gap="1rem" items="start" wrap="wrap" w="100%">
                        <Calendar id="gal-cal" captionLayout="dropdown" />
                        <ColorPicker id="gal-color-inline" variant="inline" format="hsla" />
                    </HStack>
                    <Label w="100%" text_size="0.75rem" color="muted-foreground">
                        "ColorPicker 变体（TabsList 折行保完整标签；仅挂载当前项）"
                    </Label>
                    <Tabs id="gal-cp-variants" default_value="sketch">
                        <TabsList>
                            <TabsTrigger value="sketch">"Sketch"</TabsTrigger>
                            <TabsTrigger value="chrome">"Chrome"</TabsTrigger>
                            <TabsTrigger value="github">"GitHub"</TabsTrigger>
                            <TabsTrigger value="twitter">"Twitter"</TabsTrigger>
                            <TabsTrigger value="material">"Material"</TabsTrigger>
                            <TabsTrigger value="block">"Block"</TabsTrigger>
                            <TabsTrigger value="circle">"Circle"</TabsTrigger>
                            <TabsTrigger value="slider">"Slider"</TabsTrigger>
                            <TabsTrigger value="compact">"Compact"</TabsTrigger>
                        </TabsList>
                        <TabsContent value="sketch">
                            <ColorPicker id="gal-cp-sketch" variant="sketch" value="#f47373" />
                        </TabsContent>
                        <TabsContent value="chrome">
                            <ColorPicker id="gal-cp-chrome" variant="chrome" value="#0693E3" />
                        </TabsContent>
                        <TabsContent value="github">
                            <ColorPicker id="gal-cp-github" variant="github" value="#1273DE" />
                        </TabsContent>
                        <TabsContent value="twitter">
                            <ColorPicker id="gal-cp-twitter" variant="twitter" value="#FF6900" />
                        </TabsContent>
                        <TabsContent value="material">
                            <ColorPicker id="gal-cp-material" variant="material" value="#9C27B0" />
                        </TabsContent>
                        <TabsContent value="block">
                            <ColorPicker id="gal-cp-block" variant="block" value="#37D67A" />
                        </TabsContent>
                        <TabsContent value="circle">
                            <ColorPicker id="gal-cp-circle" variant="circle" value="#BA68C8" />
                        </TabsContent>
                        <TabsContent value="slider">
                            <ColorPicker id="gal-cp-slider" variant="slider" value="#2CCCE4" />
                        </TabsContent>
                        <TabsContent value="compact">
                            <ColorPicker id="gal-cp-compact" variant="compact" value="#3b82f6" />
                        </TabsContent>
                    </Tabs>
                </VStack>
            </MyCard>
        </VStack>
    }
}

#[component]
pub(crate) fn ToggleControlsShowcase() -> jadix::core::tree::Element {
    let sw = use_signal("demo_switch_checked", || false);
    let cb = use_signal("demo_checkbox_checked", || false);

    ui! {
        <MyCard title="8) Switch · Checkbox · Separator">
            <VStack gap="0.75rem" w="100%">
                <Label text_size="0.75rem" color="muted-foreground" leading="normal">
                    "Checkbox / Switch: embed text / Label inside, or sibling Label with Solid id / for. Tab / Space on the control."
                </Label>

                <Separator />

                <Label font_weight="700" text_size="0.875rem">"Checkbox (solid-ui layout)"</Label>

                <Checkbox>"Accept terms (embedded text)"</Checkbox>

                <Checkbox>
                    <Label text_size="0.875rem" font_weight="500">
                        "Accept terms (embedded Label)"
                    </Label>
                </Checkbox>

                <HStack gap="0.5rem" items="center">
                    <Checkbox id="gallery_terms" />
                    <Label for="gallery_terms" text_size="0.875rem" font_weight="500">
                        "Accept terms and conditions"
                    </Label>
                </HStack>

                <HStack gap="0.5rem" items="start">
                    <Checkbox id="gallery_terms_desc" defaultChecked=true />
                    <VStack gap="0.25rem" min_w="0">
                        <Label for="gallery_terms_desc" text_size="0.875rem" font_weight="500">
                            "Accept terms and conditions"
                        </Label>
                        <Label text_size="0.875rem" color="muted-foreground" leading="normal">
                            "By clicking this checkbox, you agree to the terms."
                        </Label>
                    </VStack>
                </HStack>

                <HStack gap="0.5rem" items="center">
                    <Checkbox id="gallery_terms_disabled" disabled=true />
                    <Label for="gallery_terms_disabled" text_size="0.875rem" color="muted-foreground">
                        "Enable notifications"
                    </Label>
                </HStack>

                <HStack gap="0.5rem" items="center">
                    <Checkbox id="gallery_terms_invalid" invalid=true />
                    <Label for="gallery_terms_invalid" text_size="0.875rem" color="destructive">
                        "Accept terms and conditions"
                    </Label>
                </HStack>

                <Checkbox
                    id="gallery_terms_card"
                    gap="0.5rem"
                    items="start"
                    p="1rem"
                    w="100%"
                    rounded="0.5rem"
                    border_width={1.0}
                    border_color="border"
                    class="hover:bg-muted/50"
                >
                    <VStack gap="0.25rem" min_w="0">
                        <Label for="gallery_terms_card" text_size="0.875rem" font_weight="500">
                            "Enable notifications"
                        </Label>
                        <Label text_size="0.875rem" color="muted-foreground" leading="normal">
                            "You can enable or disable notifications at any time."
                        </Label>
                    </VStack>
                </Checkbox>

                <Separator />

                <Label font_weight="700" text_size="0.875rem">"Switch (solid-ui layout)"</Label>
                <Label text_size="0.75rem" color="muted-foreground" leading="normal">
                    "Embed text / Label inside, or sibling Label with Solid id / for. Thumb slides with a short transition."
                </Label>

                <Switch>"Airplane Mode (embedded text)"</Switch>

                <Switch>
                    <Label text_size="0.875rem" font_weight="500">
                        "Airplane Mode (embedded Label)"
                    </Label>
                </Switch>

                <HStack gap="0.5rem" items="center">
                    <Switch id="gallery_airplane" />
                    <Label for="gallery_airplane" text_size="0.875rem" font_weight="500">
                        "Airplane Mode"
                    </Label>
                </HStack>

                <HStack gap="0.5rem" items="center">
                    <Switch id="gallery_wifi" defaultChecked=true />
                    <Label for="gallery_wifi" text_size="0.875rem" font_weight="500">
                        "Wi-Fi (default on)"
                    </Label>
                </HStack>

                <HStack gap="0.5rem" items="center">
                    <Switch id="gallery_switch_disabled" disabled=true />
                    <Label for="gallery_switch_disabled" text_size="0.875rem" color="muted-foreground">
                        "Do not disturb"
                    </Label>
                </HStack>

                <Separator />

                <Label font_weight="700" text_size="0.875rem">"Switch · controlled Checkbox"</Label>
                <HStack gap="0.75rem" items="center" wrap="wrap">
                    <Switch id="hw_demo_switch" checked={sw} />
                    <Label text_size="0.875rem">{ format!("switch = {}", sw.get()) }</Label>
                    <Button variant="outline" size="sm" on_click={move || sw.set(!sw.get())}>
                        "Flip switch"
                    </Button>
                </HStack>
                <HStack gap="0.5rem" items="center">
                    <Checkbox id="hw_demo_checkbox" checked={cb} />
                    <Label for="hw_demo_checkbox" text_size="0.875rem">
                        { format!("checkbox = {}", cb.get()) }
                    </Label>
                    <Button variant="outline" size="sm" on_click={move || cb.set(!cb.get())}>
                        "Flip checkbox"
                    </Button>
                </HStack>

                <Separator />

                <Label font_weight="700" text_size="0.875rem">"Vertical separator in a row"</Label>
                <HStack gap="0.5rem" items="stretch" h="3rem" w="100%">
                    <Label>"Panel A"</Label>
                    { Element::from(separator().vertical(true)) }
                    <Label>"Panel B"</Label>
                    { Element::from(separator().vertical(true)) }
                    <Label>"Panel C"</Label>
                </HStack>
            </VStack>
        </MyCard>
    }
}

#[component]
pub(crate) fn RadioSliderShowcase() -> jadix::core::tree::Element {
    let tier = use_signal("gallery_radio_tier", || "free".to_string());
    let notify = use_signal("gallery_radio_notify", || "all".to_string());
    let tier_opts = use_signal("gallery_radio_tier_opts", || "pro".to_string());
    let level = use_signal("gallery_slider_level", || 0.4_f32);

    ui! {
        <MyCard title="9) Radio group · Slider">
            <VStack gap="0.75rem" w="100%">
                <Label text_size="0.75rem" color="muted-foreground" leading="normal">
                    "RadioGroup: solid-ui circular items. Embed text / ItemLabel, or options builder. Tab / Space on the control."
                </Label>

                <Label font_weight="700" text_size="0.875rem">"RadioGroup (compound RSX)"</Label>
                <RadioGroup id="gallery_radio_tier" value={tier} default_value="free">
                    <RadioGroupItem value="free">"Free"</RadioGroupItem>
                    <RadioGroupItem value="pro">
                        <RadioGroupItemLabel>"Pro"</RadioGroupItemLabel>
                    </RadioGroupItem>
                    <RadioGroupItem value="team">"Team"</RadioGroupItem>
                </RadioGroup>
                <Label text_size="0.875rem">{ format!("Tier: {}", tier.get()) }</Label>

                <Separator />

                <Label font_weight="700" text_size="0.875rem">"Notifications (default + disabled)"</Label>
                <RadioGroup id="gallery_radio_notify" value={notify} default_value="all">
                    <RadioGroupItem value="all">"All new messages"</RadioGroupItem>
                    <RadioGroupItem value="mentions">"Direct messages and mentions"</RadioGroupItem>
                    <RadioGroupItem value="none" disabled=true>"Nothing"</RadioGroupItem>
                </RadioGroup>
                <Label text_size="0.875rem">{ format!("notify: {}", notify.get()) }</Label>

                <Separator />

                <Label font_weight="700" text_size="0.875rem">"Builder · options"</Label>
                { Element::from(
                    radio_group("gallery:radio_options")
                        .value_signal(tier_opts)
                        .default_value("pro")
                        .options(vec![
                            ("free".to_string(), "Free".to_string()),
                            ("pro".to_string(), "Pro".to_string()),
                            ("team".to_string(), "Team".to_string()),
                        ]),
                ) }
                <Label text_size="0.875rem">{ format!("tier_opts: {}", tier_opts.get()) }</Label>

                <Separator />

                <Label font_weight="700" text_size="0.875rem">"Slider (solid-ui)"</Label>
                <Label text_size="0.75rem" color="muted-foreground" leading="normal">
                    "Click or drag the track; arrow keys step. Track / fill / thumb match solid-ui."
                </Label>
                <Slider id="gallery:slider" value={level} default_value={0.4} />
                <Label text_size="0.875rem">{ format!("Level: {:.0}%", level.get() * 100.0) }</Label>
                <Slider id="gallery:slider_disabled" defaultValue={0.6} disabled=true />
                <Label text_size="0.75rem" color="muted-foreground">"Disabled"</Label>

                <Separator />

                <Label font_weight="700" text_size="0.875rem">"Slider · orientation=vertical"</Label>
                <Label text_size="0.75rem" color="muted-foreground" leading="normal">
                    "纵向滑块：拖拽 / ↑↓ 调值，与上方横向示例共用 level signal。"
                </Label>
                <HStack gap="1.5rem" items="end">
                    <VStack gap="0.375rem" items="center">
                        <Slider id="gallery:slider_vertical" orientation="vertical" value={level} default_value={0.4} />
                        <Label text_size="0.75rem" color="muted-foreground">{ format!("{:.0}%", level.get() * 100.0) }</Label>
                    </VStack>
                </HStack>
            </VStack>
        </MyCard>
    }
}

#[component]
pub(crate) fn SelectPopoverTabShowcase() -> jadix::core::tree::Element {
    let tab_idx = use_signal("gallery_tabs_idx", || 0usize);
    let pop_open = use_signal("gallery_popover_open", || false);
    let pop_builder_open = use_signal("gallery_popover_builder_open", || false);
    let select_val = use_signal("gallery_select_plan", || "basic".to_string());
    let fruit_val = use_signal("gallery_select_fruit", String::new);

    ui! {
        <MyCard title="10) Select · Popover · Tab strip (AccessKit)">
            <VStack gap="0.75rem" w="100%">
                <Label text_size="0.75rem" color="muted-foreground" leading="normal">
                    "Select / Popover 自带组件级响应式边界；Tab 文案与开关用自动 bind_text。"
                </Label>
                <VStack gap="0.75rem" w="100%">
                    { Element::from(
                        select("gallery:select_plan")
                            .value_signal(select_val)
                            .default_value("basic")
                            .trigger_width("100%")
                            .item(SelectItemData::new("basic", "Basic"))
                            .item(SelectItemData::new("pro", "Pro"))
                            .item(SelectItemData::new("team", "Team")),
                    ) }
                    <Label text_size="0.875rem">{ format!("Selected plan: {}", select_val.get()) }</Label>
                    <Label text_size="0.75rem" color="muted-foreground" leading="normal">
                        "ShadCN 复合写法（SelectTrigger / Content / Item）："
                    </Label>
                    <Select id="gallery:select_fruit" value={fruit_val} placeholder="Select a fruit">
                        <SelectTrigger class="w-full">
                            <SelectValue />
                        </SelectTrigger>
                        <SelectContent max_h="12rem" class="bg-popover">
                            <SelectGroup>
                                <SelectLabel>"Fruits"</SelectLabel>
                                <SelectItem value="apple">"Apple"</SelectItem>
                                <SelectItem value="banana">"Banana"</SelectItem>
                                <SelectItem value="blueberry">"Blueberry"</SelectItem>
                            </SelectGroup>
                            <SelectSeparator />
                            <SelectItem value="pineapple">"Pineapple"</SelectItem>
                        </SelectContent>
                    </Select>
                    <Label text_size="0.875rem">{ format!("Selected fruit: {}", fruit_val.get()) }</Label>
                    <Label text_size="0.75rem" color="muted-foreground">
                        "窄触发器：选中文案省略。"
                    </Label>
                    { Element::from(
                        select("gallery:select_trunc")
                            .placeholder("A very long placeholder that should ellipsis")
                            .trigger_width("10rem")
                            .item(SelectItemData::new(
                                "long",
                                "Internationalization-ready extra long option label",
                            ))
                            .item(SelectItemData::new("short", "Short")),
                    ) }
                    <Label text_size="0.75rem" color="muted-foreground" leading="normal">
                        "Popover（RSX 复合：Trigger / Content / Header · sideOffset 宏别名）："
                    </Label>
                    <Popover id="gallery:popover" open={pop_open}>
                        <PopoverTrigger>
                            <Button variant="outline" size="sm">
                                { if pop_open.get() { "Close popover" } else { "Open popover" } }
                            </Button>
                        </PopoverTrigger>
                        <PopoverContent side="bottom" align="center" sideOffset=4>
                            <PopoverHeader>
                                <PopoverTitle>"Dimensions"</PopoverTitle>
                                <PopoverDescription>
                                    "Set the dimensions for the layer. Escape closes (stacked overlay)."
                                </PopoverDescription>
                            </PopoverHeader>
                            <Label text_size="0.875rem">"w-72 · p-4 · bg-popover · shadow-md"</Label>
                        </PopoverContent>
                    </Popover>
                    <Label text_size="0.75rem" color="muted-foreground" leading="normal">
                        "Popover Builder 双写（open_signal + PopoverTrigger / popover_content）："
                    </Label>
                    { popover("gallery:popover_builder")
                            .open_signal(pop_builder_open)
                            .child(|| {
                                let mut frag = Element::new();
                                frag.set_attr("jadix_fragment", "1");
                                frag.children(vec![
                                    PopoverTrigger(PopoverTriggerProps {
                                        id: String::new(),
                                        children: Some(
                                            button("gallery:popover_builder_btn")
                                                .variant("secondary")
                                                .size("sm")
                                                .text("Open (Builder)")
                                                .into(),
                                        ),
                                        base: None,
                                    }),
                                    popover_content(
                                        PopoverContentOpts {
                                            side: "bottom".into(),
                                            align: "start".into(),
                                            side_offset: 4.0,
                                            strategy: None,
                                            width: Some("18rem".into()),
                                            lock_page_scroll: false,
                                            scroll_freeze: None,
                                            ..Default::default()
                                        },
                                        || {
                                            Element::from(
                                                vstack()
                                                    .w("100%")
                                                    .gap("0.5rem")
                                                    .child(PopoverTitle(PopoverTitleProps {
                                                        children: Some(
                                                            Element::new().text("Builder panel"),
                                                        ),
                                                        base: None,
                                                    }))
                                                    .child(PopoverDescription(
                                                        PopoverDescriptionProps {
                                                            children: Some(Element::new().text(
                                                                "Same surface as RSX PopoverContent.",
                                                            )),
                                                            base: None,
                                                        },
                                                    )),
                                            )
                                        },
                                    ),
                                ])
                            }) }
                    <VStack gap="0.75rem" w="100%">
                        <Label text_size="0.75rem" color="muted-foreground">
                            { format!(
                                "Shared tab index: {}. Focus id `{}gallery_tabs_demo` (Tab to focus order, then ← / →).",
                                tab_idx.get(),
                                TABSTRIP_FOCUS_PREFIX,
                            ) }
                        </Label>
                        <HStack gap="0.35rem" items="center" wrap="wrap">
                            <Button variant="ghost" size="sm" on_click={move || tab_idx.set(0)}>"0"</Button>
                            <Button variant="ghost" size="sm" on_click={move || tab_idx.set(1)}>"1"</Button>
                            <Button variant="ghost" size="sm" on_click={move || tab_idx.set(2)}>"2"</Button>
                        </HStack>
                        { Element::from(
                            tabs_headless("gallery_tabs_demo", 3).selected_signal(tab_idx),
                        ) }
                    </VStack>
                </VStack>
            </VStack>
        </MyCard>
    }
}

#[component]
pub(crate) fn TextareaComboboxShowcase() -> Element {
    let notes = use_signal("gal_textarea_notes", String::new);
    let wrap_soft = use_signal("gal_textarea_wrap_soft", || {
        "padding: 8px 16px; background: #ffffff; border-width: 1px; border-radius: 6px — soft wrap folds this long hard line."
            .to_string()
    });
    let wrap_off = use_signal("gal_textarea_wrap_off", || {
        "padding: 8px 16px; background: #ffffff; border-width: 1px; border-radius: 6px — wrap=off scrolls horizontally (no clip)."
            .to_string()
    });
    let wrap_char = use_signal("gal_textarea_wrap_char", || {
        "Supercalifragilisticexpialidocious_unbreakable_token_plus_中文emoji😀 — char may mid-break."
            .to_string()
    });
    let wrap_bidi = use_signal("gal_textarea_wrap_bidi", || {
        "Hello עברית\u{2060}789 and العربية — soft wrap keeps WJ pairs.".to_string()
    });
    let combo = use_signal("gal_combo_city", || "sh".to_string());
    let tab_view = use_signal("gal_tabs_list_view", || "list".to_string());

    ui! {
        <MyCard title="Textarea · Combobox · Tabs">
            <VStack w="100%" gap="0.75rem">
                <Label text_size="0.75rem" color="muted-foreground" leading="normal">
                    "多行输入、可过滤 Combobox、复合 Tabs（List / Trigger / Content）。ToggleGroup 见下一张卡片。"
                </Label>
                <Textarea id="gal-textarea" placeholder="多行备注…（可拖右下角改高）" rows={4} value={notes} />
                <Textarea id="gal-textarea-fixed" placeholder="固定高度（resizable=false）" rows={3} resizable={false} />
                <Label text_size="0.875rem" leading="normal">{ format!("Textarea: {}", notes.get()) }</Label>
                <Label text_size="0.75rem" font_weight="600" leading="normal">
                    "Textarea wrap · word（默认） / char / off + BiDi"
                </Label>
                <Label text_size="0.75rem" color="muted-foreground" leading="normal">
                    "窄栏对照：word 按词折行；char 可中词断开；off 横向滚动。BiDi 样例验证 WJ。拖选出视口应持续跟手滚。"
                </Label>
                <HStack w="100%" min_w="0" gap="1rem" items="start" wrap="wrap">
                    <VStack gap="0.35rem" w="16rem" min_w="0" max_w="100%">
                        <Label font_weight="700" text_size="0.75rem">"wrap=\"word\"（默认）"</Label>
                        <Textarea
                            id="gal-ta-wrap-soft"
                            wrap="word"
                            rows={5}
                            resizable={false}
                            value={wrap_soft}
                        />
                    </VStack>
                    <VStack gap="0.35rem" w="16rem" min_w="0" max_w="100%">
                        <Label font_weight="700" text_size="0.75rem">"wrap=\"char\" · 失焦保留选区"</Label>
                        <Textarea
                            id="gal-ta-wrap-char"
                            wrap="char"
                            selection_on_blur="retain"
                            rows={5}
                            resizable={false}
                            value={wrap_char}
                        />
                    </VStack>
                    <VStack gap="0.35rem" w="16rem" min_w="0" max_w="100%">
                        <Label font_weight="700" text_size="0.75rem">"wrap=\"off\"（横向滚动）"</Label>
                        <Textarea
                            id="gal-ta-wrap-off"
                            wrap="off"
                            rows={5}
                            resizable={false}
                            value={wrap_off}
                        />
                    </VStack>
                    <VStack gap="0.35rem" w="16rem" min_w="0" max_w="100%">
                        <Label font_weight="700" text_size="0.75rem">"BiDi + WJ（word）"</Label>
                        <Textarea
                            id="gal-ta-wrap-bidi"
                            wrap="word"
                            rows={5}
                            resizable={false}
                            value={wrap_bidi}
                        />
                    </VStack>
                </HStack>
                <Label text_size="0.75rem" color="muted-foreground" leading="normal">
                    "选字后点击其他输入框：默认立即清除选区；保留模式显示淡色选区。"
                </Label>
                <HStack w="100%" min_w="0" gap="1rem" wrap="wrap">
                    <Input id="gal-input-blur-clear" w="16rem" placeholder="失焦清除选区（默认）" />
                    <Input id="gal-input-blur-retain" w="16rem" placeholder="失焦保留淡色选区" selection_on_blur="retain" />
                </HStack>
                                <Combobox id="gal-combo" value={combo} placeholder="搜索城市…" clearable={true}>
                    <ComboboxInput class="w-full" />
                    <ComboboxContent max_h="16rem">
                        <ComboboxList>
                            <ComboboxGroup>
                                <ComboboxLabel>"East China"</ComboboxLabel>
                                <ComboboxItem value="sh">"Shanghai"</ComboboxItem>
                                <ComboboxItem value="hz">"Hangzhou"</ComboboxItem>
                            </ComboboxGroup>
                            <ComboboxSeparator />
                            <ComboboxGroup>
                                <ComboboxLabel>"South China"</ComboboxLabel>
                                <ComboboxItem value="sz">"Shenzhen"</ComboboxItem>
                                <ComboboxItem value="gz">"Guangzhou"</ComboboxItem>
                            </ComboboxGroup>
                            <ComboboxSeparator />
                            <ComboboxItem value="bj">"Beijing"</ComboboxItem>
                        </ComboboxList>
                        <ComboboxEmpty>"No results."</ComboboxEmpty>
                    </ComboboxContent>
                </Combobox>
                <Label text_size="0.875rem">{ format!("Combobox: {}", combo.get()) }</Label>
                <Tabs id="gal-tabs" value={tab_view} default_value="list">
                    <TabsList>
                        <TabsTrigger value="list">"List"</TabsTrigger>
                        <TabsTrigger value="grid">"Grid"</TabsTrigger>
                        <TabsTrigger value="map">"Map"</TabsTrigger>
                    </TabsList>
                    <TabsContent value="list">
                        <Label text_size="0.875rem" leading="normal">"List panel — click a tab or use ←/→ then Enter."</Label>
                    </TabsContent>
                    <TabsContent value="grid">
                        <Label text_size="0.875rem" leading="normal">"Grid panel"</Label>
                    </TabsContent>
                    <TabsContent value="map">
                        <Label text_size="0.875rem" leading="normal">"Map panel"</Label>
                    </TabsContent>
                </Tabs>
                <Label text_size="0.875rem">{ format!("Tabs: {}", tab_view.get()) }</Label>
                <Tabs id="gal-tabs-line" default_value="one">
                    <TabsList variant="line">
                        <TabsTrigger value="one">"One"</TabsTrigger>
                        <TabsTrigger value="two">"Two"</TabsTrigger>
                    </TabsList>
                    <TabsContent value="one">
                        <Label text_size="0.75rem" color="muted-foreground">"line variant"</Label>
                    </TabsContent>
                    <TabsContent value="two">
                        <Label text_size="0.75rem" color="muted-foreground">"second line tab"</Label>
                    </TabsContent>
                </Tabs>
            </VStack>
        </MyCard>
    }
}

#[component]
pub(crate) fn ToggleGroupShowcase() -> jadix::core::tree::Element {
    let align = use_signal("gal_tg_align", || "left".to_string());
    let align_joined = use_signal("gal_tg_align_joined", || "center".to_string());
    let formats = use_signal("gal_tg_formats", || vec!["bold".to_string()]);
    let formats_joined = use_signal("gal_tg_formats_joined", || vec!["italic".to_string()]);
    let size_demo = use_signal("gal_tg_size", || "sm".to_string());
    let vertical = use_signal("gal_tg_vertical", || "top".to_string());

    ui! {
        <MyCard title="8b) ToggleGroup（ShadCN · 互斥 / 连体变体）">
            <VStack w="100%" gap="0.75rem">
                <Label w="100%" text_size="0.75rem" color="muted-foreground" leading="normal">
                    "Dual style: <ToggleGroup> / <ToggleGroupItem> and toggle_group().item(); default chips are independent (ShadCN spacing=2). spacing=0 / joined / layout=joined reuses ButtonGroup fused corners. type=single is exclusive; type=multiple lets each item stay on."
                </Label>

                <Label font_weight="700" text_size="0.875rem">"Single · independent (default)"</Label>
                <HStack w="100%" min_w="0" gap="0.75rem" items="center" wrap="wrap">
                    <ToggleGroup id="gal-tg-align" value={align} default_value="left" variant="outline">
                        <ToggleGroupItem value="left">"Left"</ToggleGroupItem>
                        <ToggleGroupItem value="center">"Center"</ToggleGroupItem>
                        <ToggleGroupItem value="right">"Right"</ToggleGroupItem>
                    </ToggleGroup>
                    { Element::from(
                        toggle_group("gal-tg-align-builder")
                            .value_signal(align)
                            .default_value("left")
                            .variant("outline")
                            .item("left", "Left")
                            .item("center", "Center")
                            .item("right", "Right"),
                    ) }
                </HStack>
                <Label text_size="0.75rem" color="muted-foreground">
                    { format!("Align (exclusive, click again to clear): {}", align.get()) }
                </Label>

                <Separator />

                <Label font_weight="700" text_size="0.875rem">"Single · joined (ButtonGroup fusion)"</Label>
                <HStack w="100%" min_w="0" gap="0.75rem" items="center" wrap="wrap">
                    <ToggleGroup
                        id="gal-tg-align-joined"
                        value={align_joined}
                        default_value="center"
                        variant="outline"
                        spacing={0}
                    >
                        <ToggleGroupItem value="left">"Left"</ToggleGroupItem>
                        <ToggleGroupItem value="center">"Center"</ToggleGroupItem>
                        <ToggleGroupItem value="right">"Right"</ToggleGroupItem>
                    </ToggleGroup>
                    { Element::from(
                        toggle_group("gal-tg-align-joined-b")
                            .value_signal(align_joined)
                            .variant("outline")
                            .joined(true)
                            .item("left", "Left")
                            .item("center", "Center")
                            .item("right", "Right"),
                    ) }
                </HStack>
                <Label text_size="0.75rem" color="muted-foreground">
                    { format!("Joined exclusive: {}", align_joined.get()) }
                </Label>

                <Separator />

                <Label font_weight="700" text_size="0.875rem">"Multiple · independent (not exclusive)"</Label>
                <ToggleGroup id="gal-tg-fmt" type="multiple" values={formats} variant="outline">
                    <ToggleGroupItem value="bold">"B"</ToggleGroupItem>
                    <ToggleGroupItem value="italic">"I"</ToggleGroupItem>
                    <ToggleGroupItem value="underline">"U"</ToggleGroupItem>
                </ToggleGroup>
                <Label text_size="0.75rem" color="muted-foreground">
                    { format!("Formats: {:?}", formats.get()) }
                </Label>

                <Label font_weight="700" text_size="0.875rem">"Multiple · joined"</Label>
                { Element::from(
                    toggle_group("gal-tg-fmt-joined")
                        .multiple(true)
                        .value_multi_signal(formats_joined)
                        .variant("outline")
                        .layout("joined")
                        .item("bold", "B")
                        .item("italic", "I")
                        .item("underline", "U"),
                ) }
                <Label text_size="0.75rem" color="muted-foreground">
                    { format!("Joined multi: {:?}", formats_joined.get()) }
                </Label>

                <Separator />

                <Label font_weight="700" text_size="0.875rem">"Size · required · vertical · disabled"</Label>
                <HStack w="100%" min_w="0" gap="0.75rem" items="center" wrap="wrap">
                    <ToggleGroup id="gal-tg-size" value={size_demo} default_value="sm" size="sm" required={true}>
                        <ToggleGroupItem value="sm">"SM"</ToggleGroupItem>
                        <ToggleGroupItem value="default">"Default"</ToggleGroupItem>
                        <ToggleGroupItem value="lg">"LG"</ToggleGroupItem>
                    </ToggleGroup>
                    <ToggleGroup
                        id="gal-tg-vert"
                        value={vertical}
                        default_value="top"
                        orientation="vertical"
                        variant="outline"
                        spacing={0}
                    >
                        <ToggleGroupItem value="top">"Top"</ToggleGroupItem>
                        <ToggleGroupItem value="middle">"Middle"</ToggleGroupItem>
                        <ToggleGroupItem value="bottom">"Bottom"</ToggleGroupItem>
                    </ToggleGroup>
                    <ToggleGroup id="gal-tg-disabled" default_value="on" variant="outline" disabled={true}>
                        <ToggleGroupItem value="on">"On"</ToggleGroupItem>
                        <ToggleGroupItem value="off">"Off"</ToggleGroupItem>
                    </ToggleGroup>
                </HStack>
                <Label text_size="0.75rem" color="muted-foreground">
                    { format!("Size (required): {} · Vertical: {}", size_demo.get(), vertical.get()) }
                </Label>
            </VStack>
        </MyCard>
    }
}

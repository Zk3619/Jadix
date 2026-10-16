//! 基础分区：按钮、气泡、头像等。

use crate::imports::*;
use crate::shared::*;

#[component]
pub(crate) fn BadgeShowcase() -> jadix::core::tree::Element {
    ui! {
        <MyCard title="1c) Badge 变体（ShadCN）">
            <VStack w="100%" min_w="0" gap="0.75rem">
                <Label w="100%" text_size="0.75rem" color="muted-foreground" leading="normal">
                    "双写法：<Badge variant> 与 badge().variant().text()；success / warning(=warn) / error≡destructive 为 Button soft tint；Icon 用 name+animate-spin，图片请用 Image src"
                </Label>
                <HStack w="100%" min_w="0" gap="0.5rem" items="center" wrap="wrap">
                    <Badge>"Default"</Badge>
                    <Badge variant="secondary">"Secondary"</Badge>
                    <Badge variant="success">"Success"</Badge>
                    <Badge variant="warning">"Warning"</Badge>
                    <Badge variant="destructive">"Destructive"</Badge>
                    <Badge variant="error">"Error"</Badge>
                    <Badge variant="outline">"Outline"</Badge>
                    <Badge variant="ghost">"Ghost"</Badge>
                    <Badge variant="link">"Link"</Badge>
                </HStack>
                <HStack w="100%" min_w="0" gap="0.5rem" items="center" wrap="wrap">
                    <Badge variant="secondary">
                        <Icon name="badge-check" size="0.75rem" />
                        "Verified"
                    </Badge>
                    <Badge variant="outline">
                        <Icon name="clock" size="0.75rem" />
                        "Pending"
                    </Badge>
                    <Badge variant="secondary">
                        <Icon name="loader-circle" size="0.75rem" class="animate-spin" />
                        "Loading"
                    </Badge>
                    { Element::from(badge().variant("warn").text("Builder Warn")) }
                    { Element::from(badge().variant("destructive").text("Builder Error")) }
                </HStack>
            </VStack>
        </MyCard>
    }
}

#[component]
pub(crate) fn KbdShowcase() -> jadix::core::tree::Element {
    ui! {
        <MyCard title="1c-k) Kbd（ShadCN keycap）">
            <VStack w="100%" min_w="0" gap="0.75rem">
                <Label w="100%" text_size="0.75rem" color="muted-foreground" leading="normal">
                    "Dual style: <Kbd> / <KbdGroup> and kbd() / kbd_group(); muted keycap chrome; presentational (no required id)."
                </Label>
                <VStack w="100%" min_w="0" gap="0.5rem">
                    <Label font_weight="700" text_size="0.75rem">"Single keys"</Label>
                    <HStack w="100%" min_w="0" gap="0.5rem" items="center" wrap="wrap">
                        <Kbd>"⌘"</Kbd>
                        <Kbd>"⇧"</Kbd>
                        <Kbd>"⌥"</Kbd>
                        <Kbd>"⌃"</Kbd>
                        <Kbd>"Esc"</Kbd>
                        { Element::from(kbd().text("Enter")) }
                    </HStack>
                </VStack>
                <VStack w="100%" min_w="0" gap="0.5rem">
                    <Label font_weight="700" text_size="0.75rem">"KbdGroup"</Label>
                    <HStack w="100%" min_w="0" gap="0.75rem" items="center" wrap="wrap">
                        <KbdGroup>
                            <Kbd>"⌘"</Kbd>
                            <Kbd>"K"</Kbd>
                        </KbdGroup>
                        <KbdGroup>
                            <Kbd>"Ctrl"</Kbd>
                            <Kbd>"Shift"</Kbd>
                            <Kbd>"P"</Kbd>
                        </KbdGroup>
                        { Element::from(kbd_group().kbd("Ctrl").kbd("B")) }
                    </HStack>
                </VStack>
                <VStack w="100%" min_w="0" gap="0.5rem">
                    <Label font_weight="700" text_size="0.75rem">"In Button / InputAdornment"</Label>
                    <HStack w="100%" min_w="0" gap="0.75rem" items="center" wrap="wrap">
                        <Button variant="outline">
                            "Search"
                            <KbdGroup>
                                <Kbd>"⌘"</Kbd>
                                <Kbd>"K"</Kbd>
                            </KbdGroup>
                        </Button>
                        <InputAdornment>
                            <InputAdornmentInput id="gal-kbd-search" placeholder="Search…" />
                            <InputAdornmentAddon align="inline-end">
                                <Kbd>"⌘"</Kbd>
                                <Kbd>"K"</Kbd>
                            </InputAdornmentAddon>
                        </InputAdornment>
                    </HStack>
                </VStack>
            </VStack>
        </MyCard>
    }
}

#[component]
pub(crate) fn SkeletonShowcase() -> jadix::core::tree::Element {
    ui! {
        <MyCard title="1c-s) Skeleton（ShadCN loading placeholder）">
            <VStack w="100%" min_w="0" gap="0.75rem">
                <Label w="100%" text_size="0.75rem" color="muted-foreground" leading="normal">
                    "Dual style: <Skeleton> and skeleton(); default bg-accent + rounded-md + animate-pulse; size/shape via w/h/rounded/class."
                </Label>
                <VStack w="100%" min_w="0" gap="0.5rem">
                    <Label font_weight="700" text_size="0.75rem">"Text lines"</Label>
                    <VStack w="100%" min_w="0" gap="0.5rem">
                        <Skeleton class="h-4 w-[250px]" />
                        <Skeleton class="h-4 w-[200px]" />
                        { Element::from(skeleton().w("180px").h("1rem")) }
                    </VStack>
                </VStack>
                <VStack w="100%" min_w="0" gap="0.5rem">
                    <Label font_weight="700" text_size="0.75rem">"Avatar row"</Label>
                    <HStack w="100%" min_w="0" gap="1rem" items="center">
                        <Skeleton class="h-12 w-12 rounded-full" />
                        <VStack gap="0.5rem" min_w="0">
                            <Skeleton class="h-4 w-[150px]" />
                            <Skeleton class="h-4 w-[100px]" />
                        </VStack>
                    </HStack>
                </VStack>
                <VStack w="100%" min_w="0" gap="0.5rem">
                    <Label font_weight="700" text_size="0.75rem">"Card"</Label>
                    <VStack
                        w="18rem"
                        gap="1rem"
                        p="1rem"
                        border_color="border"
                        border_width="1"
                        rounded="0.75rem"
                    >
                        <Skeleton class="h-40 w-full rounded-lg" />
                        <Skeleton class="h-4 w-3/4" />
                        <Skeleton class="h-4 w-1/2" />
                        { Element::from(skeleton().w("100%").h("2.5rem").rounded("0.375rem")) }
                    </VStack>
                </VStack>
            </VStack>
        </MyCard>
    }
}

#[component]
pub(crate) fn EmptyShowcase() -> jadix::core::tree::Element {
    ui! {
        <MyCard title="1c-e) Empty（ShadCN empty state）">
            <VStack w="100%" min_w="0" gap="1rem">
                <Label w="100%" text_size="0.75rem" color="muted-foreground" leading="normal">
                    "Dual style: <Empty> compound tags and empty() / empty_header() builders; EmptyMedia variant=icon for muted icon chrome."
                </Label>
                <HStack w="100%" min_w="0" gap="1rem" wrap="wrap" items="stretch">
                    <Box
                        flex_grow="1"
                        min_w="16rem"
                        min_h="14rem"
                        border_color="border"
                        border_width="1"
                        rounded="0.75rem"
                        p="1rem"
                    >
                        <Empty>
                            <EmptyHeader>
                                <EmptyMedia variant="icon">
                                    <Icon name="inbox" color="foreground" />
                                </EmptyMedia>
                                <EmptyTitle>"No Projects Yet"</EmptyTitle>
                                <EmptyDescription>
                                    "You have not created a project yet. Get started by creating your first project."
                                </EmptyDescription>
                            </EmptyHeader>
                            <EmptyContent>
                                <Button>"Create Project"</Button>
                            </EmptyContent>
                        </Empty>
                    </Box>
                    <Box
                        flex_grow="1"
                        min_w="16rem"
                        min_h="14rem"
                        border_color="border"
                        border_width="1"
                        rounded="0.75rem"
                        p="1rem"
                    >
                        { Element::from(
                            empty()
                                .header(
                                    empty_header()
                                        .media_icon(
                                            build_icon_text_node(
                                                lucide_glyph("folder-open").unwrap_or_else(|| "…".into())
                                            )
                                            .text_size(24.0)
                                            .text_color(parse_text_color("foreground"))
                                        )
                                        .title("No files")
                                        .description("Upload a file to get started.")
                                )
                                .content_child(button("").text("Upload"))
                        ) }
                    </Box>
                </HStack>
            </VStack>
        </MyCard>
    }
}

#[component]
pub(crate) fn MarkerShowcase() -> jadix::core::tree::Element {
    let marker_variant = use_signal("gal_marker_variant", || "default".to_string());
    let marker_var_btn = marker_variant;
    let marker_var_live = marker_variant;
    ui! {
        <MyCard title="1c-m) Marker（ShadCN conversation marker）">
            <VStack w="100%" min_w="0" gap="0.75rem">
                <Label w="100%" text_size="0.75rem" color="muted-foreground" leading="normal">
                    "Dual style: Marker / MarkerIcon / MarkerContent and marker() / ui_marker(). Variants: default, border, separator. PascalCase is the UI component; lowercase marker stays SVG. MarkerContent class shimmer is the streaming-text sweep."
                </Label>
                <VStack w="100%" min_w="0" max_w="24rem" gap="1rem">
                    <Label font_weight="700" text_size="0.75rem">"Default + icon"</Label>
                    <Marker>
                        <MarkerIcon>
                            <Icon name="git-branch" size="1rem" color="muted-foreground" />
                        </MarkerIcon>
                        <MarkerContent>"Switched to a new branch"</MarkerContent>
                    </Marker>
                    <Marker role="status">
                        <MarkerIcon>
                            <Icon name="loader-circle" size="1rem" class="animate-spin" color="muted-foreground" />
                        </MarkerIcon>
                        <MarkerContent class="shimmer">"Thinking..."</MarkerContent>
                    </Marker>
                    <Marker>
                        <MarkerIcon>
                            <Icon name="search" size="1rem" color="muted-foreground" />
                        </MarkerIcon>
                        <MarkerContent>"Explored 4 files"</MarkerContent>
                    </Marker>
                </VStack>
                <VStack w="100%" min_w="0" max_w="24rem" gap="0.75rem">
                    <Label font_weight="700" text_size="0.75rem">"Shimmer (streaming text)"</Label>
                    <Marker role="status">
                        <MarkerIcon>
                            <Icon name="loader-circle" size="1rem" class="animate-spin" color="muted-foreground" />
                        </MarkerIcon>
                        <MarkerContent class="shimmer">"Thinking..."</MarkerContent>
                    </Marker>
                    <Marker variant="separator">
                        <MarkerContent class="shimmer">"Reading 4 files"</MarkerContent>
                    </Marker>
                    <Marker>
                        <MarkerContent class="shimmer shimmer-duration-1000">"Generating response…"</MarkerContent>
                    </Marker>
                    <Marker>
                        <MarkerContent class="shimmer shimmer-count-3 shimmer-duration-900">
                            "Three sweeps then settle"
                        </MarkerContent>
                    </Marker>
                    <Box class="shimmer-box shimmer-duration-1600" w="100%" px="0.75rem" py="0.5rem" bg="secondary" rounded="0.5rem">
                        <Label text_size="0.875rem" color="muted-foreground">"shimmer-box · overlay on container"</Label>
                    </Box>
                </VStack>
                <VStack w="100%" min_w="0" max_w="24rem" gap="0.75rem">
                    <Label font_weight="700" text_size="0.75rem">"Separator"</Label>
                    <Marker w="100%" variant="separator">
                        <MarkerContent>"Today"</MarkerContent>
                    </Marker>
                    <Marker w="100%" variant="separator">
                        <MarkerIcon>
                            <Icon name="search" size="1rem" color="muted-foreground" />
                        </MarkerIcon>
                        <MarkerContent>"Explored 4 files"</MarkerContent>
                    </Marker>
                    <Marker w="100%" variant="separator">
                        <MarkerContent>"Conversation compacted"</MarkerContent>
                    </Marker>
                </VStack>
                <VStack w="100%" min_w="0" max_w="24rem" gap="0.75rem">
                    <Label font_weight="700" text_size="0.75rem">"Border"</Label>
                    <Marker variant="border">
                        <MarkerIcon>
                            <Icon name="git-branch" size="1rem" color="muted-foreground" />
                        </MarkerIcon>
                        <MarkerContent>"Switched to release-candidate"</MarkerContent>
                    </Marker>
                    <Marker variant="border">
                        <MarkerIcon>
                            <Icon name="file-text" size="1rem" color="muted-foreground" />
                        </MarkerIcon>
                        <MarkerContent>"Opened implementation notes"</MarkerContent>
                    </Marker>
                </VStack>
                <VStack w="100%" min_w="0" max_w="24rem" gap="0.75rem">
                    <Label font_weight="700" text_size="0.75rem">"Stacked icon (flex-col)"</Label>
                    <Marker class="flex-col">
                        <MarkerIcon>
                            <Icon name="book-open-check" size="1rem" color="muted-foreground" />
                        </MarkerIcon>
                        <MarkerContent>"Syncing completed"</MarkerContent>
                    </Marker>
                </VStack>
                <VStack w="100%" min_w="0" max_w="24rem" gap="0.5rem">
                    <Label font_weight="700" text_size="0.75rem">"Reactive variant bind"</Label>
                    <HStack w="100%" gap="0.5rem" wrap="wrap">
                        <Button size="sm" variant="outline" on_click={move || marker_var_btn.set("default".into())}>"default"</Button>
                        <Button size="sm" variant="outline" on_click={move || marker_var_btn.set("border".into())}>"border"</Button>
                        <Button size="sm" variant="outline" on_click={move || marker_var_btn.set("separator".into())}>"separator"</Button>
                    </HStack>
                    <Marker w="100%" variant={ marker_var_live.get() }>
                        <MarkerIcon>
                            <Icon name="info" size="1rem" color="muted-foreground" />
                        </MarkerIcon>
                        <MarkerContent>
                            { format!("Reactive variant: {}", marker_var_live.get()) }
                        </MarkerContent>
                    </Marker>
                </VStack>
                <Box w="100%" max_w="24rem">
                    { Element::from(
                        marker()
                            .variant("separator")
                            .icon(
                                build_icon_text_node(lucide_glyph("check").unwrap_or_else(|| "…".into()))
                                    .text_size(16.0)
                                    .text_color(parse_text_color("muted-foreground"))
                            )
                            .content("Builder · marker().variant().icon().content()")
                    ) }
                </Box>
            </VStack>
        </MyCard>
    }
}

#[component]
pub(crate) fn MessageShowcase() -> jadix::core::tree::Element {
    let msg_align = use_signal("gal_message_align", || "start".to_string());
    let msg_align_btn = msg_align;
    let msg_align_live = msg_align;
    message_showcase_tree(msg_align_btn, msg_align_live)
}

pub(crate) fn message_showcase_tree(
    msg_align_btn: Signal<String>,
    msg_align_live: Signal<String>,
) -> jadix::core::tree::Element {
    ui! {
        <MyCard title="1c-msg) Message（ShadCN conversation row）">
            <VStack w="100%" min_w="0" gap="0.75rem">
                <Label w="100%" text_size="0.75rem" color="muted-foreground" leading="normal">
                    "Conversation row: Message, MessageGroup, MessageAvatar, MessageContent, MessageHeader, MessageFooter."
                </Label>
                <Label w="100%" text_size="0.75rem" color="muted-foreground" leading="normal">
                    "Dual style with message() / message_group(). align is start or end. Empty avatar keeps grouped turns in column. Put the chat surface in Bubble. Images and files reuse Attachment (vertical + variant=image for photos). Download / preview stay on the app (on_click), not Message."
                </Label>
                <VStack w="100%" min_w="0" max_w="28rem" gap="0.5rem">
                    <Label font_weight="700" text_size="0.75rem">"Start / end"</Label>
                    <MessageGroup>
                        <Message align="start">
                            <MessageAvatar>
                                <Avatar>
                                    <AvatarFallback>"CN"</AvatarFallback>
                                </Avatar>
                            </MessageAvatar>
                            <MessageContent>
                                <MessageHeader>"Oliver"</MessageHeader>
                                <Bubble variant="muted" align="start">
                                    <BubbleContent>"How can I help you today?"</BubbleContent>
                                </Bubble>
                                <MessageFooter>"4:55 PM"</MessageFooter>
                            </MessageContent>
                        </Message>
                        <Message align="end">
                            <MessageAvatar>
                                <Avatar>
                                    <AvatarFallback>"ME"</AvatarFallback>
                                </Avatar>
                            </MessageAvatar>
                            <MessageContent>
                                <MessageHeader>"You"</MessageHeader>
                                <Bubble variant="default" align="end">
                                    <BubbleContent>"It's a one-line change."</BubbleContent>
                                </Bubble>
                                <MessageFooter>"Delivered"</MessageFooter>
                            </MessageContent>
                        </Message>
                    </MessageGroup>
                </VStack>
                <VStack w="100%" min_w="0" max_w="28rem" gap="0.5rem">
                    <Label font_weight="700" text_size="0.75rem">"Group (empty avatar spacer)"</Label>
                    <MessageGroup>
                        <Message>
                            <MessageAvatar />
                            <MessageContent>
                                <Bubble variant="muted">
                                    <BubbleContent>"I checked the registry addresses."</BubbleContent>
                                </Bubble>
                            </MessageContent>
                        </Message>
                        <Message>
                            <MessageAvatar>
                                <Avatar>
                                    <AvatarFallback>"CN"</AvatarFallback>
                                </Avatar>
                            </MessageAvatar>
                            <MessageContent>
                                <Bubble variant="muted">
                                    <BubbleContent>"The component and example JSON now live under the UI registry."</BubbleContent>
                                </Bubble>
                            </MessageContent>
                        </Message>
                    </MessageGroup>
                </VStack>
                <VStack w="100%" min_w="0" max_w="28rem" gap="0.5rem">
                    <Label font_weight="700" text_size="0.75rem">"Header / footer actions"</Label>
                    <Message align="start">
                        <MessageAvatar>
                            <Avatar>
                                <AvatarFallback>"OL"</AvatarFallback>
                            </Avatar>
                        </MessageAvatar>
                        <MessageContent>
                            <MessageHeader>"Oliver"</MessageHeader>
                            <Bubble variant="secondary" align="start">
                                <BubbleContent>"Send the report to the team."</BubbleContent>
                            </Bubble>
                            <MessageFooter>
                                <Button variant="ghost" size="icon-xs" class="rounded-full">
                                    <Icon name="copy" size="0.75rem" />
                                </Button>
                                <Button variant="ghost" size="icon-xs" class="rounded-full">
                                    <Icon name="refresh-cw" size="0.75rem" />
                                </Button>
                            </MessageFooter>
                        </MessageContent>
                    </Message>
                </VStack>
                <VStack w="100%" min_w="0" max_w="28rem" gap="0.5rem">
                    <Label font_weight="700" text_size="0.75rem">"Attachment (image + file)"</Label>
                    <MessageGroup>
                        <Message align="end">
                            <MessageAvatar>
                                <Avatar>
                                    <AvatarFallback>"ME"</AvatarFallback>
                                </Avatar>
                            </MessageAvatar>
                            <MessageContent>
                                <Attachment orientation="vertical">
                                    <AttachmentMedia variant="image">
                                        <Image texture="demo-checker" />
                                    </AttachmentMedia>
                                </Attachment>
                                <Bubble variant="default" align="end">
                                    <BubbleContent>"Here's the image. Can you add it to the PDF? Use it for the cover page."</BubbleContent>
                                </Bubble>
                            </MessageContent>
                        </Message>
                        <Message align="start">
                            <MessageAvatar>
                                <Avatar>
                                    <AvatarFallback>"CN"</AvatarFallback>
                                </Avatar>
                            </MessageAvatar>
                            <MessageContent>
                                <Bubble variant="muted" align="start">
                                    <BubbleContent>"Done. Here's the PDF with the image added as the cover page."</BubbleContent>
                                </Bubble>
                                <Attachment state="done" size="default" orientation="horizontal">
                                    <AttachmentMedia variant="icon">
                                        <Icon name="file-text" size="0.875rem" />
                                    </AttachmentMedia>
                                    <AttachmentContent>
                                        <AttachmentTitle>"sales-dashboard.pdf"</AttachmentTitle>
                                        <AttachmentDescription>"PDF · 2.4 MB"</AttachmentDescription>
                                    </AttachmentContent>
                                    <AttachmentActions>
                                        <AttachmentAction id="gal-msg-att-dl" variant="secondary" size="icon-sm">
                                            <Icon name="download" size="0.875rem" />
                                        </AttachmentAction>
                                    </AttachmentActions>
                                </Attachment>
                            </MessageContent>
                        </Message>
                        <Message align="end">
                            <MessageAvatar>
                                <Avatar>
                                    <AvatarFallback>"ME"</AvatarFallback>
                                </Avatar>
                            </MessageAvatar>
                            <MessageContent>
                                <Bubble variant="default" align="end">
                                    <BubbleContent>"Thanks. Looks good."</BubbleContent>
                                </Bubble>
                            </MessageContent>
                        </Message>
                    </MessageGroup>
                </VStack>
                <VStack w="100%" min_w="0" max_w="28rem" gap="0.5rem">
                    <Label font_weight="700" text_size="0.75rem">"Status (Marker inside Message)"</Label>
                    <Message>
                        <Marker role="status">
                            <MarkerIcon>
                                <Icon name="loader-circle" size="1rem" class="animate-spin" color="muted-foreground" />
                            </MarkerIcon>
                            <MarkerContent class="shimmer">"Oliver is typing..."</MarkerContent>
                        </Marker>
                    </Message>
                </VStack>
                <VStack w="100%" min_w="0" max_w="28rem" gap="0.5rem">
                    <Label font_weight="700" text_size="0.75rem">"Reactive align bind"</Label>
                    <HStack w="100%" gap="0.5rem" wrap="wrap">
                        <Button size="sm" variant="outline" on_click={move || msg_align_btn.set("start".into())}>"start"</Button>
                        <Button size="sm" variant="outline" on_click={move || msg_align_btn.set("end".into())}>"end"</Button>
                    </HStack>
                    <Message align={ msg_align_live.get() }>
                        <MessageAvatar>
                            <Avatar>
                                <AvatarFallback>"ME"</AvatarFallback>
                            </Avatar>
                        </MessageAvatar>
                        <MessageContent>
                            <MessageHeader>{ format!("align={}", msg_align_live.get()) }</MessageHeader>
                            <Bubble variant="outline" align={ msg_align_live.get() }>
                                <BubbleContent>"The row packs to this side."</BubbleContent>
                            </Bubble>
                            <MessageFooter>"Delivered"</MessageFooter>
                        </MessageContent>
                    </Message>
                </VStack>
                <Box w="100%" max_w="28rem">
                    { Element::from(
                        message_group().child(
                            message()
                                .align("end")
                                .avatar(avatar().fallback("B"))
                                .header("Builder")
                                .content_child(
                                    bubble()
                                        .variant("secondary")
                                        .align("end")
                                        .child(bubble_content().text("On it, one-line change."))
                                )
                                .footer("Delivered")
                        )
                    ) }
                </Box>
            </VStack>
        </MyCard>
    }
}

#[derive(Clone, PartialEq)]
pub(crate) struct GalMsMsg {
    id: String,
    user: bool,
    text: String,
}

pub(crate) fn gal_ms_script() -> Vec<GalMsMsg> {
    vec![
        GalMsMsg {
            id: "1".into(),
            user: true,
            text: "I'm building a chat for our app and the scroll behavior is driving me nuts. Every time the AI streams a reply, the whole thread jumps around.".into(),
        },
        GalMsMsg {
            id: "2".into(),
            user: false,
            text: "That's the classic streaming scroll problem. Wrap your message list in MessageScroller and turn on autoScroll — the viewport pins to the bottom as tokens arrive, so users always see the latest text land in place.\n\nThe important part: it only auto-scrolls while the reader is already at the bottom. The moment they scroll up to read something earlier, auto-scroll backs off and their position is preserved. You get smooth streaming without fighting the user's intent.".into(),
        },
        GalMsMsg {
            id: "3".into(),
            user: true,
            text: "Okay, but when someone sends a new message the view still feels jarring — like the whole conversation reloads from the top.".into(),
        },
        GalMsMsg {
            id: "4".into(),
            user: false,
            text: "MessageScrollerItem fixes that with turn anchoring. Set scrollAnchor on the turn that should settle near the top instead of blindly snapping to the document bottom.\n\nIt also leaves a small peek of the previous exchange visible above the anchor, so context isn't lost. The reply starts in view without that disorienting jump you get from a plain overflow container.".into(),
        },
        GalMsMsg {
            id: "5".into(),
            user: true,
            text: "And if they've scrolled up to re-read an older answer? I don't want to yank them back down.".into(),
        },
        GalMsMsg {
            id: "6".into(),
            user: false,
            text: "You won't. Auto-scroll only runs when the viewport is already pinned to the bottom, so scrolling up is a deliberate opt-out — their place in the thread stays put even as new tokens keep arriving below.\n\nWhen there is content they haven't seen yet, MessageScrollerButton appears at the bottom of the viewport. One tap jumps them back to the newest message and re-engages auto-scroll. Same pattern as Slack or iMessage: quiet when you're caught up, helpful when you're not.".into(),
        },
        GalMsMsg {
            id: "7".into(),
            user: true,
            text: "Last one — does this work with assistive tech?".into(),
        },
        GalMsMsg {
            id: "8".into(),
            user: false,
            text: "MessageScrollerContent sets role=log and aria-relevant=additions by default, so screen readers announce new messages as they stream in.\n\nThe scroll button is a real button with a label, and it's removed from the tab order when you're already at the bottom — no ghost focus stops.".into(),
        },
    ]
}

pub(crate) fn gal_ms_followups() -> Vec<GalMsMsg> {
    vec![
        GalMsMsg {
            id: "s0".into(),
            user: true,
            text: "Thanks — that matches Slack / iMessage.".into(),
        },
        GalMsMsg {
            id: "s1".into(),
            user: false,
            text: "Got it. Stay at the live edge and I'll keep following.".into(),
        },
    ]
}

/// Plain bubble children (no `BubbleContent` — must be built inside `<Bubble>` or auto-wrapped).
pub(crate) fn gal_ms_bubble_body(text: &str) -> jadix::core::tree::Element {
    let paras: Vec<String> = text
        .split("\n\n")
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .map(str::to_string)
        .collect();
    if paras.len() <= 1 {
        let t = paras.into_iter().next().unwrap_or_default();
        jadix::core::tree::Element::new().text(t)
    } else {
        let lines: Vec<jadix::core::tree::Element> = paras
            .into_iter()
            .map(|p| {
                ui! {
                    <Label w="100%" min_w="0" text_size="0.875rem" leading="normal">{ p }</Label>
                }
            })
            .collect();
        let mut frag = jadix::core::tree::Element::new();
        frag.set_attr("jadix_fragment", "1");
        let frag = frag.children(lines);
        ui! {
            <VStack w="100%" min_w="0" gap="0.5rem">
                { frag }
            </VStack>
        }
    }
}

pub(crate) fn gal_ms_item(m: &GalMsMsg) -> jadix::core::tree::Element {
    let id = m.id.clone();
    let body = gal_ms_bubble_body(&m.text);
    if m.user {
        ui! {
            <MessageScrollerItem messageId={ id } scrollAnchor={true}>
                <Message align="end">
                    <MessageContent>
                        <Bubble variant="muted" align="end">
                            { body }
                        </Bubble>
                    </MessageContent>
                </Message>
            </MessageScrollerItem>
        }
    } else {
        ui! {
            <MessageScrollerItem messageId={ id }>
                <Message align="start">
                    <MessageContent>
                        <Bubble variant="ghost" align="start">
                            { body }
                        </Bubble>
                    </MessageContent>
                </Message>
            </MessageScrollerItem>
        }
    }
}

#[component]
pub(crate) fn MessageScrollerShowcase() -> jadix::core::tree::Element {
    let thread = use_signal("gal_ms_thread", gal_ms_script);
    let extra = use_signal("gal_ms_extra", || 0u32);
    let thread_send = thread;
    let extra_send = extra;
    let thread_reset = thread;
    let extra_reset = extra;
    let thread_scroll = thread;
    let extra_preview = extra;
    // Same key as MessageScrollerProvider `{id}:auto_scroll` — Switch toggles without remount.
    let auto_scroll = use_signal("gal-ms-showcase:auto_scroll", || true);

    let rows_live = jadix::app::reactive::reactive_view_sized(
        "examples/gallery/MessageScrollerShowcase:rows",
        "100%",
        "max-content",
        move || {
            let mut col = vstack().w("100%").min_w(0.0).h("max-content").gap("1.5rem");
            for m in thread_scroll.get() {
                col = col.child(gal_ms_item(&m));
            }
            col.into()
        },
    );

    let chat = ui! {
        <MessageScrollerProvider
            id="gal-ms-showcase"
            autoScroll={true}
            defaultScrollPosition="end"
        >
            <MessageScroller h="100%">
                <MessageScrollerViewport>
                    <MessageScrollerContent p="1.5rem">
                        { rows_live }
                    </MessageScrollerContent>
                </MessageScrollerViewport>
                <MessageScrollerButton direction="end" />
            </MessageScroller>
        </MessageScrollerProvider>
    };

    let preview_live = jadix::reactive!(
        "examples/gallery/MessageScrollerShowcase:preview",
        move || {
            let extra_n = extra_preview.get() as usize;
            let followups = gal_ms_followups();
            let preview = followups
                .get(extra_n)
                .map(|m| m.text.clone())
                .unwrap_or_else(|| "No messages queued. Reset the conversation.".to_string());
            let preview_color = if extra_n < followups.len() {
                "foreground"
            } else {
                "muted-foreground"
            };
            ui! {
                <Label
                    w="100%"
                    min_w="0"
                    text_size="0.875rem"
                    leading="normal"
                    color={ parse_text_color(preview_color) }
                >
                    { preview }
                </Label>
            }
        }
    );

    ui! {
        <MyCard title="1c-ms) MessageScroller（ShadCN conversation scroller）">
            <VStack w="100%" min_w="0" gap="0.75rem" items="center">
                <Label w="100%" text_size="0.75rem" color="muted-foreground" leading="normal">
                    "Official chat card: New Chat header, muted user bubbles (align=end, scrollAnchor), ghost assistant text, composer preview + send. autoScroll follows only at the live edge."
                </Label>
                <HStack w="100%" max_w="24rem" items="center" justify="between" gap="0.75rem">
                    <Label text_size="0.75rem" color="muted-foreground" leading="normal">
                        "Auto-scroll on new messages (live edge only)"
                    </Label>
                    <Switch id="gal-ms-autoscroll" checked={auto_scroll} />
                </HStack>
                <VStack w="100%" max_w="24rem" min_w="0" gap="1rem" items="center">
                        <Card w="100%" gap="0" py="0">
                            <CardHeader py="1rem" gap="0.25rem">
                                <CardTitle>"New Chat"</CardTitle>
                                <CardDescription>"How can I help you today?"</CardDescription>
                                <CardAction>
                                    <Button
                                        variant="outline"
                                        size="icon"
                                        class="rounded-full"
                                        on_click={move || {
                                            thread_reset.set(gal_ms_script());
                                            extra_reset.set(0);
                                        }}
                                    >
                                        <Icon name="rotate-cw" size="1rem" />
                                    </Button>
                                </CardAction>
                            </CardHeader>
                            <Separator />
                            <CardContent p="0">
                                <Box w="100%" h="24rem" min_h="24rem" min_w="0" overflow="hidden">
                                    { chat }
                                </Box>
                            </CardContent>
                            <CardFooter py="1rem">
                                <VStack w="100%" min_w="0" gap="0.5rem">
                                    <InputAdornment w="100%">
                                        <Box w="100%" min_w="0" h="3.5rem" px="0.75rem" py="0.625rem">
                                            { preview_live }
                                        </Box>
                                        <InputAdornmentAddon align="block-end">
                                            <InputAdornmentButton id="gal-ms-plus" variant="outline" size="icon-sm">
                                                <Icon name="plus" size="1rem" />
                                            </InputAdornmentButton>
                                            <Box flex_grow="1" min_w="0" />
                                            <InputAdornmentButton
                                                id="gal-ms-send"
                                                variant="default"
                                                size="icon-sm"
                                                on_click={move || {
                                                    let n = extra_send.get() as usize;
                                                    let followups = gal_ms_followups();
                                                    if let Some(next) = followups.get(n).cloned() {
                                                        let mut v = thread_send.get();
                                                        v.push(next);
                                                        thread_send.set(v);
                                                        extra_send.set((n as u32) + 1);
                                                        jadix::components::message_scroller_stick_to_end(
                                                            "gal-ms-showcase",
                                                        );
                                                    }
                                                }}
                                            >
                                                <Icon name="arrow-up" size="1rem" />
                                            </InputAdornmentButton>
                                        </InputAdornmentAddon>
                                    </InputAdornment>
                                </VStack>
                            </CardFooter>
                        </Card>
                        <Label w="100%" text_size="0.75rem" color="muted-foreground" text_align="center">
                            "Demo is read only. Press send to send messages."
                        </Label>
                    </VStack>
            </VStack>
        </MyCard>
    }
}

#[component]
pub(crate) fn AlertShowcase() -> jadix::core::tree::Element {
    let alert_variant = use_signal("gal_alert_variant", || "default".to_string());
    let alert_var_btn = alert_variant;
    let alert_var_live = alert_variant;
    ui! {
        <MyCard title="1c-a) Alert（ShadCN + semantic variants）">
            <VStack w="100%" min_w="0" gap="0.75rem">
                <Label w="100%" text_size="0.75rem" color="muted-foreground" leading="normal">
                    "Dual style: <Alert> / AlertTitle / AlertDescription / AlertAction and alert() builder. ShadCN default/destructive; GPUI-style info/success/warning. Dynamic variant bind below."
                </Label>
                <VStack w="100%" min_w="0" gap="0.5rem">
                    <Alert variant="default">
                        <Icon name="info" size="1rem" color="foreground" />
                        <AlertTitle>"Heads up!"</AlertTitle>
                        <AlertDescription>
                            "You can add components and dependencies using the CLI."
                        </AlertDescription>
                    </Alert>
                    <Alert variant="destructive">
                        <Icon name="triangle-alert" size="1rem" />
                        <AlertTitle>"Error"</AlertTitle>
                        <AlertDescription>"Your session has expired. Please sign in again."</AlertDescription>
                        <AlertAction>
                            <Button variant="outline" size="sm">"Sign in"</Button>
                        </AlertAction>
                    </Alert>
                    <Alert variant="info">
                        <Icon name="info" size="1rem" />
                        <AlertTitle>"Information"</AlertTitle>
                        <AlertDescription>"New features are available in this release."</AlertDescription>
                    </Alert>
                    <Alert variant="success">
                        <Icon name="circle-check" size="1rem" />
                        <AlertTitle>"Success"</AlertTitle>
                        <AlertDescription>"Your changes have been saved."</AlertDescription>
                    </Alert>
                    <Alert variant="warning">
                        <Icon name="triangle-alert" size="1rem" />
                        <AlertTitle>"Warning"</AlertTitle>
                        <AlertDescription>"This action requires additional confirmation."</AlertDescription>
                    </Alert>
                    { Element::from(
                        alert()
                            .variant("info")
                            .icon(
                                build_icon_text_node(lucide_glyph("bell").unwrap_or_else(|| "…".into()))
                                    .text_size(16.0)
                            )
                            .title("Builder alert")
                            .description("alert().variant().icon().title().description()")
                    ) }
                </VStack>
                <VStack w="100%" min_w="0" gap="0.5rem">
                    <Label font_weight="700" text_size="0.75rem">"Reactive variant bind"</Label>
                    <HStack w="100%" gap="0.5rem" wrap="wrap">
                        <Button size="sm" variant="outline" on_click={move || alert_var_btn.set("default".into())}>"default"</Button>
                        <Button size="sm" variant="outline" on_click={move || alert_var_btn.set("destructive".into())}>"destructive"</Button>
                        <Button size="sm" variant="outline" on_click={move || alert_var_btn.set("success".into())}>"success"</Button>
                        <Button size="sm" variant="outline" on_click={move || alert_var_btn.set("warning".into())}>"warning"</Button>
                    </HStack>
                    <Alert variant={ alert_var_live.get() }>
                        <Icon name="info" size="1rem" />
                        <AlertTitle>{ format!("Live variant: {}", alert_var_live.get()) }</AlertTitle>
                        <AlertDescription>"variant={signal.get()} uses apply_bound_alert_variant."</AlertDescription>
                    </Alert>
                </VStack>
            </VStack>
        </MyCard>
    }
}

#[component]
pub(crate) fn AlertDialogShowcase() -> jadix::core::tree::Element {
    let delete_open = use_signal("gal_alert_dialog_delete", || false);
    let delete_open_action = delete_open;
    let sm_open = use_signal("gal_alert_dialog_sm", || false);
    let info_open = use_signal("gal_alert_dialog_info", || false);
    let warning_open = use_signal("gal_alert_dialog_warning", || false);
    let success_open = use_signal("gal_alert_dialog_success", || false);
    let builder_open = use_signal("gal_alert_dialog_builder", || false);
    let ad_variant = use_signal("gal_alert_dialog_variant", || "info".to_string());
    let ad_var_btn = ad_variant;
    let ad_var_live = ad_variant;
    let live_open = use_signal("gal_alert_dialog_live", || false);
    let live_open_btn = live_open;
    ui! {
        <MyCard title="1c-ad) AlertDialog（ShadCN confirm + semantic variants）">
            <VStack w="100%" min_w="0" gap="0.75rem">
                <Label w="100%" text_size="0.75rem" color="muted-foreground" leading="normal">
                    "Click each Trigger to open. Semantic variants: info / warning / success (same palette as Alert). Bottom row: reactive variant bind."
                </Label>
                <HStack w="100%" gap="0.5rem" wrap="wrap" items="center">
                    <AlertDialog id="gal-delete-account" open={delete_open}>
                        <AlertDialogTrigger>
                            <Button variant="destructive" size="sm">"destructive"</Button>
                        </AlertDialogTrigger>
                        <AlertDialogContent variant="destructive">
                            <AlertDialogHeader>
                                <AlertDialogMedia>
                                    <Icon name="triangle-alert" size="2rem" />
                                </AlertDialogMedia>
                                <AlertDialogTitle>"Are you absolutely sure?"</AlertDialogTitle>
                                <AlertDialogDescription>
                                    "This action cannot be undone."
                                </AlertDialogDescription>
                            </AlertDialogHeader>
                            <AlertDialogBody>
                                <Label text_size="0.875rem" leading="normal">
                                    "This will permanently delete your account and remove your data from our servers."
                                </Label>
                            </AlertDialogBody>
                            <AlertDialogFooter>
                                <AlertDialogCancel />
                                <AlertDialogAction>
                                    <Button
                                        variant="destructive"
                                        on_click={move || delete_open_action.set(false)}
                                    >
                                        "Continue"
                                    </Button>
                                </AlertDialogAction>
                            </AlertDialogFooter>
                        </AlertDialogContent>
                    </AlertDialog>
                    <AlertDialog id="gal-alert-dialog-sm" open={sm_open}>
                        <AlertDialogTrigger>
                            <Button variant="outline" size="sm">"sm · success"</Button>
                        </AlertDialogTrigger>
                        <AlertDialogContent size="sm" variant="success">
                            <AlertDialogHeader>
                                <AlertDialogMedia>
                                    <Icon name="circle-check" size="2rem" />
                                </AlertDialogMedia>
                                <AlertDialogTitle>"Allow notifications?"</AlertDialogTitle>
                                <AlertDialogDescription>
                                    "You can change this later in Settings."
                                </AlertDialogDescription>
                            </AlertDialogHeader>
                            <AlertDialogFooter>
                                <AlertDialogCancel>"Not now"</AlertDialogCancel>
                                <AlertDialogAction>
                                    <Button on_click={move || sm_open.set(false)}>"Allow"</Button>
                                </AlertDialogAction>
                            </AlertDialogFooter>
                        </AlertDialogContent>
                    </AlertDialog>
                    <AlertDialog id="gal-alert-dialog-info" open={info_open}>
                        <AlertDialogTrigger>
                            <Button variant="outline" size="sm">"info"</Button>
                        </AlertDialogTrigger>
                        <AlertDialogContent variant="info">
                            <AlertDialogHeader>
                                <AlertDialogMedia>
                                    <Icon name="info" size="2rem" />
                                </AlertDialogMedia>
                                <AlertDialogTitle>"Information"</AlertDialogTitle>
                                <AlertDialogDescription>
                                    "New features are available in this release."
                                </AlertDialogDescription>
                            </AlertDialogHeader>
                            <AlertDialogFooter>
                                <AlertDialogCancel />
                            </AlertDialogFooter>
                        </AlertDialogContent>
                    </AlertDialog>
                    <AlertDialog id="gal-alert-dialog-warning" open={warning_open}>
                        <AlertDialogTrigger>
                            <Button variant="outline" size="sm">"warning"</Button>
                        </AlertDialogTrigger>
                        <AlertDialogContent variant="warning">
                            <AlertDialogHeader>
                                <AlertDialogMedia>
                                    <Icon name="circle-alert" size="2rem" />
                                </AlertDialogMedia>
                                <AlertDialogTitle>"Warning"</AlertDialogTitle>
                                <AlertDialogDescription>
                                    "This action requires additional confirmation."
                                </AlertDialogDescription>
                            </AlertDialogHeader>
                            <AlertDialogFooter>
                                <AlertDialogCancel />
                            </AlertDialogFooter>
                        </AlertDialogContent>
                    </AlertDialog>
                    <AlertDialog id="gal-alert-dialog-success" open={success_open}>
                        <AlertDialogTrigger>
                            <Button variant="outline" size="sm">"success"</Button>
                        </AlertDialogTrigger>
                        <AlertDialogContent variant="success">
                            <AlertDialogHeader>
                                <AlertDialogMedia>
                                    <Icon name="circle-check" size="2rem" />
                                </AlertDialogMedia>
                                <AlertDialogTitle>"Success"</AlertDialogTitle>
                                <AlertDialogDescription>
                                    "Your changes have been saved."
                                </AlertDialogDescription>
                            </AlertDialogHeader>
                            <AlertDialogFooter>
                                <AlertDialogCancel />
                            </AlertDialogFooter>
                        </AlertDialogContent>
                    </AlertDialog>
                    <AlertDialog id="gal-alert-dialog-builder" open={builder_open}>
                        <AlertDialogTrigger>
                            <Button variant="outline" size="sm">"Builder scope"</Button>
                        </AlertDialogTrigger>
                        <AlertDialogContent variant="info">
                            <AlertDialogHeader>
                                <AlertDialogTitle>"Builder alert_dialog()"</AlertDialogTitle>
                                <AlertDialogDescription>
                                    "alert_dialog(id).open(sig).child with Trigger + Content"
                                </AlertDialogDescription>
                            </AlertDialogHeader>
                            <AlertDialogFooter>
                                <AlertDialogCancel />
                                <AlertDialogAction>
                                    <Button on_click={move || builder_open.set(false)}>"OK"</Button>
                                </AlertDialogAction>
                            </AlertDialogFooter>
                        </AlertDialogContent>
                    </AlertDialog>
                </HStack>
                <VStack w="100%" min_w="0" gap="0.5rem">
                    <Label font_weight="700" text_size="0.75rem">"Reactive variant bind"</Label>
                    <HStack w="100%" gap="0.5rem" wrap="wrap" items="center">
                        <Button
                            size="sm"
                            variant="outline"
                            on_click={move || {
                                ad_var_btn.set("info".into());
                                live_open_btn.set(true);
                            }}
                        >
                            "info"
                        </Button>
                        <Button
                            size="sm"
                            variant="outline"
                            on_click={move || {
                                ad_var_btn.set("warning".into());
                                live_open_btn.set(true);
                            }}
                        >
                            "warning"
                        </Button>
                        <Button
                            size="sm"
                            variant="outline"
                            on_click={move || {
                                ad_var_btn.set("success".into());
                                live_open_btn.set(true);
                            }}
                        >
                            "success"
                        </Button>
                        <AlertDialog id="gal-alert-dialog-live" open={live_open}>
                            <AlertDialogTrigger>
                                <Button variant="outline" size="sm">
                                    { format!("Open live · {}", ad_var_live.get()) }
                                </Button>
                            </AlertDialogTrigger>
                            <AlertDialogContent variant_signal={ad_variant}>
                                <AlertDialogHeader>
                                    <AlertDialogTitle>{ format!("Variant: {}", ad_var_live.get()) }</AlertDialogTitle>
                                    <AlertDialogDescription>
                                        "Pick variant above, then open — variant_signal subscribes inside the portal."
                                    </AlertDialogDescription>
                                </AlertDialogHeader>
                                <AlertDialogFooter>
                                    <AlertDialogCancel />
                                </AlertDialogFooter>
                            </AlertDialogContent>
                        </AlertDialog>
                    </HStack>
                </VStack>
            </VStack>
        </MyCard>
    }
}

#[component]
pub(crate) fn ItemShowcase() -> jadix::core::tree::Element {
    let item_variant = use_signal("gal_item_variant", || "outline".to_string());
    let item_var_btn = item_variant;
    let item_var_live = item_variant;
    ui! {
        <MyCard title="1c-i) Item（ShadCN list row）">
            <VStack w="100%" min_w="0" gap="0.75rem">
                <Label w="100%" text_size="0.75rem" color="muted-foreground" leading="normal">
                    "Dual style: ItemGroup / Item / ItemMedia / ItemContent / ItemActions and item() / item_group() builders. variant + size bind below."
                </Label>
                <ItemGroup>
                    <Item variant="default" size="default">
                        <ItemMedia variant="icon">
                            <Icon name="bell" size="1rem" color="foreground" />
                        </ItemMedia>
                        <ItemContent>
                            <ItemTitle>"Default item"</ItemTitle>
                            <ItemDescription>"Transparent border · default padding."</ItemDescription>
                        </ItemContent>
                        <ItemActions>
                            <Button variant="outline" size="sm">"Edit"</Button>
                        </ItemActions>
                    </Item>
                    <ItemSeparator />
                    <Item variant="outline" size="sm">
                        <ItemHeader>
                            <Badge variant="secondary">"Pro"</Badge>
                            <Label text_size="0.75rem" color="muted-foreground">"Header slot"</Label>
                        </ItemHeader>
                        <ItemMedia variant="icon">
                            <Icon name="shield" size="1rem" />
                        </ItemMedia>
                        <ItemContent>
                            <ItemTitle>"Outline · sm"</ItemTitle>
                            <ItemDescription>"Bordered row with header and trailing action."</ItemDescription>
                        </ItemContent>
                        <ItemActions>
                            <Button size="sm">"Upgrade"</Button>
                        </ItemActions>
                    </Item>
                    <ItemSeparator />
                    <Item variant="muted" size="xs">
                        <ItemContent>
                            <ItemTitle>"Muted · xs"</ItemTitle>
                            <ItemDescription>"Compact muted background for secondary rows."</ItemDescription>
                        </ItemContent>
                        <ItemFooter>
                            <Label text_size="0.75rem" color="muted-foreground">"Footer slot"</Label>
                            <Button variant="ghost" size="sm">"More"</Button>
                        </ItemFooter>
                    </Item>
                    <ItemSeparator />
                    <Item variant={ item_var_live.get() } size="default">
                        <ItemMedia variant="icon">
                            <Icon name="settings" size="1rem" />
                        </ItemMedia>
                        <ItemContent>
                            <ItemTitle>{ format!("Reactive variant: {}", item_var_live.get()) }</ItemTitle>
                            <ItemDescription>"variant={signal.get()} uses apply_bound_item_variant."</ItemDescription>
                        </ItemContent>
                    </Item>
                </ItemGroup>
                <HStack w="100%" gap="0.5rem" wrap="wrap">
                    <Button size="sm" variant="outline" on_click={move || item_var_btn.set("default".into())}>"default"</Button>
                    <Button size="sm" variant="outline" on_click={move || item_var_btn.set("outline".into())}>"outline"</Button>
                    <Button size="sm" variant="outline" on_click={move || item_var_btn.set("muted".into())}>"muted"</Button>
                </HStack>
                <Box w="100%" max_w="28rem" border_color="border" border_width="1" rounded="0.75rem" p="0.5rem">
                    { Element::from(
                        item_group()
                            .child(
                                item()
                                    .variant("outline")
                                    .size("sm")
                                    .media_icon(
                                        build_icon_text_node(lucide_glyph("user").unwrap_or_else(|| "…".into()))
                                            .text_size(16.0)
                                    )
                                    .title_description("Builder row", "item().variant().media_icon().title_description()")
                                    .action(button("gal-item-view").variant("outline").size("sm").text("View"))
                            )
                    ) }
                </Box>
            </VStack>
        </MyCard>
    }
}

#[component]
pub(crate) fn AttachmentShowcase() -> jadix::core::tree::Element {
    let att_state = use_signal("gal_attachment_state", || "done".to_string());
    let att_state_btn = att_state;
    let att_state_live = att_state;
    ui! {
        <MyCard title="1c-k) Attachment（ShadCN file chip）">
            <VStack w="100%" min_w="0" gap="0.75rem">
                <Label w="100%" text_size="0.75rem" color="muted-foreground" leading="normal">
                    "Dual style + semantic states (info/success/warning/destructive; error≡destructive) via semantic_chrome — same tone keys as Alert. Uploading/processing use loader-circle + animate-spin (1g). state bind repaints root + slots."
                </Label>
                <AttachmentGroup w="100%" wrap="wrap" class="max-w-full" items="start">
                    <Attachment state="done" size="default" orientation="horizontal">
                        <AttachmentMedia variant="icon">
                            <Icon name="file-text" size="0.875rem" color="foreground" />
                        </AttachmentMedia>
                        <AttachmentContent>
                            <AttachmentTitle>"quarterly-report.pdf"</AttachmentTitle>
                            <AttachmentDescription>"2.4 MB · Uploaded"</AttachmentDescription>
                        </AttachmentContent>
                        <AttachmentActions>
                            <AttachmentAction id="gal-att-rm">
                                <Icon name="x" size="0.75rem" />
                            </AttachmentAction>
                        </AttachmentActions>
                    </Attachment>
                    <Attachment state="uploading" size="sm" orientation="horizontal">
                        <AttachmentMedia variant="icon">
                            <Icon name="loader-circle" size="0.875rem" class="animate-spin" />
                        </AttachmentMedia>
                        <AttachmentContent>
                            <AttachmentTitle>"photo.png"</AttachmentTitle>
                            <AttachmentDescription>"Uploading · 64%"</AttachmentDescription>
                        </AttachmentContent>
                    </Attachment>
                    <Attachment state="processing" size="sm" orientation="horizontal">
                        <AttachmentMedia variant="icon">
                            <Icon name="loader-circle" size="0.875rem" class="animate-spin" />
                        </AttachmentMedia>
                        <AttachmentContent>
                            <AttachmentTitle>"sales-dashboard.pdf"</AttachmentTitle>
                            <AttachmentDescription>"Processing…"</AttachmentDescription>
                        </AttachmentContent>
                    </Attachment>
                    <Attachment state="destructive" size="sm" orientation="horizontal">
                        <AttachmentMedia variant="icon">
                            <Icon name="file-warning" size="0.875rem" />
                        </AttachmentMedia>
                        <AttachmentContent>
                            <AttachmentTitle>"archive.zip"</AttachmentTitle>
                            <AttachmentDescription>"Upload failed — tap to retry"</AttachmentDescription>
                        </AttachmentContent>
                        <AttachmentTrigger id="gal-att-retry" />
                    </Attachment>
                    <Attachment state="success" size="sm" orientation="horizontal">
                        <AttachmentMedia variant="icon">
                            <Icon name="circle-check" size="0.875rem" />
                        </AttachmentMedia>
                        <AttachmentContent>
                            <AttachmentTitle>"invoice.pdf"</AttachmentTitle>
                            <AttachmentDescription>"Verified · 128 KB"</AttachmentDescription>
                        </AttachmentContent>
                    </Attachment>
                    <Attachment state="info" size="sm" orientation="horizontal">
                        <AttachmentMedia variant="icon">
                            <Icon name="info" size="0.875rem" />
                        </AttachmentMedia>
                        <AttachmentContent>
                            <AttachmentTitle>"readme.md"</AttachmentTitle>
                            <AttachmentDescription>"Review before publish"</AttachmentDescription>
                        </AttachmentContent>
                    </Attachment>
                    <Attachment state="warning" size="sm" orientation="horizontal">
                        <AttachmentMedia variant="icon">
                            <Icon name="triangle-alert" size="0.875rem" />
                        </AttachmentMedia>
                        <AttachmentContent>
                            <AttachmentTitle>"large-asset.zip"</AttachmentTitle>
                            <AttachmentDescription>"Exceeds recommended size"</AttachmentDescription>
                        </AttachmentContent>
                    </Attachment>
                    <Attachment state="idle" size="xs" orientation="vertical">
                        <AttachmentMedia variant="icon">
                            <Icon name="plus" size="0.75rem" />
                        </AttachmentMedia>
                        <AttachmentContent>
                            <AttachmentTitle>"Add file"</AttachmentTitle>
                            <AttachmentDescription>"Dashed idle drop zone"</AttachmentDescription>
                        </AttachmentContent>
                        <AttachmentTrigger id="gal-att-pick" />
                    </Attachment>
                    <Attachment orientation="vertical">
                        <AttachmentMedia variant="image">
                            <Image texture="demo-checker" />
                        </AttachmentMedia>
                    </Attachment>
                </AttachmentGroup>
                <Attachment state={ att_state_live.get() } size="default" orientation="horizontal" w="100%" class="max-w-full" min_w="0">
                    <AttachmentMedia variant="icon">
                        <Icon
                            name={ if matches!(att_state_live.get().as_str(), "uploading" | "processing") {
                                "loader-circle".to_string()
                            } else if att_state_live.get() == "destructive" {
                                "file-warning".to_string()
                            } else {
                                "settings".to_string()
                            } }
                            size="0.875rem"
                            class={ if matches!(att_state_live.get().as_str(), "uploading" | "processing") {
                                "animate-spin".to_string()
                            } else {
                                String::new()
                            } }
                        />
                    </AttachmentMedia>
                    <AttachmentContent>
                        <AttachmentTitle>{ format!("Reactive state: {}", att_state_live.get()) }</AttachmentTitle>
                        <AttachmentDescription>"state={signal.get()} repaints border, media tint, title pulse."</AttachmentDescription>
                    </AttachmentContent>
                </Attachment>
                <HStack w="100%" gap="0.5rem" wrap="wrap">
                    <Button size="sm" variant="outline" on_click={move || att_state_btn.set("idle".into())}>"idle"</Button>
                    <Button size="sm" variant="outline" on_click={move || att_state_btn.set("uploading".into())}>"uploading"</Button>
                    <Button size="sm" variant="outline" on_click={move || att_state_btn.set("processing".into())}>"processing"</Button>
                    <Button size="sm" variant="outline" on_click={move || att_state_btn.set("done".into())}>"done"</Button>
                    <Button size="sm" variant="outline" on_click={move || att_state_btn.set("info".into())}>"info"</Button>
                    <Button size="sm" variant="outline" on_click={move || att_state_btn.set("success".into())}>"success"</Button>
                    <Button size="sm" variant="outline" on_click={move || att_state_btn.set("warning".into())}>"warning"</Button>
                    <Button size="sm" variant="outline" on_click={move || att_state_btn.set("destructive".into())}>"destructive"</Button>
                    <Button size="sm" variant="outline" on_click={move || att_state_btn.set("error".into())}>"error≡destructive"</Button>
                </HStack>
                <Box w="100%" max_w="24rem" border_color="border" border_width="1" rounded="0.75rem" p="0.5rem">
                    { Element::from(
                        attachment_group()
                            .child(
                                attachment()
                                    .state("done")
                                    .size("sm")
                                    .media_icon(
                                        build_icon_text_node(lucide_glyph("paperclip").unwrap_or_else(|| "…".into()))
                                            .text_size(16.0)
                                    )
                                    .title_description("notes.md", "Builder · 12 KB")
                                    .action(
                                        button("gal-att-view")
                                            .variant("ghost")
                                            .size("icon-xs")
                                            .child(
                                                build_icon_text_node(lucide_glyph("eye").unwrap_or_else(|| "…".into()))
                                                    .text_size(14.0)
                                            )
                                    )
                            )
                    ) }
                </Box>
            </VStack>
        </MyCard>
    }
}

#[component]
pub(crate) fn ListShowcase() -> jadix::core::tree::Element {
    ui! {
        <MyCard title="1c-j) List（语义 ul/li）">
            <VStack w="100%" min_w="0" gap="0.75rem">
                <Label w="100%" text_size="0.75rem" color="muted-foreground" leading="normal">
                    "Dual style: List / ListItem / ListHeader / ListDescription (role=list/listitem) and list() builder. Nest Item for rich rows; plain rows support marker=disc|decimal."
                </Label>
                <HStack w="100%" min_w="0" gap="1rem" wrap="wrap" items="start">
                    <VStack w="100%" min_w="0" max_w="16rem" gap="0.5rem" flex_grow="1">
                        <Label font_weight="700" text_size="0.75rem">"Bullet · RSX"</Label>
                        <List marker="disc">
                            <ListHeader>"Included"</ListHeader>
                            <ListDescription>"Everything in the Pro plan."</ListDescription>
                            <ListItem>"Unlimited projects"</ListItem>
                            <ListItem>"24/7 support"</ListItem>
                            <ListItem>"Custom domains"</ListItem>
                        </List>
                    </VStack>
                    <VStack w="100%" min_w="0" max_w="16rem" gap="0.5rem" flex_grow="1">
                        <Label font_weight="700" text_size="0.75rem">"Ordered · Builder"</Label>
                        { Element::from(
                            list()
                                .marker("decimal")
                                .header("Steps")
                                .description("Follow in order.")
                                .text("Create a project")
                                .text("Invite teammates")
                                .text("Ship to production")
                        ) }
                    </VStack>
                    <VStack w="100%" min_w="0" max_w="20rem" gap="0.5rem" flex_grow="1">
                        <Label font_weight="700" text_size="0.75rem">"Item inside ListItem"</Label>
                        <List>
                            <ListItem>
                                <Item variant="outline" size="sm">
                                    <ItemMedia variant="icon">
                                        <Icon name="inbox" size="1rem" />
                                    </ItemMedia>
                                    <ItemContent>
                                        <ItemTitle>"Inbox"</ItemTitle>
                                        <ItemDescription>"12 unread messages"</ItemDescription>
                                    </ItemContent>
                                </Item>
                            </ListItem>
                            <ListItem>
                                <Item variant="muted" size="sm">
                                    <ItemContent>
                                        <ItemTitle>"Archive"</ItemTitle>
                                        <ItemDescription>"Muted row without media."</ItemDescription>
                                    </ItemContent>
                                </Item>
                            </ListItem>
                        </List>
                    </VStack>
                </HStack>
            </VStack>
        </MyCard>
    }
}

pub(crate) fn bubble_showcase_conversation() -> Element {
    let first = Element::from(
        bubble()
            .variant("secondary")
            .align("start")
            .child(bubble_content().text("Can you tell me what's the issue?")),
    );
    let second = Element::from(
        bubble()
            .variant("default")
            .align("end")
            .child(bubble_content().text("You tell me!")),
    );
    let third = Element::from(
        bubble()
            .variant("muted")
            .align("start")
            .child(bubble_content().text("It worked yesterday."))
            .child(bubble_reactions().side("bottom").emoji("👍").emoji("😮")),
    );
    Element::from(
        vstack()
            .w("100%")
            .min_w(0.0)
            .gap(0.0)
            .p("1rem")
            .overflow("visible")
            .bg(parse_color("background"))
            .border_color(parse_color("border"))
            .border_width(1.0)
            .rounded("0.75rem")
            .child(bubble_group().child(first).child(second).child(third)),
    )
}

pub(crate) fn bubble_showcase_variants() -> Element {
    let heading: Element = ui! {
        <Label w="100%" text_size="0.75rem" font_weight="600">"Variants"</Label>
    };
    let group = bubble_group()
        .child(
            bubble()
                .variant("tinted")
                .align("start")
                .child(bubble_content().text("Tinted variant")),
        )
        .child(
            bubble()
                .variant("outline")
                .align("start")
                .child(bubble_content().text("Outline")),
        )
        .child(gal_bubble_with_rx(
            "destructive",
            "start",
            "Failed to run the command.",
            gal_rx_emojis("bottom", None, None, &["👍", "😮"]),
        ))
        .child(bubble().variant("ghost").align("start").child(
            bubble_content().text("Ghost bubbles work for assistant text without a frame."),
        ));
    Element::from(
        vstack()
            .w("100%")
            .min_w(0.0)
            .gap("0.5rem")
            .child(heading)
            .child(group),
    )
}

pub(crate) fn gal_rx_emojis(
    side: &'static str,
    align: Option<&'static str>,
    layout: Option<&'static str>,
    emojis: &[&str],
) -> Element {
    let mut r = bubble_reactions().side(side);
    if let Some(a) = align {
        r = r.align(a);
    }
    if let Some(l) = layout {
        r = r.layout(l);
    }
    for e in emojis {
        r = r.emoji(*e);
    }
    r.into()
}

pub(crate) fn gal_bubble_with_rx(
    variant: &'static str,
    align: &'static str,
    text: &'static str,
    rx: Element,
) -> Element {
    bubble()
        .variant(variant)
        .align(align)
        .child(bubble_content().text(text))
        .child(rx)
        .into()
}

pub(crate) fn bubble_showcase_reactions() -> Element {
    let heading: Element = ui! {
        <Label w="100%" text_size="0.75rem" font_weight="600">"Reactions（+N 悬停 / 点击浮层展开）"</Label>
    };
    let group = bubble_group()
        .child(gal_bubble_with_rx(
            "secondary",
            "end",
            "Reactions",
            gal_rx_emojis("bottom", None, None, &["👍", "😮"]),
        ))
        .child(gal_bubble_with_rx(
            "secondary",
            "end",
            "Party",
            gal_rx_emojis("bottom", None, None, &["🎉", "👏"]),
        ))
        .child(gal_bubble_with_rx(
            "secondary",
            "end",
            "row · 8 reactions → +5",
            gal_rx_emojis(
                "bottom",
                None,
                None,
                &["👍", "🔥", "👀", "🎉", "👏", "❤️", "🚀", "✨"],
            ),
        ))
        .child(
            bubble()
                .variant("secondary")
                .align("end")
                .child(bubble_content().text("Builder bubble"))
                .child(bubble_reactions().side("bottom").emoji("👍").emoji("🔥")),
        );
    Element::from(
        vstack()
            .w("100%")
            .min_w(0.0)
            .gap("0.5rem")
            .child(heading)
            .child(group),
    )
}

pub(crate) fn bubble_showcase_desc() -> Element {
    ui! {
        <Label w="100%" text_size="0.75rem" color="muted-foreground" leading="normal">
            "双写法：<Bubble variant align> + <BubbleContent> / <BubbleReactions>。默认独立行（左气泡右下 / 右气泡左下）；side / align 可覆盖；layout=\"overlay\" 咬住气泡边并内缩。`max_visible` 控制折叠前展示数（默认 3），宽度不足时自动收入 +N；悬停/点击展开浮层。Emoji 默认系统色光栅 + 字形盒内居中；Twemoji 位图需 JADIX_EMOJI_CUSTOM_GLYPH=1 或 JADIX_TWEMOJI_DIR。"
        </Label>
    }
}

pub(crate) fn bubble_showcase_placement() -> Element {
    let mut kids: Vec<Element> = Vec::new();
    kids.push(bubble_placement_heading());
    kids.push(bubble_placement_group_a());
    kids.push(bubble_placement_group_b());
    Element::from(vstack().w("100%").min_w(0.0).gap("0.5rem").children(kids))
}

pub(crate) fn bubble_placement_heading() -> Element {
    ui! {
        <Label w="100%" text_size="0.75rem" font_weight="600">"Placement（覆盖位置 / 独立行 vs 叠加）"</Label>
    }
}

pub(crate) fn bubble_placement_group_a() -> Element {
    bubble_group()
        .child(gal_bubble_with_rx(
            "muted",
            "start",
            "row · 居中独立行",
            gal_rx_emojis("bottom", Some("center"), None, &["👍", "🔥"]),
        ))
        .child(gal_bubble_with_rx(
            "secondary",
            "start",
            "overlay · 左气泡默认右下",
            gal_rx_emojis("bottom", None, Some("overlay"), &["😮", "🎉"]),
        ))
        .child(gal_bubble_with_rx(
            "secondary",
            "end",
            "overlay · 右气泡默认左下",
            gal_rx_emojis("bottom", None, Some("overlay"), &["👍", "🔥"]),
        ))
        .into()
}

pub(crate) fn bubble_placement_group_b() -> Element {
    let mut kids: Vec<Element> = Vec::new();
    kids.push(bubble_placement_b1());
    kids.push(bubble_placement_b2());
    kids.push(bubble_placement_b3());
    bubble_group().children(kids).into()
}

pub(crate) fn bubble_placement_b1() -> Element {
    gal_bubble_with_rx(
        "outline",
        "start",
        "overlay · 顶左（覆盖）",
        gal_rx_emojis("top", Some("start"), Some("overlay"), &["👀"]),
    )
}

pub(crate) fn bubble_placement_b2() -> Element {
    gal_bubble_with_rx(
        "secondary",
        "start",
        "overlay · 7 reactions → +4",
        gal_rx_emojis(
            "bottom",
            None,
            Some("overlay"),
            &["👍", "🔥", "👀", "🎉", "👏", "❤️", "🚀"],
        ),
    )
}

pub(crate) fn bubble_placement_b3() -> Element {
    gal_bubble_with_rx(
        "outline",
        "end",
        "overlay 顶 · 6 reactions → +3",
        gal_rx_emojis(
            "top",
            None,
            Some("overlay"),
            &["👍", "😮", "🎉", "🔥", "👀", "👏"],
        ),
    )
}

#[component]
pub(crate) fn BubbleShowcase() -> jadix::core::tree::Element {
    let mut kids: Vec<Element> = Vec::new();
    kids.push(bubble_showcase_desc());
    kids.push(bubble_showcase_conversation());
    kids.push(bubble_showcase_variants());
    kids.push(bubble_showcase_reactions());
    kids.push(bubble_showcase_placement());
    let body: Element = vstack()
        .w("100%")
        .min_w(0.0)
        .gap("1.25rem")
        .children(kids)
        .into();
    ui! {
        <MyCard title="1c-b) Bubble 聊天气泡（ShadCN）">
            { body }
        </MyCard>
    }
}

pub(crate) fn avatar_showcase_row() -> Element {
    ui! {
        <HStack w="100%" min_w="0" gap="1rem" items="center" wrap="wrap">
            <VStack gap="0.25rem" items="center">
                <Label font_weight="700" text_size="0.75rem">"Basic"</Label>
                <Avatar>
                    <AvatarImage
                        src="data:image/svg+xml,<svg xmlns='http://www.w3.org/2000/svg' width='64' height='64'><rect width='64' height='64' fill='%233b82f6'/></svg>"
                        alt="@demo"
                    />
                    <AvatarFallback>"CN"</AvatarFallback>
                </Avatar>
            </VStack>
            <VStack gap="0.25rem" items="center">
                <Label font_weight="700" text_size="0.75rem">"Fallback Icon"</Label>
                <Avatar>
                    <AvatarFallback>
                        <Icon name="user" />
                    </AvatarFallback>
                </Avatar>
            </VStack>
            <VStack gap="0.25rem" items="center">
                <Label font_weight="700" text_size="0.75rem">"Fallback Image"</Label>
                { Element::from(avatar().fallback_image("data:image/svg+xml,<svg xmlns='http://www.w3.org/2000/svg' width='64' height='64'><rect width='64' height='64' fill='%23f97316'/></svg>")) }
            </VStack>
            <VStack gap="0.25rem" items="center">
                <Label font_weight="700" text_size="0.75rem">"Rounded sq."</Label>
                <Avatar shape="rounded-square">
                    <AvatarFallback>"RS"</AvatarFallback>
                </Avatar>
            </VStack>
            <VStack gap="0.25rem" items="center">
                <Label font_weight="700" text_size="0.75rem">"Badge"</Label>
                <Avatar>
                    <AvatarImage
                        src="data:image/svg+xml,<svg xmlns='http://www.w3.org/2000/svg' width='64' height='64'><rect width='64' height='64' fill='%2310b981'/></svg>"
                    />
                    <AvatarFallback>"PP"</AvatarFallback>
                    <AvatarBadge variant="success" />
                </Avatar>
            </VStack>
            <VStack gap="0.25rem" items="center">
                <Label font_weight="700" text_size="0.75rem">"Badge + Icon"</Label>
                <Avatar size="lg">
                    <AvatarFallback>"JD"</AvatarFallback>
                    <AvatarBadge variant="destructive">
                        <Icon name="badge-check" size="0.5rem" />
                    </AvatarBadge>
                </Avatar>
            </VStack>
            <VStack gap="0.25rem" items="center">
                <Label font_weight="700" text_size="0.75rem">"Load fail"</Label>
                <Avatar>
                    <AvatarImage src="/nonexistent/jadix-gallery-avatar.png" />
                    <AvatarFallback>"404"</AvatarFallback>
                </Avatar>
            </VStack>
            <VStack gap="0.25rem" items="center">
                <Label font_weight="700" text_size="0.75rem">"Sizes"</Label>
                <HStack gap="0.5rem" items="center">
                    <Avatar size="sm"><AvatarFallback>"SM"</AvatarFallback></Avatar>
                    <Avatar><AvatarFallback>"MD"</AvatarFallback></Avatar>
                    <Avatar size="lg"><AvatarFallback>"LG"</AvatarFallback></Avatar>
                </HStack>
            </VStack>
        </HStack>
    }
}

pub(crate) fn avatar_showcase_group() -> Element {
    ui! {
        <VStack w="100%" min_w="0" gap="0.25rem">
            <Label font_weight="700" text_size="0.75rem">"AvatarGroup + Count"</Label>
            <Label w="100%" text_size="0.7rem" color="muted-foreground" leading="normal">
                "悬停 / 点击 +N：原地向下盖住展开（expand_placement / expand_class / max_visible 可配）；Count 内可嵌隐藏头像参与展开列表；宽度不足时自动折叠"
            </Label>
            <AvatarGroup>
                <Avatar>
                    <AvatarImage src="data:image/svg+xml,<svg xmlns='http://www.w3.org/2000/svg' width='64' height='64'><rect width='64' height='64' fill='%23ef4444'/></svg>" />
                    <AvatarFallback>"A"</AvatarFallback>
                </Avatar>
                <Avatar>
                    <AvatarImage src="data:image/svg+xml,<svg xmlns='http://www.w3.org/2000/svg' width='64' height='64'><rect width='64' height='64' fill='%2322c55e'/></svg>" />
                    <AvatarFallback>"B"</AvatarFallback>
                </Avatar>
                <Avatar><AvatarFallback>"C"</AvatarFallback></Avatar>
                <AvatarGroupCount>
                    "+3"
                    <Avatar><AvatarFallback>"D"</AvatarFallback></Avatar>
                    <Avatar><AvatarFallback>"E"</AvatarFallback></Avatar>
                    <Avatar><AvatarFallback>"F"</AvatarFallback></Avatar>
                </AvatarGroupCount>
            </AvatarGroup>
        </VStack>
    }
}

pub(crate) fn avatar_showcase_builders() -> Element {
    ui! {
        <HStack gap="0.5rem" items="center" wrap="wrap">
            <VStack gap="0.25rem" items="center">
                <Label font_weight="700" text_size="0.75rem">"Fallback"</Label>
                <Avatar>
                    <AvatarFallback>"ER"</AvatarFallback>
                </Avatar>
            </VStack>
            { Element::from(avatar().fallback("BLD")) }
            { Element::from(avatar().fallback_icon("user")) }
            { Element::from(
                avatar()
                    .shape("rounded-square")
                    .fallback_image("data:image/svg+xml,<svg xmlns='http://www.w3.org/2000/svg' width='64' height='64'><rect width='64' height='64' fill='%23f97316'/></svg>")
            ) }
            { Element::from(
                avatar()
                    .size("lg")
                    .image("data:image/svg+xml,<svg xmlns='http://www.w3.org/2000/svg' width='64' height='64'><rect width='64' height='64' fill='%23a855f7'/></svg>")
                    .fallback("BLD")
            ) }
        </HStack>
    }
}

#[component]
pub(crate) fn AvatarShowcase() -> jadix::core::tree::Element {
    let row = avatar_showcase_row();
    let group = avatar_showcase_group();
    let builders = avatar_showcase_builders();
    ui! {
        <MyCard title="1d) Avatar（ShadCN）">
            <VStack w="100%" min_w="0" gap="0.75rem">
                <Label w="100%" text_size="0.75rem" color="muted-foreground" leading="normal">
                    "双写法：shape=circle|rounded-square；Fallback 支持文本/Icon/遮罩图片；avatar().shape().fallback*()"
                </Label>
                { row }
                { group }
                { builders }
            </VStack>
        </MyCard>
    }
}

#[component]
pub(crate) fn BreadcrumbShowcase() -> jadix::core::tree::Element {
    ui! {
        <MyCard title="1e) Breadcrumb（ShadCN）">
            <VStack w="100%" min_w="0" gap="0.75rem">
                <Label w="100%" text_size="0.75rem" color="muted-foreground" leading="normal">
                    "双写法：RSX 复合标签与 breadcrumb().link().page()；Separator / Ellipsis 默认 Lucide 图标"
                </Label>
                <VStack w="100%" min_w="0" gap="0.5rem">
                    <Label font_weight="700" text_size="0.75rem">"RSX"</Label>
                    <Breadcrumb>
                        <BreadcrumbList>
                            <BreadcrumbItem>
                                <BreadcrumbLink on_click={|| {}}>"Home"</BreadcrumbLink>
                            </BreadcrumbItem>
                            <BreadcrumbSeparator />
                            <BreadcrumbItem>
                                <BreadcrumbLink on_click={|| {}}>"Components"</BreadcrumbLink>
                            </BreadcrumbItem>
                            <BreadcrumbSeparator />
                            <BreadcrumbItem>
                                <BreadcrumbPage>"Breadcrumb"</BreadcrumbPage>
                            </BreadcrumbItem>
                        </BreadcrumbList>
                    </Breadcrumb>
                </VStack>
                <VStack w="100%" min_w="0" gap="0.5rem">
                    <Label font_weight="700" text_size="0.75rem">"Builder"</Label>
                    { Element::from(
                        breadcrumb()
                            .link("Home", || {})
                            .link("Components", || {})
                            .page("Breadcrumb")
                    ) }
                </VStack>
                <VStack w="100%" min_w="0" gap="0.5rem">
                    <Label font_weight="700" text_size="0.75rem">"Ellipsis"</Label>
                    { Element::from(
                        breadcrumb()
                            .link("Home", || {})
                            .ellipsis()
                            .link("Settings", || {})
                            .page("Profile")
                    ) }
                </VStack>
            </VStack>
        </MyCard>
    }
}

#[component]
pub(crate) fn ButtonGroupShowcase() -> jadix::core::tree::Element {
    ui! {
        <MyCard title="1e-b) ButtonGroup（ShadCN）">
            <VStack w="100%" min_w="0" gap="0.75rem">
                <Label w="100%" text_size="0.75rem" color="muted-foreground" leading="normal">
                    "双写法：RSX 复合标签与 button_group().button()；相邻圆角合并 · 绘制期叠边 · Separator / Text"
                </Label>
                <VStack w="100%" min_w="0" gap="0.5rem">
                    <Label font_weight="700" text_size="0.75rem">"RSX · horizontal"</Label>
                    <ButtonGroup>
                        <Button variant="outline">"Copy"</Button>
                        <Button variant="outline">"Paste"</Button>
                        <Button variant="outline">"Cut"</Button>
                    </ButtonGroup>
                </VStack>
                <VStack w="100%" min_w="0" gap="0.5rem">
                    <Label font_weight="700" text_size="0.75rem">"RSX · Text + Separator"</Label>
                    <ButtonGroup>
                        <ButtonGroupText>"Archive"</ButtonGroupText>
                        <Button variant="outline">"Open"</Button>
                        <ButtonGroupSeparator />
                        <Button variant="outline">"Delete"</Button>
                    </ButtonGroup>
                </VStack>
                <VStack w="100%" min_w="0" gap="0.5rem">
                    <Label font_weight="700" text_size="0.75rem">"RSX · vertical"</Label>
                    <ButtonGroup orientation="vertical">
                        <Button variant="outline">"Top"</Button>
                        <Button variant="outline">"Middle"</Button>
                        <Button variant="outline">"Bottom"</Button>
                    </ButtonGroup>
                </VStack>
                <VStack w="100%" min_w="0" gap="0.5rem">
                    <Label font_weight="700" text_size="0.75rem">"Builder"</Label>
                    { Element::from(
                        button_group()
                            .button("Copy", || {})
                            .button("Paste", || {})
                            .separator()
                            .button("Cut", || {})
                    ) }
                </VStack>
            </VStack>
        </MyCard>
    }
}

#[component]
pub(crate) fn AspectRatioShowcase() -> jadix::core::tree::Element {
    ui! {
        <MyCard title="1f) AspectRatio（CSS aspect-ratio）">
            <VStack w="100%" min_w="0" gap="0.75rem">
                <Label w="100%" text_size="0.75rem" color="muted-foreground" leading="normal">
                    "双写法：<AspectRatio ratio> 与 aspect_ratio().ratio()；Image/Box 亦可写 aspect_ratio=\"16/9\"（MDN：定宽 + 另一轴 auto）"
                </Label>
                <HStack w="100%" min_w="0" gap="0.75rem" items="start" wrap="wrap">
                    <VStack gap="0.25rem" w="10rem">
                        <Label font_weight="700" text_size="0.75rem">"16:9 容器"</Label>
                        <AspectRatio ratio={16.0 / 9.0} w="10rem" class="rounded-md" bg="muted">
                            <Image texture="demo-checker" w="100%" h="100%" />
                        </AspectRatio>
                    </VStack>
                    <VStack gap="0.25rem" w="8rem">
                        <Label font_weight="700" text_size="0.75rem">"1:1 容器"</Label>
                        <AspectRatio ratio="1/1" w="8rem" class="rounded-md" bg="muted">
                            <Image
                                src="data:image/svg+xml,<svg xmlns='http://www.w3.org/2000/svg' width='64' height='64'><rect width='64' height='64' fill='%233b82f6'/><circle cx='32' cy='32' r='18' fill='white'/></svg>"
                                w="100%"
                                h="100%"
                            />
                        </AspectRatio>
                    </VStack>
                    <VStack gap="0.25rem" w="10rem">
                        <Label font_weight="700" text_size="0.75rem">"Image 属性"</Label>
                        <Image
                            texture="demo-checker"
                            w="10rem"
                            aspect_ratio="4/3"
                            rounded="0.375rem"
                        />
                    </VStack>
                    <VStack gap="0.25rem" w="10rem">
                        <Label font_weight="700" text_size="0.75rem">"Builder 21:9"</Label>
                        { Element::from(
                            aspect_ratio()
                                .ratio("21/9")
                                .child(
                                    Element::new()
                                        .w("100%")
                                        .h("100%")
                                        .bg(jadix::core::tree::parse_color("primary")),
                                ),
                        )
                        .w("10rem")
                        .rounded(6.0) }
                    </VStack>
                </HStack>
            </VStack>
        </MyCard>
    }
}

#[component]
pub(crate) fn RemoteAssetShowcase() -> jadix::core::tree::Element {
    const REMOTE_PNG: &str = crate::shared::GALLERY_REMOTE_PNG;
    #[cfg(feature = "remote-asset")]
    const HINT: &str = "remote-asset：http(s) src 走 set_remote_asset_fetcher（本 Gallery 用进程内映射，无外网）。换 reqwest/ureq 即可接真 HTTP。";
    #[cfg(not(feature = "remote-asset"))]
    const HINT: &str = "默认拒绝 http(s) src。cargo run -p gallery --features remote-asset 并在 main 安装 fetcher。";
    ui! {
        <MyCard title="1g) Remote Image src（remote-asset）">
            <VStack w="100%" min_w="0" gap="0.75rem">
                <Label w="100%" text_size="0.75rem" color="muted-foreground" leading="normal">
                    {HINT}
                </Label>
                <HStack w="100%" min_w="0" gap="0.75rem" items="center" wrap="wrap">
                    <VStack gap="0.25rem">
                        <Label font_weight="700" text_size="0.75rem">"Image https"</Label>
                        <Image
                            src={REMOTE_PNG}
                            w="6rem"
                            h="4rem"
                            rounded="0.375rem"
                            class="bg-muted"
                        />
                    </VStack>
                    <VStack gap="0.25rem">
                        <Label font_weight="700" text_size="0.75rem">"Avatar https"</Label>
                        <Avatar>
                            <AvatarImage src={REMOTE_PNG} />
                            <AvatarFallback>"RA"</AvatarFallback>
                        </Avatar>
                    </VStack>
                </HStack>
            </VStack>
        </MyCard>
    }
}

pub(crate) fn animation_showcase_demos(
    play: Signal<bool>,
    play_btn: Signal<bool>,
    play_box: Signal<bool>,
) -> Element {
    ui! {
        <HStack w="100%" min_w="0" gap="1.25rem" items="start" wrap="wrap" py="0.75rem" overflow="visible">
            <VStack gap="0.35rem" items="center" shrink={0.0}>
                <Label font_weight="700" text_size="0.75rem">"spin"</Label>
                <Icon name="loader-circle" size="2rem" color="foreground" class="animate-spin" />
            </VStack>
            <VStack gap="0.35rem" items="center" shrink={0.0}>
                <Label font_weight="700" text_size="0.75rem">"ping"</Label>
                <Box w="4rem" h="4rem" items="center" justify="center" overflow="hidden">
                    <Box w="2rem" h="2rem" relative={true} items="center" justify="center">
                        <Box w="2rem" h="2rem" class="rounded-full" bg="primary" />
                        <Box
                            absolute={true}
                            inset="0"
                            class="rounded-full animate-ping"
                            bg="primary"
                        />
                    </Box>
                </Box>
            </VStack>
            <VStack gap="0.35rem" items="center" shrink={0.0}>
                <Label font_weight="700" text_size="0.75rem">"pulse"</Label>
                <Box w="3rem" h="2rem" bg="secondary" class="rounded-md animate-pulse" />
            </VStack>
            <VStack gap="0.35rem" items="center" shrink={0.0}>
                <Label font_weight="700" text_size="0.75rem">"bounce"</Label>
                <Box w="2rem" h="2rem" bg="accent" class="rounded-md animate-bounce" />
            </VStack>
            <VStack gap="0.35rem" items="center" shrink={0.0}>
                <Label font_weight="700" text_size="0.75rem">"alternate pulse"</Label>
                <Box
                    w="3rem"
                    h="2rem"
                    bg="destructive"
                    class="rounded-md"
                    animation="1s ease-in-out infinite alternate pulse"
                />
            </VStack>
            <VStack gap="0.35rem" items="center" shrink={0.0}>
                <Label font_weight="700" text_size="0.75rem">"play-state"</Label>
                <Box w="5rem" h="5rem" items="center" justify="center">
                    <Box
                        w="2.5rem"
                        h="2.5rem"
                        bg="primary"
                        class="rounded-md"
                        animation="1s linear infinite spin"
                        animation_play_state={ if play_box.get() { "running" } else { "paused" } }
                    />
                </Box>
                <Box min_w="4.75rem" items="center">
                    <Button size="sm" variant="outline" on_click={move || play_btn.set(!play_btn.get())}>
                        { if play.get() { "Pause" } else { "Play" } }
                    </Button>
                </Box>
            </VStack>
            <VStack gap="0.35rem" items="center" shrink={0.0}>
                <Label font_weight="700" text_size="0.75rem">"fade alternate"</Label>
                <Box
                    w="3rem"
                    h="2rem"
                    bg="muted"
                    class="rounded-md"
                    animation="1.2s ease-in-out infinite alternate fade-in"
                />
            </VStack>
        </HStack>
    }
}

#[component]
pub(crate) fn AnimationShowcase() -> jadix::core::tree::Element {
    let play = use_signal("gal_anim_play", || true);
    let play_btn = play;
    let play_box = play;
    let demos = animation_showcase_demos(play, play_btn, play_box);
    ui! {
        <MyCard title="1g) CSS animation（MDN）">
            <VStack w="100%" min_w="0" gap="0.75rem">
                <Label w="100%" text_size="0.75rem" color="muted-foreground" leading="normal">
                    "utility：animate-spin / ping / pulse / bounce；属性：animation / animation_play_state / fill-mode；Icon+spin 走字形贴图+transform（非 glyphon）；fade 用 infinite alternate"
                </Label>
                { demos }
            </VStack>
        </MyCard>
    }
}

#[component]
pub(crate) fn WaapiShowcase() -> jadix::core::tree::Element {
    let alice = use_signal("gal_waapi_alice", || {
        jadix::style::Animation::animate(
            vec![
                jadix::style::Keyframe::new()
                    .transform("rotate(0deg)")
                    .color("#18181b"),
                jadix::style::Keyframe::new().color("#431236").offset(0.3),
                jadix::style::Keyframe::new()
                    .transform("rotate(360deg)")
                    .color("#18181b"),
            ],
            jadix::style::KeyframeEffect::new()
                .duration(3000.0)
                .iterations(f32::INFINITY),
        )
    });
    let anim = alice.get();
    let stage = {
        let el: Element = ui! {
            <Box w="3.5rem" h="3.5rem" bg="primary" class="rounded-md" />
        };
        el.animate(anim)
    };
    ui! {
        <MyCard title="1h) 编程式动画句柄">
            <VStack w="100%" min_w="0" gap="0.75rem">
                <Label w="100%" text_size="0.75rem" color="muted-foreground" leading="normal">
                    "关键帧对象 + play / pause / reverse / set_rate；cancel 不触发 finish。句柄 + 注册表，不是 WAAPI 规范对象。finished 是 Signal。布局分区还有 spring() / linear() / clip-path。"
                </Label>
                <HStack gap="0.75rem" items="center" wrap="wrap">
                    { stage }
                    <Button size="sm" variant="outline" on_click={move || anim.pause()}>"pause"</Button>
                    <Button size="sm" variant="outline" on_click={move || anim.play()}>"play"</Button>
                    <Button size="sm" variant="outline" on_click={move || anim.reverse()}>"reverse"</Button>
                    <Button size="sm" variant="outline" on_click={move || {
                        anim.set_rate(anim.playback_rate() * 1.25);
                    }}>"faster"</Button>
                    <Button size="sm" variant="outline" on_click={move || anim.update_playback_rate(1.0)}>"rate=1"</Button>
                </HStack>
            </VStack>
        </MyCard>
    }
}

/// Gallery track (`16rem`) minus ball (`1.75rem`). Bind this, not a shorter 200px.
fn gen_track_travel_px() -> f32 {
    const TRACK_REM: f32 = 16.0;
    const BALL_REM: f32 = 1.75;
    (TRACK_REM - BALL_REM) * jadix::layout::dimension::css_root_font_size_px()
}

fn gen_track_ball(x: jadix::core::reactivity::Signal<f32>, bind_key: &str, tone: &str) -> Element {
    let target_id = format!("gal_gen_tx:{bind_key}");
    let ball = {
        let el: Element = ui! {
            <Box
                id={target_id}
                w="1.75rem"
                h="1.75rem"
                class="rounded-full"
                bg={tone}
                shrink={0.0}
            />
        };
        jadix::app::reactive::bind_prop(
            el,
            format!("gal_gen_tx:{bind_key}"),
            move || x.get(),
            |el, px| {
                // `translate` feeds paint AABB / dest-reset. `transform: translateX`
                // leaves `vis` at the layout box and ghosts the previous pose.
                el.style.transform = None;
                if (el.style.translate[0] - px).abs() <= 0.05 {
                    LayoutStatus::Clean
                } else {
                    el.style.translate[0] = px;
                    LayoutStatus::StyleChanged
                }
            },
        )
    };
    ui! {
        <VStack gap="0.35rem" items="start" w="16rem" shrink={0.0}>
            <Label font_weight="700" text_size="0.75rem">{ bind_key }</Label>
            <Box
                w="16rem"
                h="2.5rem"
                class="rounded-full"
                bg="muted"
                items="center"
                overflow="hidden"
            >
                { ball }
            </Box>
        </VStack>
    }
}

fn gen_css_chip(gone: Signal<bool>, timing: &str, tone: &str, caption: &str, key: &str) -> Element {
    let tid = format!("gal-gen-css-{key}");
    let bind = format!("gal_gen_css_cls:{key}");
    let trans = format!("transform {timing}");
    let ball: Element = ui! {
        <Box
            id={tid}
            w="1.75rem"
            h="1.75rem"
            class="rounded-full"
            bg={tone}
            transition={trans}
        />
    };
    // Direct translate — do not `bind_class` (replaces ClassSet + clones Style).
    // Travel = track `14rem` − ball `1.75rem` (not a shorter `translate-x-40`).
    let travel = (14.0 - 1.75) * jadix::layout::dimension::css_root_font_size_px();
    let ball = jadix::app::reactive::bind_prop(
        ball,
        bind,
        move || gone.get(),
        move |el, gone| {
            let px = if gone { travel } else { 0.0 };
            if (el.style.translate[0] - px).abs() <= f32::EPSILON {
                LayoutStatus::Clean
            } else {
                el.style.translate[0] = px;
                LayoutStatus::StyleChanged
            }
        },
    );
    ui! {
        <VStack gap="0.35rem" items="start" shrink={0.0}>
            <Label font_weight="700" text_size="0.75rem">{ caption }</Label>
            <Box w="14rem" h="2.25rem" items="center" overflow="hidden">
                { ball }
            </Box>
        </VStack>
    }
}

fn gen_stagger_bar(on: Signal<bool>, index: usize, tone: &str) -> Element {
    let id = format!("gal-gen-stagger-{index}");
    let bar = Element::new()
        .template_id(id.clone())
        .w("1.25rem")
        .h("3.5rem")
        .rounded("0.375rem")
        .bg(tone)
        .transform_origin("center bottom")
        .transition(format!(
            "transform 520ms spring(visual 0.28) {}ms",
            index * 50
        ));
    jadix::app::reactive::bind_prop(
        bar,
        id,
        move || on.get(),
        |el, on| {
            let scale = if on { 1.0 } else { 0.75 / 3.5 };
            if (el.style.scale[1] - scale).abs() < 1e-6 {
                return LayoutStatus::Clean;
            }
            el.style.scale[1] = scale;
            LayoutStatus::StyleChanged
        },
    )
}

/// Motion 生成器内核：CSS `spring()` 解析 + 标量 `animate` / follow / inertia / stagger / 序列。
#[component]
pub(crate) fn GeneratorShowcase() -> jadix::core::tree::Element {
    use jadix::style::{
        use_animated, AnimatedNumber, InertiaOptions, RepeatMode, TimingFunction, ValueDriver,
    };

    let travel = gen_track_travel_px();
    let mid = travel * 0.5;
    let css_go = use_signal("gal_gen_css_go", || false);
    let spring = use_animated("gal_gen_spring", 0.0);
    let follow_n = {
        let t0 = 0.0;
        let sig = use_signal("gal_gen_follow", move || AnimatedNumber::follow(t0));
        sig.peek()
    };
    let inert = use_animated("gal_gen_inert", 0.0);
    let wave = {
        let to = travel;
        let sig = use_signal("gal_gen_wave", move || {
            let n = AnimatedNumber::tween(0.0, to, 900.0, TimingFunction::EaseInOut)
                .repeat(RepeatMode::PingPong);
            n.pause();
            n
        });
        sig.peek()
    };
    let seq = use_animated("gal_gen_seq", 0.0);
    let stagger_on = use_signal("gal_gen_stagger", || false);

    let spring_x = spring.signal();
    let follow_x = follow_n.signal();
    let inert_x = inert.signal();
    let wave_x = wave.signal();
    let seq_x = seq.signal();
    let spring_go = spring.clone();
    let follow_a = follow_n.clone();
    let follow_b = follow_n.clone();
    let follow_c = follow_n;
    let inert_h = inert;
    let seq_h = seq.clone();
    let seq_q = seq;
    let wave_pause = wave.clone();
    let wave_play = wave.clone();
    let wave_half = wave.clone();
    let wave_one = wave.clone();
    let wave_double = wave;

    ui! {
        <MyCard title="1i) 生成器内核（Motion）">
            <VStack w="100%" min_w="0" gap="1rem">
                <Label w="100%" text_size="0.75rem" color="muted-foreground" leading="normal">
                    "对照 motion-dom：闭式弹簧 / visualDuration / inertia / follow 改道 / stagger / then_to。上排 CSS spring()；下排句柄绑 translateX（StyleChanged，不绑 w）。pingpong 默认暂停。"
                </Label>

                <VStack w="100%" min_w="0" gap="0.5rem">
                    <HStack gap="0.5rem" items="center" wrap="wrap">
                        <Label font_weight="700" text_size="0.75rem">"CSS spring()"</Label>
                        <Button
                            size="sm"
                            variant="outline"
                            w="4.25rem"
                            on_click={move || css_go.set(!css_go.peek())}
                        >
                            { jadix::app::reactive::bind_text("gal_gen_css_go_lbl", move || {
                                if css_go.get() {
                                    "Reset".into()
                                } else {
                                    "Go".into()
                                }
                            })
                            .w("100%") }
                        </Button>
                    </HStack>
                    <HStack w="100%" min_w="0" gap="1rem" items="start" wrap="wrap">
                        { gen_css_chip(css_go, "spring(800ms, 0.35)", "primary", "spring(800ms, 0.35)", "dur") }
                        { gen_css_chip(css_go, "spring(visual 0.25)", "chart-2", "spring(visual 0.25)", "vis") }
                        { gen_css_chip(css_go, "400ms ease-out", "chart-4", "400ms ease-out", "ease") }
                    </HStack>
                </VStack>

                <VStack w="100%" min_w="0" gap="0.5rem">
                    <HStack gap="0.5rem" items="center" wrap="wrap">
                        <Label font_weight="700" text_size="0.75rem">"animate / follow"</Label>
                        <Button size="sm" variant="outline" on_click={move || {
                            spring_go.set(if spring_go.peek() > travel * 0.4 { 0.0 } else { travel });
                        }}>"spring 改道"</Button>
                        <Button size="sm" variant="outline" on_click={move || follow_a.set(0.0)}>"follow ←"</Button>
                        <Button size="sm" variant="outline" on_click={move || follow_b.set(mid)}>"follow ·"</Button>
                        <Button size="sm" variant="outline" on_click={move || follow_c.set(travel)}>"follow →"</Button>
                    </HStack>
                    <VStack gap="0.5rem" items="start" w="100%">
                        { gen_track_ball(spring_x, "spring", "primary") }
                        { gen_track_ball(follow_x, "follow", "chart-2") }
                    </VStack>
                </VStack>

                <VStack w="100%" min_w="0" gap="0.5rem">
                    <HStack gap="0.5rem" items="center" wrap="wrap">
                        <Label font_weight="700" text_size="0.75rem">"inertia · pingpong · sequence"</Label>
                        <Button size="sm" variant="outline" on_click={move || {
                            let mut opts = InertiaOptions::default();
                            opts.velocity = 720.0;
                            opts.max = Some(travel);
                            opts.min = Some(0.0);
                            inert_h.restart(inert_h.peek(), ValueDriver::Inertia(opts));
                        }}>"flick →"</Button>
                        <Button size="sm" variant="outline" on_click={move || wave_pause.pause()}>"pause"</Button>
                        <Button size="sm" variant="outline" on_click={move || wave_play.play()}>"play"</Button>
                        <Button size="sm" variant="outline" on_click={move || wave_half.set_speed(0.5)}>"½×"</Button>
                        <Button size="sm" variant="outline" on_click={move || wave_one.set_speed(1.0)}>"1×"</Button>
                        <Button size="sm" variant="outline" on_click={move || wave_double.set_speed(2.0)}>"2×"</Button>
                        <Button size="sm" variant="outline" on_click={move || {
                            seq_h.restart(travel, ValueDriver::tween(420.0, TimingFunction::EaseOut));
                            seq_q.clone().then_to(0.0, ValueDriver::spring());
                        }}>"then_to"</Button>
                    </HStack>
                    <VStack gap="0.5rem" items="start" w="100%">
                        { gen_track_ball(inert_x, "inert", "chart-4") }
                        { gen_track_ball(wave_x, "wave", "destructive") }
                        { gen_track_ball(seq_x, "seq", "primary") }
                    </VStack>
                </VStack>

                <VStack w="100%" min_w="0" gap="0.5rem">
                    <HStack gap="0.5rem" items="center" wrap="wrap">
                        <Label font_weight="700" text_size="0.75rem">"stagger"</Label>
                        <Button size="sm" variant="outline" on_click={move || stagger_on.set(!stagger_on.get())}>
                            { jadix::app::reactive::bind_text("gal_gen_stagger_label", move || {
                                if stagger_on.get() { "Reset".into() } else { "Play".into() }
                            }).w("3rem") }
                        </Button>
                    </HStack>
                    <HStack gap="0.4rem" items="end" h="4rem">
                        { gen_stagger_bar(stagger_on, 0, "primary") }
                        { gen_stagger_bar(stagger_on, 1, "chart-2") }
                        { gen_stagger_bar(stagger_on, 2, "chart-3") }
                        { gen_stagger_bar(stagger_on, 3, "chart-4") }
                        { gen_stagger_bar(stagger_on, 4, "chart-5") }
                    </HStack>
                </VStack>
            </VStack>
        </MyCard>
    }.template_id("gal-generator-showcase")
}

/// WP-R.7：`format!(…, signal.get())` 走自动 bind_text；`#[component(reactive)]` 体不因 count 重跑。
#[component(reactive)]
pub(crate) fn ReactiveCountChip(
    id: String,
    count: jadix::core::reactivity::Signal<i32>,
) -> jadix::core::tree::Element {
    ui! {
        <HStack gap="0.5rem" items="center" p="0.5rem" class="rounded" bg="muted">
            <Label text_size="0.875rem">{ format!("count {}", count.get()) }</Label>
        </HStack>
    }
}

#[component]
pub(crate) fn ButtonShowcase() -> jadix::core::tree::Element {
    let count = use_signal("gallery_btn_count", || 0);
    let selected = use_signal("gallery_selected_variant", || "default".to_string());

    ui! {
        <MyCard title="1) Button Variants + State">
            <VStack w="100%" min_w="0" gap="0.75rem">
                <HStack w="100%" min_w="0" gap="0.5rem" items="center" wrap="wrap">
                    <Button variant="default" on_click={move || {
                        increment_count(count);
                        choose_variant(selected, "default");
                    }}>
                        { format!("Default ({})", count.get()) }
                    </Button>
                    <Button variant="secondary" on_click={move || choose_variant(selected, "secondary")}>
                        "Secondary"
                    </Button>
                    <Button variant="outline" on_click={move || choose_variant(selected, "outline")}>
                        "Outline"
                    </Button>
                    <Button variant="ghost" on_click={move || choose_variant(selected, "ghost")}>
                        "Ghost"
                    </Button>
                    <Button variant="destructive" on_click={move || choose_variant(selected, "destructive")}>
                        "Destructive"
                    </Button>
                    <Button variant="success" on_click={move || choose_variant(selected, "success")}>
                        "Success"
                    </Button>
                    <Button variant="warning" on_click={move || choose_variant(selected, "warning")}>
                        "Warning"
                    </Button>
                    <Button variant="link" on_click={move || choose_variant(selected, "link")}>
                        "Link"
                    </Button>
                    <Button variant="default" disabled=true>"Disabled"</Button>
                </HStack>
                <Label w="100%" text_size="0.875rem" color="muted-foreground" leading="normal">
                    { format!("Selected variant: {}", selected.get()) }
                </Label>
            </VStack>
        </MyCard>
    }
}

#[component]
pub(crate) fn ButtonHoverEasingShowcase() -> jadix::core::tree::Element {
    ui! {
        <MyCard title="1b) Hover：弹簧 vs cubic-bezier">
            <VStack w="100%" min_w="0" gap="0.75rem">
                <Label w="100%" text_size="0.75rem" color="muted-foreground" leading="normal">
                    "默认 hover_easing=\"spring\"；设为 \"cubic\" 时对标 CSS transition + cubic-bezier(0.4, 0, 0.2, 1)"
                </Label>
                <HStack w="100%" min_w="0" gap="0.5rem" items="center" wrap="wrap">
                    <Button variant="ghost" hover_easing="spring">"Ghost · spring"</Button>
                    <Button variant="ghost" hover_easing="cubic">"Ghost · cubic"</Button>
                </HStack>
            </VStack>
        </MyCard>
    }
}

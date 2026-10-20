//! Gallery 壳：侧栏、分区切换、根 `app_builder`。

use crate::imports::*;
use crate::pages::*;

pub(crate) fn gallery_nav_menu(
    section: Signal<String>,
    prefix: &str,
    close: Option<Signal<bool>>,
) -> Element {
    let select = move |key: &str| {
        section.set(key.to_string());
        if let Some(open) = close {
            open.set(false);
        }
    };
    ui! {
        <SidebarGroup>
            <SidebarGroupLabel>"导航"</SidebarGroupLabel>
            <SidebarGroupContent>
                <SidebarMenu>
                    <SidebarMenuItem>
                        <SidebarMenuButton
                            id={format!("{prefix}-nav-basics")}
                            isActive={section.get() == "basics"}
                            on_click={move || select("basics")}
                        >
                            <HStack w="100%" items="center" gap="0.5rem" grow="1" min_w="0">
                                <Icon name="layout-dashboard" size="16px" color="sidebar-foreground" pointer_events="none" />
                                <Label grow="1" min_w="0" text_size="0.875rem" color="sidebar-foreground">"基础"</Label>
                            </HStack>
                        </SidebarMenuButton>
                    </SidebarMenuItem>
                    <SidebarMenuItem>
                        <SidebarMenuButton
                            id={format!("{prefix}-nav-events")}
                            isActive={section.get() == "events"}
                            on_click={move || select("events")}
                        >
                            <HStack w="100%" items="center" gap="0.5rem" grow="1" min_w="0">
                                <Icon name="mouse-pointer-click" size="16px" color="sidebar-foreground" pointer_events="none" />
                                <Label grow="1" min_w="0" text_size="0.875rem" color="sidebar-foreground">"指针事件"</Label>
                            </HStack>
                        </SidebarMenuButton>
                    </SidebarMenuItem>
                    <SidebarMenuItem>
                        <SidebarMenuButton
                            id={format!("{prefix}-nav-forms")}
                            isActive={section.get() == "forms"}
                            on_click={move || select("forms")}
                        >
                            <HStack w="100%" items="center" gap="0.5rem" grow="1" min_w="0">
                                <Icon name="file-text" size="16px" color="sidebar-foreground" pointer_events="none" />
                                <Label grow="1" min_w="0" text_size="0.875rem" color="sidebar-foreground">"表单"</Label>
                            </HStack>
                        </SidebarMenuButton>
                    </SidebarMenuItem>
                    <SidebarMenuItem>
                        <SidebarMenuButton
                            id={format!("{prefix}-nav-overlays")}
                            isActive={section.get() == "overlays"}
                            on_click={move || select("overlays")}
                        >
                            <HStack w="100%" items="center" gap="0.5rem" grow="1" min_w="0">
                                <Icon name="layers" size="16px" color="sidebar-foreground" pointer_events="none" />
                                <Label grow="1" min_w="0" text_size="0.875rem" color="sidebar-foreground">"浮层 / 反馈"</Label>
                            </HStack>
                        </SidebarMenuButton>
                    </SidebarMenuItem>
                    <SidebarMenuItem>
                        <SidebarMenuButton
                            id={format!("{prefix}-nav-data")}
                            isActive={section.get() == "data"}
                            on_click={move || select("data")}
                        >
                            <HStack w="100%" items="center" gap="0.5rem" grow="1" min_w="0">
                                <Icon name="table" size="16px" color="sidebar-foreground" pointer_events="none" />
                                <Label grow="1" min_w="0" text_size="0.875rem" color="sidebar-foreground">"数据"</Label>
                            </HStack>
                        </SidebarMenuButton>
                    </SidebarMenuItem>
                    <SidebarMenuItem>
                        <SidebarMenuButton
                            id={format!("{prefix}-nav-draw")}
                            isActive={section.get() == "draw"}
                            on_click={move || select("draw")}
                        >
                            <HStack w="100%" items="center" gap="0.5rem" grow="1" min_w="0">
                                <Icon name="pencil" size="16px" color="sidebar-foreground" pointer_events="none" />
                                <Label grow="1" min_w="0" text_size="0.875rem" color="sidebar-foreground">"绘制"</Label>
                            </HStack>
                        </SidebarMenuButton>
                    </SidebarMenuItem>
                    <SidebarMenuItem>
                        <SidebarMenuButton
                            id={format!("{prefix}-nav-navigation")}
                            isActive={section.get() == "navigation"}
                            on_click={move || select("navigation")}
                        >
                            <HStack w="100%" items="center" gap="0.5rem" grow="1" min_w="0">
                                <Icon name="compass" size="16px" color="sidebar-foreground" pointer_events="none" />
                                <Label grow="1" min_w="0" text_size="0.875rem" color="sidebar-foreground">"导航 / 窗口"</Label>
                            </HStack>
                        </SidebarMenuButton>
                    </SidebarMenuItem>
                    <SidebarMenuItem>
                        <SidebarMenuButton
                            id={format!("{prefix}-nav-layout")}
                            isActive={section.get() == "layout"}
                            on_click={move || select("layout")}
                        >
                            <HStack w="100%" items="center" gap="0.5rem" grow="1" min_w="0">
                                <Icon name="layout-grid" size="16px" color="sidebar-foreground" pointer_events="none" />
                                <Label grow="1" min_w="0" text_size="0.875rem" color="sidebar-foreground">"布局"</Label>
                            </HStack>
                        </SidebarMenuButton>
                    </SidebarMenuItem>
                    <SidebarMenuItem>
                        <SidebarMenuButton
                            id={format!("{prefix}-nav-text")}
                            isActive={section.get() == "text"}
                            on_click={move || select("text")}
                        >
                            <HStack w="100%" items="center" gap="0.5rem" grow="1" min_w="0">
                                <Icon name="type" size="16px" color="sidebar-foreground" pointer_events="none" />
                                <Label grow="1" min_w="0" text_size="0.875rem" color="sidebar-foreground">"文本"</Label>
                            </HStack>
                        </SidebarMenuButton>
                    </SidebarMenuItem>
                    <SidebarMenuItem>
                        <SidebarMenuButton
                            id={format!("{prefix}-nav-theme")}
                            isActive={section.get() == "theme"}
                            on_click={move || select("theme")}
                        >
                            <HStack w="100%" items="center" gap="0.5rem" grow="1" min_w="0">
                                <Icon name="sun" size="16px" color="sidebar-foreground" pointer_events="none" />
                                <Label grow="1" min_w="0" text_size="0.875rem" color="sidebar-foreground">"主题"</Label>
                            </HStack>
                        </SidebarMenuButton>
                    </SidebarMenuItem>
                    <SidebarMenuItem>
                        <SidebarMenuButton
                            id={format!("{prefix}-nav-about")}
                            isActive={section.get() == "about"}
                            on_click={move || select("about")}
                        >
                            <HStack w="100%" items="center" gap="0.5rem" grow="1" min_w="0">
                                <Icon name="info" size="16px" color="sidebar-foreground" pointer_events="none" />
                                <Label grow="1" min_w="0" text_size="0.875rem" color="sidebar-foreground">"关于"</Label>
                            </HStack>
                        </SidebarMenuButton>
                    </SidebarMenuItem>
                </SidebarMenu>
            </SidebarGroupContent>
        </SidebarGroup>
    }
}

#[component]
pub(crate) fn GallerySectionForms() -> Element {
    jadix::core::reactivity::isolate_reads(|| {
        ui! {
            <ScrollArea id="scroll-forms" flex_grow="1" min_h="0" overflow_y="auto" overflow_x="auto" scroll_behavior="smooth">
                <VStack w="100%" min_w="0" gap="1rem" class="p-3 sm:p-4 lg:p-6">
                    <InputShowcase />
                    <InputAdornmentShowcase />
                    <InputOTPShowcase />
                    <FormValidateShowcase />
                    <QuestionnaireShowcase />
                    <PickerPlaceholdersShowcase />
                    <TextareaComboboxShowcase />
                    <ToggleGroupShowcase />
                    <ToggleControlsShowcase />
                    <RadioSliderShowcase />
                    <SelectPopoverTabShowcase />
                </VStack>
            </ScrollArea>
        }
    })
}

#[component]
pub(crate) fn GallerySectionEvents() -> Element {
    jadix::core::reactivity::isolate_reads(|| {
        ui! {
            <ScrollArea id="scroll-events" flex_grow="1" min_h="0" overflow_y="auto" overflow_x="auto" scroll_behavior="smooth">
                <VStack w="100%" min_w="0" gap="1rem" class="p-3 sm:p-4 lg:p-6">
                    <MouseEventShowcase />
                    <CursorShowcase />
                </VStack>
            </ScrollArea>
        }
    })
}

#[component]
pub(crate) fn GallerySectionOverlays() -> Element {
    jadix::core::reactivity::isolate_reads(|| {
        ui! {
            <ScrollArea id="scroll-overlays" flex_grow="1" min_h="0" overflow_y="auto" overflow_x="auto" scroll_behavior="smooth">
                <VStack w="100%" min_w="0" gap="1rem" class="p-3 sm:p-4 lg:p-6">
                    <ViewportModalShowcase />
                    <LocalOverlayShowcase />
                    <AnchoredOverlayShowcase />
                    <PresenceShowcase />
                    <DragShowcase />
                    <OverlayFeedbackShowcase />
                    <PrimitivesShowcase />
                </VStack>
            </ScrollArea>
        }
    })
}

#[component]
pub(crate) fn GallerySectionDraw() -> Element {
    jadix::core::reactivity::isolate_reads(|| {
        ui! {
            <ScrollArea id="scroll-draw" flex_grow="1" min_h="0" overflow_y="auto" overflow_x="auto" scroll_behavior="smooth">
                <VStack w="100%" min_w="0" gap="1rem" class="p-3 sm:p-4 lg:p-6">
                    <DrawingSvgShowcase />
                    <LucideAnimatedShowcase />
                    <MorphIconShowcase />
                    <DrawingCanvasShowcase />
                    <ImageStreamShowcase />
                    <MediaPlayerShowcase />
                    <DrawingScene3dShowcase />
                    <DrawingBillboardShowcase />
                    <DrawingShaderShowcase />
                </VStack>
            </ScrollArea>
        }
    })
}

#[component]
pub(crate) fn GallerySectionData() -> Element {
    jadix::core::reactivity::isolate_reads(|| {
        ui! {
            <ScrollArea id="scroll-data" flex_grow="1" min_h="0" overflow_y="auto" overflow_x="auto" scroll_behavior="smooth">
                <VStack w="100%" min_w="0" gap="1rem" class="p-3 sm:p-4 lg:p-6">
                    <TableShowcase />
                    <PaginationShowcase />
                    <TimelineShowcase />
                    <DataChartShowcase />
                    <VirtualMasonryShowcase />
                </VStack>
            </ScrollArea>
        }
    })
}

#[component]
pub(crate) fn GallerySectionNavigation() -> Element {
    jadix::core::reactivity::isolate_reads(|| {
        ui! {
            <ScrollArea id="scroll-navigation" flex_grow="1" min_h="0" overflow_y="auto" overflow_x="auto" scroll_behavior="smooth">
                <VStack w="100%" min_w="0" gap="1rem" class="p-3 sm:p-4 lg:p-6">
                    <FullscreenShowcase />
                    <InstanceShowcase />
                    <MultiWindowShowcase />
                    <DockBrowserShowcase />
                    <SidebarShowcase />
                    <NavbarNavigationMenuShowcase />
                    <MenubarShowcase />
                    <StepperShowcase />
                    <NavigatorShowcase />
                    <RouterShowcase />
                    <TrayShowcase />
                    <WindowChromeShowcase />
                    <SelectPopoverTabShowcase />
                </VStack>
            </ScrollArea>
        }
    })
}

#[component]
pub(crate) fn GallerySectionLayout() -> Element {
    jadix::core::reactivity::isolate_reads(|| {
        ui! {
            <ScrollArea id="scroll-layout" flex_grow="1" min_h="0" overflow_y="auto" overflow_x="auto" scroll_behavior="smooth">
                <VStack w="100%" min_w="0" gap="1rem" class="p-3 sm:p-4 lg:p-6">
                    <ResponsiveShowcase />
                    <TextFlowShowcase />
                    <LayoutSubtreeShowcase />
                    <DamageShowcase />
                    <FgMatrixShowcase />
                    <OwnerRegistryShowcase />
                    <StoreResourceShowcase />
                    <RootBareGetShowcase />
                    <IdentityApiShowcase />
                    <BoxModelShowcase />
                    <WritingModeShowcase />
                    <ScrollShowcase />
                    <AccordionShowcase />
                    <CollapsibleShowcase />
                    <ResizableShowcase />
                    <LogicalShowcase />
                    <UtilityClassShowcase />
                    <GridAndDragShowcase />
                    <ShadowShowcase />
                    <FilterShowcase />
                    <GradientShowcase />
                    <BorderStyleShowcase />
                    <OutlineShowcase />
                    <TransitionShowcase />
                    <AnimAlignmentShowcase />
                    <ScaleZoomShowcase />
                    <RotateTranslateShowcase />
                    <TransformShowcase />
                </VStack>
            </ScrollArea>
        }
    })
}

#[component]
pub(crate) fn GallerySectionText() -> Element {
    jadix::core::reactivity::isolate_reads(|| {
        ui! {
            <ScrollArea id="scroll-text" flex_grow="1" min_h="0" overflow_y="auto" overflow_x="auto" scroll_behavior="smooth">
                <VStack w="100%" min_w="0" gap="1rem" class="p-3 sm:p-4 lg:p-6">
                    <TextFlowShowcase />
                    <FontOverrideShowcase />
                    <TextAlignShowcase />
                    <MsdfMultichannelShowcase />
                    <WritingModeShowcase />
                    <TextCompatibilityShowcase />
                    <EmojiRealtimeRegressionShowcase />
                    <TypographyShowcase />
                </VStack>
            </ScrollArea>
        }
    })
}

#[component]
pub(crate) fn GallerySectionAbout() -> Element {
    jadix::core::reactivity::isolate_reads(|| {
        ui! {
            <ScrollArea id="scroll-about" flex_grow="1" min_h="0" overflow_y="auto" overflow_x="auto" scroll_behavior="smooth">
                <VStack w="100%" min_w="0" gap="1rem" class="p-3 sm:p-4 lg:p-6" max_w="48rem">
                    <Box bg="secondary" p="1.5rem" rounded="0.75rem" w="100%">
                        <VStack gap="0.5rem">
                            <Label font_weight="700">"Technical Stack"</Label>
                            <Label>"Layout: Taffy (Flexbox / Grid)"</Label>
                            <Label>"Text: Glyphon (Cosmic-text)"</Label>
                            <Label>"Rendering: WGPU"</Label>
                            <Label>"Reactivity: Signal fine-grained updates"</Label>
                            <Label>"A11y: AccessKit bridge"</Label>
                        </VStack>
                    </Box>
                    <Label text_size="0.875rem" color="muted-foreground">
                        "侧栏分区对应 ENGINEERING_IMPLEMENTATION_PLAN WP-3.1 Gallery 覆盖矩阵。"
                    </Label>
                </VStack>
            </ScrollArea>
        }
    })
}

#[component]
pub(crate) fn GallerySectionBasics() -> Element {
    jadix::core::reactivity::isolate_reads(|| {
        ui! {
            <ScrollArea id="scroll-basics" flex_grow="1" min_h="0" overflow_y="auto" overflow_x="auto" scroll_behavior="smooth">
                <VStack w="100%" min_w="0" gap="1rem" class="p-3 sm:p-4 lg:p-6">
                    <ButtonShowcase />
                    <ButtonHoverEasingShowcase />
                    <BadgeShowcase />
                    <KbdShowcase />
                    <MarkerShowcase />
                    <MessageShowcase />
                    <MessageScrollerShowcase />
                    <SkeletonShowcase />
                    <EmptyShowcase />
                    <AlertShowcase />
                    <AlertDialogShowcase />
                    <ItemShowcase />
                    <AttachmentShowcase />
                    <ListShowcase />
                    <BubbleShowcase />
                    <AvatarShowcase />
                    <BreadcrumbShowcase />
                    <ButtonGroupShowcase />
                    <AspectRatioShowcase />
                    <RemoteAssetShowcase />
                    <AnimationShowcase />
                    <WaapiShowcase />
                    <GeneratorShowcase />
                    <TypographyShowcase />
                </VStack>
            </ScrollArea>
        }
    })
}

pub(crate) fn build_gallery_app() -> Element {
    let navigator = gallery_page_navigator();
    let page_nav = navigator.clone();
    navigation_host(navigator, move |route| match route {
        GalleryPageRoute::Gallery => build_gallery_home(),
        GalleryPageRoute::DockBrowser => dock_browser_page(page_nav.clone()),
    }).into()
}

fn build_gallery_home() -> Element {
    let gallery_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    jadix::style::watch_stylesheets([
        gallery_dir.join("theme.toml"),
        gallery_dir.join("styles.toml"),
    ]);
    let section = use_signal("gal_nav_section", || {
        std::env::var("JADIX_GALLERY_SECTION").unwrap_or_else(|_| "basics".to_string())
    });

    // Shell + a single root `<Toaster/>` (Sonner-style). Title / desc / body use `<Match scope>`
    // so island ids stay `examples/gallery/App:section_*` instead of drifting with `file:line`.
    ui! {
        <Box w="100%" h="100%" relative={true}>
            <SidebarProvider id="gal-shell" variant="inset" defaultOpen={true}>
                <Sidebar id="gal-sidebar" variant="inset" collapsible="icon">
                    <SidebarHeader>
                        <SidebarMenu>
                            <SidebarMenuItem>
                                <SidebarMenuButton id="gal-workspace">
                                    <HStack w="100%" items="center" gap="0.5rem" grow="1" min_w="0">
                                        <Icon name="box" size="16px" color="sidebar-foreground" pointer_events="none" />
                                        <Label grow="1" min_w="0" text_size="0.875rem" font_weight="700" color="sidebar-foreground">"Jadix Gallery"</Label>
                                    </HStack>
                                </SidebarMenuButton>
                            </SidebarMenuItem>
                        </SidebarMenu>
                    </SidebarHeader>
                    <SidebarSeparator />
                    <SidebarContent>
                        { gallery_nav_menu(section, "gal", use_sidebar().map(|ctx| ctx.open_mobile)) }
                    </SidebarContent>
                    <SidebarSeparator />
                    <SidebarFooter>
                        <SidebarMenu>
                            <SidebarMenuItem>
                                <SidebarMenuButton id="gal-user">
                                    <HStack w="100%" items="center" gap="0.5rem" grow="1" min_w="0">
                                        <Icon name="user" size="16px" color="sidebar-foreground" pointer_events="none" />
                                        <Label grow="1" min_w="0" text_size="0.875rem" color="sidebar-foreground">"Gallery"</Label>
                                    </HStack>
                                </SidebarMenuButton>
                            </SidebarMenuItem>
                        </SidebarMenu>
                    </SidebarFooter>
                </Sidebar>
                <SidebarInset variant="inset">
                    <HStack
                        w="100%"
                        items="center"
                        gap="0.5rem"
                        px="1rem"
                        py="0.5rem"
                        flex_shrink="0"
                        min_w="0"
                    >
                        <SidebarTrigger id="gal-sidebar-trigger" />
                        <Separator vertical={true} />
                        <VStack gap="0.125rem" min_w="0" grow="1">
                            <Match scope="examples/gallery/App:section_title">
                                <When when={section.get() == "forms"}>
                                    <Label id="gal-section-title" w="100%" min_w="0" text_size="1.25rem" font_weight="700" text_overflow="ellipsis">"表单"</Label>
                                </When>
                                <When when={section.get() == "events"}>
                                    <Label id="gal-section-title" w="100%" min_w="0" text_size="1.25rem" font_weight="700" text_overflow="ellipsis">"指针事件"</Label>
                                </When>
                                <When when={section.get() == "overlays"}>
                                    <Label id="gal-section-title" w="100%" min_w="0" text_size="1.25rem" font_weight="700" text_overflow="ellipsis">"浮层 / 反馈"</Label>
                                </When>
                                <When when={section.get() == "draw"}>
                                    <Label id="gal-section-title" w="100%" min_w="0" text_size="1.25rem" font_weight="700" text_overflow="ellipsis">"绘制"</Label>
                                </When>
                                <When when={section.get() == "data"}>
                                    <Label id="gal-section-title" w="100%" min_w="0" text_size="1.25rem" font_weight="700" text_overflow="ellipsis">"数据"</Label>
                                </When>
                                <When when={section.get() == "navigation"}>
                                    <Label id="gal-section-title" w="100%" min_w="0" text_size="1.25rem" font_weight="700" text_overflow="ellipsis">"导航 / 窗口"</Label>
                                </When>
                                <When when={section.get() == "layout"}>
                                    <Label id="gal-section-title" w="100%" min_w="0" text_size="1.25rem" font_weight="700" text_overflow="ellipsis">"布局"</Label>
                                </When>
                                <When when={section.get() == "text"}>
                                    <Label id="gal-section-title" w="100%" min_w="0" text_size="1.25rem" font_weight="700" text_overflow="ellipsis">"文本"</Label>
                                </When>
                                <When when={section.get() == "theme"}>
                                    <Label id="gal-section-title" w="100%" min_w="0" text_size="1.25rem" font_weight="700" text_overflow="ellipsis">"主题"</Label>
                                </When>
                                <When when={section.get() == "about"}>
                                    <Label id="gal-section-title" w="100%" min_w="0" text_size="1.25rem" font_weight="700" text_overflow="ellipsis">"关于"</Label>
                                </When>
                                <When when={true}>
                                    <Label id="gal-section-title" w="100%" min_w="0" text_size="1.25rem" font_weight="700" text_overflow="ellipsis">"基础"</Label>
                                </When>
                            </Match>
                            <Match scope="examples/gallery/App:section_desc">
                                <When when={section.get() == "events"}>
                                    <Label id="gal-section-desc" w="100%" min_w="0" text_size="0.875rem" color="muted-foreground" text_overflow="ellipsis">
                                        "crates/jadix/src/app/pointer_events.rs：MouseEvent 字段、捕获/冒泡、click LCA"
                                    </Label>
                                </When>
                                <When when={section.get() == "draw"}>
                                    <Label id="gal-section-desc" w="100%" min_w="0" text_size="0.875rem" color="muted-foreground" text_overflow="ellipsis">
                                        "WP-3.11 <svg> · WP-3.12 <canvas> · WP-3.13 <scene3d> feature · shader() 叶 · lucide-animated。"
                                    </Label>
                                </When>
                                <When when={section.get() == "theme"}>
                                    <Label id="gal-section-desc" w="100%" min_w="0" text_size="0.875rem" color="muted-foreground" text_overflow="ellipsis">
                                        "theme.toml 覆盖 destructive · styles.toml 按 data-slot 统一 Bubble · 不实现 CSSOM"
                                    </Label>
                                </When>
                                <When when={section.get() == "basics"}>
                                    <Label id="gal-section-desc" w="100%" min_w="0" text_size="0.875rem" color="muted-foreground" text_overflow="ellipsis">
                                        "Button 变体 / 尺寸 / hover 缓动 · Badge · Kbd · Marker · Skeleton · Empty · Alert · Attachment · Bubble · Avatar · Breadcrumb · ButtonGroup · AspectRatio · CSS animation · 编程式句柄 · 生成器内核 · 排版装饰"
                                    </Label>
                                </When>
                            </Match>
                        </VStack>
                    </HStack>
                    <VStack min_w="0" min_h="0" grow="1">
                        <Match scope="examples/gallery/App:section_body">
                            <When when={section.get() == "forms"}>
                                <GallerySectionForms />
                            </When>
                            <When when={section.get() == "events"}>
                                <GallerySectionEvents />
                            </When>
                            <When when={section.get() == "overlays"}>
                                <GallerySectionOverlays />
                            </When>
                            <When when={section.get() == "draw"}>
                                <GallerySectionDraw />
                            </When>
                            <When when={section.get() == "data"}>
                                <GallerySectionData />
                            </When>
                            <When when={section.get() == "navigation"}>
                                <GallerySectionNavigation />
                            </When>
                            <When when={section.get() == "layout"}>
                                <GallerySectionLayout />
                            </When>
                            <When when={section.get() == "text"}>
                                <GallerySectionText />
                            </When>
                            <When when={section.get() == "theme"}>
                                <GallerySectionTheme />
                            </When>
                            <When when={section.get() == "about"}>
                                <GallerySectionAbout />
                            </When>
                            <When when={true}>
                                <GallerySectionBasics />
                            </When>
                        </Match>
                    </VStack>
                </SidebarInset>
            </SidebarProvider>
            <Toaster variant="stack" overflow="displace" limit={3} position="top-center" />
            { crate::shared::gallery_performance_overlay() }
        </Box>
    }
}

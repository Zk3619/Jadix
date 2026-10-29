//! Gallery 壳：侧栏、分区切换、根 `app_builder`。

use crate::imports::*;
use crate::pages::*;
use crate::shared::{MyCard, MyCardProps};

pub(crate) struct GallerySection {
    pub(crate) key: &'static str,
    pub(crate) group: &'static str,
    pub(crate) title: &'static str,
    icon: &'static str,
    description: &'static str,
}

pub(crate) const GALLERY_GROUPS: &[&str] = &[
    "组件库",
    "交互与导航",
    "布局与样式",
    "图形与媒体",
    "文档与开发",
    "桌面与运行时",
    "Gallery",
];

pub(crate) const GALLERY_SECTIONS: &[GallerySection] = &[
    GallerySection {
        key: "basics",
        group: "组件库",
        title: "基础控件",
        icon: "layout-dashboard",
        description: "按钮、徽标、头像、列表与容器",
    },
    GallerySection {
        key: "forms",
        group: "组件库",
        title: "表单与输入",
        icon: "file-text",
        description: "输入、校验、选择、开关与组合控件",
    },
    GallerySection {
        key: "feedback",
        group: "组件库",
        title: "反馈与消息",
        icon: "message-square",
        description: "提示、消息、气泡、骨架屏与空状态",
    },
    GallerySection {
        key: "data",
        group: "组件库",
        title: "数据与图表",
        icon: "table",
        description: "表格、分页、时间线、图表与虚拟瀑布流",
    },
    GallerySection {
        key: "navigation",
        group: "交互与导航",
        title: "导航组件",
        icon: "panels-top-left",
        description: "侧栏、菜单、步骤、路由与页面栈",
    },
    GallerySection {
        key: "overlays",
        group: "交互与导航",
        title: "弹层与弹窗",
        icon: "layers",
        description: "模态、局部与锚定浮层、生命周期与无样式基元",
    },
    GallerySection {
        key: "events",
        group: "交互与导航",
        title: "指针与光标",
        icon: "mouse-pointer-click",
        description: "捕获、冒泡、鼠标事件与自定义光标",
    },
    GallerySection {
        key: "drag",
        group: "交互与导航",
        title: "拖放与排序",
        icon: "move",
        description: "拖动、放置、排序与网格交互",
    },
    GallerySection {
        key: "layout",
        group: "布局与样式",
        title: "布局与容器",
        icon: "panels-left-right",
        description: "盒模型、逻辑属性、滚动、折叠与可调整面板",
    },
    GallerySection {
        key: "responsive",
        group: "布局与样式",
        title: "响应式布局",
        icon: "monitor",
        description: "断点、自适应容器与布局投影",
    },
    GallerySection {
        key: "style",
        group: "布局与样式",
        title: "外观与样式",
        icon: "paintbrush",
        description: "工具类、阴影、滤镜、渐变、边框与轮廓",
    },
    GallerySection {
        key: "transforms",
        group: "布局与样式",
        title: "几何变换",
        icon: "rotate-cw",
        description: "缩放、平移、旋转与组合变换",
    },
    GallerySection {
        key: "text",
        group: "布局与样式",
        title: "文本与排版",
        icon: "type",
        description: "字体、文本流、对齐、书写方向、Emoji 与装饰",
    },
    GallerySection {
        key: "theme",
        group: "布局与样式",
        title: "主题与设计系统",
        icon: "palette",
        description: "主题预设、语义颜色、材质与运行时样式",
    },
    GallerySection {
        key: "draw",
        group: "图形与媒体",
        title: "矢量与画布",
        icon: "pencil",
        description: "SVG、Canvas 2D、Lucide 动态图标与形态插值",
    },
    GallerySection {
        key: "scene",
        group: "图形与媒体",
        title: "3D 与着色器",
        icon: "box",
        description: "三维场景、Billboard 与自定义 GPU 着色器",
    },
    GallerySection {
        key: "media",
        group: "图形与媒体",
        title: "图像与音视频",
        icon: "play",
        description: "远程图像、图像流、音视频播放与媒体控制",
    },
    GallerySection {
        key: "motion",
        group: "图形与媒体",
        title: "动画与过渡",
        icon: "activity",
        description: "布局动画、保留动画、时间线、生成器与状态过渡",
    },
    GallerySection {
        key: "documents",
        group: "文档与开发",
        title: "文档与科学内容",
        icon: "file-text",
        description: "Markdown 编辑、阅读、公式、化学与 Mermaid 图表",
    },
    GallerySection {
        key: "code",
        group: "文档与开发",
        title: "代码与差异",
        icon: "code",
        description: "Rust 编辑、语法高亮、并排与统一差异",
    },
    GallerySection {
        key: "desktop",
        group: "桌面与运行时",
        title: "窗口与系统集成",
        icon: "app-window",
        description: "窗口会话、全屏、多窗口、进程实例、托盘与窗口装饰",
    },
    GallerySection {
        key: "workspace",
        group: "桌面与运行时",
        title: "工作区与浏览器",
        icon: "panels-top-left",
        description: "Dock 工作区、独立页面与嵌入式 WebView",
    },
    GallerySection {
        key: "reactivity",
        group: "桌面与运行时",
        title: "响应式与状态",
        icon: "workflow",
        description: "Signal、Store、Resource、作用域与稳定标识",
    },
    GallerySection {
        key: "rendering",
        group: "桌面与运行时",
        title: "布局与重绘",
        icon: "scan",
        description: "布局子树、局部更新与绘制失效",
    },
    GallerySection {
        key: "about",
        group: "Gallery",
        title: "能力与构建",
        icon: "info",
        description: "运行时 crate、feature 与双写法入口",
    },
];

fn gallery_section(key: &str) -> &'static GallerySection {
    GALLERY_SECTIONS
        .iter()
        .find(|entry| entry.key == key)
        .unwrap_or(&GALLERY_SECTIONS[0])
}

pub(crate) fn gallery_nav_item(
    section: Signal<String>,
    prefix: &str,
    key: &'static str,
    title: &'static str,
    icon: &'static str,
    close: Option<Signal<bool>>,
) -> Element {
    ui! {
        <SidebarMenuItem>
            <SidebarMenuButton
                id={format!("{prefix}-nav-{key}")}
                isActive={section.get() == key}
                on_click={move || {
                    section.set(key.to_string());
                    if let Some(open) = close {
                        open.set(false);
                    }
                }}
            >
                <HStack w="100%" items="center" gap="0.5rem" grow="1" min_w="0">
                    <Icon name={icon} size="16px" color="sidebar-foreground" pointer_events="none" />
                    <Label grow="1" min_w="0" text_size="0.875rem" color="sidebar-foreground">{title}</Label>
                </HStack>
            </SidebarMenuButton>
        </SidebarMenuItem>
    }
}

pub(crate) fn gallery_nav_menu(
    section: Signal<String>,
    prefix: &str,
    close: Option<Signal<bool>>,
) -> Element {
    let groups: Vec<Element> = GALLERY_GROUPS
        .iter()
        .enumerate()
        .map(|(position, group)| {
            let group = *group;
            let id = format!("{prefix}-branch-{position}");
            let expanded = use_signal(&format!("{id}-open"), || true);
            let trigger_id = format!("{id}:trigger");
            let chevron_id = format!("{id}-chevron");
            let panel_id = format!("{id}:panel");
            let prefix = prefix.to_owned();
            create_effect(move || {
                let selected = section.get();
                if GALLERY_SECTIONS
                    .iter()
                    .any(|entry| entry.key == selected && entry.group == group)
                    && !expanded.peek()
                {
                    expanded.set(true);
                }
            });
            collapsible(id)
                .open(expanded)
                .trigger(move || {
                    let mut chevron = ui! {
                        <Icon name="chevron-right" size="14px" pointer_events="none" />
                    }
                    .transition("transform 200ms ease-out");
                    chevron.template_id = Some(chevron_id.clone());
                    chevron.style.rotate_deg = if expanded.peek() { 90.0 } else { 0.0 };
                    chevron.paint_region_replay = true;
                    let chevron = chevron.reactive_sync(move |element, _| {
                        let target = if expanded.peek() { 90.0 } else { 0.0 };
                        let changed = (element.style.rotate_deg - target).abs() > 0.01;
                        element.style.rotate_deg = target;
                        if changed
                            || jadix::style::css_transition_needs_frame(
                                &element.style,
                                &element.transition_snapshot,
                            )
                            || element.transition_snapshot.has_active_tweens()
                        {
                            LayoutStatus::StyleChanged
                        } else {
                            LayoutStatus::Clean
                        }
                    });
                    let trigger = ui! {
                    <SidebarMenuButton id={trigger_id}>
                        <HStack w="100%" min_w="0" items="center" gap="0.5rem">
                            {chevron}
                            <Label grow="1" text_size="0.875rem" font_weight="600">{group}</Label>
                        </HStack>
                    </SidebarMenuButton>
                    };
                    trigger
                        .on_key_down(move |event| {
                            if event.state != winit::event::ElementState::Pressed {
                                return false;
                            }
                            match event.logical_key {
                                winit::keyboard::Key::Named(
                                    winit::keyboard::NamedKey::ArrowLeft,
                                ) => {
                                    expanded.set(false);
                                    true
                                }
                                winit::keyboard::Key::Named(
                                    winit::keyboard::NamedKey::ArrowRight,
                                ) => {
                                    expanded.set(true);
                                    true
                                }
                                _ => false,
                            }
                        })
                        .reactive_sync(move |element, _| {
                            let value = expanded.peek().to_string();
                            if element.get_attr("aria-expanded") == Some(&value) {
                                return LayoutStatus::Clean;
                            }
                            element.set_attr("aria-expanded", value);
                            element.set_attr("aria-controls", panel_id.clone());
                            LayoutStatus::StyleChanged
                        })
                })
                .content({
                    let prefix = prefix.to_owned();
                    move || {
                        let items: Vec<Element> = GALLERY_SECTIONS
                            .iter()
                            .filter(|entry| entry.group == group)
                            .map(|entry| {
                                gallery_nav_item(
                                    section,
                                    &prefix,
                                    entry.key,
                                    entry.title,
                                    entry.icon,
                                    close,
                                )
                            })
                            .collect();
                        ui! {
                            <SidebarGroupContent>
                                <VStack w="100%" min_w="0" pl="0.75rem">
                                    <SidebarMenu>{items}</SidebarMenu>
                                </VStack>
                            </SidebarGroupContent>
                        }
                    }
                })
                .into()
        })
        .collect();
    ui! { <SidebarGroup py="0" gap="0.25rem">{groups}</SidebarGroup> }
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
                    <ItemShowcase />
                    <AttachmentShowcase />
                    <ListShowcase />
                    <AvatarShowcase />
                    <ButtonGroupShowcase />
                    <AspectRatioShowcase />
                </VStack>
            </ScrollArea>
        }
    })
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
pub(crate) fn GallerySectionFeedback() -> Element {
    jadix::core::reactivity::isolate_reads(|| {
        ui! {
            <ScrollArea id="scroll-feedback" flex_grow="1" min_h="0" overflow_y="auto" overflow_x="auto" scroll_behavior="smooth">
                <VStack w="100%" min_w="0" gap="1rem" class="p-3 sm:p-4 lg:p-6">
                    <MessageShowcase />
                    <MessageScrollerShowcase />
                    <BubbleShowcase />
                    <SkeletonShowcase />
                    <EmptyShowcase />
                    <AlertShowcase />
                    <OverlayFeedbackShowcase />
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
                    <BreadcrumbShowcase />
                    <SidebarShowcase />
                    <NavbarNavigationMenuShowcase />
                    <MenubarShowcase />
                    <StepperShowcase />
                    <NavigatorShowcase />
                    <RouterShowcase />
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
                    <AlertDialogShowcase />
                    <ViewportModalShowcase />
                    <LocalOverlayShowcase />
                    <AnchoredOverlayShowcase />
                    <PresenceShowcase />
                    <PrimitivesShowcase />
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
pub(crate) fn GallerySectionDrag() -> Element {
    jadix::core::reactivity::isolate_reads(|| {
        ui! {
            <ScrollArea id="scroll-drag" flex_grow="1" min_h="0" overflow_y="auto" overflow_x="auto" scroll_behavior="smooth">
                <VStack w="100%" min_w="0" gap="1rem" class="p-3 sm:p-4 lg:p-6">
                    <DragShowcase />
                    <GridAndDragShowcase />
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
                    <BoxModelShowcase />
                    <LogicalShowcase />
                    <ScrollShowcase />
                    <AccordionShowcase />
                    <CollapsibleShowcase />
                    <ResizableShowcase />
                </VStack>
            </ScrollArea>
        }
    })
}

#[component]
pub(crate) fn GallerySectionResponsive() -> Element {
    jadix::core::reactivity::isolate_reads(|| {
        ui! {
            <ScrollArea id="scroll-responsive" flex_grow="1" min_h="0" overflow_y="auto" overflow_x="auto" scroll_behavior="smooth">
                <VStack w="100%" min_w="0" gap="1rem" class="p-3 sm:p-4 lg:p-6">
                    <ResponsiveShowcase />
                </VStack>
            </ScrollArea>
        }
    })
}

#[component]
pub(crate) fn GallerySectionStyle() -> Element {
    jadix::core::reactivity::isolate_reads(|| {
        ui! {
            <ScrollArea id="scroll-style" flex_grow="1" min_h="0" overflow_y="auto" overflow_x="auto" scroll_behavior="smooth">
                <VStack w="100%" min_w="0" gap="1rem" class="p-3 sm:p-4 lg:p-6">
                    <UtilityClassShowcase />
                    <ShadowShowcase />
                    <FilterShowcase />
                    <GradientShowcase />
                    <BorderStyleShowcase />
                    <OutlineShowcase />
                </VStack>
            </ScrollArea>
        }
    })
}

#[component]
pub(crate) fn GallerySectionTransforms() -> Element {
    jadix::core::reactivity::isolate_reads(|| {
        ui! {
            <ScrollArea id="scroll-transforms" flex_grow="1" min_h="0" overflow_y="auto" overflow_x="auto" scroll_behavior="smooth">
                <VStack w="100%" min_w="0" gap="1rem" class="p-3 sm:p-4 lg:p-6">
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
pub(crate) fn GallerySectionDraw() -> Element {
    jadix::core::reactivity::isolate_reads(|| {
        ui! {
            <ScrollArea id="scroll-draw" flex_grow="1" min_h="0" overflow_y="auto" overflow_x="auto" scroll_behavior="smooth">
                <VStack w="100%" min_w="0" gap="1rem" class="p-3 sm:p-4 lg:p-6">
                    <DrawingSvgShowcase />
                    <LucideAnimatedShowcase />
                    <MorphIconShowcase />
                    <DrawingCanvasShowcase />
                </VStack>
            </ScrollArea>
        }
    })
}

#[component]
pub(crate) fn GallerySectionScene() -> Element {
    jadix::core::reactivity::isolate_reads(|| {
        ui! {
            <ScrollArea id="scroll-scene" flex_grow="1" min_h="0" overflow_y="auto" overflow_x="auto" scroll_behavior="smooth">
                <VStack w="100%" min_w="0" gap="1rem" class="p-3 sm:p-4 lg:p-6">
                    <DrawingScene3dShowcase />
                    <DrawingBillboardShowcase />
                    <DrawingShaderShowcase />
                </VStack>
            </ScrollArea>
        }
    })
}

#[component]
pub(crate) fn GallerySectionMedia() -> Element {
    jadix::core::reactivity::isolate_reads(|| {
        ui! {
            <ScrollArea id="scroll-media" flex_grow="1" min_h="0" overflow_y="auto" overflow_x="auto" scroll_behavior="smooth">
                <VStack w="100%" min_w="0" gap="1rem" class="p-3 sm:p-4 lg:p-6">
                    <RemoteAssetShowcase />
                    <ImageStreamShowcase />
                    <MediaPlayerShowcase />
                </VStack>
            </ScrollArea>
        }
    })
}

#[component]
pub(crate) fn GallerySectionMotion() -> Element {
    jadix::core::reactivity::isolate_reads(|| {
        ui! {
            <ScrollArea id="scroll-motion" flex_grow="1" min_h="0" overflow_y="auto" overflow_x="auto" scroll_behavior="smooth">
                <VStack w="100%" min_w="0" gap="1rem" class="p-3 sm:p-4 lg:p-6">
                    <LayoutMotionShowcase />
                    <AnimationShowcase />
                    <WaapiShowcase />
                    <GeneratorShowcase />
                    <RetainedMotionShowcase />
                    <TransitionShowcase />
                    <AnimAlignmentShowcase />
                </VStack>
            </ScrollArea>
        }
    })
}

#[component]
pub(crate) fn GallerySectionDesktop() -> Element {
    jadix::core::reactivity::isolate_reads(|| {
        ui! {
            <ScrollArea id="scroll-desktop" flex_grow="1" min_h="0" overflow_y="auto" overflow_x="auto" scroll_behavior="smooth">
                <VStack w="100%" min_w="0" gap="1rem" class="p-3 sm:p-4 lg:p-6">
                    <WindowSessionShowcase />
                    <FullscreenShowcase />
                    <InstanceShowcase />
                    <MultiWindowShowcase />
                    <TrayShowcase />
                    <WindowChromeShowcase />
                </VStack>
            </ScrollArea>
        }
    })
}

#[component]
pub(crate) fn GallerySectionWorkspace() -> Element {
    jadix::core::reactivity::isolate_reads(|| {
        ui! {
            <ScrollArea id="scroll-workspace" flex_grow="1" min_h="0" overflow_y="auto" overflow_x="auto" scroll_behavior="smooth">
                <VStack w="100%" min_w="0" gap="1rem" class="p-3 sm:p-4 lg:p-6">
                    <DockBrowserShowcase />
                </VStack>
            </ScrollArea>
        }
    })
}

#[component]
pub(crate) fn GallerySectionReactivity() -> Element {
    jadix::core::reactivity::isolate_reads(|| {
        ui! {
            <ScrollArea id="scroll-reactivity" flex_grow="1" min_h="0" overflow_y="auto" overflow_x="auto" scroll_behavior="smooth">
                <VStack w="100%" min_w="0" gap="1rem" class="p-3 sm:p-4 lg:p-6">
                    <OwnerRegistryShowcase />
                    <StoreResourceShowcase />
                    <RootBareGetShowcase />
                    <IdentityApiShowcase />
                </VStack>
            </ScrollArea>
        }
    })
}

#[component]
pub(crate) fn GallerySectionRendering() -> Element {
    jadix::core::reactivity::isolate_reads(|| {
        ui! {
            <ScrollArea id="scroll-rendering" flex_grow="1" min_h="0" overflow_y="auto" overflow_x="auto" scroll_behavior="smooth">
                <VStack w="100%" min_w="0" gap="1rem" class="p-3 sm:p-4 lg:p-6">
                    <LayoutSubtreeShowcase />
                    <DamageShowcase />
                    <FgMatrixShowcase />
                </VStack>
            </ScrollArea>
        }
    })
}

#[component]
pub(crate) fn GallerySectionDocuments() -> Element {
    jadix::core::reactivity::isolate_reads(|| {
        ui! {
            <ScrollArea id="scroll-documents" flex_grow="1" min_h="0" overflow_y="auto" overflow_x="auto" scroll_behavior="smooth">
                <VStack w="100%" min_w="0" gap="1rem" class="p-3 sm:p-4 lg:p-6">
                    <DocumentEditingShowcase />
                    <DocumentScienceShowcase />
                </VStack>
            </ScrollArea>
        }
    })
}

#[component]
pub(crate) fn GallerySectionCode() -> Element {
    jadix::core::reactivity::isolate_reads(|| {
        ui! {
            <ScrollArea id="scroll-code" flex_grow="1" min_h="0" overflow_y="auto" overflow_x="auto" scroll_behavior="smooth">
                <VStack w="100%" min_w="0" gap="1rem" class="p-3 sm:p-4 lg:p-6">
                    <CodeEditingShowcase />
                    <DocumentDiffShowcase />
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
                <VStack w="100%" min_w="0" gap="1rem" class="p-3 sm:p-4 lg:p-6" transition="padding 200ms ease-out" max_w="48rem">
                    <MyCard title="Jadix capability gallery">
                        <VStack gap="0.5rem">
                            <Label color="muted-foreground">"A runnable reference for the framework and component library, built from the same public crates used by applications."</Label>
                            <Label>"Static ui! and imperative Builder composition are intentionally mixed throughout the pages."</Label>
                        </VStack>
                    </MyCard>
                    <MyCard title="Runtime crates in this consumer">
                        <VStack gap="0.5rem">
                            <Label>"jadix · jadix_core · jadix_layout · jadix_renderer"</Label>
                            <Label>"jadix_components · jadix_dnd · jadix_lucide"</Label>
                            <Label>"jadix_animation · jadix_theme · jadix_style_tokens"</Label>
                            <Label>"jadix_document · jadix_lsp · jadix_hot_reload"</Label>
                            <Label>"jadix_media · jadix_net · jadix_tasks · jadix_tokio"</Label>
                            <Label>"jadix_window_state · jadix_window_materials · jadix_clipboard"</Label>
                        </VStack>
                    </MyCard>
                    <MyCard title="完整配置能力（默认 full）">
                        <VStack gap="0.5rem">
                            <Label>"themes · animation · SVG · images · document editing · diff · science"</Label>
                            <Label>"code · syntax · scale preview · drag and drop · text undo · file dialogs"</Label>
                            <Label>"webview · hot reload · devtools · remote assets · compute · scene3d"</Label>
                            <Label>"i18n · accessibility · system tray · window materials · native clipboard"</Label>
                            <Label color="muted-foreground">{if cfg!(feature = "full") { "当前构建：full；平台能力仍取决于宿主系统。" } else { "当前构建：自定义功能子集；可选页面会提示缺少的 feature。" }}</Label>
                        </VStack>
                    </MyCard>
                    <Label text_size="0.875rem" color="muted-foreground">
                        "侧栏分区对应 Gallery 能力目录。"
                    </Label>
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
    })
    .into()
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
                        <VStack gap="0.125rem" min_w="0" grow="1" overflow="hidden">
                            <Label id="gal-section-title" w="100%" min_w="0" h="1.5rem" leading="1.2" text_size="1.25rem" font_weight="700" text_overflow="ellipsis">{gallery_section(&section.get()).title}</Label>
                            <Label id="gal-section-desc" w="100%" min_w="0" h="1.25rem" leading="1.4285714" text_size="0.875rem" color="muted-foreground" text_overflow="ellipsis">{gallery_section(&section.get()).description}</Label>
                        </VStack>
                    </HStack>
                    <VStack min_w="0" min_h="0" grow="1">
                        <Match scope="examples/gallery/App:section_body">
                            <When when={section.get() == "forms"}><GallerySectionForms /></When>
                            <When when={section.get() == "feedback"}><GallerySectionFeedback /></When>
                            <When when={section.get() == "data"}><GallerySectionData /></When>
                            <When when={section.get() == "navigation"}><GallerySectionNavigation /></When>
                            <When when={section.get() == "overlays"}><GallerySectionOverlays /></When>
                            <When when={section.get() == "events"}><GallerySectionEvents /></When>
                            <When when={section.get() == "drag"}><GallerySectionDrag /></When>
                            <When when={section.get() == "layout"}><GallerySectionLayout /></When>
                            <When when={section.get() == "responsive"}><GallerySectionResponsive /></When>
                            <When when={section.get() == "style"}><GallerySectionStyle /></When>
                            <When when={section.get() == "transforms"}><GallerySectionTransforms /></When>
                            <When when={section.get() == "text"}><GallerySectionText /></When>
                            <When when={section.get() == "theme"}><GallerySectionTheme /></When>
                            <When when={section.get() == "draw"}><GallerySectionDraw /></When>
                            <When when={section.get() == "scene"}><GallerySectionScene /></When>
                            <When when={section.get() == "media"}><GallerySectionMedia /></When>
                            <When when={section.get() == "motion"}><GallerySectionMotion /></When>
                            <When when={section.get() == "desktop"}><GallerySectionDesktop /></When>
                            <When when={section.get() == "documents"}><GallerySectionDocuments /></When>
                            <When when={section.get() == "code"}><GallerySectionCode /></When>
                            <When when={section.get() == "workspace"}><GallerySectionWorkspace /></When>
                            <When when={section.get() == "reactivity"}><GallerySectionReactivity /></When>
                            <When when={section.get() == "rendering"}><GallerySectionRendering /></When>
                            <When when={section.get() == "about"}><GallerySectionAbout /></When>
                            <When when={true}><GallerySectionBasics /></When>
                        </Match>
                    </VStack>
                </SidebarInset>
            </SidebarProvider>
            <Toaster variant="stack" overflow="displace" limit={3} position="top-center" />
            { crate::shared::gallery_performance_overlay() }
        </Box>
    }
}

//! 浮层 / 反馈与 Portal 原语。

use crate::imports::*;
use crate::pages::navigation::gallery_secondary_shell_root;
use crate::shared::*;

#[component]
pub(crate) fn PrimitivesShowcase() -> jadix::core::tree::Element {
    let overlay_open = use_signal("prim_overlay_open", || false);
    let local_open = use_signal("prim_local_portal_open", || false);
    let trap_on = use_signal("prim_trap_on", || false);

    ui! {
        <VStack w="100%" gap="0">
            <MyCard title="11) Image · Portal · focus_trap · VirtualList">
                <Reactive id={PRIMITIVES_SHOWCASE_REACTIVE}>
                    <VStack gap="0.75rem" w="100%">
                        <Label text_size="0.75rem" color="muted-foreground" leading="normal">
                            "GPU Image：queue_image_rgba8 + texture，或 src（路径 / data:）。PNG/ICO/BMP 默认；JPEG/GIF/WebP 需 images；SVG 需 svg。Portal 默认同层；focus_trap 限制 Tab。VirtualList：visible_item_range。"
                        </Label>
                        <HStack gap="0.75rem" items="start" wrap="wrap">
                            <VStack gap="0.25rem">
                                <Label font_weight="700" text_size="0.75rem">"Texture quad"</Label>
                                <Image texture="demo-checker" w="5.5rem" h="5.5rem" rounded="0.375rem" />
                                <Image
                                    src="data:image/svg+xml,<svg xmlns='http://www.w3.org/2000/svg' width='32' height='32'><circle cx='16' cy='16' r='14' fill='steelblue'/></svg>"
                                    w="5.5rem"
                                    h="5.5rem"
                                    class="rounded-full"
                                />
                            </VStack>
                            <VStack gap="0.35rem" flex_grow="1" min_w="12rem">
                                <Label font_weight="700" text_size="0.75rem">"Controls"</Label>
                                <HStack gap="0.5rem" wrap="wrap">
                                    <Button variant="outline" on_click={move || overlay_open.set(!overlay_open.get())}>
                                        { if overlay_open.get() { "关闭根 Portal" } else { "打开根 Portal" } }
                                    </Button>
                                    <Button variant="outline" on_click={move || local_open.set(!local_open.get())}>
                                        { if local_open.get() { "关闭局部 Portal" } else { "打开局部 Portal" } }
                                    </Button>
                                    <Button variant="secondary" on_click={move || {
                                        let next = !trap_on.get();
                                        trap_on.set(next);
                                        jadix::sync_focus_trap("prim-trap", next, "trap-a,trap-b");
                                    }}>
                                        { if trap_on.get() { "关闭焦点陷阱" } else { "启用焦点陷阱" } }
                                    </Button>
                                </HStack>
                                <Box id="prim-portal-host" relative=true w="100%" min_h="6.5rem" p="0.75rem" bg="muted" rounded="0.5rem" overflow="hidden">
                                    <VStack gap="0.2rem">
                                        <Label text_size="0.75rem" color="muted-foreground">
                                            "局部宿主 prim-portal-host（mount 叠层裁剪在此框内）"
                                        </Label>
                                        <Label text_size="0.7rem" color="foreground">
                                            "ABCDEFGHIJKLMNOPQRSTUVWXYZ 0123456789"
                                        </Label>
                                        <Label text_size="0.7rem" color="foreground">
                                            "打开后遮罩应模糊这些文字，而不是整块纯色。"
                                        </Label>
                                    </VStack>
                                </Box>
                                <HStack gap="0.5rem" items="center" wrap="wrap">
                                    <Label text_size="0.75rem" color="muted-foreground">"陷阱内 Tab:"</Label>
                                    <Button id="trap-a" variant="default">"A"</Button>
                                    <Button id="trap-b" variant="outline">"B"</Button>
                                </HStack>
                            </VStack>
                        </HStack>
                        <Label id="gal-s11-virt-title" font_weight="700" text_size="0.875rem">"虚拟列表窗口（200 行，仅挂载可见区间）"</Label>
                        <Box bg="muted" rounded="0.375rem" p="0.25rem" w="100%">
                            { Element::from(
                                virtual_list(PRIMITIVES_VIRT)
                                    .item_count(200)
                                    .item_height(22.0)
                                    .height(176.0)
                                    .overscan(8)
                                    .scrollbar_visibility("always")
                                    .render_item(|i| {
                                        label(format!("virt-row-{i}"))
                                            .text(format!("virtual row {i:04}"))
                                            .into()
                                    }),
                            ) }
                        </Box>
                    </VStack>
                </Reactive>
            </MyCard>
            <Portal id="prim-portal-root">
                <Reactive id="examples/gallery/PrimitivesShowcase:portal">
                    <Show when={overlay_open.get()}>
                        <Box absolute=true inset="0" w="100%" h="100%" items="center" justify="center" bg="black" z_index="900">
                            <VStack gap="0.75rem" p="1.25rem" bg="secondary" rounded="0.75rem" max_w="22rem" m="1rem">
                                <Label font_weight="700" text_size="1.125rem">"根 Portal 浮层"</Label>
                                <Label text_size="0.875rem" color="muted-foreground">
                                    "未指定 mount：提升到根叠层（对齐 Solid document.body）。"
                                </Label>
                                <Button variant="default" on_click={move || overlay_open.set(false)}>
                                    "关闭"
                                </Button>
                            </VStack>
                        </Box>
                    </Show>
                </Reactive>
            </Portal>
            <Portal id="prim-portal-local" mount="prim-portal-host">
                <Reactive id="examples/gallery/PrimitivesShowcase:portal_local">
                    <Show when={local_open.get()}>
                        <Box absolute=true inset="0" w="100%" h="100%" items="center" justify="center">
                            <Box w="85%" h="85%" rounded="0.5rem"
                                 bg="rgba(15,23,42,0.55)" backdrop_blur="0.75"
                                 items="center" justify="center">
                                <VStack gap="0.5rem" p="0.75rem" bg="secondary" rounded="0.5rem" m="0.5rem">
                                    <Label font_weight="700" text_size="0.875rem">"局部 Portal"</Label>
                                    <Label text_size="0.75rem" color="muted-foreground">
                                        "mount=\"prim-portal-host\"：局部 backdrop_blur 应模糊宿主背景。"
                                    </Label>
                                    <Button size="sm" variant="default" on_click={move || local_open.set(false)}>
                                        "关闭"
                                    </Button>
                                </VStack>
                            </Box>
                        </Box>
                    </Show>
                </Reactive>
            </Portal>
        </VStack>
    }
}

#[component]
pub(crate) fn OverlayFeedbackShowcase() -> Element {
    let cmd_open = use_signal("gal_cmd_open", || false);
    let cmd_dlg_open = use_signal("gal_cmd_dlg_open", || false);
    let dialog_open = use_signal("gal_dialog_open", || false);
    let dialog_select = use_signal("gal_dialog_select", || "opt-a".to_string());
    let sheet_open = use_signal("gal_sheet_open", || false);
    let sheet_name = use_signal("gal_sheet_name", || "Pedro Duarte".to_string());
    let sheet_user = use_signal("gal_sheet_user", || "@peduarte".to_string());
    let drawer_open = use_signal("gal_drawer_open", || false);
    let drawer_link = use_signal("gal_drawer_link", || "https://ui.shadcn.com".to_string());
    let progress_val = use_signal("gal_progress", || 0.45f32);
    let open_cmd = cmd_open;
    let open_cmd_dlg = cmd_dlg_open;
    let bump = progress_val;

    let cmd_el: Element = {
        #[inline(never)]
        fn gal_overlay_command(cmd_open: Signal<bool>) -> Element {
            ui! {
            <CommandOverlay id="gal-cmd" open={cmd_open}>
                <CommandInput placeholder="Type a command or search..." />
                <CommandList>
                    <CommandEmpty>"No results found."</CommandEmpty>
                    <CommandGroup heading="Suggestions">
                        <CommandItem
                            value="toast"
                            on_click={move || {
                                gallery_feedback_toast(
                                    "Hello from Command",
                                    "Hover the stack to expand.",
                                );
                            }}
                        >
                            "Show toast"
                        </CommandItem>
                        <CommandItem
                            value="log"
                            on_click={move || {
                                log::info!("gallery command: ping");
                            }}
                        >
                            "Log ping"
                        </CommandItem>
                    </CommandGroup>
                    <CommandSeparator />
                    <CommandGroup heading="Settings">
                        <CommandItem value="profile">
                            "Profile"
                            <CommandShortcut>"⌘P"</CommandShortcut>
                        </CommandItem>
                        <CommandItem value="billing">
                            "Billing"
                            <CommandShortcut>"⌘B"</CommandShortcut>
                        </CommandItem>
                    </CommandGroup>
                </CommandList>
            </CommandOverlay>
                }
        }
        gal_overlay_command(cmd_open)
    };
    let cmd_dlg_el: Element = {
        #[inline(never)]
        fn gal_overlay_command_dialog(cmd_dlg_open: Signal<bool>) -> Element {
            ui! {
            <CommandDialog id="gal-cmd-dlg" open={cmd_dlg_open}>
                <CommandInput placeholder="Type a command or search..." />
                <CommandList>
                    <CommandEmpty>"No results found."</CommandEmpty>
                    <CommandGroup heading="Suggestions">
                        <CommandItem
                            value="dlg-toast"
                            on_click={move || {
                                gallery_feedback_toast(
                                    "Hello from CommandDialog",
                                    "Hover the stack to expand.",
                                );
                            }}
                        >
                            "Show toast"
                        </CommandItem>
                        <CommandItem
                            value="calendar"
                            on_click={move || {
                                gallery_goto_section("forms");
                                gallery_feedback_toast(
                                    "Calendar",
                                    "已跳到「表单」：Date / Time / Color / Calendar。",
                                );
                            }}
                        >
                            "Calendar"
                        </CommandItem>
                        <CommandItem
                            value="emoji"
                            on_click={move || {
                                gallery_goto_section("text");
                                gallery_feedback_toast("Search Emoji", "已跳到「文本」分区。");
                            }}
                        >
                            "Search Emoji"
                        </CommandItem>
                        <CommandItem value="calc" disabled={true}>"Calculator"</CommandItem>
                    </CommandGroup>
                    <CommandSeparator />
                    <CommandGroup heading="Settings">
                        <CommandItem
                            value="profile"
                            on_click={move || {
                                gallery_feedback_toast("Profile", "CommandDialog 演示项。");
                            }}
                        >
                            "Profile"
                            <CommandShortcut>"⌘P"</CommandShortcut>
                        </CommandItem>
                        <CommandItem
                            value="billing"
                            on_click={move || {
                                gallery_feedback_toast("Billing", "CommandDialog 演示项。");
                            }}
                        >
                            "Billing"
                            <CommandShortcut>"⌘B"</CommandShortcut>
                        </CommandItem>
                        <CommandItem
                            value="settings"
                            on_click={move || {
                                gallery_feedback_toast("Settings", "CommandDialog 演示项。");
                            }}
                        >
                            "Settings"
                            <CommandShortcut>"⌘S"</CommandShortcut>
                        </CommandItem>
                    </CommandGroup>
                    <CommandSeparator />
                    <CommandGroup heading="File">
                        <CommandItem value="new-file">
                            "New File"
                            <CommandShortcut>"⌘N"</CommandShortcut>
                        </CommandItem>
                        <CommandItem value="open">
                            "Open..."
                            <CommandShortcut>"⌘O"</CommandShortcut>
                        </CommandItem>
                        <CommandItem
                            value="save"
                            on_click={move || {
                                toast("Saved")
                                    .description("Changes applied.")
                                    .variant("success")
                                    .duration(2500)
                                    .show();
                            }}
                        >
                            "Save"
                            <CommandShortcut>"⌘S"</CommandShortcut>
                        </CommandItem>
                    </CommandGroup>
                </CommandList>
            </CommandDialog>
                }
        }
        gal_overlay_command_dialog(cmd_dlg_open)
    };
    let cmd_inline: Element = {
        #[inline(never)]
        fn gal_overlay_command_inline() -> Element {
            ui! {
            <Command id="gal-cmd-inline">
                <CommandInput placeholder="Type a command or search..." />
                <CommandList>
                    <CommandEmpty>"No results found."</CommandEmpty>
                    <CommandGroup heading="Suggestions">
                        <CommandItem value="calendar">"Calendar"</CommandItem>
                        <CommandItem value="emoji">"Search Emoji"</CommandItem>
                        <CommandItem value="calc" disabled={true}>"Calculator"</CommandItem>
                    </CommandGroup>
                    <CommandSeparator />
                    <CommandGroup heading="Settings">
                        <CommandItem value="inline-profile">
                            "Profile"
                            <CommandShortcut>"⌘P"</CommandShortcut>
                        </CommandItem>
                    </CommandGroup>
                </CommandList>
            </Command>
                }
        }
        gal_overlay_command_inline()
    };
    let dialog_el: Element = {
        #[inline(never)]
        fn gal_overlay_dialog(open: Signal<bool>, nested_select: Signal<String>) -> Element {
            ui! {
            <Dialog id="gal-dialog" open={open}>
                <DialogTrigger>
                    <Button variant="outline" size="sm">Dialog</Button>
                </DialogTrigger>
                <DialogContent>
                    <DialogHeader>
                        <DialogTitle>Gallery Dialog</DialogTitle>
                        <DialogDescription>
                            "desktop_smoke #7：Body 内含 Select；先开下拉再 Esc，应只关顶层浮层。"
                        </DialogDescription>
                    </DialogHeader>
                    <DialogBody>
                        <VStack gap="0.5rem" w="100%">
                            <Label text_size="0.875rem" font_weight="600">
                                "嵌套浮层（Select）"
                            </Label>
                            <Select
                                id="gal-dialog-nested-select"
                                value={nested_select}
                                placeholder="Pick an option"
                            >
                                <SelectTrigger class="w-full">
                                    <SelectValue />
                                </SelectTrigger>
                                <SelectContent max_h="12rem">
                                    <SelectItem value="opt-a">"Option A"</SelectItem>
                                    <SelectItem value="opt-b">"Option B"</SelectItem>
                                    <SelectSeparator />
                                    <SelectGroup>
                                        <SelectLabel>"More"</SelectLabel>
                                        <SelectItem value="opt-c">"Option C"</SelectItem>
                                        <SelectItem value="opt-d">"Option D — long label"</SelectItem>
                                        <SelectItem value="opt-e">"Option E"</SelectItem>
                                    </SelectGroup>
                                </SelectContent>
                            </Select>
                            <Label text_size="0.75rem" color="muted-foreground">
                                { format!("Selected: {}", nested_select.get()) }
                            </Label>
                            <Label text_size="0.875rem">"行 1 — 演示长内容"</Label>
                            <Label text_size="0.875rem">"行 2"</Label>
                            <Label text_size="0.875rem">"行 3"</Label>
                            <Label text_size="0.875rem">"行 4"</Label>
                            <Label text_size="0.875rem">"行 5"</Label>
                            <Label text_size="0.875rem">"行 6"</Label>
                        </VStack>
                    </DialogBody>
                    <DialogFooter>
                        <Button
                            variant="outline"
                            size="sm"
                            on_click={move || open.set(false)}
                        >
                            Cancel
                        </Button>
                        <Button
                            variant="default"
                            size="sm"
                            on_click={move || {
                                toast_push("Dialog confirmed", 1500);
                                open.set(false);
                            }}
                        >
                            Confirm
                        </Button>
                    </DialogFooter>
                </DialogContent>
            </Dialog>
            }
        }
        gal_overlay_dialog(dialog_open, dialog_select)
    };
    let sheet_el: Element = {
        #[inline(never)]
        fn gal_overlay_sheet(
            open: Signal<bool>,
            name: Signal<String>,
            user: Signal<String>,
        ) -> Element {
            ui! {
            <Sheet id="gal-sheet" open={open}>
                <SheetTrigger>
                    <Button variant="outline" size="sm">Sheet</Button>
                </SheetTrigger>
                <SheetContent side="right" w="24rem">
                    <SheetHeader>
                        <SheetTitle>Edit profile</SheetTitle>
                        <SheetDescription>
                            "Make changes to your profile here. Click save when you're done."
                        </SheetDescription>
                    </SheetHeader>
                    <SheetBody>
                        <VStack gap="0.75rem" w="100%">
                            <Label text_size="0.875rem" font_weight="600">"Name"</Label>
                            <Input id="gal-sheet-name" value={name} class="w-full" />
                            <Label text_size="0.875rem" font_weight="600">"Username"</Label>
                            <Input id="gal-sheet-user" value={user} class="w-full" />
                        </VStack>
                    </SheetBody>
                    <SheetFooter>
                        <Button
                            size="sm"
                            on_click={move || {
                                toast_push("Profile saved", 1500);
                                open.set(false);
                            }}
                        >
                            "Save changes"
                        </Button>
                        <SheetClose>
                            <Button variant="outline" size="sm">Close</Button>
                        </SheetClose>
                    </SheetFooter>
                </SheetContent>
            </Sheet>
            }
        }
        gal_overlay_sheet(sheet_open, sheet_name, sheet_user)
    };
    let drawer_el: Element = {
        #[inline(never)]
        fn gal_overlay_drawer(open: Signal<bool>, link: Signal<String>) -> Element {
            ui! {
            <Drawer id="gal-drawer" open={open}>
                <DrawerTrigger>
                    <Button variant="outline" size="sm">Drawer</Button>
                </DrawerTrigger>
                <DrawerContent
                    direction="down"
                    show_swipe_handle={true}
                    snap_points={[0.5_f32, 1.0]}
                    snap_to_sequential_point={true}
                >
                    <DrawerHeader>
                        <DrawerTitle>Share link</DrawerTitle>
                        <DrawerDescription>
                            "Anyone with the link can view this document."
                        </DrawerDescription>
                    </DrawerHeader>
                    <DrawerBody>
                        <VStack gap="0.5rem" w="100%">
                            <Label text_size="0.875rem" font_weight="600">"Link"</Label>
                            <Input id="gal-drawer-link" value={link} class="w-full" />
                            <Label text_size="0.875rem" leading="relaxed">
                                "Half-open snap shortens the panel — Body scrolls when content overflows. Put actions in Body so they scroll with the copy; DrawerFooter is sticky to the visible bottom."
                            </Label>
                            <Label text_size="0.875rem" leading="relaxed">
                                "Lorem ipsum dolor sit amet, consectetur adipiscing elit."
                            </Label>
                            <Label text_size="0.875rem" leading="relaxed">
                                "Sed do eiusmod tempor incididunt ut labore et dolore magna aliqua."
                            </Label>
                            <Label text_size="0.875rem" leading="relaxed">
                                "Ut enim ad minim veniam, quis nostrud exercitation ullamco."
                            </Label>
                            <Label text_size="0.875rem" leading="relaxed">
                                "Duis aute irure dolor in reprehenderit in voluptate velit esse."
                            </Label>
                            <Label text_size="0.875rem" leading="relaxed">
                                "Excepteur sint occaecat cupidatat non proident, sunt in culpa."
                            </Label>
                            <Label text_size="0.875rem" leading="relaxed">
                                "Mollit anim id est laborum. Drag the handle to snap or dismiss."
                            </Label>
                            <Button
                                size="sm"
                                on_click={move || {
                                    toast_push("Link copied", 1500);
                                    open.set(false);
                                }}
                            >
                                "Copy link"
                            </Button>
                            <DrawerClose>
                                <Button variant="outline" size="sm">Close</Button>
                            </DrawerClose>
                        </VStack>
                    </DrawerBody>
                </DrawerContent>
            </Drawer>
            }
        }
        gal_overlay_drawer(drawer_open, drawer_link)
    };
    let sheet_sides_el: Element = {
        #[inline(never)]
        fn gal_overlay_sheet_sides() -> Element {
            let open_top = use_signal("gal_sheet_top", || false);
            let open_right = use_signal("gal_sheet_right", || false);
            let open_bottom = use_signal("gal_sheet_bottom", || false);
            let open_left = use_signal("gal_sheet_left", || false);
            let o_top = open_top;
            let o_right = open_right;
            let o_bottom = open_bottom;
            let o_left = open_left;
            ui! {
                <HStack gap="0.5rem" wrap="wrap" items="center">
                    <Sheet id="gal-sheet-top" open={open_top}>
                        <SheetTrigger>
                            <Button variant="outline" size="sm">"top"</Button>
                        </SheetTrigger>
                        <SheetContent side="top">
                            <SheetHeader>
                                <SheetTitle>"Sheet · top"</SheetTitle>
                                <SheetDescription>
                                    "贴边方向演示；长文滚动 + Footer 固定。点遮罩或 Esc 关闭。"
                                </SheetDescription>
                            </SheetHeader>
                            <SheetBody>
                                <VStack gap="0.5rem" w="100%">
                                    <Label text_size="0.875rem" leading="relaxed">
                                        "Lorem ipsum dolor sit amet, consectetur adipiscing elit."
                                    </Label>
                                    <Label text_size="0.875rem" leading="relaxed">
                                        "Ut enim ad minim veniam, quis nostrud exercitation ullamco."
                                    </Label>
                                    <Label text_size="0.875rem" leading="relaxed">
                                        "Duis aute irure dolor in reprehenderit in voluptate velit."
                                    </Label>
                                </VStack>
                            </SheetBody>
                            <SheetFooter>
                                <Button size="sm" on_click={move || { toast_push("Sheet confirmed", 1500); o_top.set(false); }}>
                                    "Save changes"
                                </Button>
                                <SheetClose>
                                    <Button variant="outline" size="sm">Cancel</Button>
                                </SheetClose>
                            </SheetFooter>
                        </SheetContent>
                    </Sheet>
                    <Sheet id="gal-sheet-right" open={open_right}>
                        <SheetTrigger>
                            <Button variant="outline" size="sm">"right"</Button>
                        </SheetTrigger>
                        <SheetContent side="right">
                            <SheetHeader>
                                <SheetTitle>"Sheet · right"</SheetTitle>
                                <SheetDescription>
                                    "贴边方向演示；长文滚动 + Footer 固定。点遮罩或 Esc 关闭。"
                                </SheetDescription>
                            </SheetHeader>
                            <SheetBody>
                                <VStack gap="0.5rem" w="100%">
                                    <Label text_size="0.875rem" leading="relaxed">
                                        "Lorem ipsum dolor sit amet, consectetur adipiscing elit."
                                    </Label>
                                    <Label text_size="0.875rem" leading="relaxed">
                                        "Ut enim ad minim veniam, quis nostrud exercitation ullamco."
                                    </Label>
                                    <Label text_size="0.875rem" leading="relaxed">
                                        "Duis aute irure dolor in reprehenderit in voluptate velit."
                                    </Label>
                                </VStack>
                            </SheetBody>
                            <SheetFooter>
                                <Button size="sm" on_click={move || { toast_push("Sheet confirmed", 1500); o_right.set(false); }}>
                                    "Save changes"
                                </Button>
                                <SheetClose>
                                    <Button variant="outline" size="sm">Cancel</Button>
                                </SheetClose>
                            </SheetFooter>
                        </SheetContent>
                    </Sheet>
                    <Sheet id="gal-sheet-bottom" open={open_bottom}>
                        <SheetTrigger>
                            <Button variant="outline" size="sm">"bottom"</Button>
                        </SheetTrigger>
                        <SheetContent side="bottom">
                            <SheetHeader>
                                <SheetTitle>"Sheet · bottom"</SheetTitle>
                                <SheetDescription>
                                    "贴边方向演示；长文滚动 + Footer 固定。点遮罩或 Esc 关闭。"
                                </SheetDescription>
                            </SheetHeader>
                            <SheetBody>
                                <VStack gap="0.5rem" w="100%">
                                    <Label text_size="0.875rem" leading="relaxed">
                                        "Lorem ipsum dolor sit amet, consectetur adipiscing elit."
                                    </Label>
                                    <Label text_size="0.875rem" leading="relaxed">
                                        "Ut enim ad minim veniam, quis nostrud exercitation ullamco."
                                    </Label>
                                    <Label text_size="0.875rem" leading="relaxed">
                                        "Duis aute irure dolor in reprehenderit in voluptate velit."
                                    </Label>
                                </VStack>
                            </SheetBody>
                            <SheetFooter>
                                <Button size="sm" on_click={move || { toast_push("Sheet confirmed", 1500); o_bottom.set(false); }}>
                                    "Save changes"
                                </Button>
                                <SheetClose>
                                    <Button variant="outline" size="sm">Cancel</Button>
                                </SheetClose>
                            </SheetFooter>
                        </SheetContent>
                    </Sheet>
                    <Sheet id="gal-sheet-left" open={open_left}>
                        <SheetTrigger>
                            <Button variant="outline" size="sm">"left"</Button>
                        </SheetTrigger>
                        <SheetContent side="left">
                            <SheetHeader>
                                <SheetTitle>"Sheet · left"</SheetTitle>
                                <SheetDescription>
                                    "贴边方向演示；长文滚动 + Footer 固定。点遮罩或 Esc 关闭。"
                                </SheetDescription>
                            </SheetHeader>
                            <SheetBody>
                                <VStack gap="0.5rem" w="100%">
                                    <Label text_size="0.875rem" leading="relaxed">
                                        "Lorem ipsum dolor sit amet, consectetur adipiscing elit."
                                    </Label>
                                    <Label text_size="0.875rem" leading="relaxed">
                                        "Ut enim ad minim veniam, quis nostrud exercitation ullamco."
                                    </Label>
                                    <Label text_size="0.875rem" leading="relaxed">
                                        "Duis aute irure dolor in reprehenderit in voluptate velit."
                                    </Label>
                                </VStack>
                            </SheetBody>
                            <SheetFooter>
                                <Button size="sm" on_click={move || { toast_push("Sheet confirmed", 1500); o_left.set(false); }}>
                                    "Save changes"
                                </Button>
                                <SheetClose>
                                    <Button variant="outline" size="sm">Cancel</Button>
                                </SheetClose>
                            </SheetFooter>
                        </SheetContent>
                    </Sheet>
                </HStack>
            }
        }
        gal_overlay_sheet_sides()
    };
    let tip: Element = ui! {
        <Tooltip id="gal-tip">
            <TooltipTrigger>
                <Button variant="outline" size="sm">
                    "Hover for tooltip"
                </Button>
            </TooltipTrigger>
            <TooltipContent>
                "Hover tip"
            </TooltipContent>
        </Tooltip>
    };
    let hover_card_el: Element = {
        let hc_open = use_signal("gal_hover_card_open", || false);
        let hc_builder_open = use_signal("gal_hover_card_builder_open", || false);
        let rsx: Element = ui! {
            <HoverCard id="gallery:hover_card" open={hc_open}>
                <HoverCardTrigger openDelay=10 closeDelay=100>
                    <Button variant="link" size="sm">"@nextjs"</Button>
                </HoverCardTrigger>
                <HoverCardContent side="bottom" align="center" sideOffset=4 class="w-64">
                    <HStack gap="0.75rem" items="start" w="100%">
                        <Avatar size="lg">
                            <AvatarFallback>"NX"</AvatarFallback>
                        </Avatar>
                        <VStack gap="0.125rem" min_w="0" flex_grow={1.0}>
                            <Label font_weight=600 text_size="0.875">"@nextjs"</Label>
                            <Label text_size="0.875rem" leading="relaxed">
                                "The React Framework – created and maintained by @vercel."
                            </Label>
                            <Label text_size="0.75rem" color="muted-foreground">
                                "Joined December 2021"
                            </Label>
                        </VStack>
                    </HStack>
                </HoverCardContent>
            </HoverCard>
        };
        let builder: Element = hover_card("gallery:hover_card_builder")
            .open_signal(hc_builder_open)
            .child(|| {
                let mut frag = Element::new();
                frag.set_attr("jadix_fragment", "1");
                frag.children(vec![
                    HoverCardTrigger(HoverCardTriggerProps {
                        id: String::new(),
                        open_delay: Some(10),
                        close_delay: Some(100),
                        children: Some(
                            button("gallery:hover_card_builder_btn")
                                .variant("outline")
                                .size("sm")
                                .text("HoverCard Builder")
                                .into(),
                        ),
                        base: None,
                    }),
                    hover_card_content(
                        HoverCardContentOpts {
                            side: "bottom".into(),
                            align: "center".into(),
                            side_offset: 4.0,
                            width: None,
                            ..Default::default()
                        }
                        .class("w-64"),
                        || {
                            Element::from(
                                vstack()
                                    .gap("0.25rem")
                                    .child(
                                        label("gal-hc-b-title")
                                            .text("@jadix")
                                            .size(14.0)
                                            .font_weight(600.0),
                                    )
                                    .child(
                                        label("gal-hc-b-body")
                                            .text("Same surface as RSX HoverCardContent.")
                                            .size(14.0),
                                    ),
                            )
                        },
                    ),
                ])
            });
        ui! {
            <HStack gap="0.75rem" wrap="wrap" items="center">
                { rsx }
                { builder }
            </HStack>
        }
    };
    let dd: Element = {
        #[inline(never)]
        fn gal_overlay_dropdown() -> Element {
            ui! {
            <DropdownMenu id="gal-dd">
                <DropdownMenuTrigger>
                    <Button variant="outline" size="sm">
                        "DropdownMenu"
                    </Button>
                </DropdownMenuTrigger>
                <DropdownMenuContent>
                    <DropdownMenuLabel>"Actions"</DropdownMenuLabel>
                    <DropdownMenuItem on_click={|| {
                        toast_push("Profile", 1500);
                    }}>
                        "Profile"
                        <DropdownMenuShortcut>"⇧⌘P"</DropdownMenuShortcut>
                    </DropdownMenuItem>
                    <DropdownMenuItem on_click={|| {
                        toast_push("Billing", 1500);
                    }}>
                        "Billing"
                        <DropdownMenuShortcut>"⌘B"</DropdownMenuShortcut>
                    </DropdownMenuItem>
                    <DropdownMenuSeparator />
                    <DropdownMenuCheckboxItem defaultChecked={true} on_click={|| {
                        gallery_feedback_toast("Status Bar toggled", "DropdownMenu CheckboxItem");
                    }}>
                        "Status Bar"
                    </DropdownMenuCheckboxItem>
                    <DropdownMenuCheckboxItem on_click={|| {
                        gallery_feedback_toast("Activity Bar toggled", "DropdownMenu CheckboxItem");
                    }}>
                        "Activity Bar"
                    </DropdownMenuCheckboxItem>
                    <DropdownMenuSeparator />
                    <DropdownMenuRadioGroup defaultValue="bottom">
                        <DropdownMenuLabel>"Panel Position"</DropdownMenuLabel>
                        <DropdownMenuRadioItem value="top" on_click={|| {
                            gallery_feedback_toast("Panel: Top", "DropdownMenu RadioItem");
                        }}>
                            "Top"
                        </DropdownMenuRadioItem>
                        <DropdownMenuRadioItem value="bottom" on_click={|| {
                            gallery_feedback_toast("Panel: Bottom", "DropdownMenu RadioItem");
                        }}>
                            "Bottom"
                        </DropdownMenuRadioItem>
                        <DropdownMenuRadioItem value="right" on_click={|| {
                            gallery_feedback_toast("Panel: Right", "DropdownMenu RadioItem");
                        }}>
                            "Right"
                        </DropdownMenuRadioItem>
                    </DropdownMenuRadioGroup>
                    <DropdownMenuSeparator />
                    <DropdownMenuSub>
                        <DropdownMenuSubTrigger>"Invite users"</DropdownMenuSubTrigger>
                        <DropdownMenuSubContent>
                            <DropdownMenuItem on_click={|| {
                                toast_push("Email…", 1500);
                            }}>
                                "Email…"
                            </DropdownMenuItem>
                            <DropdownMenuItem on_click={|| {
                                toast_push("Message…", 1500);
                            }}>
                                "Message…"
                            </DropdownMenuItem>
                            <DropdownMenuSeparator />
                            <DropdownMenuItem on_click={|| {
                                toast_push("More…", 1500);
                            }}>
                                "More…"
                            </DropdownMenuItem>
                        </DropdownMenuSubContent>
                    </DropdownMenuSub>
                    <DropdownMenuSeparator />
                    <DropdownMenuItem variant="destructive" on_click={|| {
                        toast_push("Log out", 1500);
                    }}>
                        "Log out"
                        <DropdownMenuShortcut>"⇧⌘Q"</DropdownMenuShortcut>
                    </DropdownMenuItem>
                </DropdownMenuContent>
            </DropdownMenu>
                }
        }
        gal_overlay_dropdown()
    };
    let ctx: Element = {
        #[inline(never)]
        fn gal_overlay_context_menu() -> Element {
            ui! {
            <ContextMenu id="gal-ctx">
                <ContextMenuTrigger>
                    <Box p="0.75rem" bg="muted" class="rounded-md" w="100%">
                        <Label text_size="0.875rem">{ "右键此区域打开 ContextMenu" }</Label>
                    </Box>
                </ContextMenuTrigger>
                <ContextMenuContent>
                    <ContextMenuLabel>"Actions"</ContextMenuLabel>
                    <ContextMenuItem on_click={|| {
                        toast_push("Copy", 1500);
                    }}>
                        "Copy"
                        <ContextMenuShortcut>"⌘C"</ContextMenuShortcut>
                    </ContextMenuItem>
                    <ContextMenuItem on_click={|| {
                        toast_push("Paste", 1500);
                    }}>
                        "Paste"
                        <ContextMenuShortcut>"⌘V"</ContextMenuShortcut>
                    </ContextMenuItem>
                    <ContextMenuSeparator />
                    <ContextMenuCheckboxItem defaultChecked={true} on_click={|| {
                        gallery_feedback_toast("Show Bookmarks Bar toggled", "ContextMenu CheckboxItem");
                    }}>
                        "Show Bookmarks Bar"
                    </ContextMenuCheckboxItem>
                    <ContextMenuCheckboxItem on_click={|| {
                        gallery_feedback_toast("Show Full URLs toggled", "ContextMenu CheckboxItem");
                    }}>
                        "Show Full URLs"
                    </ContextMenuCheckboxItem>
                    <ContextMenuSeparator />
                    <ContextMenuRadioGroup defaultValue="pedro">
                        <ContextMenuLabel>"People"</ContextMenuLabel>
                        <ContextMenuRadioItem value="pedro" on_click={|| {
                            gallery_feedback_toast("Pedro Duarte", "ContextMenu RadioItem");
                        }}>
                            "Pedro Duarte"
                        </ContextMenuRadioItem>
                        <ContextMenuRadioItem value="colm" on_click={|| {
                            gallery_feedback_toast("Colm Tuite", "ContextMenu RadioItem");
                        }}>
                            "Colm Tuite"
                        </ContextMenuRadioItem>
                    </ContextMenuRadioGroup>
                    <ContextMenuSeparator />
                    <ContextMenuSub>
                        <ContextMenuSubTrigger>"More Tools"</ContextMenuSubTrigger>
                        <ContextMenuSubContent>
                            <ContextMenuItem on_click={|| {
                                toast_push("Save Page As…", 1500);
                            }}>
                                "Save Page As…"
                                <ContextMenuShortcut>"⇧⌘S"</ContextMenuShortcut>
                            </ContextMenuItem>
                            <ContextMenuItem on_click={|| {
                                toast_push("Create Shortcut…", 1500);
                            }}>
                                "Create Shortcut…"
                            </ContextMenuItem>
                            <ContextMenuItem on_click={|| {
                                toast_push("Name Window…", 1500);
                            }}>
                                "Name Window…"
                            </ContextMenuItem>
                            <ContextMenuSeparator />
                            <ContextMenuItem on_click={|| {
                                toast_push("Developer Tools", 1500);
                            }}>
                                "Developer Tools"
                            </ContextMenuItem>
                            <ContextMenuSeparator />
                            <ContextMenuItem variant="destructive" on_click={|| {
                                toast_push("Delete", 1500);
                            }}>
                                "Delete"
                                <ContextMenuShortcut>"⌘⌫"</ContextMenuShortcut>
                            </ContextMenuItem>
                        </ContextMenuSubContent>
                    </ContextMenuSub>
                    <ContextMenuSeparator />
                    <ContextMenuItem variant="destructive" on_click={|| {
                        toast_push("Delete", 1500);
                    }}>
                        "Delete"
                    </ContextMenuItem>
                </ContextMenuContent>
            </ContextMenu>
                }
        }
        gal_overlay_context_menu()
    };
    let progress_el: Element = progress().value_signal(progress_val).into();
    let progress_vertical_el: Element = progress()
        .orientation("vertical")
        .value_signal(progress_val)
        .into();

    #[inline(never)]
    #[expect(
        clippy::too_many_arguments,
        reason = "The existing rendering adapter keeps its independent geometry, state and callback parameters"
    )]
    fn gal_overlay_card(
        open_cmd: Signal<bool>,
        open_cmd_dlg: Signal<bool>,
        dialog_el: Element,
        sheet_el: Element,
        drawer_el: Element,
        tip: Element,
        hover_card_el: Element,
        dd: Element,
        ctx: Element,
        progress_val: Signal<f32>,
        progress_el: Element,
        progress_vertical_el: Element,
        bump: Signal<f32>,
        cmd_inline: Element,
        cmd_el: Element,
        cmd_dlg_el: Element,
        sheet_sides_el: Element,
    ) -> Element {
        const PROGRESS_IMAGE_SRC: &str = concat!(
            "data:image/svg+xml,<svg xmlns='http://www.w3.org/2000/svg' width='128' height='128' viewBox='0 0 128 128'>",
            "<rect width='128' height='128' fill='#dbeafe'/><circle cx='94' cy='32' r='13' fill='#fbbf24'/>",
            "<path d='M0 100 L44 36 L94 100 Z' fill='#3b82f6'/><path d='M48 128 L98 57 L150 128 Z' fill='#1e40af'/>",
            "<path d='M32 54 L44 36 L58 54 Z M86 74 L98 57 L110 74 Z' fill='white'/></svg>"
        );
        let circle_composed: Element = progress_circle()
            .id("gal-progress-circle-composed")
            .value_signal(progress_val)
            .size(ProgressCircleSize::Lg)
            .child(
                vstack()
                    .gap(2.0)
                    .items_center()
                    .child(
                        ui_image()
                            .src(PROGRESS_IMAGE_SRC)
                            .style(|b| b.w(24.0).h(24.0)),
                    )
                    .child(
                        Element::new()
                            .template_id("gal-progress-circle-composed-value")
                            .text_size(20.0)
                            .font_weight(600.0)
                            .text(move || {
                                format!("{:.0}%", progress_val.get().clamp(0.0, 1.0) * 100.0)
                            }),
                    )
                    .child(Element::new().text_size(12.0).text("上传中")),
            )
            .into();
        ui! {
            <MyCard title="Command · Dialog · Sheet · Drawer · Toast · Tooltip · HoverCard · Dropdown · Progress">
                <VStack w="100%" gap="0.75rem">
                    <Label text_size="0.75rem" color="muted-foreground" leading="normal">
                        "浮层与反馈：Command 顶部居中浮层 / CommandDialog 模态、Dialog、Sheet（复合 Trigger/Content/Side）、Drawer（底滑 + 拖柄）、Toast、Tooltip、HoverCard（悬停预览）、DropdownMenu、进度条。desktop_smoke #7/#8：Dialog+Select 嵌套 Esc；次窗按钮。"
                    </Label>
                    <HStack gap="0.5rem" wrap="wrap" items="center">
                        <Button variant="secondary" size="sm" on_click={move || open_cmd.set(true)}>
                            "Command"
                        </Button>
                        <Button variant="outline" size="sm" on_click={move || open_cmd_dlg.set(true)}>
                            "Command Dialog"
                        </Button>
                        { dialog_el }
                        { sheet_el }
                        { drawer_el }
                        <Button variant="default" size="sm" on_click={move || {
                            gallery_feedback_toast(
                                "Toast from Gallery",
                                "Hover the stack to expand.",
                            );
                        }}>
                            "Toast"
                        </Button>
                        <Button variant="secondary" size="sm" on_click={move || {
                            toast_set_layout("stack");
                            toast_set_overflow("displace");
                            toast_set_limit(3);
                            let _ = toast("First").description("Stack peek #1").duration(6000).show();
                            let _ = toast("Second").description("Stack peek #2").duration(6000).show();
                            let _ = toast("Third").description("Front of stack — hover to expand").duration(6000).show();
                        }}>
                            "Stack ×3"
                        </Button>
                        <Button variant="outline" size="sm" on_click={move || {
                            toast_set_layout("list");
                            toast_set_overflow("displace");
                            toast_set_limit(3);
                            let _ = toast("List toast").description("Single list item (bottom-right)").duration(5000).show();
                        }}>
                            "List"
                        </Button>
                        <Button variant="outline" size="sm" on_click={move || {
                            toast_set_layout("list");
                            toast_set_overflow("displace");
                            toast_set_limit(3);
                            let _ = toast("List A").description("Non-stacked list layout").duration(6000).show();
                            let _ = toast("List B").variant("success").duration(6000).show();
                            let _ = toast("List C").variant("warning").duration(6000).show();
                        }}>
                            "List ×3"
                        </Button>
                        <Button variant="outline" size="sm" on_click={move || {
                            toast_set_layout("stack");
                            toast_set_overflow("displace");
                            toast_set_limit(2);
                            let _ = toast("Keep").description("limit=2 displace").duration(8000).show();
                            let _ = toast("Stay").duration(8000).show();
                            let _ = toast("New").description("Oldest held, returns later").duration(8000).show();
                        }}>
                            "Displace"
                        </Button>
                        <Button variant="outline" size="sm" on_click={move || {
                            toast_set_layout("stack");
                            toast_set_overflow("wait");
                            toast_set_limit(2);
                            let _ = toast("Visible 1").description("limit=2 wait").duration(5000).show();
                            let _ = toast("Visible 2").duration(5000).show();
                            let _ = toast("Waiting A").description("Enters after a slot frees").duration(5000).show();
                            let _ = toast("Waiting B").duration(5000).show();
                        }}>
                            "Wait"
                        </Button>
                        <Button variant="secondary" size="sm" on_click={move || {
                            toast("Saved")
                                .description("Changes applied.")
                                .variant("success")
                                .duration(2500)
                                .show();
                        }}>
                            "Success"
                        </Button>
                        <Button variant="outline" size="sm" on_click={move || {
                            toast("FYI")
                                .description("Optional left icon is on by default.")
                                .variant("info")
                                .duration(2500)
                                .show();
                        }}>
                            "Info"
                        </Button>
                        <Button variant="outline" size="sm" on_click={move || {
                            toast("Check network")
                                .variant("warning")
                                .duration(2500)
                                .show();
                        }}>
                            "Warning"
                        </Button>
                        <Button variant="destructive" size="sm" on_click={move || {
                            toast("Something failed")
                                .variant("destructive")
                                .duration(3000)
                                .show();
                        }}>
                            "Error"
                        </Button>
                        <Button variant="ghost" size="sm" on_click={move || {
                            toast("No icon")
                                .description("Same success tone without the glyph.")
                                .variant("success")
                                .icon(false)
                                .duration(2500)
                                .show();
                        }}>
                            "Success · no icon"
                        </Button>
                        <Button variant="ghost" size="sm" on_click={move || {
                            toast_set_position("top-left");
                            toast("Top left").description("Swipe up to dismiss").duration(5000).show();
                        }}>
                            "↖"
                        </Button>
                        <Button variant="ghost" size="sm" on_click={move || {
                            toast_set_position("top-center");
                            toast("Top center").duration(5000).show();
                        }}>
                            "↑"
                        </Button>
                        <Button variant="ghost" size="sm" on_click={move || {
                            toast_set_position("top-right");
                            toast("Top right").duration(5000).show();
                        }}>
                            "↗"
                        </Button>
                        <Button variant="ghost" size="sm" on_click={move || {
                            toast_set_position("bottom-left");
                            toast("Bottom left").description("Swipe down to dismiss").duration(5000).show();
                        }}>
                            "↙"
                        </Button>
                        <Button variant="ghost" size="sm" on_click={move || {
                            toast_set_position("bottom-center");
                            toast("Bottom center").duration(5000).show();
                        }}>
                            "↓"
                        </Button>
                        <Button variant="ghost" size="sm" on_click={move || {
                            toast_set_position("bottom-right");
                            toast("Bottom right").duration(5000).show();
                        }}>
                            "↘"
                        </Button>
                        <Button
                            variant="secondary"
                            size="sm"
                            on_click={move || {
                                match open_secondary_window_with(
                                    WindowOptions::new()
                                        .title("Jadix Gallery — Secondary")
                                        .window_label("gallery-secondary"),
                                    gallery_secondary_shell_root,
                                ) {
                                    Ok(()) => {
                                        toast_push("Secondary window opened", 2000);
                                    }
                                    Err(e) => {
                                        log::warn!("open_secondary_window: {e}");
                                        toast_push(format!("次窗失败: {e}"), 3000);
                                    }
                                }
                            }}
                        >
                            "Secondary window"
                        </Button>
                        { tip }
                        { hover_card_el }
                        { dd }
                    </HStack>
                    { ctx }
                    <VStack gap="0.5rem" w="100%">
                        <Label font_weight="600" text_size="0.875rem">{ "Progress" }</Label>
                        <Label text_size="0.75rem" color="muted-foreground" leading="normal">
                            "ShadCN 复合（Label / Value + Track）；value ∈ 0..=1；Bump 演示受控。"
                        </Label>
                        <Progress id="gal-progress" valueSignal={progress_val}>
                            <ProgressLabel>"Upload"</ProgressLabel>
                            <ProgressValue />
                        </Progress>
                        <Label text_size="0.75rem" color="muted-foreground" leading="normal">
                            "Builder 双写（简单条，无 Label）："
                        </Label>
                        { progress_el }
                        <Label text_size="0.75rem" color="muted-foreground" leading="normal">
                            "ProgressCircle（md / sm / lg，与上方共用 value_signal）："
                        </Label>
                        <HStack gap="1rem" items="center" wrap="wrap">
                            <ProgressCircle id="gal-progress-circle-md" valueSignal={progress_val} size={ProgressCircleSize::Md} />
                            <ProgressCircle id="gal-progress-circle-sm" valueSignal={progress_val} size={ProgressCircleSize::Sm} />
                            <ProgressCircle id="gal-progress-circle-lg" valueSignal={progress_val} size={ProgressCircleSize::Lg} />
                        </HStack>
                        <Label text_size="0.75rem" color="muted-foreground" leading="normal">
                            "圈内自定义内容：百分比、图片、图片与文字组合；点击 Bump 同步更新。"
                        </Label>
                        <HStack gap="1.5rem" items="start" wrap="wrap">
                            <VStack gap="0.375rem" items="center">
                                <ProgressCircle id="gal-progress-circle-percent" valueSignal={progress_val} size={ProgressCircleSize::Lg}>
                                    { Element::new()
                                        .template_id("gal-progress-circle-percent-value")
                                        .text_size(24.0)
                                        .font_weight(600.0)
                                        .text(move || format!("{:.0}%", progress_val.get().clamp(0.0, 1.0) * 100.0)) }
                                </ProgressCircle>
                                <Label text_size="0.75rem" color="muted-foreground">"百分比"</Label>
                            </VStack>
                            <VStack gap="0.375rem" items="center">
                                <ProgressCircle id="gal-progress-circle-image" valueSignal={progress_val} size={ProgressCircleSize::Lg}>
                                    <Image src={PROGRESS_IMAGE_SRC} w="100%" h="100%" />
                                </ProgressCircle>
                                <Label text_size="0.75rem" color="muted-foreground">"图片"</Label>
                            </VStack>
                            <VStack gap="0.375rem" items="center">
                                { circle_composed }
                                <Label text_size="0.75rem" color="muted-foreground">"组合内容 · Builder"</Label>
                            </VStack>
                        </HStack>
                        <Separator />
                        <Label font_weight="600" text_size="0.875rem">{ "orientation=vertical" }</Label>
                        <Label text_size="0.75rem" color="muted-foreground" leading="normal">
                            "线条型 Progress 纵向：默认高 8rem，与 Bump 共用 value_signal。"
                        </Label>
                        <HStack gap="1.5rem" items="end" wrap="wrap">
                            <VStack gap="0.375rem" items="center">
                                <Label text_size="0.75rem" color="muted-foreground">{ "RSX" }</Label>
                                <Progress id="gal-progress-vertical" orientation="vertical" valueSignal={progress_val}>
                                    <ProgressLabel>"Upload"</ProgressLabel>
                                    <ProgressValue />
                                </Progress>
                            </VStack>
                            <VStack gap="0.375rem" items="center">
                                <Label text_size="0.75rem" color="muted-foreground">{ "Builder" }</Label>
                                { progress_vertical_el }
                            </VStack>
                        </HStack>
                        <Button variant="outline" size="sm" on_click={move || {
                            let v = bump.peek();
                            bump.set(if v >= 1.0 { 0.0 } else { (v + 0.1).min(1.0) });
                        }}>{ "Bump +10%" }</Button>
                    </VStack>
                    { cmd_inline }
                    { cmd_el }
                    { cmd_dlg_el }
                    <VStack gap="0.5rem" w="100%">
                        <Label font_weight="600" text_size="0.875rem">{ "Sheet · sides" }</Label>
                        <Label text_size="0.75rem" color="muted-foreground" leading="normal">
                            "top / right / bottom / left 贴边；对齐 shadcn sheet-side。"
                        </Label>
                        { sheet_sides_el }
                    </VStack>
                </VStack>
            </MyCard>
        }
    }
    gal_overlay_card(
        open_cmd,
        open_cmd_dlg,
        dialog_el,
        sheet_el,
        drawer_el,
        tip,
        hover_card_el,
        dd,
        ctx,
        progress_val,
        progress_el,
        progress_vertical_el,
        bump,
        cmd_inline,
        cmd_el,
        cmd_dlg_el,
        sheet_sides_el,
    )
}

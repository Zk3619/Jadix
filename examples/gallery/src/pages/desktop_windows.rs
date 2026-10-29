use crate::imports::*;
use crate::shared::{MyCard, MyCardProps};

pub(crate) fn instance_demo_mode(
    args: impl IntoIterator<Item = std::ffi::OsString>,
) -> Option<&'static str> {
    args.into_iter().find_map(|arg| match arg.to_str()? {
        "--gallery-instance-demo=single" => Some("single"),
        "--gallery-instance-demo=profile" => Some("profile"),
        "--gallery-instance-demo=multiple" => Some("multiple"),
        _ => None,
    })
}

fn instance_launches() -> Signal<(u32, String)> {
    shared_slot("gal-desktop-instance-launches", || {
        (
            0,
            "尚未收到重复启动；点击同一单例按钮再次启动。".to_string(),
        )
    })
}

pub(crate) fn record_second_instance(launch: jadix::app::InstanceLaunch) {
    let launches = instance_launches();
    launches.set((
        launches.peek().0 + 1,
        format!(
            "参数：{:?}\n工作目录：{}",
            launch.args,
            launch.working_directory.display()
        ),
    ));
}

fn desktop_result(result: Result<(), String>, success: &str) {
    match result {
        Ok(()) => toast_push(success, 2500),
        Err(error) => toast_push(format!("窗口演示失败：{error}"), 4000),
    };
}

fn launch_instance(mode: &str) {
    let result = (|| -> Result<(), String> {
        let executable = std::env::current_exe().map_err(|error| error.to_string())?;
        let mut command = std::process::Command::new(executable);
        command.arg(format!("--gallery-instance-demo={mode}"));
        command.arg(format!(
            "--gallery-instance-payload={}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis()
        ));
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000000);
        }
        let mut child = command.spawn().map_err(|error| error.to_string())?;
        std::thread::spawn(move || {
            let _ = child.wait();
        });
        Ok(())
    })();
    desktop_result(
        result,
        "已启动演示进程；重复启动同一单例将激活原窗口并转交参数",
    );
}

fn fullscreen_status() -> String {
    match jadix::app::current_window().map(|window| window.is_fullscreen()) {
        Some(Ok(true)) => "最近查询：全屏".to_string(),
        Some(Ok(false)) => "最近查询：窗口模式".to_string(),
        _ => "最近查询：当前没有原生窗口".to_string(),
    }
}

#[component]
pub(crate) fn FullscreenShowcase() -> Element {
    let window_id = jadix::app::current_window().map(|window| window.id());
    let status = use_signal(
        &format!("gal-desktop-fullscreen-{window_id:?}"),
        fullscreen_status,
    );
    let shortcuts = use_signal(&format!("gal-desktop-shortcuts-{window_id:?}"), || true);
    ui! {
        <MyCard title="桌面全屏">
            <VStack w="100%" min_w="0" gap="0.5rem">
                <Label text_size="0.75rem" color="muted-foreground" leading="normal">
                    "Windows / Linux：F11；Windows：Alt+Enter；macOS：Control+Command+F。Esc 先关闭弹层，再退出全屏。主窗和次窗均可操作。"
                </Label>
                <Label id="gal-desktop-fullscreen-status" text_size="0.875rem">{status.get()}</Label>
                <HStack w="100%" gap="0.5rem" wrap="wrap">
                    <Button id="gal-desktop-fullscreen-toggle" size="sm" on_click={move || {
                        let result = jadix::app::current_window()
                            .ok_or_else(|| "当前没有原生窗口".to_string())
                            .and_then(|window| window.toggle_fullscreen());
                        desktop_result(result, "已请求切换全屏；可刷新状态确认");
                    }}>"切换全屏"</Button>
                    <Button id="gal-desktop-fullscreen-exit" size="sm" variant="outline" on_click={move || {
                        let result = jadix::app::current_window()
                            .ok_or_else(|| "当前没有原生窗口".to_string())
                            .and_then(|window| window.set_fullscreen(false));
                        desktop_result(result, "已请求退出全屏");
                    }}>"退出全屏"</Button>
                    <Button id="gal-desktop-fullscreen-refresh" size="sm" variant="outline"
                        on_click={move || status.set(fullscreen_status())}>"刷新全屏状态"</Button>
                    <Button id="gal-desktop-fullscreen-shortcuts" size="sm" variant="secondary" on_click={move || {
                        let enabled = !shortcuts.peek();
                        let result = jadix::app::current_window()
                            .ok_or_else(|| "当前没有原生窗口".to_string())
                            .and_then(|window| window.set_fullscreen_shortcuts(enabled));
                        if result.is_ok() { shortcuts.set(enabled); }
                        desktop_result(result, "已更新本窗全屏快捷键设置");
                    }}>{if shortcuts.get() { "禁用全屏快捷键" } else { "启用全屏快捷键" }}</Button>
                </HStack>
                <Label text_size="0.75rem" color="muted-foreground">
                    "操作后点击刷新读取原生状态。禁用快捷键后仍可用按钮退出全屏。"
                </Label>
            </VStack>
        </MyCard>
    }
}

#[component]
pub(crate) fn InstanceShowcase() -> Element {
    let launches = instance_launches();
    let mode = instance_demo_mode(std::env::args_os().skip(1));
    ui! {
        <MyCard title="单例与多开（进程）">
            <VStack w="100%" min_w="0" gap="0.5rem">
                <Label text_size="0.75rem" color="muted-foreground" leading="normal">
                    "单例 A：重复点击只保留一个进程，原窗口恢复并接收参数。单例 B 使用另一应用 ID，可与 A 并存。允许多开：每次创建独立进程。策略在启动时生效。"
                </Label>
                <Label id="gal-desktop-instance-status" text_size="0.875rem">
                    {format!("PID：{} · 当前模式：{} · 收到重复启动：{} 次", std::process::id(), mode.unwrap_or("Gallery 默认多开"), launches.get().0)}
                </Label>
                <HStack w="100%" gap="0.5rem" wrap="wrap">
                    <Button id="gal-desktop-instance-single" size="sm" on_click={move || launch_instance("single")}>"启动 / 激活单例 A"</Button>
                    <Button id="gal-desktop-instance-profile" size="sm" variant="outline" on_click={move || launch_instance("profile")}>"启动 / 激活单例 B"</Button>
                    <Button id="gal-desktop-instance-multiple" size="sm" variant="secondary" on_click={move || launch_instance("multiple")}>"允许多开：新进程"</Button>
                </HStack>
                <Label id="gal-desktop-instance-launch" w="100%" min_w="0" text_size="0.75rem" leading="normal">{launches.get().1}</Label>
                <Label text_size="0.75rem" color="muted-foreground" leading="normal">
                    "观察新进程窗口中的 PID、重复启动次数、参数与工作目录。主窗关闭后，单例仍可激活存活的独立次窗。"
                </Label>
            </VStack>
        </MyCard>
    }
}

fn open_desktop_window(independent: bool, on_top: bool) {
    let title = if on_top {
        "全局置顶次窗"
    } else if independent {
        "独立次窗"
    } else {
        "跟随主窗的次窗"
    };
    let result = open_secondary_window_with(
        WindowOptions::new()
            .title(format!("Jadix Gallery — {title}"))
            .size(760.0, 760.0)
            .icon(crate::gallery_icon())
            .close_with_primary(!independent)
            .always_on_top(on_top),
        move || gallery_desktop_window_root(independent, on_top),
    );
    desktop_result(result, "已请求打开次窗；可在次窗内调整独立存活与置顶设置");
}

#[component]
pub(crate) fn WindowSessionShowcase() -> Element {
    use jadix::app::WindowSession;
    let status = use_signal("gal-session-status", || "尚未打开会话次窗".to_string());
    let session_slot = std::rc::Rc::new(std::cell::RefCell::new(
        None::<WindowSession<String, String, String>>,
    ));
    let opened_slot = session_slot.clone();
    let send_slot = session_slot.clone();
    ui! {
        <MyCard title="主次窗传参 / 双向通信 / 关闭回传">
            <VStack w="100%" min_w="0" gap="0.5rem">
                <Label id="gal-session-status" text_size="0.875rem">{status.get()}</Label>
                <HStack w="100%" gap="0.5rem" wrap="wrap">
                    <Button id="gal-session-open" size="sm" on_click={move || {
                        if let Some(window) = opened_slot.borrow().as_ref().and_then(WindowSession::window) {
                            let _ = window.focus();
                            return;
                        }
                        let session = WindowSession::<String, String, String>::new("Gallery launch argument".into());
                        session.on_opened(move |_| { status.try_set("次窗已打开：可以发送消息".into()); });
                        session.on_child_message(move |message| {
                            log::info!(target: "gallery::window_session", "child message: {message}");
                            status.try_set(message);
                        });
                        session.on_closed(move |event| {
                            let result = event.result.unwrap_or_else(|| "原生关闭，无返回值".into());
                            log::info!(target: "gallery::window_session", "closed: {result}");
                            status.try_set(format!("次窗已关闭：{result}"));
                        });
                        let result = session.open(
                            WindowOptions::new().title("Jadix WindowSession").size(540.0, 340.0),
                            |context| {
                                let value = use_signal("gal-session-received", || "等待主窗消息".to_string());
                                context.on_parent_message(move |message| {
                                    log::info!(target: "gallery::window_session", "parent message: {message}");
                                    value.try_set(message);
                                });
                                let args = context.args().clone();
                                let reply = context.clone();
                                let finish = context.clone();
                                ui! {
                                    <VStack w="100%" h="100%" p="1rem" gap="0.75rem" bg="background" color="foreground">
                                        <Label font_weight="600">{format!("启动参数：{args}")}</Label>
                                        <Input id="gal-session-value" w="100%" value={value} />
                                        <HStack gap="0.5rem" wrap="wrap">
                                            <Button id="gal-session-reply" size="sm" on_click={move || {
                                                let _ = reply.send_to_parent(format!("次窗回复：{}", value.peek()));
                                            }}>"发送给主窗"</Button>
                                            <Button id="gal-session-return" size="sm" variant="outline" on_click={move || {
                                                let _ = finish.close_with_result(value.peek());
                                            }}>"返回结果并关闭"</Button>
                                        </HStack>
                                    </VStack>
                                }
                            },
                        );
                        if let Err(error) = result { status.try_set(error); }
                        *opened_slot.borrow_mut() = Some(session);
                    }}>"打开会话次窗"</Button>
                    <Button id="gal-session-send" size="sm" variant="outline" on_click={move || {
                        let result = send_slot.borrow().as_ref()
                            .ok_or_else(|| "请先打开会话次窗".to_string())
                            .and_then(|session| session.send_to_child("Hello from parent".into()));
                        if let Err(error) = result { status.try_set(error); }
                    }}>"向次窗发送消息"</Button>
                </HStack>
            </VStack>
        </MyCard>
    }
}

#[component]
pub(crate) fn MultiWindowShowcase() -> Element {
    ui! {
        <MyCard title="多窗口与生命周期">
            <VStack w="100%" min_w="0" gap="0.5rem">
                <Label text_size="0.75rem" color="muted-foreground" leading="normal">
                    "跟随次窗会随主窗关闭；独立次窗在主窗销毁后继续运行，还能创建新窗口。全局置顶会高于其他应用的普通窗口。关闭最后一个窗口退出进程。"
                </Label>
                <HStack w="100%" gap="0.5rem" wrap="wrap">
                    <Button id="gal-desktop-window-dependent" size="sm" variant="outline" on_click={move || open_desktop_window(false, false)}>"打开跟随次窗"</Button>
                    <Button id="gal-desktop-window-independent" size="sm" on_click={move || open_desktop_window(true, false)}>"打开独立次窗"</Button>
                    <Button id="gal-desktop-window-topmost" size="sm" variant="secondary" on_click={move || open_desktop_window(true, true)}>"打开独立置顶次窗"</Button>
                </HStack>
                <Label text_size="0.75rem" color="muted-foreground" leading="normal">
                    "验证：先打开跟随次窗和独立次窗，再关闭 Gallery 主窗。仅独立次窗保留；可在其中继续开窗、切换全屏，或退出整个演示进程。"
                </Label>
            </VStack>
        </MyCard>
    }
}

pub(crate) fn gallery_desktop_window_root(independent: bool, on_top: bool) -> Element {
    let window_id = jadix::app::current_window().map(|window| window.id());
    let follows = use_signal(&format!("gal-desktop-follows-{window_id:?}"), || {
        !independent
    });
    let topmost = use_signal(&format!("gal-desktop-topmost-{window_id:?}"), || on_top);
    ui! {
        <VStack w="100%" h="100%" min_w="0" min_h="0" bg="background" color="foreground">
            <ScrollArea id="gal-desktop-window-scroll" flex_grow="1" min_h="0" overflow_x="hidden">
                <VStack w="100%" min_w="0" p="1rem" gap="1rem">
                    <Label text_size="1.25rem" font_weight="700">"桌面窗口能力 · 次窗"</Label>
                    <Label text_size="0.875rem">{format!("PID：{} · 窗口：{window_id:?}", std::process::id())}</Label>
                    <HStack w="100%" gap="0.5rem" wrap="wrap">
                        <Button id="gal-desktop-window-follow" size="sm" variant="outline" on_click={move || {
                            let enabled = !follows.peek();
                            let result = jadix::app::current_window()
                                .ok_or_else(|| "当前没有原生窗口".to_string())
                                .and_then(|window| window.set_close_with_primary(enabled));
                            if result.is_ok() { follows.set(enabled); }
                            desktop_result(result, "已更新本窗关闭策略");
                        }}>{if follows.get() { "随主窗关闭 → 改为独立" } else { "独立存活 → 改为跟随" }}</Button>
                        <Button id="gal-desktop-window-priority" size="sm" variant="outline" on_click={move || {
                            let enabled = !topmost.peek();
                            let result = jadix::app::current_window()
                                .ok_or_else(|| "当前没有原生窗口".to_string())
                                .and_then(|window| window.set_always_on_top(enabled));
                            if result.is_ok() { topmost.set(enabled); }
                            desktop_result(result, "已更新本窗置顶设置");
                        }}>{if topmost.get() { "取消全局置顶" } else { "启用全局置顶" }}</Button>
                        <Button id="gal-desktop-window-close" size="sm" variant="outline" on_click={close_window}>"仅关闭本窗"</Button>
                        <Button id="gal-desktop-window-quit" size="sm" variant="destructive" on_click={request_exit}>"退出整个进程"</Button>
                    </HStack>
                    <FullscreenShowcase />
                    <MultiWindowShowcase />
                    <InstanceShowcase />
                </VStack>
            </ScrollArea>
        </VStack>
    }
}

pub(crate) fn gallery_instance_demo_root() -> Element {
    ui! {
        <VStack w="100%" h="100%" min_w="0" min_h="0" bg="background" color="foreground">
            <ScrollArea id="gal-desktop-instance-scroll" flex_grow="1" min_h="0" overflow_x="hidden">
                <VStack w="100%" min_w="0" p="1rem" gap="1rem">
                    <Label text_size="1.25rem" font_weight="700">"进程实例演示"</Label>
                    <InstanceShowcase />
                    <FullscreenShowcase />
                    <MultiWindowShowcase />
                    <HStack w="100%" gap="0.5rem" wrap="wrap">
                        <Button id="gal-desktop-primary-close" size="sm" variant="outline" on_click={close_window}>"关闭主窗（验证独立存活）"</Button>
                        <Button size="sm" variant="destructive" on_click={request_exit}>"退出整个演示进程"</Button>
                    </HStack>
                </VStack>
            </ScrollArea>
        </VStack>
    }
}

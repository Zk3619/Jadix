#[cfg(not(windows))]
fn main() {
    eprintln!("native-collapsible requires Windows");
    std::process::exit(1);
}

#[cfg(windows)]
fn main() {
    if let Err(error) = application::run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

#[cfg(windows)]
mod application {
    use jadix_components::{authoring::native::ElementProjection, prelude::*};
    use jadix_native::{
        host::{NativeRoot, PlatformHost},
        presentation::*,
        windows::WindowsHost,
    };
    use std::io::Write;

    fn find<'node>(
        node: &'node PresentationNode,
        predicate: &impl Fn(&NodeContent) -> bool,
    ) -> Option<&'node PresentationNode> {
        if predicate(&node.descriptor.content) {
            return Some(node);
        }
        node.children
            .iter()
            .find_map(|child| find(child, predicate))
    }

    fn render(
        root: &mut NativeRoot<WindowsHost>,
        projection: &ElementProjection,
        element: &mut Element,
    ) -> Result<PresentationNode, String> {
        let viewport = root.host().viewport();
        element.descriptor.style.width = Some(viewport[0]);
        element.descriptor.style.height = Some(viewport[1]);
        let mut tree = projection
            .snapshot_with_focus(element, root.model().focused())
            .map_err(|error| format!("{error:?}"))?;
        let environment = root.host().environment();
        root.present(std::slice::from_mut(&mut tree), viewport, environment)
            .map_err(|error| error.to_string())?;
        let bindings = projection.navigation_nodes(element);
        if !root.host_mut().bind_navigation(&bindings)? {
            return Err("Windows Accordion navigation binding is unavailable".into());
        }
        if let Some(node) = projection.take_focus_request(element) {
            root.focus(Some(node)).map_err(|error| error.to_string())?;
        }
        Ok(tree)
    }

    fn tick(
        root: &mut NativeRoot<WindowsHost>,
        projection: &ElementProjection,
        element: &mut Element,
    ) -> Result<(), String> {
        root.host().pump();
        for event in root.poll_events().map_err(|error| error.to_string())? {
            if !matches!(event.payload, EventPayload::Focus) {
                projection
                    .dispatch(element, &event)
                    .map_err(|error| format!("{error:?}"))?;
            }
        }
        Ok(())
    }

    pub fn run() -> Result<(), String> {
        let arguments: Vec<_> = std::env::args().skip(1).collect();
        let probe = match arguments.as_slice() {
            [] => None,
            [mode, nonce, output]
                if mode == "--probe" && !nonce.is_empty() && nonce.len() <= 128 =>
            {
                Some((nonce, std::path::Path::new(output)))
            }
            _ => return Err("Usage: native-collapsible [--probe NONCE NEW_REPORT]".into()),
        };
        let (_owner, (mut element, open, draft, selected)) = create_root(|| {
            let open = create_signal(false);
            let draft = create_signal("中文😀".to_string());
            let selected = create_signal(Vec::<String>::new());
            let content = ui! {
                <Collapsible id="native-details" open={open} exclusive={true} group="account">
                    <CollapsibleTrigger>"Account details"</CollapsibleTrigger>
                    <CollapsibleContent>
                        <Input id="account" value={draft} placeholder="Account name" />
                    </CollapsibleContent>
                </Collapsible>
            };
            let preferences = ui! {
                <Collapsible id="preferences" exclusive={true} group="account">
                    <CollapsibleTrigger>"Preferences"</CollapsibleTrigger>
                    <CollapsibleContent>"Only one account panel is open."</CollapsibleContent>
                </Collapsible>
            };
            let disabled = ui! {
                <Collapsible id="disabled-details" disabled={true}>
                    <CollapsibleTrigger>"Disabled details"</CollapsibleTrigger>
                    <CollapsibleContent>"Unavailable"</CollapsibleContent>
                </Collapsible>
            };
            let faq = accordion("faq")
                .value_signal(selected)
                .item("general", "General FAQ", || {
                    Element::new().text("Native disclosure controls")
                })
                .item_disabled("disabled", "Unavailable FAQ", || {
                    Element::new().text("Unavailable")
                })
                .item("privacy", "Privacy FAQ", move || {
                    input("privacy-notes")
                        .value_signal(draft)
                        .placeholder("Privacy notes")
                        .into()
                });
            (
                Element::new()
                    .flex_col()
                    .p(12.0)
                    .gap(8.0)
                    .child(content)
                    .child(preferences)
                    .child(disabled)
                    .child(faq),
                open,
                draft,
                selected,
            )
        });
        let mut root = NativeRoot::new(WindowsHost::new(
            "Jadix portable Native Collapsible and Accordion",
            [640, 520],
        )?);
        let projection = ElementProjection::with_lifetime(root.model().lifetime());
        let initial = render(&mut root, &projection, &mut element)?;
        root.host().show();
        if let Some((nonce, output)) = probe {
            let trigger = find(&initial, &|content| matches!(content, NodeContent::Button { label, .. } if label == "Account details")).ok_or("missing trigger")?;
            let trigger_id = trigger.id;
            if trigger.descriptor.accessible_expanded != Some(false)
                || find(&initial, &|content| {
                    matches!(content, NodeContent::Input { .. })
                })
                .is_some()
            {
                return Err("closed Collapsible mounted content or lost its expanded state".into());
            }
            let disabled = find(&initial, &|content| matches!(content, NodeContent::Button { label, .. } if label == "Disabled details")).ok_or("missing disabled trigger")?;
            let disabled_window = root
                .host()
                .control_identity(disabled.id)
                .ok_or("missing disabled native button")?;
            if unsafe {
                windows_sys::Win32::UI::Input::KeyboardAndMouse::IsWindowEnabled(
                    disabled_window as _,
                )
            } != 0
            {
                return Err("disabled Collapsible has an enabled native trigger".into());
            }
            root.host().activate(trigger_id)?;
            tick(&mut root, &projection, &mut element)?;
            let opened = render(&mut root, &projection, &mut element)?;
            let editor = find(&opened, &|content| {
                matches!(content, NodeContent::Input { .. })
            })
            .ok_or("open Collapsible has no editor")?
            .id;
            if !open.get() || find(&opened, &|content| matches!(content, NodeContent::Button { label, .. } if label == "Account details")).unwrap().descriptor.accessible_expanded != Some(true) {
                return Err("Collapsible did not expose its open state".into());
            }
            root.host().select_range(editor, [0, 4])?;
            root.host()
                .replace_selection(editor, &format!("{nonce} 中文😀"))?;
            tick(&mut root, &projection, &mut element)?;
            let committed = draft.get();
            if committed != format!("{nonce} 中文😀") {
                return Err("native edit did not commit to the Collapsible business signal".into());
            }
            render(&mut root, &projection, &mut element)?;
            root.focus(Some(editor))
                .map_err(|error| error.to_string())?;
            root.host().activate(trigger_id)?;
            tick(&mut root, &projection, &mut element)?;
            let closed = render(&mut root, &projection, &mut element)?;
            if open.get()
                || root.model().node(editor).is_some()
                || root.host().control_identity(editor).is_some()
                || find(&closed, &|content| {
                    matches!(content, NodeContent::Input { .. })
                })
                .is_some()
            {
                return Err("closed Collapsible retained the native editor".into());
            }
            if root.model().focused() != Some(trigger_id)
                || unsafe { windows_sys::Win32::UI::Input::KeyboardAndMouse::GetFocus() } as usize
                    != root
                        .host()
                        .control_identity(trigger_id)
                        .ok_or("missing Collapsible trigger")?
            {
                return Err("Collapsible close did not restore OS focus to its trigger".into());
            }
            root.host().activate(trigger_id)?;
            tick(&mut root, &projection, &mut element)?;
            let restored = render(&mut root, &projection, &mut element)?;
            let restored_editor = find(&restored, &|content| {
                matches!(content, NodeContent::Input { .. })
            })
            .ok_or("Collapsible did not restore its editor")?
            .id;
            if restored_editor == editor
                || root.host().text(restored_editor)? != committed
                || root.host().control_identity(trigger_id).is_none()
            {
                return Err("Collapsible remount lost text or reused the retired editor".into());
            }
            let preferences = find(&restored, &|content| matches!(content, NodeContent::Button { label, .. } if label == "Preferences"))
                .ok_or("missing grouped trigger")?.id;
            root.host().activate(preferences)?;
            tick(&mut root, &projection, &mut element)?;
            let grouped = render(&mut root, &projection, &mut element)?;
            if open.get() || root.host().control_identity(restored_editor).is_some()
                || find(&grouped, &|content| matches!(content, NodeContent::Input { .. })).is_some()
                || find(&grouped, &|content| matches!(content, NodeContent::Button { label, .. } if label == "Preferences"))
                    .unwrap().descriptor.accessible_expanded != Some(true)
            {
                return Err("exclusive group did not close and unmount the previous editor".into());
            }
            root.host().activate(trigger_id)?;
            tick(&mut root, &projection, &mut element)?;
            let grouped_restored = render(&mut root, &projection, &mut element)?;
            let grouped_editor = find(&grouped_restored, &|content| {
                matches!(content, NodeContent::Input { .. })
            })
            .ok_or("group switch did not restore editor")?
            .id;
            if grouped_editor == restored_editor || root.host().text(grouped_editor)? != committed
                || find(&grouped_restored, &|content| matches!(content, NodeContent::Button { label, .. } if label == "Preferences"))
                    .unwrap().descriptor.accessible_expanded != Some(false)
            {
                return Err("exclusive group remount lost text or kept both panels expanded".into());
            }
            let general = find(&grouped_restored, &|content| matches!(content, NodeContent::Button { label, .. } if label == "General FAQ"))
                .ok_or("missing general Accordion trigger")?.id;
            let privacy = find(&grouped_restored, &|content| matches!(content, NodeContent::Button { label, .. } if label == "Privacy FAQ"))
                .ok_or("missing privacy Accordion trigger")?.id;
            let unavailable = find(&grouped_restored, &|content| matches!(content, NodeContent::Button { label, .. } if label == "Unavailable FAQ"))
                .ok_or("missing disabled Accordion trigger")?.id;
            let unavailable_window = root
                .host()
                .control_identity(unavailable)
                .ok_or("missing disabled Accordion HWND")?;
            if unsafe {
                windows_sys::Win32::UI::Input::KeyboardAndMouse::IsWindowEnabled(
                    unavailable_window as _,
                )
            } != 0
            {
                return Err("disabled Accordion trigger is enabled".into());
            }
            root.focus(Some(general))
                .map_err(|error| error.to_string())?;
            for (source, key, target) in [
                (general, 0x28, privacy),
                (privacy, 0x28, general),
                (general, 0x26, privacy),
                (privacy, 0x24, general),
                (general, 0x23, privacy),
            ] {
                let source_window = root
                    .host()
                    .control_identity(source)
                    .ok_or("missing Accordion source HWND")?;
                if unsafe {
                    windows_sys::Win32::UI::WindowsAndMessaging::PostMessageW(
                        source_window as _,
                        windows_sys::Win32::UI::WindowsAndMessaging::WM_KEYDOWN,
                        key,
                        0,
                    )
                } == 0
                {
                    return Err("could not post Accordion navigation".into());
                }
                tick(&mut root, &projection, &mut element)?;
                render(&mut root, &projection, &mut element)?;
                let focused_window =
                    unsafe { windows_sys::Win32::UI::Input::KeyboardAndMouse::GetFocus() } as usize;
                if root.model().focused() != Some(target)
                    || root.host().control_identity(target) != Some(focused_window)
                    || !selected.get().is_empty()
                {
                    return Err("Accordion navigation did not focus the enabled header independently of selection".into());
                }
            }
            root.host().activate(privacy)?;
            tick(&mut root, &projection, &mut element)?;
            let accordion_open = render(&mut root, &projection, &mut element)?;
            if selected.get() != vec!["privacy"] || find(&accordion_open, &|content| matches!(content, NodeContent::Button { label, .. } if label == "Privacy FAQ"))
                .unwrap().descriptor.accessible_expanded != Some(true) {
                return Err("Accordion activation did not expand the focused header".into());
            }
            let privacy_editor = find(&accordion_open, &|content| matches!(content, NodeContent::Input { placeholder, .. } if placeholder == "Privacy notes"))
                .ok_or("missing Accordion editor")?.id;
            root.focus(Some(privacy_editor))
                .map_err(|error| error.to_string())?;
            root.host().activate(general)?;
            tick(&mut root, &projection, &mut element)?;
            let accordion_switched = render(&mut root, &projection, &mut element)?;
            if selected.get() != vec!["general"] || find(&accordion_switched, &|content| matches!(content, NodeContent::Button { label, .. } if label == "Privacy FAQ"))
                .unwrap().descriptor.accessible_expanded != Some(false) {
                return Err("single Accordion selection retained the previous panel".into());
            }
            if root.model().focused() != Some(general)
                || unsafe { windows_sys::Win32::UI::Input::KeyboardAndMouse::GetFocus() } as usize
                    != root
                        .host()
                        .control_identity(general)
                        .ok_or("missing Accordion trigger")?
            {
                return Err("Accordion stole focus from the activated header".into());
            }
            root.host().activate(privacy)?;
            tick(&mut root, &projection, &mut element)?;
            let reopened = render(&mut root, &projection, &mut element)?;
            let privacy_editor = find(&reopened, &|content| matches!(content, NodeContent::Input { placeholder, .. } if placeholder == "Privacy notes"))
                .ok_or("missing remounted Accordion editor")?.id;
            root.focus(Some(privacy_editor))
                .map_err(|error| error.to_string())?;
            tick(&mut root, &projection, &mut element)?;
            selected.set(vec!["general".into()]);
            render(&mut root, &projection, &mut element)?;
            if root.model().focused() != Some(privacy)
                || root.host().control_identity(privacy)
                    != Some(unsafe { windows_sys::Win32::UI::Input::KeyboardAndMouse::GetFocus() } as usize)
            {
                return Err("programmatic Accordion close did not restore OS focus".into());
            }
            root.host().activate(general)?;
            tick(&mut root, &projection, &mut element)?;
            render(&mut root, &projection, &mut element)?;
            if !selected.get().is_empty() {
                return Err("Accordion could not collapse all panels".into());
            }
            let before = root.host().stats().commits;
            render(&mut root, &projection, &mut element)?;
            render(&mut root, &projection, &mut element)?;
            if root.host().stats().commits != before {
                return Err("idle Collapsible committed native mutations".into());
            }
            let report = serde_json::json!({
                "schema": 1, "nonce": nonce, "application": "native-collapsible", "native_controls": true,
                "rust_rsx_compound": true, "lazy_native_mount": true, "disabled_native_trigger": true,
                "collapsed_editor_unmounted": true, "business_text_preserved": true,
                "remount_uses_fresh_identity": true, "expanded_descriptor_updated": true,
                "exclusive_group_switch": true, "group_editor_text_preserved": true,
                "accordion_keyboard_navigation": true, "accordion_navigation_keeps_open_state": true,
                "accordion_disabled_skipped": true, "accordion_single_selection": true,
                "accordion_collapse_all": true, "native_expand_collapse_pattern_accepted": false,
                "collapsible_close_focus_restored": true, "accordion_close_focus_restored": true,
                "accordion_activated_header_keeps_focus": true,
                "idle_native_commits": 0, "native": root.host().diagnostics(),
                "physical_keyboard_accepted": false, "physical_ime_accepted": false,
                "screen_reader_accepted": false, "native_undo_preserved_across_collapse": false
            });
            let mut file = std::fs::OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(output)
                .map_err(|error| error.to_string())?;
            file.write_all(&serde_json::to_vec_pretty(&report).map_err(|error| error.to_string())?)
                .map_err(|error| error.to_string())?;
            return Ok(());
        }
        loop {
            tick(&mut root, &projection, &mut element)?;
            if root.host().is_closed() {
                return Ok(());
            }
            render(&mut root, &projection, &mut element)?;
            root.host().wait_for_events(u32::MAX)?;
        }
    }
}

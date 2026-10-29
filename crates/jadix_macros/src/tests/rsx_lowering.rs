use super::*;

fn expand(src: &str) -> String {
    let root: RsxNode = syn::parse_str(src).unwrap_or_else(|e| panic!("{e} in {src}"));
    let mut c = 0usize;
    expand_node(&root, &mut c, false).to_string()
}

#[test]
fn sidebar_provider_without_class_types_option_string() {
    let s = expand(
        r#"<SidebarProvider id="gal-shell" variant="inset" defaultOpen={true}></SidebarProvider>"#,
    );
    assert!(
        s.contains("__jadix_sidebar_class"),
        "class chrome must bind a named Option<String>: {s}"
    );
    assert!(
        s.contains("Option < String >") || s.contains("Option<String>"),
        "untyped `if let Some = None` is E0282: {s}"
    );
}

#[test]
fn toaster_position_is_host_prop_not_css() {
    let s = expand(
        r#"<Toaster variant="stack" overflow="displace" limit={3} position="top-center"></Toaster>"#,
    );
    assert!(s.contains("position"), "{s}");
    assert!(
        !s.contains("b = b . position") && !s.contains("b = b.position"),
        "Toaster position must not go through BaseComponentBuilder: {s}"
    );
}

#[test]
fn match_key_emits_switch_reactive() {
    let s = expand("<Match key={tab.get()}>{ |k| k }</Match>");
    assert!(s.contains("switch_reactive"), "{s}");
    assert!(s.contains("Match #") || s.contains("Match#"), "{s}");
}

#[test]
fn match_scope_uses_explicit_anchor() {
    let s = expand(r#"<Match scope={"stable-island"} key={tab.get()}>{ |k| k }</Match>"#);
    assert!(s.contains("switch_reactive"), "{s}");
    assert!(s.contains("stable-island"), "{s}");
    assert!(!s.contains("Match#"), "{s}");
}

#[test]
fn match_id_alias_uses_explicit_anchor() {
    let s = expand(r#"<Match id={"stable-island"} key={tab.get()}>{ |k| k }</Match>"#);
    assert!(s.contains("switch_reactive"), "{s}");
    assert!(s.contains("stable-island"), "{s}");
}

#[test]
fn error_boundary_requires_fallback() {
    let s = expand("<ErrorBoundary><Box></Box></ErrorBoundary>");
    assert!(
        s.contains("compile_error"),
        "missing fallback must fail: {s}"
    );
}

#[test]
fn error_boundary_lowers_to_app_fn() {
    let s = expand(r#"<ErrorBoundary fallback={|m| m}><Box></Box></ErrorBoundary>"#);
    let compact = s.replace(' ', "");
    assert!(compact.contains("error_boundary"), "{s}");
    assert!(
        s.contains("\"ErrorBoundary\"") && compact.contains("format!"),
        "generated anchor: {s}"
    );
}

#[test]
fn error_boundary_explicit_id() {
    let s = expand(r#"<ErrorBoundary id={"eb"} fallback={|m| m}><Box></Box></ErrorBoundary>"#);
    let compact = s.replace(' ', "");
    assert!(compact.contains("error_boundary"), "{s}");
    assert!(s.contains("eb"), "{s}");
    assert!(
        !compact.contains("ErrorBoundary#"),
        "explicit id must not use file:line: {s}"
    );
}

#[test]
fn error_boundary_empty_id_generates_anchor() {
    let s = expand(r#"<ErrorBoundary id="" fallback={|m| m}><Box></Box></ErrorBoundary>"#);
    let compact = s.replace(' ', "");
    assert!(compact.contains("error_boundary"), "{s}");
    assert!(
        s.contains("\"ErrorBoundary\"") && compact.contains("format!"),
        "empty id must not be identity: {s}"
    );
}

#[test]
fn suspense_requires_fallback() {
    let s = expand("<Suspense><Box></Box></Suspense>");
    assert!(
        s.contains("compile_error"),
        "missing fallback must fail: {s}"
    );
}

#[test]
fn suspense_lowers_to_app_fn() {
    let s = expand(r#"<Suspense fallback={|| x}><Box></Box></Suspense>"#);
    let compact = s.replace(' ', "");
    assert!(compact.contains("suspense"), "{s}");
    assert!(
        s.contains("\"Suspense\"") && compact.contains("format!"),
        "generated anchor: {s}"
    );
}

#[test]
fn suspense_explicit_id() {
    let s = expand(r#"<Suspense id={"sus"} fallback={|| x}><Box></Box></Suspense>"#);
    let compact = s.replace(' ', "");
    assert!(compact.contains("suspense"), "{s}");
    assert!(s.contains("sus"), "{s}");
    assert!(
        !compact.contains("Suspense#"),
        "explicit id must not use file:line: {s}"
    );
}

#[test]
fn error_boundary_stmt_block_child() {
    let s = expand(
        r#"<ErrorBoundary fallback={|m| m}>{ throw_error("x"); Element::new() }</ErrorBoundary>"#,
    );
    let compact = s.replace(' ', "");
    assert!(compact.contains("error_boundary"), "{s}");
    assert!(compact.contains("throw_error"), "{s}");
}

#[test]
fn error_boundary_double_brace_stmt_block_child() {
    let s = expand(
        r#"<ErrorBoundary fallback={|m| m}>{ { throw_error("x"); Element::new() } }</ErrorBoundary>"#,
    );
    let compact = s.replace(' ', "");
    assert!(compact.contains("error_boundary"), "{s}");
    assert!(compact.contains("throw_error"), "{s}");
}

#[test]
fn sidebar_menu_button_is_active_get_emits_live_bind() {
    let s = expand(r#"<SidebarMenuButton isActive={tab.get() == "a"}></SidebarMenuButton>"#);
    assert!(s.contains("apply_bound_sidebar_menu_button_active"), "{s}");
    assert!(s.contains("bind_attr"), "{s}");
    assert!(s.contains("untrack"), "{s}");
}

#[test]
fn z_index_if_expr_emits_cast() {
    let s = expand(r#"<Box z_index={if hl { 1 } else { 0 }}></Box>"#);
    assert!(s.contains("z_index"), "{s}");
    assert!(s.contains("as i32"), "{s}");
}

#[test]
fn drag_reorder_handle_tuple_emits_builder() {
    let s = expand(r#"<Box drag_reorder_handle={(list_id, i)}></Box>"#);
    assert!(s.contains("drag_reorder_handle"), "{s}");
    assert!(s.contains("as usize"), "{s}");
}

#[test]
fn div_alias_lowers_to_box_primitive() {
    let from_div = expand(r#"<div w="2rem" id="shell">"hi"</div>"#);
    let from_box = expand(r#"<Box w="2rem" id="shell">"hi"</Box>"#);
    assert!(
        from_div.contains("primitives :: div"),
        "div must lower to primitives::div(); got {from_div}"
    );
    assert!(
        from_div.contains(". id") || from_div.contains(".id"),
        "div id must use BoxBuilder::id; got {from_div}"
    );
    assert_eq!(from_div, from_box, "div and Box must expand identically");
}

#[test]
fn div_pascal_alias_lowers_to_box_primitive() {
    let from_div = expand(r#"<Div h="1rem"></Div>"#);
    let from_box = expand(r#"<Box h="1rem"></Box>"#);
    assert!(from_div.contains("primitives :: div"), "{from_div}");
    assert_eq!(from_div, from_box);
}

#[test]
fn drag_reorder_handle_split_attrs_emit_builder() {
    let s = expand(r#"<Box drag_reorder_handle={list_id} drag_reorder_index={i}></Box>"#);
    assert!(s.contains("drag_reorder_handle"), "{s}");
    assert!(s.contains("as usize"), "{s}");
}

#[test]
fn drag_drop_provider_emits_drag_scope() {
    let s = expand(
        r#"<DragProvider id="kanban" collision="closest-center" onDragEnd={on_end}><Draggable id="a">"x"</Draggable></DragProvider>"#,
    );
    assert!(s.contains("drag_scope"), "{s}");
    assert!(s.contains("CollisionAlg"), "{s}");
    assert!(s.contains("on_drag_end"), "{s}");
    assert!(s.contains("draggable_from_rsx"), "{s}");
}

#[test]
fn droppable_and_overlay_emit_builders() {
    let s = expand(
        r#"<DragProvider id="p"><Droppable id="z" accept="card"><Label>"z"</Label></Droppable><DragOverlay><Label>"g"</Label></DragOverlay></DragProvider>"#,
    );
    assert!(s.contains("droppable"), "{s}");
    assert!(s.contains("drag_overlay"), "{s}");
    assert!(s.contains("child"), "{s}");
}

#[test]
fn droppable_overlap_and_hooks_emit_builder_chain() {
    let s = expand(
        r#"<DragProvider id="p"><Droppable id="z" accept="card" overlap="center" onDragEnter={on_ent} onDrop={on_dp}></Droppable></DragProvider>"#,
    );
    assert!(s.contains("DropOverlap"), "{s}");
    assert!(s.contains("on_drag_enter"), "{s}");
    assert!(s.contains("on_drop"), "{s}");
}

#[test]
fn drag_provider_snap_fn_emits_builder() {
    let s = expand(r#"<DragProvider id="p" snap_fn={f} snap={g} snap_targets={t}></DragProvider>"#);
    let compact = s.replace(' ', "");
    assert!(compact.contains("snap_fn"), "{s}");
    assert!(compact.contains("snap_targets"), "{s}");
    assert!(compact.contains(".snap(g)"), "{s}");
}

#[test]
fn box_opacity_accepts_percent_and_fraction() {
    let pct = expand(r#"<Box opacity="40%"></Box>"#);
    assert!(pct.contains("opacity"), "{pct}");
    let frac = expand(r#"<Box opacity={0.4}></Box>"#);
    assert!(frac.contains("opacity"), "{frac}");
}

#[test]
fn opacity_group_uses_framework_element_for_primitives_and_components() {
    for source in [
        r#"<div opacity_group={true} />"#,
        r#"<Button opacityGroup="true" />"#,
    ] {
        let expanded = expand(source);
        assert!(expanded.contains(". opacity_group"), "{expanded}");
        assert!(!expanded.contains("__jadix_unknown_attr"), "{expanded}");
    }
    for source in [
        r#"<Div opacity_group={enabled.get()} />"#,
        r#"<Button opacityGroup={enabled.get()} />"#,
    ] {
        let expanded = expand(source);
        assert!(expanded.contains("bind_opacity_group"), "{expanded}");
        assert!(expanded.contains("set_opacity_group"), "{expanded}");
    }
}

#[test]
fn box_opacity_get_lowers_to_bind_prop() {
    let s = expand(r#"<Box opacity={n.get()}></Box>"#);
    assert!(
        s.contains("bind_opacity"),
        "opacity={{n.get()}} must lower to a leaf bind, got {s}"
    );
    assert!(s.contains("apply_bound_opacity"), "{s}");
}

#[test]
fn empty_drag_overlay_emits_style_without_child_body() {
    let s = expand(
        r#"<DragProvider id="p"><DragOverlay opacity={0.9} class="shadow-lg"></DragOverlay></DragProvider>"#,
    );
    assert!(s.contains("drag_overlay"), "{s}");
    assert!(s.contains("style"), "{s}");
    assert!(s.contains("opacity"), "{s}");
    assert!(s.contains("with_class_list"), "{s}");
    assert!(
        !s.contains("drag_overlay_from_rsx"),
        "empty overlay must not inject a dummy child: {s}"
    );
}

#[test]
fn drag_overlay_source_opacity_emits_builder_call() {
    let s = expand(
        r#"<DragProvider id="p"><DragOverlay source_opacity={0}></DragOverlay></DragProvider>"#,
    );
    assert!(s.contains("source_opacity"), "{s}");
    assert!(!s.contains("hide_source"), "{s}");
}

#[test]
fn drag_overlay_rotate_emits_style() {
    let s =
        expand(r#"<DragProvider id="p"><DragOverlay rotate={45.0}></DragOverlay></DragProvider>"#);
    assert!(s.contains("rotate"), "{s}");
    assert!(s.contains("style"), "{s}");
}

#[test]
fn sortable_item_sets_also_droppable() {
    let s =
        expand(r#"<SortableItem id="row" index={i} list="todo"><Label>"r"</Label></SortableItem>"#);
    assert!(s.contains("also_droppable"), "{s}");
    assert!(s.contains("true"), "{s}");
    assert!(s.contains("draggable_from_rsx"), "{s}");
}

#[test]
fn drag_handle_emits_builder() {
    let s = expand(
        r#"<Draggable id="a" handle={true}><DragHandle><Label>"grip"</Label></DragHandle></Draggable>"#,
    );
    assert!(s.contains("handle_only"), "{s}");
    assert!(s.contains("drag_handle_from_rsx"), "{s}");
}

#[test]
fn draggable_stack_emits_drag_stack() {
    let s = expand(r#"<Draggable id="a" stack="keep"><Label>"x"</Label></Draggable>"#);
    assert!(s.contains("DragStack"), "{s}");
    assert!(s.contains("keep"), "{s}");
}

#[test]
fn draggable_restrict_emits_element_id() {
    let s = expand(r#"<Draggable id="a" restrict="corners-arena"><Label>"x"</Label></Draggable>"#);
    assert!(
        s.contains("Restrict") && s.contains("parse") && s.contains("corners-arena"),
        "{s}"
    );
}

#[test]
fn draggable_restrict_emits_parent() {
    let s = expand(r#"<Draggable id="a" restrict="parent"><Label>"x"</Label></Draggable>"#);
    assert!(
        s.contains("Restrict") && s.contains("parse") && s.contains("parent"),
        "{s}"
    );
}

#[test]
fn drag_ignore_emits_builder() {
    let s =
        expand(r#"<Draggable id="a"><DragIgnore><Button>"x"</Button></DragIgnore></Draggable>"#);
    assert!(s.contains("drag_ignore_from_rsx"), "{s}");
}

#[test]
fn drag_provider_emits_delay_and_autoscroll() {
    let s = expand(
        r#"<DragProvider id="p" delay_ms={200} empty_insert={12} autoscroll={false} canDrop={ok} onDragMove={mv}></DragProvider>"#,
    );
    assert!(s.contains("delay_ms"), "{s}");
    assert!(s.contains("empty_insert"), "{s}");
    assert!(s.contains("autoscroll"), "{s}");
    assert!(s.contains("can_drop"), "{s}");
    assert!(s.contains("on_drag_move"), "{s}");
}

#[test]
fn drag_provider_emits_animation() {
    let s = expand(r#"<DragProvider id="p" animation={false}></DragProvider>"#);
    assert!(s.contains("animation"), "{s}");
    assert!(s.contains("false"), "{s}");
}

#[test]
fn drag_provider_emits_sort_mode_and_multi_drag() {
    let s = expand(
        r#"<DragProvider id="p" sort_mode="swap" multi_drag={true} reorder_preview={true}></DragProvider>"#,
    );
    let compact = s.replace(' ', "");
    assert!(compact.contains("SortMode::parse"), "{s}");
    assert!(compact.contains("\"swap\""), "{s}");
    assert!(compact.contains("multi_drag(true)"), "{s}");
    assert!(compact.contains("reorder_preview(true)"), "{s}");
    let camel = expand(
        r#"<DragProvider id="p" sortMode="swap" multiDrag={true} reorderPreview={true}></DragProvider>"#,
    );
    let compact_camel = camel.replace(' ', "");
    assert!(compact_camel.contains("SortMode::parse"), "{camel}");
    assert!(compact_camel.contains("multi_drag(true)"), "{camel}");
    assert!(compact_camel.contains("reorder_preview(true)"), "{camel}");
}

#[test]
fn drag_provider_emits_scroll_lock() {
    let s = expand(r#"<DragProvider id="p" scroll_lock="all"></DragProvider>"#);
    assert!(s.contains("scroll_lock"), "{s}");
    assert!(s.contains("\"all\""), "{s}");
    let kebab = expand(r#"<DragProvider id="p" scroll-lock="x"></DragProvider>"#);
    assert!(kebab.contains("scroll_lock"), "{kebab}");
    assert!(kebab.contains("\"x\""), "{kebab}");
    let flag = expand(r#"<DragProvider id="p" scroll-lock-y></DragProvider>"#);
    assert!(flag.contains("scroll_lock"), "{flag}");
    assert!(flag.contains("\"y\""), "{flag}");
    let all_flag = expand(r#"<DragProvider id="p" scroll-lock-all></DragProvider>"#);
    assert!(all_flag.contains("\"all\""), "{all_flag}");
    let none = expand(r#"<DragProvider id="p" scroll-lock-none></DragProvider>"#);
    assert!(none.contains("\"none\""), "{none}");
    let auto = expand(r#"<DragProvider id="p" scroll-lock-auto></DragProvider>"#);
    assert!(auto.contains("\"auto\""), "{auto}");
}

#[test]
fn drag_provider_emits_scroll_sensitivity_and_speed() {
    let s =
        expand(r#"<DragProvider id="p" scroll_sensitivity={30} scroll_speed={12}></DragProvider>"#);
    assert!(s.contains("scroll_sensitivity"), "{s}");
    assert!(s.contains("scroll_speed"), "{s}");
    let camel =
        expand(r#"<DragProvider id="p" scrollSensitivity={28} scrollSpeed={10}></DragProvider>"#);
    assert!(camel.contains("scroll_sensitivity"), "{camel}");
    assert!(camel.contains("scroll_speed"), "{camel}");
}

#[test]
fn drag_provider_emits_debug_hits() {
    let s = expand(r#"<DragProvider id="p" debug_hits={true}></DragProvider>"#);
    let compact = s.replace(' ', "");
    assert!(compact.contains("debug_hits(true)"), "{s}");
}

#[test]
fn drag_provider_emits_inertia() {
    let s = expand(r#"<DragProvider id="p" inertia={true}></DragProvider>"#);
    let compact = s.replace(' ', "");
    assert!(compact.contains("inertia(true)"), "{s}");
}

#[test]
fn sortable_list_defaults_list_to_id() {
    let s = expand(r#"<SortableList id="todo"><Label>"x"</Label></SortableList>"#);
    let compact = s.replace(' ', "");
    assert!(compact.contains("droppable"), "{s}");
    assert!(compact.contains(".list("), "{s}");
}

#[test]
fn sortable_list_emits_direction() {
    let s = expand(r#"<SortableList id="todo" direction="x"><Label>"x"</Label></SortableList>"#);
    let compact = s.replace(' ', "");
    assert!(compact.contains("Axis::parse"), "{s}");
    assert!(compact.contains("direction"), "{s}");
}

#[test]
fn drag_overlay_emits_align() {
    let s = expand(r#"<DragOverlay align="center" />"#);
    let compact = s.replace(' ', "");
    assert!(compact.contains("OverlayAlign::parse"), "{s}");
}

#[test]
fn drag_provider_emits_activation() {
    let s = expand(r#"<DragProvider id="p" activation="or"></DragProvider>"#);
    let compact = s.replace(' ', "");
    assert!(compact.contains("Activation::parse"), "{s}");
}

#[test]
fn drag_provider_emits_restrict_overflow() {
    let s = expand(r#"<DragProvider id="p" restrict_overflow="hidden"></DragProvider>"#);
    let compact = s.replace(' ', "");
    assert!(compact.contains("RestrictOverflow::parse"), "{s}");
    assert!(compact.contains("restrict_overflow"), "{s}");
    let camel = expand(r#"<DragProvider id="p" restrictOverflow="hidden"></DragProvider>"#);
    assert!(
        camel.replace(' ', "").contains("RestrictOverflow::parse"),
        "{camel}"
    );
}

#[test]
fn drag_provider_emits_sensor() {
    let s = expand(r#"<DragProvider id="p" sensor_id="kbd" sensor={|_| false}></DragProvider>"#);
    let compact = s.replace(' ', "");
    assert!(compact.contains(".sensor("), "{s}");
}

#[test]
fn sortable_item_emits_pull_clone_and_data() {
    let s = expand(
        r#"<SortableItem id="t" pull_clone={true} skip_shift={true} data={std::rc::Rc::new(1i32)}><Label>"x"</Label></SortableItem>"#,
    );
    let compact = s.replace(' ', "");
    assert!(compact.contains("pull_clone:true"), "{s}");
    assert!(compact.contains("skip_shift:true"), "{s}");
    assert!(compact.contains("data:Some"), "{s}");
}

#[test]
fn match_when_emits_match_when_reactive() {
    let s = expand(
        r#"<Match fallback={fallback}><MatchArm when={ok.get()}><Box></Box></MatchArm></Match>"#,
    );
    assert!(s.contains("match_when_reactive"), "{s}");
}

#[test]
fn reactive_island_emits_bind_text() {
    let s = expand(r#"<Reactive id={"isle"}><Box>{ label.get() }</Box></Reactive>"#);
    assert!(s.contains("bind_text"), "{s}");
}

#[test]
fn unknown_attr_emits_bind_attr() {
    let s = expand("<Box data_status={status.get()} />");
    assert!(s.contains("bind_attr"), "{s}");
    assert!(s.contains("apply_bound_attr"), "{s}");
}

#[test]
fn animation_play_state_get_emits_bind_prop() {
    let s = expand(
        r#"<Box animation="1s linear infinite spin" animation_play_state={ if play.get() { "running" } else { "paused" } } />"#,
    );
    assert!(s.contains("bind_prop"), "{s}");
    assert!(s.contains("apply_bound_animation_play_state"), "{s}");
}

#[test]
fn for_key_fallback_emits_keyed_with_fallback() {
    let s = expand(
        r#"<For each={items.get()} key={*item} item={item} fallback={empty}><Box></Box></For>"#,
    );
    assert!(s.contains("for_keyed_reactive_with_fallback"), "{s}");
}

#[test]
fn with_emits_bind_text_peek_does_not() {
    let with_s = expand("<Box>{ label.with(|s| s.clone()) }</Box>");
    assert!(with_s.contains("bind_text"), "{with_s}");
    let peek_s = expand("<Box>{ label.peek() }</Box>");
    assert!(!peek_s.contains("bind_text"), "{peek_s}");
}

#[test]
fn sibling_children_emit_build_ui_child() {
    let s = expand("<HStack><Box></Box><Box></Box></HStack>");
    assert!(s.contains("build_ui_child"), "{s}");
}

#[test]
fn ui_macro_wraps_build_ui_root() {
    let expanded = quote! { jadix::core::tree::Element::new() };
    let hash = syn::LitStr::new("h", proc_macro2::Span::call_site());
    let s = wrap_ui_expansion(expanded, hash, "<Box />").to_string();
    assert!(s.contains("build_ui_root"), "{s}");
    assert!(s.contains("is_none"), "{s}");
}

#[test]
fn app_shell_props_cast_numeric_and_aliases() {
    let s = expand(
        r#"<App title="Hi" width={1280} height={800} font="Inter" font_size={18} font_weight={500}></App>"#,
    );
    assert!(s.contains("App"), "{s}");
    assert!(s.contains("title"), "{s}");
    assert!(s.contains("font_family") || s.contains("font"), "{s}");
    assert!(s.contains("as f64"), "{s}");
    assert!(s.contains("as u16"), "{s}");
    assert!(s.contains("width : Some"), "{s}");
    assert!(!s.contains("b . w (1280)"), "{s}");
    let camel = expand(r#"<App fontSize={20} fontFamily="Georgia"></App>"#);
    assert!(camel.contains("font_size"), "{camel}");
    assert!(camel.contains("font_family"), "{camel}");
}

#[test]
fn hover_card_delay_casts_unsuffixed_int_to_u64() {
    let s = expand(r#"<HoverCardTrigger openDelay={10} closeDelay={100}></HoverCardTrigger>"#);
    assert!(s.contains("as u64"), "{s}");
    assert!(s.contains("open_delay"), "{s}");
    assert!(s.contains("close_delay"), "{s}");
    // Must not rely on `.into()` alone (unsuffixed int → u64 fails).
    assert!(!s.contains("open_delay : Some ((10) . into ())"), "{s}");
}

#[test]
fn hover_card_side_offset_uses_into_f32() {
    let s = expand(r#"<HoverCardContent sideOffset={4}></HoverCardContent>"#);
    assert!(s.contains("IntoF32"), "{s}");
    assert!(s.contains("side_offset"), "{s}");
    let s_px = expand(r#"<HoverCardContent sideOffset="4px"></HoverCardContent>"#);
    assert!(s_px.contains("IntoF32"), "{s_px}");
    let s_rem = expand(r#"<HoverCardContent sideOffset="0.25"></HoverCardContent>"#);
    assert!(s_rem.contains("IntoF32"), "{s_rem}");
}

#[test]
fn bare_lit_number_and_bool_attrs_parse() {
    let s = expand(r#"<HoverCardTrigger openDelay=10 closeDelay=100></HoverCardTrigger>"#);
    assert!(s.contains("as u64"), "{s}");
    let s2 = expand(r#"<HoverCardContent sideOffset=4></HoverCardContent>"#);
    assert!(s2.contains("IntoF32"), "{s2}");
    let s3 = expand(r#"<Label font_weight=600 text_size="0.875"></Label>"#);
    assert!(
        s3.contains("font_weight (600)") || s3.contains("font_weight(600)"),
        "{s3}"
    );
    assert!(s3.contains("0.875"), "{s3}");
}

#[test]
fn length_attrs_use_into_f32_unitless_use_raw() {
    let blur = expand(r#"<DialogContent overlayBlur="4px" overlayOpacity=0.5></DialogContent>"#);
    assert!(blur.contains("IntoF32"), "{blur}");
    assert!(
        blur.contains("IntoF32Raw") || blur.contains("into_f32_raw"),
        "{blur}"
    );
    let h = expand(r#"<ResizablePanelGroup h="12rem"></ResizablePanelGroup>"#);
    assert!(h.contains("IntoF32"), "{h}");
    let lead = expand(r#"<Box leading=1.5></Box>"#);
    assert!(
        lead.contains("into_f32_raw") || lead.contains("IntoF32Raw"),
        "{lead}"
    );
}

#[test]
fn memo_and_store_field_get_emit_bind_text() {
    let memo = expand("<Box>{ memo.get() }</Box>");
    assert!(memo.contains("bind_text"), "{memo}");
    let field = expand("<Box>{ user.name.get() }</Box>");
    assert!(field.contains("bind_text"), "{field}");
    let paren = expand("<Box>{ (n.get()) }</Box>");
    assert!(paren.contains("bind_text"), "{paren}");
    let cast = expand("<Box>{ n.get() as i32 }</Box>");
    assert!(cast.contains("bind_text"), "{cast}");
    let recv_cast = expand("<Box>{ (n as _).get() }</Box>");
    assert!(
        recv_cast.contains("bind_text"),
        "cast receiver `(n as _).get()` must still lower: {recv_cast}"
    );
}

#[test]
fn unkeyed_tracked_for_emits_for_reactive_warn_path() {
    let s = expand(r#"<For each={items.get()} item={item}>{ item }</For>"#);
    assert!(
        s.contains("for_reactive"),
        "tracked <For> without key must lower to for_reactive: {s}"
    );
    assert!(
        !s.contains("for_keyed_reactive"),
        "unkeyed For must not take the item-owner path: {s}"
    );
}

#[test]
fn format_and_nested_if_text_emit_bind_text() {
    let fmt = expand(r#"<Box>{ format!("{}", n.get()) }</Box>"#);
    assert!(fmt.contains("bind_text"), "{fmt}");
    let nested = expand(r#"<Box>{ if ok { a.get() } else { b.get() } }</Box>"#);
    assert!(nested.contains("bind_text"), "{nested}");
}

#[test]
fn peek_get_untracked_untrack_and_closures_do_not_emit_bind() {
    assert!(!expand("<Box>{ n.peek() }</Box>").contains("bind_text"));
    assert!(!expand("<Box>{ n.get_untracked() }</Box>").contains("bind_text"));
    assert!(!expand("<Box>{ untrack(|| n.get()) }</Box>").contains("bind_text"));
    assert!(
        !expand("<Box>{ jadix::core::reactivity::untrack(|| n.get()) }</Box>")
            .contains("bind_text")
    );
    assert!(!expand("<Box>{ move || n.get() }</Box>").contains("bind_text"));
    let click = expand(r#"<Box on_click={move |_| n.get()}>"x"</Box>"#);
    assert!(
        !click.contains("bind_text"),
        "event-closure get must not emit a text leaf bind: {click}"
    );
}

#[test]
fn on_hover_maps_to_element() {
    let hover = expand(r#"<Box onHover={|inside| { let _ = inside; }}>x</Box>"#);
    assert!(
        hover.contains("on_hover"),
        "onHover must map to Element::on_hover: {hover}"
    );
}

#[test]
fn tracked_if_ui_child_emits_compile_error() {
    let s =
        expand(r#"<Box>{ if n.get() { ui!(<Box>"a"</Box>) } else { ui!(<Box>"b"</Box>) } }</Box>"#);
    assert!(
        s.contains("compile_error"),
        "structural if+ui! child must fail with Show/Match guidance: {s}"
    );
    assert!(s.contains("Show") || s.contains("Match"), "{s}");
}

#[test]
fn shader_texture_attr_calls_builder_slot0() {
    let s = expand(r#"<shader w={80.0} h={60.0} wgsl={"fn"} texture={"jadix_canvas:x"} />"#);
    assert!(
        s.contains("texture"),
        "shader texture attr must call ShaderBuilder::texture: {s}"
    );
    assert!(s.contains("src"), "{s}");
    assert!(s.contains("jadix_canvas:x"), "{s}");
}

#[test]
fn shader_src1_attr_calls_builder_slot1() {
    let s = expand(r#"<shader w={80.0} h={60.0} wgsl={"fn"} src1={"jadix_path:y"} />"#);
    assert!(s.contains("texture"), "{s}");
    assert!(s.contains("src1"), "{s}");
    assert!(s.contains("jadix_path:y"), "{s}");
}

#[test]
fn shader_src2_attr_calls_builder_slot2() {
    let s = expand(r#"<shader w={80.0} h={60.0} wgsl={"fn"} src2={"jadix_path:z"} />"#);
    assert!(s.contains("texture"), "{s}");
    assert!(s.contains("src2"), "{s}");
    assert!(s.contains("jadix_path:z"), "{s}");
}

#[test]
fn shader_preset_and_buffer_attrs_call_builder() {
    let s = expand(r#"<shader w={40.0} h={30.0} preset={"blur"} buffer={true} />"#);
    assert!(s.contains("preset"), "{s}");
    assert!(s.contains("buffer"), "{s}");
    assert!(s.contains("blur"), "{s}");
}

//! Product-specific DnD RSX lowering. Kept together for the jadix_dnd migration.

use super::*;

pub(super) fn supports(name: &str) -> bool {
    matches!(
        name,
        "DragProvider"
            | "Draggable"
            | "SortableItem"
            | "Droppable"
            | "SortableList"
            | "DragOverlay"
            | "DragHandle"
            | "DragIgnore"
    )
}

pub(super) fn expand(
    name_str: &str,
    el: &RsxElement,
    node_counter: &mut usize,
    in_reactive_boundary: bool,
) -> proc_macro2::TokenStream {
    let get_attr = |name: &str| -> Option<&Expr> {
        el.attributes
            .iter()
            .find(|(key, _)| key == &syn::Ident::new(name, proc_macro2::Span::call_site()))
            .map(|(_, value)| value)
    };
    if name_str == "DragProvider" {
        let id_expr = get_attr("id");
        let collision = get_attr("collision")
            .map(|e| quote! { (#e).to_string() })
            .unwrap_or_else(|| quote! { "most-intersecting".to_string() });
        let axis = get_attr("axis")
            .or_else(|| get_attr("direction"))
            .map(|e| quote! { (#e).to_string() })
            .unwrap_or_else(|| quote! { "xy".to_string() });
        let start_axis = get_attr("start_axis")
            .or_else(|| get_attr("startAxis"))
            .map(|e| quote! { __jadix_dnd::Axis::parse(&(#e)) })
            .unwrap_or_else(|| quote! { __jadix_dnd::Axis::Xy });
        let restrict_end_only = get_attr("restrict_end_only")
            .or_else(|| get_attr("restrictEndOnly"))
            .map(|e| quote! { #e })
            .unwrap_or_else(|| quote! { false });
        let restrict_anchor = get_attr("restrict_anchor")
            .or_else(|| get_attr("restrictAnchor"))
            .map(|e| quote! { __jadix_dnd::RestrictAnchor::parse(&(#e)) })
            .unwrap_or_else(|| quote! { __jadix_dnd::RestrictAnchor::FULL });
        let restrict_overflow = get_attr("restrict_overflow")
            .or_else(|| get_attr("restrictOverflow"))
            .map(|e| quote! { __jadix_dnd::RestrictOverflow::parse(&(#e)) })
            .unwrap_or_else(|| quote! { __jadix_dnd::RestrictOverflow::Visible });
        let threshold = get_attr("threshold")
            .map(|e| quote_unitless_f32(e))
            .unwrap_or_else(|| quote! { 4.0f32 });
        let restrict = get_attr("restrict")
            .map(|e| quote! { (#e).to_string() })
            .unwrap_or_else(|| quote! { String::new() });
        let delay_ms = get_attr("delay_ms")
            .or_else(|| get_attr("delayMs"))
            .map(|e| quote_unitless_f32(e))
            .unwrap_or_else(|| quote! { 0.0f32 });
        let activation = get_attr("activation")
            .map(|e| quote! { __jadix_dnd::Activation::parse(&(#e)) })
            .unwrap_or_else(|| quote! { __jadix_dnd::Activation::And });
        let empty_insert = get_attr("empty_insert")
            .or_else(|| get_attr("emptyInsert"))
            .map(|e| quote_unitless_f32(e))
            .unwrap_or_else(|| quote! { 8.0f32 });
        let autoscroll = get_attr("autoscroll")
            .map(|e| quote! { #e })
            .unwrap_or_else(|| quote! { true });
        let scroll_sensitivity =
            get_attr("scroll_sensitivity").or_else(|| get_attr("scrollSensitivity"));
        let scroll_speed = get_attr("scroll_speed").or_else(|| get_attr("scrollSpeed"));
        let scroll_lock = get_attr("scroll_lock")
            .or_else(|| get_attr("scrollLock"))
            .map(|e| quote! { (#e).to_string() })
            .unwrap_or_else(|| {
                if get_attr("scroll_lock_all").is_some() {
                    quote! { "all".to_string() }
                } else if get_attr("scroll_lock_x").is_some() {
                    quote! { "x".to_string() }
                } else if get_attr("scroll_lock_y").is_some() {
                    quote! { "y".to_string() }
                } else if get_attr("scroll_lock_none").is_some() {
                    quote! { "none".to_string() }
                } else if get_attr("scroll_lock_auto").is_some() {
                    quote! { "auto".to_string() }
                } else {
                    quote! { "auto".to_string() }
                }
            });
        let animation = get_attr("animation")
            .map(|e| quote! { #e })
            .unwrap_or_else(|| quote! { true });
        let sort_mode = get_attr("sort_mode")
            .or_else(|| get_attr("sortMode"))
            .map(|e| quote! { (#e).to_string() })
            .unwrap_or_else(|| quote! { "insert".to_string() });
        let multi_drag = get_attr("multi_drag")
            .or_else(|| get_attr("multiDrag"))
            .map(|e| quote! { #e })
            .unwrap_or_else(|| quote! { false });
        let reorder_preview = get_attr("reorder_preview")
            .or_else(|| get_attr("reorderPreview"))
            .map(|e| quote! { #e })
            .unwrap_or_else(|| quote! { false });
        let debug_hits = get_attr("debug_hits")
            .or_else(|| get_attr("debugHits"))
            .map(|e| quote! { #e })
            .unwrap_or_else(|| quote! { false });
        let inertia = get_attr("inertia")
            .map(|e| quote! { #e })
            .unwrap_or_else(|| quote! { false });
        let on_end = get_attr("on_drag_end").or_else(|| get_attr("onDragEnd"));
        let on_start = get_attr("on_drag_start").or_else(|| get_attr("onDragStart"));
        let on_over = get_attr("on_drag_over").or_else(|| get_attr("onDragOver"));
        let on_move = get_attr("on_drag_move").or_else(|| get_attr("onDragMove"));
        let can_drop = get_attr("can_drop").or_else(|| get_attr("canDrop"));
        let sensor_id = get_attr("sensor_id")
            .or_else(|| get_attr("sensorId"))
            .map(|e| quote! { (#e).to_string() })
            .unwrap_or_else(|| quote! { "sensor".to_string() });
        let sensor_fn = get_attr("sensor");
        let sensor_ts = if let Some(f) = sensor_fn {
            quote! { __b = __b.sensor(#sensor_id, #f); }
        } else {
            quote! {}
        };
        let idx = *node_counter;
        *node_counter += 1;
        let mut expanded_children = Vec::new();
        for child in &el.children {
            expanded_children.push(expand_node(child, node_counter, in_reactive_boundary));
        }
        let child_el = if expanded_children.is_empty() {
            quote! { jadix::core::tree::Element::new() }
        } else {
            quote! {
                {
                    let mut __list: Vec<jadix::core::tree::Element> = Vec::new();
                    #( __list.extend(jadix::core::tree::build_ui_child(|| (#expanded_children))); )*
                    let mut __frag = jadix::core::tree::Element::new();
                    __frag.set_attr("jadix_fragment", "1");
                    __frag.children(__list)
                }
            }
        };
        let id_ts = if let Some(id) = id_expr {
            quote! { (#id).to_string() }
        } else {
            quote! { format!("{}:{}:DragProvider#{}", file!(), line!(), #idx) }
        };
        let on_end_ts = if let Some(h) = on_end {
            quote! { __b = __b.on_drag_end(#h); }
        } else {
            quote! {}
        };
        let on_start_ts = if let Some(h) = on_start {
            quote! { __b = __b.on_drag_start(#h); }
        } else {
            quote! {}
        };
        let on_over_ts = if let Some(h) = on_over {
            quote! { __b = __b.on_drag_over(#h); }
        } else {
            quote! {}
        };
        let on_move_ts = if let Some(h) = on_move {
            quote! { __b = __b.on_drag_move(#h); }
        } else {
            quote! {}
        };
        let can_drop_ts = if let Some(h) = can_drop {
            quote! { __b = __b.can_drop(#h); }
        } else {
            quote! {}
        };
        let snap = get_attr("snap");
        let snap_targets = get_attr("snap_targets").or_else(|| get_attr("snapTargets"));
        let snap_fn = get_attr("snap_fn").or_else(|| get_attr("snapFn"));
        let snap_ts = if let Some(s) = snap {
            quote! { __b = __b.snap(#s); }
        } else {
            quote! {}
        };
        let snap_targets_ts = if let Some(t) = snap_targets {
            quote! { __b = __b.snap_targets(#t); }
        } else {
            quote! {}
        };
        let snap_fn_ts = if let Some(f) = snap_fn {
            quote! { __b = __b.snap_fn(#f); }
        } else {
            quote! {}
        };
        let scroll_sensitivity_ts = if let Some(v) = scroll_sensitivity {
            quote! { __b = __b.scroll_sensitivity(#v); }
        } else {
            quote! {}
        };
        let scroll_speed_ts = if let Some(v) = scroll_speed {
            quote! { __b = __b.scroll_speed(#v); }
        } else {
            quote! {}
        };
        return quote! {
            {
                let mut __b = __jadix_dnd::drag_scope(#id_ts);
                __b = __b.collision(__jadix_dnd::CollisionAlg::parse(&(#collision)));
                __b = __b.axis(__jadix_dnd::Axis::parse(&(#axis)));
                __b = __b.start_axis(#start_axis);
                __b = __b.threshold(#threshold);
                __b = __b.restrict(__jadix_dnd::Restrict::parse(&(#restrict)));
                __b = __b.restrict_end_only(#restrict_end_only);
                __b = __b.restrict_anchor(#restrict_anchor);
                __b = __b.restrict_overflow(#restrict_overflow);
                __b = __b.delay_ms(#delay_ms);
                __b = __b.activation(#activation);
                __b = __b.empty_insert(#empty_insert);
                __b = __b.autoscroll(#autoscroll);
                #scroll_sensitivity_ts
                #scroll_speed_ts
                __b = __b.scroll_lock(__jadix_dnd::ScrollLock::parse(&(#scroll_lock)));
                __b = __b.animation(#animation);
                __b = __b.sort_mode(__jadix_dnd::SortMode::parse(&(#sort_mode)));
                __b = __b.multi_drag(#multi_drag);
                __b = __b.reorder_preview(#reorder_preview);
                __b = __b.debug_hits(#debug_hits);
                __b = __b.inertia(#inertia);
                #snap_ts
                #snap_targets_ts
                #snap_fn_ts
                #on_start_ts
                #on_over_ts
                #on_move_ts
                #on_end_ts
                #can_drop_ts
                #sensor_ts
                __b.child(|| #child_el)
            }
        };
    }

    if name_str == "Draggable" || name_str == "SortableItem" {
        let id_expr = get_attr("id");
        let item_type = get_attr("item_type")
            .or_else(|| get_attr("itemType"))
            .map(|e| quote! { (#e).to_string() })
            .unwrap_or_else(|| quote! { String::new() });
        let group = get_attr("group")
            .map(|e| quote! { (#e).to_string() })
            .unwrap_or_else(|| quote! { String::new() });
        let list = get_attr("list")
            .map(|e| quote! { Some((#e).to_string()) })
            .unwrap_or_else(|| quote! { None });
        let index = get_attr("index")
            .map(|e| quote! { Some((#e) as usize) })
            .unwrap_or_else(|| quote! { None });
        let accept = get_attr("accept")
            .map(|e| quote! { (#e).to_string() })
            .unwrap_or_else(|| quote! { String::new() });
        let disabled = get_attr("disabled")
            .map(|e| quote! { #e })
            .unwrap_or_else(|| quote! { false });
        let handle_only = get_attr("handle")
            .or_else(|| get_attr("handle_only"))
            .or_else(|| get_attr("handleOnly"))
            .map(|e| quote! { #e })
            .unwrap_or_else(|| quote! { false });
        let opacity = get_attr("opacity")
            .map(|e| {
                quote! { Some(jadix::core::tree::IntoOpacity::into_opacity(#e)) }
            })
            .unwrap_or_else(|| quote! { None });
        let source_opacity = get_attr("source_opacity")
            .or_else(|| get_attr("sourceOpacity"))
            .map(|e| {
                quote! { Some(jadix::core::tree::IntoOpacity::into_opacity(#e)) }
            })
            .unwrap_or_else(|| quote! { None });
        let stack = get_attr("stack")
            .map(|e| quote! { __jadix_dnd::DragStack::from(#e) })
            .unwrap_or_else(|| quote! { __jadix_dnd::DragStack::Auto });
        let restrict = get_attr("restrict")
            .map(|e| {
                quote! { Some(__jadix_dnd::Restrict::parse(&(#e).to_string())) }
            })
            .unwrap_or_else(|| quote! { None });
        let skip_shift = get_attr("skip_shift")
            .or_else(|| get_attr("skipShift"))
            .map(|e| quote! { #e })
            .unwrap_or_else(|| quote! { false });
        let skip_follow = get_attr("skip_follow")
            .or_else(|| get_attr("skipFollow"))
            .map(|e| quote! { #e })
            .unwrap_or_else(|| quote! { false });
        let pull_clone = get_attr("pull_clone")
            .or_else(|| get_attr("pullClone"))
            .map(|e| quote! { #e })
            .unwrap_or_else(|| quote! { false });
        let data = get_attr("data")
            .map(|e| quote! { Some(#e) })
            .unwrap_or_else(|| quote! { None });
        let idx = *node_counter;
        *node_counter += 1;
        let mut expanded_children = Vec::new();
        for child in &el.children {
            expanded_children.push(expand_node(child, node_counter, true));
        }
        let child_el = if expanded_children.is_empty() {
            quote! { jadix::core::tree::Element::new() }
        } else {
            quote! {
                {
                    let mut __list: Vec<jadix::core::tree::Element> = Vec::new();
                    #( __list.extend(jadix::core::tree::build_ui_child(|| (#expanded_children))); )*
                    if __list.len() == 1 {
                        __list.remove(0)
                    } else {
                        let mut __v = jadix::primitives::vstack();
                        __v = __v.children(__list);
                        jadix::core::tree::Element::from(__v)
                    }
                }
            }
        };
        let id_ts = if let Some(id) = id_expr {
            quote! { (#id).to_string() }
        } else {
            quote! { format!("{}:{}:{}#{}", file!(), line!(), #name_str, #idx) }
        };
        let also = if name_str == "SortableItem" {
            quote! { true }
        } else {
            quote! { false }
        };
        return quote! {
            __jadix_dnd::draggable_from_rsx(
                __jadix_dnd::DraggableRsxOpts {
                    id: #id_ts,
                    item_type: #item_type,
                    group: #group,
                    list: #list,
                    index: #index,
                    disabled: #disabled,
                    handle_only: #handle_only,
                    also_droppable: #also,
                    accept: #accept,
                    opacity: #opacity,
                    source_opacity: #source_opacity,
                    stack: #stack,
                    restrict: #restrict,
                    skip_shift: #skip_shift,
                    skip_follow: #skip_follow,
                    pull_clone: #pull_clone,
                    data: #data,
                },
                move || #child_el,
            )
        };
    }

    if name_str == "Droppable" || name_str == "SortableList" {
        let id_expr = get_attr("id");
        let accept = get_attr("accept")
            .map(|e| quote! { (#e).to_string() })
            .unwrap_or_else(|| quote! { String::new() });
        let group = get_attr("group")
            .map(|e| quote! { (#e).to_string() })
            .unwrap_or_else(|| quote! { String::new() });
        let disabled = get_attr("disabled")
            .map(|e| quote! { #e })
            .unwrap_or_else(|| quote! { false });
        let overlap_attr = get_attr("overlap");
        let overlap_ts = if let Some(e) = overlap_attr {
            quote! {
                __db = __db.overlap(
                    __jadix_dnd::DropOverlap::parse(&(#e).to_string()),
                );
            }
        } else {
            quote! {}
        };
        let drop_checker = get_attr("drop_checker").or_else(|| get_attr("dropChecker"));
        let on_drop_activate = get_attr("on_drop_activate").or_else(|| get_attr("onDropActivate"));
        let on_drop_deactivate =
            get_attr("on_drop_deactivate").or_else(|| get_attr("onDropDeactivate"));
        let on_drag_enter = get_attr("on_drag_enter").or_else(|| get_attr("onDragEnter"));
        let on_drag_leave = get_attr("on_drag_leave").or_else(|| get_attr("onDragLeave"));
        let on_drop_move = get_attr("on_drop_move").or_else(|| get_attr("onDropMove"));
        let on_drop = get_attr("on_drop").or_else(|| get_attr("onDrop"));
        let drop_checker_ts = if let Some(f) = drop_checker {
            quote! { __db = __db.drop_checker(std::rc::Rc::new(#f)); }
        } else {
            quote! {}
        };
        let on_drop_activate_ts = if let Some(h) = on_drop_activate {
            quote! { __db = __db.on_drop_activate(#h); }
        } else {
            quote! {}
        };
        let on_drop_deactivate_ts = if let Some(h) = on_drop_deactivate {
            quote! { __db = __db.on_drop_deactivate(#h); }
        } else {
            quote! {}
        };
        let on_drag_enter_ts = if let Some(h) = on_drag_enter {
            quote! { __db = __db.on_drag_enter(#h); }
        } else {
            quote! {}
        };
        let on_drag_leave_ts = if let Some(h) = on_drag_leave {
            quote! { __db = __db.on_drag_leave(#h); }
        } else {
            quote! {}
        };
        let on_drop_move_ts = if let Some(h) = on_drop_move {
            quote! { __db = __db.on_drop_move(#h); }
        } else {
            quote! {}
        };
        let on_drop_ts = if let Some(h) = on_drop {
            quote! { __db = __db.on_drop(#h); }
        } else {
            quote! {}
        };
        let list_attr = get_attr("list");
        let list_ts = if let Some(e) = list_attr {
            quote! { __db = __db.list((#e).to_string()); }
        } else if name_str == "SortableList" {
            if let Some(id) = get_attr("id") {
                quote! { __db = __db.list((#id).to_string()); }
            } else {
                quote! {}
            }
        } else {
            quote! {}
        };
        let opacity_attr = get_attr("opacity");
        let opacity_ts = if let Some(e) = opacity_attr {
            quote! {
                __db = __db.opacity(
                    jadix::core::tree::IntoOpacity::into_opacity(#e),
                );
            }
        } else {
            quote! {}
        };
        let direction_attr = get_attr("direction");
        let direction_ts = if let Some(e) = direction_attr {
            quote! {
                __db = __db.direction(__jadix_dnd::Axis::parse(&(#e)));
            }
        } else {
            quote! {}
        };
        let idx = *node_counter;
        *node_counter += 1;
        let mut expanded_children = Vec::new();
        for child in &el.children {
            expanded_children.push(expand_node(child, node_counter, true));
        }
        let child_el = if expanded_children.is_empty() {
            quote! { jadix::core::tree::Element::new() }
        } else {
            quote! {
                {
                    let mut __list: Vec<jadix::core::tree::Element> = Vec::new();
                    #( __list.extend(jadix::core::tree::build_ui_child(|| (#expanded_children))); )*
                    if __list.len() == 1 {
                        __list.remove(0)
                    } else {
                        let mut __v = jadix::primitives::vstack();
                        __v = __v.children(__list);
                        jadix::core::tree::Element::from(__v)
                    }
                }
            }
        };
        let id_ts = if let Some(id) = id_expr {
            quote! { (#id).to_string() }
        } else {
            quote! { format!("{}:{}:{}#{}", file!(), line!(), #name_str, #idx) }
        };
        return quote! {
            {
                let mut __db = __jadix_dnd::droppable(#id_ts)
                    .accept(#accept)
                    .group(#group)
                    .disabled(#disabled);
                #list_ts
                #opacity_ts
                #direction_ts
                #overlap_ts
                #drop_checker_ts
                #on_drop_activate_ts
                #on_drop_deactivate_ts
                #on_drag_enter_ts
                #on_drag_leave_ts
                #on_drop_move_ts
                #on_drop_ts
                __db.child(move || #child_el)
            }
        };
    }

    if name_str == "DragOverlay" {
        let mut expanded_children = Vec::new();
        for child in &el.children {
            expanded_children.push(expand_node(child, node_counter, true));
        }
        let child_ts = if expanded_children.is_empty() {
            quote! {}
        } else {
            quote! {
                .child(move || {
                    let mut __list: Vec<jadix::core::tree::Element> = Vec::new();
                    #( __list.extend(jadix::core::tree::build_ui_child(|| (#expanded_children))); )*
                    if __list.len() == 1 {
                        __list.remove(0)
                    } else {
                        let mut __v = jadix::primitives::vstack();
                        __v = __v.children(__list);
                        jadix::core::tree::Element::from(__v)
                    }
                })
            }
        };
        let mut style_stmts = Vec::new();
        if let Some(class) = get_attr("class")
            .or_else(|| get_attr("className"))
            .or_else(|| get_attr("tw"))
            .or_else(|| get_attr("uno"))
        {
            style_stmts.push(quote_apply_class_list(class));
        }
        if let Some(op) = get_attr("opacity") {
            style_stmts.push(quote! {
                __jadix_el = __jadix_el.opacity(#op);
            });
        }
        if let Some(rot) = get_attr("rotate") {
            style_stmts.push(quote! {
                __jadix_el = __jadix_el.rotate(#rot);
            });
        }
        if let Some(bg) = get_attr("bg") {
            style_stmts.push(quote! {
                __jadix_el = __jadix_el.bg(jadix::core::tree::parse_color(#bg));
            });
        }
        if let Some(rounded) = get_attr("rounded")
            .or_else(|| get_attr("radius"))
            .or_else(|| get_attr("border_radius"))
            .or_else(|| get_attr("borderRadius"))
        {
            style_stmts.push(quote! {
                __jadix_el = __jadix_el.rounded(jadix::core::tree::IntoF32::into_f32(#rounded));
            });
        }
        if let Some(shadow) = get_attr("box_shadow").or_else(|| get_attr("boxShadow")) {
            style_stmts.push(quote! {
                __jadix_el = __jadix_el.box_shadow(#shadow);
            });
        }
        if let Some(p) = get_attr("p") {
            style_stmts.push(quote! {
                __jadix_el = __jadix_el.p(#p);
            });
        }
        if let Some(w) = get_attr("w").or_else(|| get_attr("width")) {
            style_stmts.push(quote! {
                __jadix_el = __jadix_el.w(#w);
            });
        }
        if let Some(h) = get_attr("h").or_else(|| get_attr("height")) {
            style_stmts.push(quote! {
                __jadix_el = __jadix_el.h(#h);
            });
        }
        let style_ts = if style_stmts.is_empty() {
            quote! {}
        } else {
            quote! {
                .style(move |__jadix_el| {
                    let mut __jadix_el = __jadix_el;
                    #(#style_stmts)*
                    __jadix_el
                })
            }
        };
        let source_opacity_ts =
            if let Some(v) = get_attr("source_opacity").or_else(|| get_attr("sourceOpacity")) {
                quote! { .source_opacity(#v) }
            } else {
                quote! {}
            };
        let align_ts = if let Some(v) = get_attr("align") {
            quote! { .align(__jadix_dnd::OverlayAlign::parse(&(#v))) }
        } else {
            quote! {}
        };
        return quote! {
            jadix::core::tree::Element::from(
                __jadix_dnd::drag_overlay()
                    #child_ts
                    #style_ts
                    #source_opacity_ts
                    #align_ts
            )
        };
    }

    if name_str == "DragHandle" {
        let mut expanded_children = Vec::new();
        for child in &el.children {
            expanded_children.push(expand_node(child, node_counter, true));
        }
        let child_el = if expanded_children.is_empty() {
            quote! { jadix::core::tree::Element::new() }
        } else {
            quote! {
                {
                    let mut __list: Vec<jadix::core::tree::Element> = Vec::new();
                    #( __list.extend(jadix::core::tree::build_ui_child(|| (#expanded_children))); )*
                    if __list.len() == 1 {
                        __list.remove(0)
                    } else {
                        let mut __v = jadix::primitives::vstack();
                        __v = __v.children(__list);
                        jadix::core::tree::Element::from(__v)
                    }
                }
            }
        };
        return quote! {
            __jadix_dnd::drag_handle_from_rsx(|| #child_el)
        };
    }

    if name_str == "DragIgnore" {
        let mut expanded_children = Vec::new();
        for child in &el.children {
            expanded_children.push(expand_node(child, node_counter, true));
        }
        let child_el = if expanded_children.is_empty() {
            quote! { jadix::core::tree::Element::new() }
        } else {
            quote! {
                {
                    let mut __list: Vec<jadix::core::tree::Element> = Vec::new();
                    #( __list.extend(jadix::core::tree::build_ui_child(|| (#expanded_children))); )*
                    if __list.len() == 1 {
                        __list.remove(0)
                    } else {
                        let mut __v = jadix::primitives::vstack();
                        __v = __v.children(__list);
                        jadix::core::tree::Element::from(__v)
                    }
                }
            }
        };
        return quote! {
            __jadix_dnd::drag_ignore_from_rsx(|| #child_el)
        };
    }

    unreachable!("DnD tag dispatch requires supports(name)")
}

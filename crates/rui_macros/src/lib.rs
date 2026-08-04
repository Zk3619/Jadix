use std::sync::atomic::{AtomicUsize, Ordering};

use proc_macro::TokenStream;
use quote::quote;
use syn::parse::{Parse, ParseStream};
use syn::{Expr, Ident, Token, braced};

/// RSX 中省略 `id` 的 `<Input>`：编译期为每个标签分配序号，生成稳定字面量，避免多个 Input 共用空 `id` 导致信号冲突。
static INPUT_RSX_ANON_SEQ: AtomicUsize = AtomicUsize::new(0);

fn expr_as_u16(value: &Expr) -> Option<u16> {
    match value {
        Expr::Lit(l) => match &l.lit {
            syn::Lit::Int(i) => i.base10_parse().ok(),
            syn::Lit::Str(s) => s.value().parse().ok(),
            _ => None,
        },
        _ => None,
    }
}

struct RsxElement {
    name: Ident,
    attributes: Vec<(Ident, Expr)>,
    children: Vec<RsxNode>,
}

enum RsxNode {
    Element(RsxElement),
    Expr(Expr),
}

impl Parse for RsxNode {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        // If it starts with `{`, it's an expression
        if input.peek(syn::token::Brace) {
            let content;
            braced!(content in input);
            let expr: Expr = content.parse()?;
            return Ok(RsxNode::Expr(expr));
        }

        if input.peek(syn::LitStr) {
            let lit: syn::LitStr = input.parse()?;
            let expr = syn::Expr::Lit(syn::ExprLit { attrs: Vec::new(), lit: syn::Lit::Str(lit) });
            return Ok(RsxNode::Expr(expr));
        }

        // Otherwise it must be `<`
        input.parse::<Token![<]>()?;
        let name: Ident = input.parse()?;

        let mut attributes = Vec::new();
        while !input.peek(Token![>]) && !input.peek(Token![/]) {
            let attr_name: Ident = if input.peek(Token![in]) {
                input.parse::<Token![in]>()
                    .map_err(|e| syn::Error::new(e.span(), format!("Failed at keyword attr_name {}", e)))?;
                syn::Ident::new("in", proc_macro2::Span::call_site())
            } else if input.peek(Token![type]) {
                // HTML-like `type="password"`；`type` 是关键字，需单独吃掉 token。
                input.parse::<Token![type]>()
                    .map_err(|e| syn::Error::new(e.span(), format!("Failed at keyword attr type: {}", e)))?;
                syn::Ident::new("type", proc_macro2::Span::call_site())
            } else {
                input.parse().map_err(|e| syn::Error::new(e.span(), format!("Failed at attr_name {}", e)))?
            };
            input.parse::<Token![=]>().map_err(|e| syn::Error::new(e.span(), format!("Failed at = after attr {}", attr_name)))?;
            
            let attr_value: Expr = if input.peek(syn::token::Brace) {
                let content;
                braced!(content in input);
                content.parse().map_err(|e| syn::Error::new(e.span(), format!("Failed to parse braced attribute '{}': {}", attr_name, e)))?
            } else {
                let lit: syn::Lit = input.parse().map_err(|e| syn::Error::new(e.span(), format!("Expected literal: {}", e)))?;
                syn::Expr::Lit(syn::ExprLit { attrs: Vec::new(), lit })
            };
            
            attributes.push((attr_name, attr_value));
        }

        let mut children = Vec::new();

        if input.peek(Token![/]) {
            // Self closing: `/>`
            input.parse::<Token![/]>().unwrap();
            input.parse::<Token![>]>().unwrap();
        } else {
            // Open tag: `>`
            input.parse::<Token![>]>().unwrap();

            // Read children until `</`
            loop {
                if input.is_empty() {
                    break;
                }
                
                let fork = input.fork();
                if fork.parse::<Token![<]>().is_ok() {
                    if fork.parse::<Token![/]>().is_ok() {
                        break;
                    }
                }
                children.push(input.parse::<RsxNode>().map_err(|e| syn::Error::new(e.span(), format!("Failed at child: {}", e)))?);
            }

            // Close tag: `</Name>`
            input.parse::<Token![<]>().map_err(|e| syn::Error::new(e.span(), "Expected < for close tag"))?;
            input.parse::<Token![/]>().map_err(|e| syn::Error::new(e.span(), "Expected / for close tag"))?;
            let close_name: Ident = input.parse().map_err(|e| syn::Error::new(e.span(), "Expected identifier for close tag"))?;
            if close_name != name {
                return Err(syn::Error::new(close_name.span(), "Tag name mismatch"));
            }
            input.parse::<Token![>]>().map_err(|e| syn::Error::new(e.span(), "Expected > for close tag"))?;
        }

        Ok(RsxNode::Element(RsxElement {
            name,
            attributes,
            children,
        }))
    }
}

/// A procedural macro that parses an HTML-like block (RSX) and converts it 
/// into `rui::core::tree::Element` builder calls.
#[proc_macro]
pub fn ui(input: TokenStream) -> TokenStream {
    let raw_input = input.to_string();
    let template_hash = stable_template_hash(&raw_input);
    match syn::parse::<RsxNode>(input) {
        Ok(root) => {
            let mut node_counter = 0usize;
            let expanded = expand_node(&root, &mut node_counter);
            let template_hash_lit = syn::LitStr::new(&template_hash, proc_macro2::Span::call_site());
            
            quote! {
                {
                    let __rui_root: rui::core::tree::Element = (#expanded).into();
                    let __rui_root = __rui_root.template_hash(#template_hash_lit);
                    let __rui_root = __rui_root.template_id(format!("{}:{}:{}", file!(), line!(), column!()));
                    __rui_root
                }
            }.into()
        }
        Err(err) => {
            let e = err.to_string();
            quote! { compile_error!(#e) }.into()
        }
    }
}

/// A marker attribute for functional components.
/// It automatically generates a Props struct based on the function arguments.
#[proc_macro_attribute]
pub fn component(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let input = syn::parse_macro_input!(item as syn::ItemFn);
    let vis = &input.vis;
    let name = &input.sig.ident;
    let props_name = syn::Ident::new(&format!("{}Props", name), name.span());

    let mut fields = Vec::new();
    let mut field_names = Vec::new();
    let mut field_unwrapping = Vec::new();
    let mut has_base_builder_arg = false;

    let mut crate_path = quote! { rui };
    let attr_str = _attr.to_string();
    if attr_str.contains("crate_path") {
        if let Ok(meta) = syn::parse::<syn::Meta>(_attr) {
            if let syn::Meta::NameValue(nv) = meta {
                if nv.path.is_ident("crate_path") {
                    if let syn::Expr::Lit(syn::ExprLit { lit: syn::Lit::Str(s), .. }) = nv.value {
                        let path: syn::Path = s.parse().unwrap();
                        crate_path = quote! { #path };
                    }
                }
            }
        }
    }

    for arg in &input.sig.inputs {
        if let syn::FnArg::Typed(pat_type) = arg {
            if let syn::Pat::Ident(pat_ident) = &*pat_type.pat {
                let name = &pat_ident.ident;
                let ty = &pat_type.ty;
                let name_str = name.to_string();
                let ty_str = quote!(#ty).to_string();
                let is_already_option = ty_str.starts_with("Option");
                if name_str == "base_builder" {
                    has_base_builder_arg = true;
                }
                
                field_names.push(name.clone());
                
                if name_str == "id" {
                    // id is always a String, never optional
                    fields.push(quote! { pub #name: String });
                    field_unwrapping.push(quote! { let #name = #name; });
                } else if is_already_option {
                    // Already Option<T>, keep as-is
                    fields.push(quote! { pub #name: #ty });
                    let mut_tok = if pat_ident.mutability.is_some() { quote! { mut } } else { quote! {} };
                    field_unwrapping.push(quote! { let #mut_tok #name = #name; });
                } else {
                    // Wrap in Option<T>
                    fields.push(quote! { pub #name: Option<#ty> });
                    let mut_tok = if pat_ident.mutability.is_some() { quote! { mut } } else { quote! {} };
                    field_unwrapping.push(quote! { let #mut_tok #name = #name.unwrap_or_default(); });
                }
            }
        }
    }

    // Add children if not present
    if !field_names.iter().any(|n| n.to_string() == "children") {
        fields.push(quote! { pub children: Option<#crate_path::core::tree::Element> });
        field_names.push(syn::Ident::new("children", proc_macro2::Span::call_site()));
        field_unwrapping.push(quote! { let children = children; });
    }

    // Add base prop for styling/layout
    fields.push(quote! { pub base: Option<Box<dyn FnOnce(#crate_path::components::base::BaseComponentBuilder) -> #crate_path::components::base::BaseComponentBuilder>> });
    field_names.push(syn::Ident::new("base", proc_macro2::Span::call_site()));
    field_unwrapping.push(quote! { let base = base; });

    let body = &input.block;

    let merge_base_stmt = if has_base_builder_arg {
        quote! {
            let base_builder = match (base_builder, base) {
                (Some(inner), Some(outer)) => Some(Box::new(move |b: #crate_path::components::base::BaseComponentBuilder| {
                    let b = inner(b);
                    outer(b)
                }) as Box<dyn FnOnce(#crate_path::components::base::BaseComponentBuilder) -> #crate_path::components::base::BaseComponentBuilder>),
                (Some(inner), None) => Some(inner),
                (None, Some(outer)) => Some(outer),
                (None, None) => None,
            };
        }
    } else {
        quote! {}
    };
    let post_apply_base_stmt = if has_base_builder_arg {
        quote! {}
    } else {
        quote! {
            if let Some(base_fn) = base {
                let builder = #crate_path::components::base::BaseComponentBuilder::new(__el);
                __el = base_fn(builder).el;
            }
        }
    };

    let expanded = quote! {
        #[derive(Default)]
        #vis struct #props_name {
            #(#fields),*
        }

        #vis fn #name(props: #props_name) -> #crate_path::core::tree::Element {
            #vis use #props_name;
            let #props_name { #(#field_names),* } = props;
            #(#field_unwrapping)*
            #merge_base_stmt
            let mut __el = { #body };
            #post_apply_base_stmt
            __el
        }
    };

    expanded.into()
}

fn normalize_template_source(source: &str) -> String {
    if let Ok(ts) = source.parse::<proc_macro2::TokenStream>() {
        return ts.to_string();
    }
    let mut out = String::with_capacity(source.len());
    let mut prev_ws = false;
    for ch in source.chars() {
        if ch.is_whitespace() {
            if !prev_ws {
                out.push(' ');
                prev_ws = true;
            }
        } else {
            out.push(ch);
            prev_ws = false;
        }
    }
    out.trim().to_string()
}

fn stable_template_hash(source: &str) -> String {
    let normalized = normalize_template_source(source);
    let mut hash: u64 = 0xcbf29ce484222325;
    for &b in normalized.as_bytes() {
        hash ^= b as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}

fn expand_node(node: &RsxNode, node_counter: &mut usize) -> proc_macro2::TokenStream {
    match node {
        RsxNode::Expr(expr) => {
            // Usually an expression child evaluates to something that can be added to the tree
            quote! { #expr }
        }
        RsxNode::Element(el) => {
            let name_str = el.name.to_string();
            if name_str == "Reactive" || name_str == "Show" || name_str == "Popover" || name_str == "ForKeyed" || name_str == "For" {
                let get_attr = |name: &str| -> Option<&Expr> {
                    el.attributes
                        .iter()
                        .find(|(k, _)| k == &syn::Ident::new(name, proc_macro2::Span::call_site()))
                        .map(|(_, v)| v)
                };

                if name_str == "Reactive" {
                    let Some(id_expr) = get_attr("id") else {
                        return quote! { compile_error!("<Reactive> requires `id={...}` attribute"); };
                    };
                    let mut expanded_children = Vec::new();
                    for child in &el.children {
                        expanded_children.push(expand_node(child, node_counter));
                    }
                    let child_el = if expanded_children.is_empty() {
                        quote! { rui::core::tree::Element::new() }
                    } else {
                        quote! {
                            {
                                use rui::core::tree::IntoElementList;
                                let mut __list: Vec<rui::core::tree::Element> = Vec::new();
                                #( __list.extend((#expanded_children).into_element_list()); )*
                                if __list.len() == 1 {
                                    __list.remove(0)
                                } else {
                                    let mut __v = rui::primitives::vstack();
                                    __v = __v.children(__list);
                                    rui::core::tree::Element::from(__v)
                                }
                            }
                        }
                    };
                    return quote! {
                        rui::reactive!(#id_expr, move || { #child_el })
                    };
                }

                if name_str == "Show" {
                    let Some(when_expr) = get_attr("when") else {
                        return quote! { compile_error!("<Show> requires `when={...}` attribute"); };
                    };
                    let mut expanded_children = Vec::new();
                    for child in &el.children {
                        expanded_children.push(expand_node(child, node_counter));
                    }
                    let child_el = if expanded_children.is_empty() {
                        quote! { rui::core::tree::Element::new() }
                    } else {
                        quote! {
                            {
                                use rui::core::tree::IntoElementList;
                                let mut __list: Vec<rui::core::tree::Element> = Vec::new();
                                #( __list.extend((#expanded_children).into_element_list()); )*
                                if __list.len() == 1 {
                                    __list.remove(0)
                                } else {
                                    let mut __v = rui::primitives::vstack();
                                    __v = __v.children(__list);
                                    rui::core::tree::Element::from(__v)
                                }
                            }
                        }
                    };
                    let idx = *node_counter;
                    *node_counter += 1;
                    return quote! {
                        rui::app::reactive::show_scoped(
                            format!("{}:{}:Show#{}", file!(), line!(), #idx),
                            #when_expr,
                            || { #child_el }
                        )
                    };
                }

                if name_str == "Popover" {
                    let Some(open_expr) = get_attr("open") else {
                        return quote! { compile_error!("<Popover> requires `open={...}` (`rui::Signal<bool>`)"); };
                    };
                    let mut expanded_children = Vec::new();
                    for child in &el.children {
                        expanded_children.push(expand_node(child, node_counter));
                    }
                    let child_el = if expanded_children.is_empty() {
                        quote! { rui::core::tree::Element::new() }
                    } else {
                        quote! {
                            {
                                use rui::core::tree::IntoElementList;
                                let mut __list: Vec<rui::core::tree::Element> = Vec::new();
                                #( __list.extend((#expanded_children).into_element_list()); )*
                                if __list.len() == 1 {
                                    __list.remove(0)
                                } else {
                                    let mut __v = rui::primitives::vstack();
                                    __v = __v.children(__list);
                                    rui::core::tree::Element::from(__v)
                                }
                            }
                        }
                    };
                    return quote! {
                        rui::core::tree::Element::from(
                            rui::components::popover_panel(#open_expr).child(#child_el)
                        )
                    };
                }

                // For / ForKeyed
                let scope_ts = if let Some(scope_expr) = get_attr("scope") {
                    quote! { #scope_expr }
                } else {
                    let idx = *node_counter;
                    *node_counter += 1;
                    quote! { format!("{}:{}:For#{}", file!(), line!(), #idx) }
                };
                let each_expr_opt = get_attr("each").or_else(|| get_attr("of")).or_else(|| get_attr("in"));
                let Some(each_expr) = each_expr_opt else {
                    return quote! { compile_error!("<For>/<ForKeyed> requires `each={...}` (or `in={...}` / `of={...}`) attribute"); };
                };
                let maybe_key_ts = get_attr("key").map(|e| quote! { #e });
                let maybe_fallback_ts = get_attr("fallback").map(|e| quote! { #e });
                let maybe_item_ident = get_attr("item").map(|expr| match expr {
                    syn::Expr::Path(p) if p.path.segments.len() == 1 && p.qself.is_none() => {
                        Ok(p.path.segments[0].ident.clone())
                    }
                    _ => Err(()),
                });
                if let Some(Err(())) = maybe_item_ident {
                    return quote! { compile_error!("<For item={...}> expects an identifier, e.g. item={i}"); };
                }

                let item_ident_for_render = maybe_item_ident
                    .as_ref()
                    .and_then(|v| v.as_ref().ok())
                    .cloned()
                    .or_else(|| Some(syn::Ident::new("item", proc_macro2::Span::call_site())));
                let render_ts = if let Some(item_ident) = item_ident_for_render {
                    let mut expanded_children = Vec::new();
                    for child in &el.children {
                        expanded_children.push(expand_node(child, node_counter));
                    }
                    let child_el = if expanded_children.is_empty() {
                        quote! { rui::core::tree::Element::new() }
                    } else {
                        quote! {
                            {
                                use rui::core::tree::IntoElementList;
                                let mut __list: Vec<rui::core::tree::Element> = Vec::new();
                                #( __list.extend((#expanded_children).into_element_list()); )*
                                if __list.len() == 1 {
                                    __list.remove(0)
                                } else {
                                    let mut __v = rui::primitives::vstack();
                                    __v = __v.children(__list);
                                    rui::core::tree::Element::from(__v)
                                }
                            }
                        }
                    };
                    quote! {
                        |__rui_for_item| {
                            let #item_ident = __rui_for_item;
                            #child_el
                        }
                    }
                } else {
                    if el.children.len() != 1 {
                        return quote! { compile_error!("<For> without `item={...}` must contain exactly one closure child `{ |item| ... }`"); };
                    }
                    match &el.children[0] {
                        RsxNode::Expr(e) => quote! { #e },
                        _ => {
                            return quote! { compile_error!("<For> without `item` expects closure child expression"); };
                        }
                    }
                };

                if let Some(key_ts) = maybe_key_ts {
                    if let Some(item_ident) = maybe_item_ident
                        .as_ref()
                        .and_then(|v| v.as_ref().ok())
                        .cloned()
                        .or_else(|| Some(syn::Ident::new("item", proc_macro2::Span::call_site())))
                    {
                        let base = quote! {
                            rui::app::reactive::for_keyed_iter(
                                #scope_ts,
                                #each_expr,
                                |__rui_for_item| {
                                    let #item_ident = __rui_for_item;
                                    #key_ts
                                },
                                #render_ts,
                            )
                        };
                        if let Some(fallback_ts) = maybe_fallback_ts.clone() {
                            return quote! {{
                                use rui::core::tree::IntoElementList;
                                let __out = #base;
                                if __out.is_empty() { (#fallback_ts).into_element_list() } else { __out }
                            }};
                        }
                        return quote! {{ #base }};
                    } else {
                        let base = quote! { rui::app::reactive::for_keyed_iter(#scope_ts, #each_expr, #key_ts, #render_ts) };
                        if let Some(fallback_ts) = maybe_fallback_ts.clone() {
                            return quote! {{
                                use rui::core::tree::IntoElementList;
                                let __out = #base;
                                if __out.is_empty() { (#fallback_ts).into_element_list() } else { __out }
                            }};
                        }
                        return quote! {{ #base }};
                    }
                } else {
                    let base = quote! { rui::app::reactive::for_each_iter(#scope_ts, #each_expr, #render_ts) };
                    if let Some(fallback_ts) = maybe_fallback_ts {
                        return quote! {{
                            use rui::core::tree::IntoElementList;
                            let __out = #base;
                            if __out.is_empty() { (#fallback_ts).into_element_list() } else { __out }
                        }};
                    }
                    return quote! {{ #base }};
                }
            }
            let is_custom = name_str.chars().next().map_or(false, |c| c.is_uppercase()) 
                && !["VStack", "HStack", "GStack", "Grid", "Rect", "Icon", "Box", "Text", "Interactive"].contains(&name_str.as_str());

            let mut element = if is_custom {
                let name_ident = &el.name;
                let props_ident = syn::Ident::new(&format!("{}Props", name_str), name_ident.span());
                
                let mut prop_names = Vec::new();
                let mut prop_values = Vec::new();
                let mut base_calls = Vec::new();

                for (attr, value) in &el.attributes {
                    let attr_str = attr.to_string();
                    let mut attr_name = attr_str.as_str();
                    if attr_name.starts_with("el.") {
                        attr_name = &attr_name[3..];
                    }
                    
                    let is_base = match attr_name {
                        "w" | "width" | "max_w" | "maxWidth" | "max_width" | "min_w" | "minWidth" | "min_width" |
                        "h" | "height" | "max_h" | "maxHeight" | "max_height" | "min_h" | "minHeight" | "min_height" |
                        "p" | "padding" | "mx" | "my" | "px" | "py" | "pt" | "pr" | "pb" | "pl" | "mt" | "mr" | "mb" | "ml" |
                        "rounded" | "bg" | "color" | "text_color" | "font_weight" | "text_weight" |
                        "flex_grow" | "flex_shrink" | "grow" | "shrink" | "flex_basis" | "absolute" | "relative" | "top" | "right" | "bottom" | "left" | "inset" | "z_index" |
                        "cols" | "grid_cols" | "rows" | "grid_rows" | "col_span" | "grid_column_span" | "row_span" | "grid_row_span" | "align_self" |
                        "text_size" | "leading" | "text_leading" | "line_height" | "lineHeight" | "underline" | 
                        "text_decoration" | "textDecoration" |
                        "text_decoration_line" | "textDecorationLine" |
                        "text_decoration_style" | "textDecorationStyle" |
                        "text_decoration_color" | "textDecorationColor" |
                        "text_decoration_thickness" | "textDecorationThickness" |
                        "items" | "wrap" | "gap" |
                        "border_style" | "borderStyle" | "border_color" | "borderColor" | "border_width" | "borderWidth" |
                        "outline_style" | "outlineStyle" | "outline_color" | "outlineColor" | "outline_width" | "outlineWidth" |
                        "shadow" | "box_shadow" | "boxShadow" | "box-shadow" |
                        "box_shadow_color" | "boxShadowColor" | "box_shadow_blur" | "boxShadowBlur" |
                        "box_shadow_offset_x" | "boxShadowOffsetX" | "box_shadow_offset_y" | "boxShadowOffsetY" |
                        "box_shadow_spread" | "boxShadowSpread" | "box_shadow_inset" | "boxShadowInset" |
                        "text_shadow" | "textShadow" | "text-shadow" |
                        "text_shadow_color" | "textShadowColor" | "text_shadow_blur" | "textShadowBlur" |
                        "text_shadow_offset_x" | "textShadowOffsetX" | "text_shadow_offset_y" | "textShadowOffsetY" |
                        "text_align" |
                        "class" | "className" | "tw" | "uno" => true,
                        _ => false,
                    };

                    if is_base {
                        if attr_name == "class" || attr_name == "className" || attr_name == "tw" || attr_name == "uno" {
                            let calls = parse_utility_calls(value);
                            for call in calls {
                                // Replace __rui_el with b for component base context
                                let call_str = call.to_string().replace("__rui_el", "b");
                                let call_tokens: proc_macro2::TokenStream = call_str.parse().unwrap();
                                base_calls.push(call_tokens);
                            }
                        } else if matches!(attr_name, "leading" | "text_leading") {
                            // 与原生节点分支一致：`leading="normal"` 等不能走 `IntoF32 for &str`（会按 CSS 长度解析失败 → 0 → 行高≈0，文本被裁掉）。
                            let s = if let syn::Expr::Lit(expr_lit) = value {
                                if let syn::Lit::Str(lit_str) = &expr_lit.lit {
                                    Some(lit_str.value())
                                } else {
                                    None
                                }
                            } else {
                                None
                            };
                            let call = match s.as_deref() {
                                Some("none") => quote! { b = b.leading_none(); },
                                Some("tight") => quote! { b = b.leading_tight(); },
                                Some("normal") => quote! { b = b.leading_normal(); },
                                Some("relaxed") => quote! { b = b.leading_relaxed(); },
                                _ => quote! { b = b.leading(rui::core::tree::IntoF32::into_f32(#value)); },
                            };
                            base_calls.push(call);
                        } else if matches!(attr_name, "cols" | "grid_cols") {
                            if let Some(n) = expr_as_u16(value) {
                                base_calls.push(quote! { b = b.grid_cols_equal(#n); });
                            }
                        } else if matches!(attr_name, "rows" | "grid_rows") {
                            if let Some(n) = expr_as_u16(value) {
                                base_calls.push(quote! { b = b.grid_rows_equal(#n); });
                            }
                        } else if matches!(attr_name, "col_span" | "grid_column_span" | "colSpan") {
                            if let Some(n) = expr_as_u16(value) {
                                base_calls.push(quote! { b = b.grid_column_span(#n); });
                            }
                        } else if matches!(attr_name, "row_span" | "grid_row_span" | "rowSpan") {
                            if let Some(n) = expr_as_u16(value) {
                                base_calls.push(quote! { b = b.grid_row_span(#n); });
                            }
                        } else if matches!(attr_name, "align_self") {
                            base_calls.push(quote! { b = b.align_self_kw(#value); });
                        } else if matches!(attr_name, "flex_basis") {
                            base_calls.push(quote! { b = b.flex_basis(#value); });
                        } else {
                            let method = match attr_name {
                                "width" => "w",
                                "maxWidth" | "max_width" => "max_w",
                                "minWidth" | "min_width" => "min_w",
                                "height" => "h",
                                "maxHeight" | "max_height" => "max_h",
                                "minHeight" | "min_height" => "min_h",
                                "padding" => "p",
                                "color" | "text_color" => "text_color",
                                "text_weight" => "font_weight",
                                "textDecoration" => "text_decoration",
                                "textDecorationLine" => "text_decoration_line",
                                "textDecorationStyle" => "text_decoration_style",
                                "textDecorationColor" => "text_decoration_color",
                                "textDecorationThickness" => "text_decoration_thickness",
                                "lineHeight" => "line_height",
                                "grow" => "flex_grow",
                                "shrink" => "flex_shrink",
                                "scroll_behavior" | "scrollBehavior" => "scroll_behavior",
                                "borderStyle" => "border_style",
                                "borderColor" => "border_color",
                                "borderWidth" => "border_width",
                                "outlineStyle" => "outline_style",
                                "outlineColor" => "outline_color",
                                "outlineWidth" => "outline_width",
                                "boxShadow" | "box-shadow" => "box_shadow",
                                "boxShadowColor" => "box_shadow_color",
                                "boxShadowBlur" => "box_shadow_blur",
                                "boxShadowOffsetX" => "box_shadow_offset_x",
                                "boxShadowOffsetY" => "box_shadow_offset_y",
                                "boxShadowSpread" => "box_shadow_spread",
                                "boxShadowInset" => "box_shadow_inset",
                                "textShadow" | "text-shadow" => "text_shadow",
                                "textShadowColor" => "text_shadow_color",
                                "textShadowBlur" => "text_shadow_blur",
                                "textShadowOffsetX" => "text_shadow_offset_x",
                                "textShadowOffsetY" => "text_shadow_offset_y",
                                _ => attr_name,
                            };
                            let method_ident = syn::Ident::new(method, attr.span());
                            let final_value = match method {
                                "bg" | "border" | "shadow" => {
                                    quote! { rui::core::tree::parse_color(#value) }
                                }
                                "border_color" | "outline_color" | "box_shadow_color" => {
                                    quote! { rui::core::tree::parse_color(#value) }
                                }
                                "text_color" | "text_decoration_color" | "text_shadow_color" => {
                                    // 字面量走 parse；`{ ... }` 表达式假定已是 `[u8; 4]`（避免双重 parse）
                                    if let syn::Expr::Lit(syn::ExprLit {
                                        lit: syn::Lit::Str(lit),
                                        ..
                                    }) = value
                                    {
                                        quote! { rui::core::tree::parse_text_color(#lit) }
                                    } else {
                                        quote! { #value }
                                    }
                                }
                                "box_shadow" | "text_shadow" | "border_style" | "outline_style" => {
                                    quote! { #value }
                                }
                                _ => quote! { #value },
                            };
                            base_calls.push(quote! { b = b.#method_ident(#final_value); });
                        }
                    } else {
                        let final_attr = if name_str == "Input" && attr_str == "type" {
                            // HTML `type` → 组件 prop `input_type`（避免 Rust 关键字）。
                            syn::Ident::new("input_type", attr.span())
                        } else {
                            match attr_str.as_str() {
                                "onClick" => syn::Ident::new("on_click", attr.span()),
                                "onChange" => syn::Ident::new("on_change", attr.span()),
                                _ => attr.clone(),
                            }
                        };
                        let final_value = if attr_str == "id" {
                            // RSX string attrs are `&str`; component `id` is `String`.
                            quote! { (#value).into() }
                        } else if attr_name == "children" {
                            quote! { #value }
                        } else if matches!(attr_name, "on_click" | "onClick" | "on_change" | "onChange") {
                            quote! { Some(rui::core::tree::Handler::from(rui::core::tree::ZeroArgFn(#value))) }
                        } else {
                            quote! { Some((#value).into()) }
                        };
                        prop_names.push(final_attr);
                        prop_values.push(final_value);
                    }
                }

                if name_str == "Input" && !prop_names.iter().any(|p| p.to_string() == "id") {
                    let seq = INPUT_RSX_ANON_SEQ.fetch_add(1, Ordering::Relaxed);
                    prop_names.push(syn::Ident::new("id", name_ident.span()));
                    prop_values.push(quote! {
                        format!("__rui_input_auto_{}", #seq)
                    });
                }

                let base_expr = if base_calls.is_empty() {
                    quote! { None }
                } else {
                    quote! { Some(Box::new(|mut b: rui::components::base::BaseComponentBuilder| { #(#base_calls)* b })) }
                };
                
                let mut children_expr = quote! { None };
                if !el.children.is_empty() {
                    let mut expanded_children = Vec::new();
                    for child in &el.children {
                        expanded_children.push(expand_node(child, node_counter));
                    }
                    // 多子节点默认包进 VStack（组件 children 槽是单根）；布局原语 HStack/VStack/GStack 走下方 primitive 路径。
                    if expanded_children.len() == 1 {
                        let first = &expanded_children[0];
                        children_expr = quote! { 
                            Some({
                                use rui::core::tree::IntoElementList;
                                let mut __list = (#first).into_element_list();
                                if __list.len() == 1 {
                                    __list.remove(0)
                                } else {
                                    let mut __v = rui::primitives::vstack();
                                    __v = __v.children(__list);
                                    rui::core::tree::Element::from(__v)
                                }
                            }) 
                        };
                    } else {
                        children_expr = quote! { 
                            Some({
                                let mut __v = rui::primitives::vstack();
                                #( __v = __v.child(rui::core::tree::Element::from(#expanded_children)); )*
                                rui::core::tree::Element::from(__v)
                            })
                        };
                    }
                }

                return quote! {
                    #name_ident(#props_ident {
                        base: #base_expr,
                        children: #children_expr,
                        #(#prop_names: #prop_values,)*
                        ..Default::default()
                    })
                };
            } else {
                match name_str.as_str() {
                    "VStack" => quote! { rui::primitives::vstack() },
                    "HStack" => quote! { rui::primitives::hstack() },
                    // Grid 为 GStack 的 HTML/Solid 风格别名。
                    "GStack" | "Grid" => quote! { rui::primitives::gstack() },
                    "Box" | "Rect" => quote! { rui::primitives::r#box() },
                    "Interactive" => quote! { rui::primitives::interactive(rui::core::tree::Element::new()) },
                    "Text" => quote! { rui::primitives::text("") },
                    _ => quote! { rui::core::tree::Element::new() },
                }
            };
            
            if name_str == "Icon" {
                let mut icon_code = "ea0e".to_string(); // fallback to an album icon
                for (attr, value) in &el.attributes {
                    if attr.to_string() == "name" {
                        if let syn::Expr::Lit(expr_lit) = value {
                            if let syn::Lit::Str(lit_str) = &expr_lit.lit {
                                let name_str = lit_str.value();
                                let json_str = include_str!("../../rui/assets/lucide.json");
                                static ICONS: std::sync::OnceLock<std::collections::HashMap<String, String>> = std::sync::OnceLock::new();
                                let map = ICONS.get_or_init(|| {
                                    serde_json::from_str(json_str).unwrap_or_default()
                                });
                                if let Some(code) = map.get(&name_str) {
                                    icon_code = code.clone();
                                } else {
                                    panic!("Icon '{}' not found in lucide.json. Ensure the name is correct.", name_str);
                                }
                            }
                        }
                    }
                }
                
                if let Ok(cp) = u32::from_str_radix(&icon_code, 16) {
                    if let Some(c) = std::char::from_u32(cp) {
                        let c_str = c.to_string();
                        element = quote! { rui::utils::text_node::build_icon_text_node(#c_str) };
                    }
                }
            }
            
            let mut utility_calls: Vec<proc_macro2::TokenStream> = Vec::new();
            for (attr, value) in &el.attributes {
                let attr_str = attr.to_string();
                if matches!(attr_str.as_str(), "class" | "className" | "tw" | "uno") {
                    utility_calls.extend(parse_utility_calls(value));
                    continue;
                }
                if attr_str == "id" && name_str == "ScrollArea" {
                    element = quote! { #element.id(#value) };
                    continue;
                }
                match attr_str.as_str() {
                    "w" | "width" => element = apply_min_max_size_expr(element, value, "w"),
                    "max_w" | "maxWidth" | "max_width" => element = quote! { #element.max_w(#value) },
                    "min_w" | "minWidth" | "min_width" => element = quote! { #element.min_w(#value) },
                    "inline_size" | "inlineSize" => element = apply_min_max_size_expr(element, value, "w"),
                    "h" | "height" => element = apply_min_max_size_expr(element, value, "h"),
                    "max_h" | "maxHeight" | "max_height" => element = quote! { #element.max_h(#value) },
                    "min_h" | "minHeight" | "min_height" => element = quote! { #element.min_h(#value) },
                    "block_size" | "blockSize" => element = apply_min_max_size_expr(element, value, "h"),
                    "bg" => {
                        element = quote! { #element.bg(rui::core::tree::parse_color(#value)) }
                    },
                    "border_style" | "borderStyle" => {
                        element = quote! { #element.border_style(#value) }
                    }
                    "border_color" | "borderColor" => {
                        element = quote! { #element.border_color(rui::core::tree::parse_color(#value)) }
                    }
                    "border_width" | "borderWidth" => {
                        element = quote! { #element.border_width(rui::core::tree::IntoF32::into_f32(#value)) }
                    }
                    "outline_style" | "outlineStyle" => {
                        element = quote! { #element.outline_style(#value) }
                    }
                    "outline_color" | "outlineColor" => {
                        element = quote! { #element.outline_color(rui::core::tree::parse_color(#value)) }
                    }
                    "outline_width" | "outlineWidth" => {
                        element = quote! { #element.outline_width(rui::core::tree::IntoF32::into_f32(#value)) }
                    }
                    "color" => {
                        element = quote! { #element.text_color(rui::core::tree::parse_text_color(#value)) }
                    },
                    "font_weight" | "text_weight" => {
                        element = quote! { #element.font_weight(rui::core::tree::IntoF32::into_f32(#value)) }
                    },
                    "underline" => {
                        element = quote! { #element.underline(#value) }
                    },
                    "text_decoration" | "textDecoration" | "decoration" => {
                        element = quote! { #element.text_decoration(#value) }
                    },
                    "text_decoration_line" | "textDecorationLine" => {
                        element = quote! { #element.text_decoration_line(#value) }
                    }
                    "text_size" | "textSize" | "size" if name_str == "Label" => {
                        element = quote! { #element.text_size(#value) }
                    },
                    "gap" => {
                        element = quote! { #element.gap(#value) }
                    },
                    "items" | "align" => {
                        element = quote! { #element.items(#value) }
                    },
                    "justify" => {
                        element = quote! { #element.justify(#value) }
                    },
                    "wrap" => {
                        element = quote! { #element.wrap(#value) }
                    },
                    "text_decoration_style" | "textDecorationStyle" => {
                        element = quote! { #element.text_decoration_style(#value) }
                    },
                    "text_decoration_color" | "textDecorationColor" => {
                        element = quote! { #element.text_decoration_color(rui::core::tree::parse_text_color(#value)) }
                    },
                    "text_decoration_thickness" | "textDecorationThickness" => {
                        element = quote! { #element.text_decoration_thickness(rui::core::tree::IntoF32::into_f32(#value)) }
                    },
                    "text_underline_offset" | "textUnderlineOffset" => {
                        element = quote! { #element.text_underline_offset(rui::core::tree::IntoF32::into_f32(#value)) }
                    },
                    "line_height" | "lineHeight" | "lh" | "text_line_height" => {
                        if is_custom {
                            element = quote! { #element.line_height(rui::core::tree::IntoF32::into_f32(#value)) }
                        } else {
                            element = quote! { #element.text_line_height(rui::core::tree::IntoF32::into_f32(#value)) }
                        }
                    },
                    "leading" | "text_leading" => {
                        let s = if let syn::Expr::Lit(expr_lit) = value {
                            if let syn::Lit::Str(lit_str) = &expr_lit.lit { Some(lit_str.value()) } else { None }
                        } else { None };
                        let s = s.as_deref().unwrap_or("");
                        if s == "none" {
                            if is_custom { element = quote! { #element.leading_none() } }
                            else { element = quote! { #element.text_leading_none() } }
                        } else if s == "tight" {
                            if is_custom { element = quote! { #element.leading_tight() } }
                            else { element = quote! { #element.text_leading_tight() } }
                        } else if s == "normal" {
                            if is_custom { element = quote! { #element.leading_normal() } }
                            else { element = quote! { #element.text_leading_normal() } }
                        } else if s == "relaxed" {
                            if is_custom { element = quote! { #element.leading_relaxed() } }
                            else { element = quote! { #element.text_leading_relaxed() } }
                        } else {
                            if is_custom { element = quote! { #element.leading(rui::core::tree::IntoF32::into_f32(#value)) } }
                            else { element = quote! { #element.text_leading(rui::core::tree::IntoF32::into_f32(#value)) } }
                        }
                    },
                    "size" => {
                        if is_custom || name_str == "Button" || name_str == "Input" {
                            element = quote! { #element.size(#value) }
                        } else {
                            element = quote! { #element.text_size(rui::core::tree::IntoF32::into_f32(#value)) }
                        }
                    },
                    "text_size" | "font_size" => {
                        element = quote! { #element.text_size(rui::core::tree::IntoF32::into_f32(#value)) }
                    },
                    "p" | "padding" => element = quote! { #element.p(#value) },
                    "px" | "padding_x" | "paddingX" => element = quote! { #element.px(#value) },
                    "py" | "padding_y" | "paddingY" => element = quote! { #element.py(#value) },
                    "pt" | "padding_top" | "paddingTop" => element = quote! { #element.pt(#value) },
                    "pr" | "padding_right" | "paddingRight" => element = quote! { #element.pr(#value) },
                    "pb" | "padding_bottom" | "paddingBottom" => element = quote! { #element.pb(#value) },
                    "pl" | "padding_left" | "paddingLeft" => element = quote! { #element.pl(#value) },
                    "padding_inline" | "paddingInline" => element = quote! { #element.padding_inline(#value) },
                    "padding_block" | "paddingBlock" => element = quote! { #element.padding_block(#value) },
                    "padding_inline_start" | "paddingInlineStart" => element = quote! { #element.padding_inline_start(#value) },
                    "padding_inline_end" | "paddingInlineEnd" => element = quote! { #element.padding_inline_end(#value) },
                    "padding_block_start" | "paddingBlockStart" => element = quote! { #element.padding_block_start(#value) },
                    "padding_block_end" | "paddingBlockEnd" => element = quote! { #element.padding_block_end(#value) },
                    "m" | "margin" => element = quote! { #element.m(#value) },
                    "overflow" => element = quote! { #element.overflow(#value) },
                    "overflow_x" | "overflowX" => element = quote! { #element.overflow_x(#value) },
                    "overflow_y" | "overflowY" => element = quote! { #element.overflow_y(#value) },
                    "scroll_behavior" | "scrollBehavior" => element = quote! { #element.scroll_behavior(#value) },
                    "hover_mode" | "hoverMode" => element = quote! { #element.hover_mode(#value) },
                    "scrollbar_visibility" | "scrollbarVisibility" => {
                        element = quote! { #element.scrollbar_visibility(#value) }
                    },
                    "scrollbar_inset" | "scrollbarInset" => element = quote! { #element.scrollbar_inset(#value) },
                    "scrollbar_inset_x" | "scrollbarInsetX" => element = quote! { #element.scrollbar_inset_x(#value) },
                    "scrollbar_inset_y" | "scrollbarInsetY" => element = quote! { #element.scrollbar_inset_y(#value) },
                    "scrollbar_y_side" | "scrollbarYSide" => element = quote! { #element.scrollbar_y_side(#value) },
                    "scrollbar_x_side" | "scrollbarXSide" => element = quote! { #element.scrollbar_x_side(#value) },
                    "scrollbar_thickness" | "scrollbarThickness" => {
                        element = quote! { #element.scrollbar_thickness(rui::core::tree::IntoF32::into_f32(#value)) }
                    },
                    "scrollbar_variant" | "scrollbarVariant" => element = quote! { #element.scrollbar_variant(#value) },
                    "scrollbar_track_color" | "scrollbarTrackColor" => {
                        element = quote! { #element.scrollbar_track_color(#value) }
                    },
                    "scrollbar_thumb_color" | "scrollbarThumbColor" => {
                        element = quote! { #element.scrollbar_thumb_color(#value) }
                    },
                    "mx" | "margin_x" | "marginX" => element = quote! { #element.mx(#value) },
                    "my" | "margin_y" | "marginY" => element = quote! { #element.my(#value) },
                    "mt" | "margin_top" | "marginTop" => element = quote! { #element.mt(#value) },
                    "mr" | "margin_right" | "marginRight" => element = quote! { #element.mr(#value) },
                    "mb" | "margin_bottom" | "marginBottom" => element = quote! { #element.mb(#value) },
                    "ml" | "margin_left" | "marginLeft" => element = quote! { #element.ml(#value) },
                    "margin_inline" | "marginInline" => element = quote! { #element.margin_inline(#value) },
                    "margin_block" | "marginBlock" => element = quote! { #element.margin_block(#value) },
                    "margin_inline_start" | "marginInlineStart" => element = quote! { #element.margin_inline_start(#value) },
                    "margin_inline_end" | "marginInlineEnd" => element = quote! { #element.margin_inline_end(#value) },
                    "margin_block_start" | "marginBlockStart" => element = quote! { #element.margin_block_start(#value) },
                    "margin_block_end" | "marginBlockEnd" => element = quote! { #element.margin_block_end(#value) },
                    "rounded" | "radius" | "border_radius" | "borderRadius" => {
                        element = quote! { #element.rounded(rui::core::tree::IntoF32::into_f32(#value)) }
                    },
                    "shadow" => {
                        element = quote! { #element.shadow([0.0, 0.0, 0.0, 0.5], rui::core::tree::IntoF32::into_f32(#value)) }
                    },
                    "box_shadow" | "boxShadow" | "box-shadow" => {
                        element = quote! { #element.box_shadow(#value) }
                    }
                    "box_shadow_color" | "boxShadowColor" => {
                        element = quote! { #element.box_shadow_color(rui::core::tree::parse_color(#value)) }
                    }
                    "box_shadow_blur" | "boxShadowBlur" => {
                        element = quote! { #element.box_shadow_blur(rui::core::tree::IntoF32::into_f32(#value)) }
                    }
                    "box_shadow_offset_x" | "boxShadowOffsetX" => {
                        element = quote! { #element.box_shadow_offset_x(rui::core::tree::IntoF32::into_f32(#value)) }
                    }
                    "box_shadow_offset_y" | "boxShadowOffsetY" => {
                        element = quote! { #element.box_shadow_offset_y(rui::core::tree::IntoF32::into_f32(#value)) }
                    }
                    "box_shadow_spread" | "boxShadowSpread" => {
                        element = quote! { #element.box_shadow_spread(rui::core::tree::IntoF32::into_f32(#value)) }
                    }
                    "box_shadow_inset" | "boxShadowInset" => {
                        element = quote! { #element.box_shadow_inset(#value) }
                    }
                    "text_shadow" => {
                        element = quote! { #element.text_shadow(#value) }
                    },
                    "textShadow" | "text-shadow" => {
                        element = quote! { #element.text_shadow(#value) }
                    },
                    "text_shadow_color" | "textShadowColor" => {
                        element = quote! { #element.text_shadow_color(rui::core::tree::parse_text_color(#value)) }
                    },
                    "text_shadow_blur" | "textShadowBlur" => {
                        element = quote! { #element.text_shadow_blur(rui::core::tree::IntoF32::into_f32(#value)) }
                    },
                    "text_shadow_offset_x" | "textShadowOffsetX" => {
                        element = quote! { #element.text_shadow_offset_x(rui::core::tree::IntoF32::into_f32(#value)) }
                    },
                    "text_shadow_offset_y" | "textShadowOffsetY" => {
                        element = quote! { #element.text_shadow_offset_y(rui::core::tree::IntoF32::into_f32(#value)) }
                    },
                    "text_align" => {
                        element = quote! { #element.text_align(#value) }
                    },
                    "on_click" | "onClick" => element = quote! { #element.on_click(#value) },
                    "variant" | "type" => element = quote! { #element.variant(#value) },
                    "show" | "visibility" | "display_mode" => {
                        element = quote! { #element.visibility(#value) }
                    }
                    "target" => element = quote! { #element.target(#value) },
                    "thickness" | "scrollbar_width" | "bar_width" | "scrollbarWidth" => {
                        element = quote! { #element.thickness(rui::core::tree::IntoF32::into_f32(#value)) }
                    }
                    "track_color" => {
                        element = quote! { #element.track_color(rui::core::tree::parse_color(#value)) }
                    }
                    "thumb_color" => {
                        element = quote! { #element.thumb_color(rui::core::tree::parse_color(#value)) }
                    }
                    "disabled" => element = quote! { #element.disabled(#value) },
                    "placeholder" => element = quote! { #element.placeholder(#value) },
                    "invalid" => element = quote! { #element.invalid(#value) },

                    "grow" | "flex_grow" | "flexGrow" => {
                        element = quote! { #element.flex_grow(rui::core::tree::IntoF32Raw::into_f32_raw(#value)) }
                    },
                    "shrink" | "flex_shrink" | "flexShrink" => {
                        element = quote! { #element.flex_shrink(rui::core::tree::IntoF32Raw::into_f32_raw(#value)) }
                    },
                    "cols" | "grid_cols" => {
                        if let Some(n) = expr_as_u16(value) {
                            element = quote! { #element.grid_cols_equal(#n) };
                        }
                    }
                    "rows" | "grid_rows" => {
                        if let Some(n) = expr_as_u16(value) {
                            element = quote! { #element.grid_rows_equal(#n) };
                        }
                    }
                    "col_span" | "grid_column_span" | "colSpan" => {
                        if let Some(n) = expr_as_u16(value) {
                            element = quote! { #element.grid_column_span(#n) };
                        }
                    }
                    "row_span" | "grid_row_span" | "rowSpan" => {
                        if let Some(n) = expr_as_u16(value) {
                            element = quote! { #element.grid_row_span(#n) };
                        }
                    }
                    "align_self" => {
                        element = quote! { #element.align_self_kw(#value) }
                    }
                    "flex_basis" => {
                        element = quote! { #element.flex_basis(#value) }
                    }
                    _ => {}
                }
            }

            for child in &el.children {
                match child {
                    RsxNode::Expr(expr) => {
                        // We map text strings or elements inside { } to `.children()`
                        element = quote! { 
                            {
                                use rui::core::tree::IntoElementList;
                                #element.children((#expr).into_element_list())
                            }
                        };
                    }
                    RsxNode::Element(_) => {
                        let expanded_child = expand_node(child, node_counter);
                        element = quote! {
                            {
                                use rui::core::tree::IntoElementList;
                                #element.children((#expanded_child).into_element_list())
                            }
                        };
                    }
                }
            }
            
            // Apply flex row/col explicitly if tag is known
            if name_str == "VStack" {
                element = quote! { #element.flex_col() };
            }
            if name_str == "HStack" {
                element = quote! { #element.flex_row() };
            }

            if !utility_calls.is_empty() {
                element = quote! {
                    {
                        let mut __rui_el: rui::core::tree::Element = (#element).into();
                        #(#utility_calls)*
                        __rui_el
                    }
                };
            }

            element
        }
    }
}

fn parse_utility_calls(value: &Expr) -> Vec<proc_macro2::TokenStream> {
    let Expr::Lit(expr_lit) = value else {
        return Vec::new();
    };
    let syn::Lit::Str(lit_str) = &expr_lit.lit else {
        return Vec::new();
    };
    lit_str
        .value()
        .split_whitespace()
        .filter_map(utility_token_to_call)
        .collect()
}

fn utility_token_to_call(token: &str) -> Option<proc_macro2::TokenStream> {
    match token {
        "flex" => return Some(quote! { __rui_el = __rui_el.flex_row(); }),
        "flex-col" => return Some(quote! { __rui_el = __rui_el.flex_col(); }),
        "flex-row" => return Some(quote! { __rui_el = __rui_el.flex_row(); }),
        "grid" => return Some(quote! { __rui_el = __rui_el.display_grid(); }),
        "items-center" => return Some(quote! { __rui_el = __rui_el.items_center(); }),
        "justify-center" => return Some(quote! { __rui_el = __rui_el.justify_center(); }),
        "grow" => return Some(quote! { __rui_el = __rui_el.flex_grow(1.0); }),
        "grow-0" => return Some(quote! { __rui_el = __rui_el.flex_grow(0.0); }),
        "shrink" => return Some(quote! { __rui_el = __rui_el.flex_shrink(1.0); }),
        "shrink-0" => return Some(quote! { __rui_el = __rui_el.flex_shrink(0.0); }),
        "overflow-hidden" => return Some(quote! { __rui_el = __rui_el.overflow("hidden"); }),
        "overflow-auto" => return Some(quote! { __rui_el = __rui_el.overflow("auto"); }),
        "overflow-scroll" => return Some(quote! { __rui_el = __rui_el.overflow("scroll"); }),
        _ => {}
    }

    if let Some(raw) = token.strip_prefix("grid-cols-") {
        if let Ok(n) = raw.parse::<u16>() {
            if n > 0 {
                return Some(quote! {
                    __rui_el = __rui_el.display_grid().grid_template_columns(rui::layout::cols_equal_fr(#n));
                });
            }
        }
    }

    if let Some(raw) = token.strip_prefix("grid-rows-") {
        if let Ok(n) = raw.parse::<u16>() {
            if n > 0 {
                return Some(quote! {
                    __rui_el = __rui_el.display_grid().grid_template_rows(rui::layout::cols_equal_fr(#n));
                });
            }
        }
    }

    if let Some(v) = parse_tailwind_value(token, "gap-x-") {
        let lit = syn::LitStr::new(&v, proc_macro2::Span::call_site());
        return Some(quote! { __rui_el = __rui_el.gap_x(#lit); });
    }
    if let Some(v) = parse_tailwind_value(token, "gap-y-") {
        let lit = syn::LitStr::new(&v, proc_macro2::Span::call_site());
        return Some(quote! { __rui_el = __rui_el.gap_y(#lit); });
    }

    if let Some(v) = parse_tailwind_value(token, "w-") {
        let lit = syn::LitStr::new(&v, proc_macro2::Span::call_site());
        return Some(quote! { __rui_el = __rui_el.w(#lit); });
    }
    if let Some(v) = parse_tailwind_value(token, "h-") {
        let lit = syn::LitStr::new(&v, proc_macro2::Span::call_site());
        return Some(quote! { __rui_el = __rui_el.h(#lit); });
    }
    if let Some(v) = parse_tailwind_value(token, "min-w-") {
        let lit = syn::LitStr::new(&v, proc_macro2::Span::call_site());
        return Some(quote! { __rui_el = __rui_el.min_w(#lit); });
    }
    if let Some(v) = parse_tailwind_value(token, "max-w-") {
        let lit = syn::LitStr::new(&v, proc_macro2::Span::call_site());
        return Some(quote! { __rui_el = __rui_el.max_w(#lit); });
    }
    if let Some(v) = parse_tailwind_value(token, "min-h-") {
        let lit = syn::LitStr::new(&v, proc_macro2::Span::call_site());
        return Some(quote! { __rui_el = __rui_el.min_h(#lit); });
    }
    if let Some(v) = parse_tailwind_value(token, "max-h-") {
        let lit = syn::LitStr::new(&v, proc_macro2::Span::call_site());
        return Some(quote! { __rui_el = __rui_el.max_h(#lit); });
    }
    if let Some(v) = parse_tailwind_value(token, "p-") {
        let lit = syn::LitStr::new(&v, proc_macro2::Span::call_site());
        return Some(quote! { __rui_el = __rui_el.p(#lit); });
    }
    if let Some(v) = parse_tailwind_value(token, "px-") {
        let lit = syn::LitStr::new(&v, proc_macro2::Span::call_site());
        return Some(quote! { __rui_el = __rui_el.px(#lit); });
    }
    if let Some(v) = parse_tailwind_value(token, "py-") {
        let lit = syn::LitStr::new(&v, proc_macro2::Span::call_site());
        return Some(quote! { __rui_el = __rui_el.py(#lit); });
    }
    if let Some(v) = parse_tailwind_value(token, "pt-") {
        let lit = syn::LitStr::new(&v, proc_macro2::Span::call_site());
        return Some(quote! { __rui_el = __rui_el.pt(#lit); });
    }
    if let Some(v) = parse_tailwind_value(token, "pr-") {
        let lit = syn::LitStr::new(&v, proc_macro2::Span::call_site());
        return Some(quote! { __rui_el = __rui_el.pr(#lit); });
    }
    if let Some(v) = parse_tailwind_value(token, "pb-") {
        let lit = syn::LitStr::new(&v, proc_macro2::Span::call_site());
        return Some(quote! { __rui_el = __rui_el.pb(#lit); });
    }
    if let Some(v) = parse_tailwind_value(token, "pl-") {
        let lit = syn::LitStr::new(&v, proc_macro2::Span::call_site());
        return Some(quote! { __rui_el = __rui_el.pl(#lit); });
    }
    if let Some(v) = parse_tailwind_value(token, "m-") {
        let lit = syn::LitStr::new(&v, proc_macro2::Span::call_site());
        return Some(quote! { __rui_el = __rui_el.m(#lit); });
    }
    if let Some(v) = parse_tailwind_value(token, "mx-") {
        let lit = syn::LitStr::new(&v, proc_macro2::Span::call_site());
        return Some(quote! { __rui_el = __rui_el.mx(#lit); });
    }
    if let Some(v) = parse_tailwind_value(token, "my-") {
        let lit = syn::LitStr::new(&v, proc_macro2::Span::call_site());
        return Some(quote! { __rui_el = __rui_el.my(#lit); });
    }
    if let Some(v) = parse_tailwind_value(token, "mt-") {
        let lit = syn::LitStr::new(&v, proc_macro2::Span::call_site());
        return Some(quote! { __rui_el = __rui_el.mt(#lit); });
    }
    if let Some(v) = parse_tailwind_value(token, "mr-") {
        let lit = syn::LitStr::new(&v, proc_macro2::Span::call_site());
        return Some(quote! { __rui_el = __rui_el.mr(#lit); });
    }
    if let Some(v) = parse_tailwind_value(token, "mb-") {
        let lit = syn::LitStr::new(&v, proc_macro2::Span::call_site());
        return Some(quote! { __rui_el = __rui_el.mb(#lit); });
    }
    if let Some(v) = parse_tailwind_value(token, "ml-") {
        let lit = syn::LitStr::new(&v, proc_macro2::Span::call_site());
        return Some(quote! { __rui_el = __rui_el.ml(#lit); });
    }
    if let Some(v) = parse_tailwind_value(token, "gap-") {
        if let Ok(px) = parse_space_value(&v) {
            return Some(quote! { __rui_el = __rui_el.gap(#px); });
        }
    }

    if let Some(rest) = token.strip_prefix("rounded-") {
        let px = match rest {
            "none" => 0.0_f32,
            "sm" => 2.0_f32,
            "md" => 6.0_f32,
            "lg" => 8.0_f32,
            "xl" => 12.0_f32,
            "2xl" => 16.0_f32,
            "full" => return Some(quote! { __rui_el = __rui_el.rounded_full(); }),
            _ => return None,
        };
        return Some(quote! { __rui_el = __rui_el.rounded(#px); });
    }
    if token == "rounded" {
        return Some(quote! { __rui_el = __rui_el.rounded(6.0_f32); });
    }

    if let Some(rest) = token.strip_prefix("text-") {
        if let Some(size_px) = text_size_token_to_px(rest) {
            return Some(quote! { __rui_el = __rui_el.text_size(#size_px); });
        }
        let lit = syn::LitStr::new(rest, proc_macro2::Span::call_site());
        return Some(quote! { __rui_el = __rui_el.text_color(rui::core::tree::parse_text_color(#lit)); });
    }
    if let Some(rest) = token.strip_prefix("bg-") {
        let lit = syn::LitStr::new(rest, proc_macro2::Span::call_site());
        return Some(quote! { __rui_el = __rui_el.bg(rui::core::tree::parse_color(#lit)); });
    }
    if let Some(rest) = token.strip_prefix("font-") {
        let w = match rest {
            "thin" => 100.0_f32,
            "extralight" => 200.0_f32,
            "light" => 300.0_f32,
            "normal" => 400.0_f32,
            "medium" => 500.0_f32,
            "semibold" => 600.0_f32,
            "bold" => 700.0_f32,
            "extrabold" => 800.0_f32,
            "black" => 900.0_f32,
            _ => return None,
        };
        return Some(quote! { __rui_el = __rui_el.font_weight(#w); });
    }
    if let Some(rest) = token.strip_prefix("overflow-x-") {
        let lit = syn::LitStr::new(rest, proc_macro2::Span::call_site());
        return Some(quote! { __rui_el = __rui_el.overflow_x(#lit); });
    }
    if let Some(rest) = token.strip_prefix("overflow-y-") {
        let lit = syn::LitStr::new(rest, proc_macro2::Span::call_site());
        return Some(quote! { __rui_el = __rui_el.overflow_y(#lit); });
    }

    match token {
        "self-auto" => {
            return Some(quote! { __rui_el = __rui_el.align_self_kw("auto"); });
        }
        "self-center" => {
            return Some(quote! { __rui_el = __rui_el.align_self_kw("center"); });
        }
        "self-start" => {
            return Some(quote! { __rui_el = __rui_el.align_self_kw("start"); });
        }
        "self-end" => {
            return Some(quote! { __rui_el = __rui_el.align_self_kw("end"); });
        }
        "self-stretch" => {
            return Some(quote! { __rui_el = __rui_el.align_self_kw("stretch"); });
        }
        "self-baseline" => {
            return Some(quote! { __rui_el = __rui_el.align_self_kw("baseline"); });
        }
        _ => {}
    }

    None
}

fn parse_tailwind_value(token: &str, prefix: &str) -> Option<String> {
    let raw = token.strip_prefix(prefix)?;
    if raw.is_empty() {
        return None;
    }
    if let Some(inner) = raw.strip_prefix('[').and_then(|v| v.strip_suffix(']')) {
        let v = inner.trim();
        if v.is_empty() {
            return None;
        }
        return Some(v.to_string());
    }
    parse_space_value(raw).ok().map(|px| format!("{px}px"))
}

fn parse_space_value(raw: &str) -> Result<f32, ()> {
    let n: f32 = raw.parse().map_err(|_| ())?;
    Ok(n * 4.0)
}

fn text_size_token_to_px(token: &str) -> Option<f32> {
    match token {
        "xs" => Some(12.0),
        "sm" => Some(14.0),
        "base" => Some(16.0),
        "lg" => Some(18.0),
        "xl" => Some(20.0),
        "2xl" => Some(24.0),
        "3xl" => Some(30.0),
        "4xl" => Some(36.0),
        _ => None,
    }
}

fn parse_min_max_call(expr: &Expr) -> Option<(&'static str, String, String)> {
    let Expr::Lit(expr_lit) = expr else {
        return None;
    };
    let syn::Lit::Str(lit_str) = &expr_lit.lit else {
        return None;
    };
    let raw = lit_str.value();
    let s = raw.trim();
    let (kind, inner) = if let Some(v) = s.strip_prefix("min(").and_then(|v| v.strip_suffix(')')) {
        ("min", v)
    } else if let Some(v) = s.strip_prefix("max(").and_then(|v| v.strip_suffix(')')) {
        ("max", v)
    } else {
        return None;
    };
    let mut parts = inner.splitn(2, ',');
    let left = parts.next()?.trim();
    let right = parts.next()?.trim();
    if left.is_empty() || right.is_empty() {
        return None;
    }
    Some((kind, left.to_string(), right.to_string()))
}

fn apply_min_max_size_expr(
    element: proc_macro2::TokenStream,
    value: &Expr,
    axis: &str,
) -> proc_macro2::TokenStream {
    if let Some((kind, left, right)) = parse_min_max_call(value) {
        let left_lit = syn::LitStr::new(&left, proc_macro2::Span::call_site());
        let right_lit = syn::LitStr::new(&right, proc_macro2::Span::call_site());
        return match (axis, kind) {
            // width = min(a, b) <=> width: a; max-width: b
            ("w", "min") => quote! { #element.w(#left_lit).max_w(#right_lit) },
            // width = max(a, b) <=> width: a; min-width: b
            ("w", "max") => quote! { #element.w(#left_lit).min_w(#right_lit) },
            // height = min(a, b) <=> height: a; max-height: b
            ("h", "min") => quote! { #element.h(#left_lit).max_h(#right_lit) },
            // height = max(a, b) <=> height: a; min-height: b
            ("h", "max") => quote! { #element.h(#left_lit).min_h(#right_lit) },
            _ => quote! { #element },
        };
    }
    match axis {
        "w" => quote! { #element.w(#value) },
        "h" => quote! { #element.h(#value) },
        _ => quote! { #element },
    }
}

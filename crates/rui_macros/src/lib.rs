use std::sync::atomic::{AtomicUsize, Ordering};

use proc_macro::TokenStream;
use quote::{quote, quote_spanned};
use syn::parse::{Parse, ParseStream};
use syn::visit::Visit;
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

fn expr_as_i32(value: &Expr) -> Option<i32> {
    match value {
        Expr::Lit(l) => match &l.lit {
            syn::Lit::Int(i) => i.base10_parse().ok(),
            syn::Lit::Str(s) => s.value().trim().parse().ok(),
            _ => None,
        },
        Expr::Unary(u) if matches!(u.op, syn::UnOp::Neg(_)) => expr_as_i32(&u.expr).map(|v| -v),
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
            let expr = syn::Expr::Lit(syn::ExprLit {
                attrs: Vec::new(),
                lit: syn::Lit::Str(lit),
            });
            return Ok(RsxNode::Expr(expr));
        }

        // Otherwise it must be `<`
        input.parse::<Token![<]>()?;
        let name: Ident = input.parse()?;

        let mut attributes = Vec::new();
        while !input.peek(Token![>]) && !input.peek(Token![/]) {
            // Props spread: `..expr` or `..{expr}` (Solid-like `{...props}` MVP).
            if input.peek(Token![..]) {
                input.parse::<Token![..]>()?;
                let attr_value: Expr = if input.peek(syn::token::Brace) {
                    let content;
                    braced!(content in input);
                    content.parse().map_err(|e| {
                        syn::Error::new(e.span(), format!("Failed to parse spread `..{{...}}`: {}", e))
                    })?
                } else {
                    input.parse().map_err(|e| {
                        syn::Error::new(e.span(), format!("Failed to parse spread `..expr`: {}", e))
                    })?
                };
                attributes.push((
                    syn::Ident::new("__spread", proc_macro2::Span::call_site()),
                    attr_value,
                ));
                continue;
            }

            let attr_name: Ident = if input.peek(Token![in]) {
                input.parse::<Token![in]>().map_err(|e| {
                    syn::Error::new(e.span(), format!("Failed at keyword attr_name {}", e))
                })?;
                syn::Ident::new("in", proc_macro2::Span::call_site())
            } else if input.peek(Token![type]) {
                // HTML-like `type="password"`；`type` 是关键字，需单独吃掉 token。
                input.parse::<Token![type]>().map_err(|e| {
                    syn::Error::new(e.span(), format!("Failed at keyword attr type: {}", e))
                })?;
                syn::Ident::new("type", proc_macro2::Span::call_site())
            } else if input.peek(Token![ref]) {
                // Solid-like `ref={signal}`；`ref` 是关键字。
                input.parse::<Token![ref]>().map_err(|e| {
                    syn::Error::new(e.span(), format!("Failed at keyword attr ref: {}", e))
                })?;
                syn::Ident::new("ref", proc_macro2::Span::call_site())
            } else {
                input
                    .parse()
                    .map_err(|e| syn::Error::new(e.span(), format!("Failed at attr_name {}", e)))?
            };
            input.parse::<Token![=]>().map_err(|e| {
                syn::Error::new(e.span(), format!("Failed at = after attr {}", attr_name))
            })?;

            let attr_value: Expr = if input.peek(syn::token::Brace) {
                let content;
                braced!(content in input);
                content.parse().map_err(|e| {
                    syn::Error::new(
                        e.span(),
                        format!("Failed to parse braced attribute '{}': {}", attr_name, e),
                    )
                })?
            } else {
                let lit: syn::Lit = input
                    .parse()
                    .map_err(|e| syn::Error::new(e.span(), format!("Expected literal: {}", e)))?;
                syn::Expr::Lit(syn::ExprLit {
                    attrs: Vec::new(),
                    lit,
                })
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
                if fork.parse::<Token![<]>().is_ok() && fork.parse::<Token![/]>().is_ok() {
                    break;
                }
                children.push(
                    input.parse::<RsxNode>().map_err(|e| {
                        syn::Error::new(e.span(), format!("Failed at child: {}", e))
                    })?,
                );
            }

            // Close tag: `</Name>`
            input
                .parse::<Token![<]>()
                .map_err(|e| syn::Error::new(e.span(), "Expected < for close tag"))?;
            input
                .parse::<Token![/]>()
                .map_err(|e| syn::Error::new(e.span(), "Expected / for close tag"))?;
            let close_name: Ident = input
                .parse()
                .map_err(|e| syn::Error::new(e.span(), "Expected identifier for close tag"))?;
            if close_name != name {
                return Err(syn::Error::new(close_name.span(), "Tag name mismatch"));
            }
            input
                .parse::<Token![>]>()
                .map_err(|e| syn::Error::new(e.span(), "Expected > for close tag"))?;
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
            let expanded = expand_node(&root, &mut node_counter, false);
            let template_hash_lit =
                syn::LitStr::new(&template_hash, proc_macro2::Span::call_site());

            quote! {
                {
                    let __rui_root: rui::core::tree::Element = (#expanded).into();
                    let __rui_root = __rui_root.template_hash(#template_hash_lit);
                    let __rui_root = __rui_root.template_id(format!("{}:{}:{}", file!(), line!(), column!()));
                    __rui_root
                }
            }.into()
        }
        Err(err) => err.to_compile_error().into(),
    }
}

/// A marker attribute for functional components.
/// It automatically generates a Props struct based on the function arguments.
///
/// `#[component(reactive)]` wraps the body in [`reactive_component`] (requires `id: String`).
///
/// Field-level Solid alignment:
/// - `Signal` / closure / `Element` props are **excluded** from the PartialEq snapshot (static
///   props only trigger subtree refresh).
/// - The component effect uses [`isolate_reads`] so naked `signal.get()` in the body does **not**
///   re-subscribe the component boundary; prefer `ui!` auto `bind_*` / `<Show>` / `<For>` for
///   field updates.
#[proc_macro_attribute]
pub fn component(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let input = syn::parse_macro_input!(item as syn::ItemFn);
    let vis = &input.vis;
    let name = &input.sig.ident;
    let generics = &input.sig.generics;
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    let fn_docs: Vec<_> = input
        .attrs
        .iter()
        .filter(|a| a.path().is_ident("doc"))
        .collect();
    let props_name = syn::Ident::new(&format!("{}Props", name), name.span());
    let snapshot_name = syn::Ident::new(&format!("__{name}ReactiveSnapshot"), name.span());

    let mut fields = Vec::new();
    let mut field_names = Vec::new();
    let mut field_unwrapping = Vec::new();
    let mut snapshot_fields = Vec::new();
    let mut snapshot_inits = Vec::new();
    let mut snapshot_assigns = Vec::new();
    let mut has_base_builder_arg = false;
    let mut has_id_arg = false;

    let mut reactive = false;
    let mut crate_path = quote! { rui };
    if !_attr.is_empty() {
        let attr_parser = syn::meta::parser(|meta| {
            if meta.path.is_ident("reactive") {
                reactive = true;
                Ok(())
            } else if meta.path.is_ident("crate_path") {
                let value = meta.value()?;
                let s: syn::LitStr = value.parse()?;
                let path: syn::Path = s.parse()?;
                crate_path = quote! { #path };
                Ok(())
            } else {
                Err(meta.error(
                    "unsupported #[component] attribute; expected `reactive` or `crate_path = \"...\"`",
                ))
            }
        });
        syn::parse_macro_input!(_attr with attr_parser);
    }

    for arg in &input.sig.inputs {
        if let syn::FnArg::Typed(pat_type) = arg
            && let syn::Pat::Ident(pat_ident) = &*pat_type.pat
        {
            let name = &pat_ident.ident;
            let ty = &pat_type.ty;
            let name_str = name.to_string();
            let ty_str = quote!(#ty).to_string();
            let is_already_option = ty_str.starts_with("Option");
            let arg_docs: Vec<_> = pat_type
                .attrs
                .iter()
                .filter(|a| a.path().is_ident("doc"))
                .collect();
            if name_str == "base_builder" {
                has_base_builder_arg = true;
            }
            if name_str == "id" {
                has_id_arg = true;
            }

            field_names.push(name.clone());

            if name_str == "id" {
                fields.push(quote! { #(#arg_docs)* pub #name: String });
                field_unwrapping.push(quote! { let #name = #name; });
            } else if is_already_option {
                fields.push(quote! { #(#arg_docs)* pub #name: #ty });
                let mut_tok = if pat_ident.mutability.is_some() {
                    quote! { mut }
                } else {
                    quote! {}
                };
                field_unwrapping.push(quote! { let #mut_tok #name = #name; });
            } else if ty_str.contains("Signal") {
                // Solid getter-props: required Signal props (no Default). Missing → panic with name.
                fields.push(quote! { #(#arg_docs)* pub #name: Option<#ty> });
                let mut_tok = if pat_ident.mutability.is_some() {
                    quote! { mut }
                } else {
                    quote! {}
                };
                let missing = format!("missing required Signal prop `{name_str}`");
                field_unwrapping.push(quote! {
                    let #mut_tok #name = #name.expect(#missing);
                });
            } else {
                fields.push(quote! { #(#arg_docs)* pub #name: Option<#ty> });
                let mut_tok = if pat_ident.mutability.is_some() {
                    quote! { mut }
                } else {
                    quote! {}
                };
                field_unwrapping.push(quote! { let #mut_tok #name = #name.unwrap_or_default(); });
            }

            if reactive && !reactive_snapshot_skip(&name_str, &ty_str) {
                // After field unwrapping, non-Option params become T; Option params stay Option<T>.
                let snapshot_ty = quote! { #ty };
                snapshot_fields.push(quote! { #name: #snapshot_ty });
                snapshot_inits.push(quote! { #name: #name.clone() });
                snapshot_assigns.push(quote! { let #name = __snap.#name.clone(); });
            }
        }
    }

    if reactive && !has_id_arg {
        return quote! {
            compile_error!("#[component(reactive)] requires an `id: String` parameter")
        }
        .into();
    }

    // Add children if not present
    if !field_names.iter().any(|n| *n == "children") {
        fields.push(quote! { pub children: Option<#crate_path::core::tree::Element> });
        field_names.push(syn::Ident::new("children", proc_macro2::Span::call_site()));
        field_unwrapping.push(quote! { let children = children; });
    }

    // Add base prop for styling/layout
    fields.push(quote! { pub base: Option<Box<dyn FnOnce(#crate_path::components::base::BaseComponentBuilder) -> #crate_path::components::base::BaseComponentBuilder>> });
    field_names.push(syn::Ident::new("base", proc_macro2::Span::call_site()));
    field_unwrapping.push(quote! { let mut base = base; });

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

    let fn_body = if reactive {
        quote! {
            #(#field_unwrapping)*
            #crate_path::app::reactive::stash_component_children(&id, children);
            #[derive(Clone, PartialEq)]
            struct #snapshot_name #impl_generics #where_clause {
                #(#snapshot_fields),*
            }
            let __snapshot = #snapshot_name {
                #(#snapshot_inits),*
            };
            #crate_path::app::reactive::reactive_component(
                format!("{id}__component"),
                __snapshot,
                move |__snap| {
                    #(#snapshot_assigns)*
                    let children = #crate_path::app::reactive::take_component_children(&id);
                    #merge_base_stmt
                    let mut __el = { #body };
                    if let Some(base_fn) = base.take() {
                        let builder = #crate_path::components::base::BaseComponentBuilder::new(__el);
                        __el = base_fn(builder).el;
                    }
                    __el
                },
            )
        }
    } else {
        quote! {
            #(#field_unwrapping)*
            #merge_base_stmt
            let mut __el = { #body };
            #post_apply_base_stmt
            __el
        }
    };

    let expanded = quote! {
        #(#fn_docs)*
        #[derive(Default)]
        #vis struct #props_name #impl_generics #where_clause {
            #(#fields),*
        }

        #(#fn_docs)*
        #vis fn #name #impl_generics (props: #props_name #ty_generics) -> #crate_path::core::tree::Element
        #where_clause
        {
            #vis use #props_name;
            let #props_name { #(#field_names),* } = props;
            #fn_body
        }
    };

    expanded.into()
}

fn reactive_snapshot_skip(name: &str, ty_str: &str) -> bool {
    if matches!(name, "id" | "children" | "base" | "base_builder") {
        return true;
    }
    ty_str.contains("Signal")
        || ty_str.contains("Element")
        || ty_str.contains("Box<dyn")
        || ty_str.contains("FnOnce")
        || ty_str.contains("FnMut")
        || ty_str.contains("Fn(")
        || ty_str.contains("Handler")
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

/// Best-effort AST heuristic for Solid-style signal reads.
///
/// Tightened vs a bare zero-arg `.get()` scan:
/// - receiver must be a path / field / index / paren / reference chain (not a call result);
/// - also treats `.get_untracked()` as non-subscribing (ignored);
/// - ignores `.get(...)` with arguments (e.g. `HashMap::get`).
fn expr_reads_get(expr: &Expr) -> bool {
    struct Finder {
        found: bool,
    }
    impl Finder {
        fn receiver_ok(expr: &Expr) -> bool {
            match expr {
                Expr::Path(_) | Expr::Field(_) | Expr::Index(_) => true,
                Expr::Paren(p) => Self::receiver_ok(&p.expr),
                Expr::Group(g) => Self::receiver_ok(&g.expr),
                Expr::Reference(r) => Self::receiver_ok(&r.expr),
                Expr::Try(t) => Self::receiver_ok(&t.expr),
                Expr::MethodCall(m) => {
                    // Allow chains like `self.count.get()` / `state.foo.get()`.
                    Self::receiver_ok(&m.receiver)
                }
                _ => false,
            }
        }
    }
    impl Visit<'_> for Finder {
        fn visit_expr_method_call(&mut self, node: &syn::ExprMethodCall) {
            if self.found {
                return;
            }
            if node.method == "get" && node.args.is_empty() && Self::receiver_ok(&node.receiver) {
                self.found = true;
                return;
            }
            syn::visit::visit_expr_method_call(self, node);
        }
    }
    let mut finder = Finder { found: false };
    finder.visit_expr(expr);
    finder.found
}

fn expand_node(
    node: &RsxNode,
    node_counter: &mut usize,
    in_reactive_boundary: bool,
) -> proc_macro2::TokenStream {
    match node {
        RsxNode::Expr(expr) => {
            // Solid-style: dynamic text children that read `.get()` become in-place bindings
            // instead of subscribing the surrounding (often root) effect.
            if !in_reactive_boundary && expr_reads_get(expr) {
                let idx = *node_counter;
                *node_counter += 1;
                return quote! {
                    rui::app::reactive::bind_text(
                        format!("{}:{}:bind#{}", file!(), line!(), #idx),
                        move || {
                            let __rui_text = #expr;
                            __rui_text.to_string()
                        },
                    )
                };
            }
            quote! { #expr }
        }
        RsxNode::Element(el) => {
            let name_str = el.name.to_string();
            if name_str == "Reactive"
                || name_str == "Show"
                || name_str == "Popover"
                || name_str == "ForKeyed"
                || name_str == "For"
                || name_str == "Fragment"
                || name_str == "Dynamic"
            {
                let get_attr = |name: &str| -> Option<&Expr> {
                    el.attributes
                        .iter()
                        .find(|(k, _)| k == &syn::Ident::new(name, proc_macro2::Span::call_site()))
                        .map(|(_, v)| v)
                };

                if name_str == "Fragment" {
                    let mut expanded_children = Vec::new();
                    for child in &el.children {
                        expanded_children.push(expand_node(
                            child,
                            node_counter,
                            in_reactive_boundary,
                        ));
                    }
                    // Flat multi-child wrapper; runtime unpacks via `rui_fragment`.
                    return quote! {
                        {
                            use rui::core::tree::IntoElementList;
                            let mut __list: Vec<rui::core::tree::Element> = Vec::new();
                            #( __list.extend((#expanded_children).into_element_list()); )*
                            let mut __frag = rui::core::tree::Element::new();
                            __frag.set_attr("rui_fragment", "1");
                            __frag.children(__list)
                        }
                    };
                }

                if name_str == "Dynamic" {
                    let Some(component_expr) = get_attr("component") else {
                        return quote! { compile_error!("<Dynamic> requires `component={...}` attribute"); };
                    };
                    let mut expanded_children = Vec::new();
                    for child in &el.children {
                        expanded_children.push(expand_node(
                            child,
                            node_counter,
                            in_reactive_boundary,
                        ));
                    }
                    if expanded_children.is_empty() {
                        return quote! {
                            {
                                use rui::core::tree::IntoDynamicComponent;
                                (#component_expr).into_dynamic_component()
                            }
                        };
                    }
                    return quote! {
                        {
                            use rui::core::tree::{IntoDynamicComponent, IntoElementList};
                            let mut __rui_dyn = (#component_expr).into_dynamic_component();
                            let mut __list: Vec<rui::core::tree::Element> = Vec::new();
                            #( __list.extend((#expanded_children).into_element_list()); )*
                            __rui_dyn = __rui_dyn.children(__list);
                            __rui_dyn
                        }
                    };
                }

                if name_str == "Reactive" {
                    let Some(id_expr) = get_attr("id") else {
                        return quote! { compile_error!("<Reactive> requires `id={...}` attribute"); };
                    };
                    let mut expanded_children = Vec::new();
                    for child in &el.children {
                        expanded_children.push(expand_node(child, node_counter, true));
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
                        expanded_children.push(expand_node(
                            child,
                            node_counter,
                            in_reactive_boundary,
                        ));
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
                    // Solid-style: when `when` reads `.get()`, subscribe in a dedicated reactive_view.
                    if expr_reads_get(when_expr) {
                        return quote! {
                            rui::app::reactive::show_reactive(
                                format!("{}:{}:Show#{}", file!(), line!(), #idx),
                                move || { #when_expr },
                                move || { #child_el },
                            )
                        };
                    }
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
                        // Body is rendered inside Popover's reactive_view effect.
                        expanded_children.push(expand_node(child, node_counter, true));
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
                            rui::components::popover_panel(#open_expr).child(move || #child_el)
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
                let each_expr_opt = get_attr("each")
                    .or_else(|| get_attr("of"))
                    .or_else(|| get_attr("in"));
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
                        expanded_children.push(expand_node(
                            child,
                            node_counter,
                            in_reactive_boundary,
                        ));
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

                let each_is_reactive = expr_reads_get(each_expr);
                let items_scope_ts = if each_is_reactive {
                    quote! { format!("{}:items", #scope_ts) }
                } else {
                    quote! { #scope_ts }
                };
                let emit_for =
                    |base: proc_macro2::TokenStream, fallback: Option<proc_macro2::TokenStream>| {
                        let list_body = if let Some(fallback_ts) = fallback {
                            quote! {{
                                use rui::core::tree::IntoElementList;
                                let __out = #base;
                                if __out.is_empty() { (#fallback_ts).into_element_list() } else { __out }
                            }}
                        } else {
                            quote! {{ #base }}
                        };
                        if each_is_reactive {
                            quote! {
                                rui::app::reactive::for_reactive(
                                    #scope_ts,
                                    move || #list_body,
                                )
                            }
                        } else {
                            list_body
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
                                #items_scope_ts,
                                #each_expr,
                                |__rui_for_item| {
                                    let #item_ident = __rui_for_item;
                                    #key_ts
                                },
                                #render_ts,
                            )
                        };
                        return emit_for(base, maybe_fallback_ts.clone());
                    } else {
                        let base = quote! {
                            rui::app::reactive::for_keyed_iter(
                                #items_scope_ts,
                                #each_expr,
                                #key_ts,
                                #render_ts,
                            )
                        };
                        return emit_for(base, maybe_fallback_ts.clone());
                    }
                } else {
                    let base = quote! {
                        rui::app::reactive::for_each_iter(
                            #items_scope_ts,
                            #each_expr,
                            #render_ts,
                        )
                    };
                    return emit_for(base, maybe_fallback_ts);
                }
            }
            let is_custom = name_str.chars().next().is_some_and(|c| c.is_uppercase())
                && ![
                    "VStack",
                    "HStack",
                    "GStack",
                    "Grid",
                    "Rect",
                    "Icon",
                    "Box",
                    "Text",
                    "Interactive",
                ]
                .contains(&name_str.as_str());

            let mut element = if is_custom {
                let name_ident = &el.name;
                let props_ident = syn::Ident::new(&format!("{}Props", name_str), name_ident.span());

                let mut prop_names = Vec::new();
                let mut prop_values = Vec::new();
                let mut base_calls = Vec::new();
                let mut chrome_binds: Vec<proc_macro2::TokenStream> = Vec::new();
                let mut spread_expr: Option<&Expr> = None;
                let mut ref_expr: Option<&Expr> = None;

                for (attr, value) in &el.attributes {
                    let attr_str = attr.to_string();
                    let mut attr_name = attr_str.as_str();
                    if attr_name.starts_with("el.") {
                        attr_name = &attr_name[3..];
                    }

                    if matches!(attr_name, "spread" | "__spread") {
                        spread_expr = Some(value);
                        continue;
                    }
                    if attr_name == "ref" {
                        ref_expr = Some(value);
                        continue;
                    }

                    // Button / Input chrome attrs → bind_attr (Solid-style, no rebuild).
                    if !in_reactive_boundary
                        && matches!(name_str.as_str(), "Button" | "Input")
                        && matches!(attr_name, "variant" | "disabled" | "invalid")
                        && expr_reads_get(value)
                    {
                        let idx = *node_counter;
                        *node_counter += 1;
                        if name_str == "Button" && attr_name == "variant" {
                            prop_names.push(attr.clone());
                            prop_values.push(quote! {
                                Some(rui::core::reactivity::untrack(|| (#value).to_string()).into())
                            });
                            chrome_binds.push(quote! {
                                __rui_el = rui::app::reactive::bind_attr(
                                    __rui_el,
                                    format!("{}:{}:bind_btn_variant#{}", file!(), line!(), #idx),
                                    move || (#value).to_string(),
                                    |el, v| rui::components::apply_bound_button_variant(el, v),
                                );
                            });
                            continue;
                        }
                        if name_str == "Button" && attr_name == "disabled" {
                            prop_names.push(attr.clone());
                            prop_values.push(quote! {
                                Some(rui::core::reactivity::untrack(|| #value))
                            });
                            chrome_binds.push(quote! {
                                __rui_el = rui::app::reactive::bind_attr(
                                    __rui_el,
                                    format!("{}:{}:bind_btn_disabled#{}", file!(), line!(), #idx),
                                    move || #value,
                                    |el, d| rui::components::apply_bound_button_disabled(el, d),
                                );
                            });
                            continue;
                        }
                        if name_str == "Input" && attr_name == "invalid" {
                            prop_names.push(attr.clone());
                            prop_values.push(quote! {
                                Some(rui::core::reactivity::untrack(|| #value))
                            });
                            chrome_binds.push(quote! {
                                __rui_el = rui::app::reactive::bind_attr(
                                    __rui_el,
                                    format!("{}:{}:bind_input_invalid#{}", file!(), line!(), #idx),
                                    move || #value,
                                    |el, v| rui::components::apply_bound_input_invalid(el, v),
                                );
                            });
                            continue;
                        }
                        if name_str == "Input" && attr_name == "disabled" {
                            prop_names.push(attr.clone());
                            prop_values.push(quote! {
                                Some(rui::core::reactivity::untrack(|| #value))
                            });
                            chrome_binds.push(quote! {
                                __rui_el = rui::app::reactive::bind_attr(
                                    __rui_el,
                                    format!("{}:{}:bind_input_disabled#{}", file!(), line!(), #idx),
                                    move || #value,
                                    |el, d| rui::components::apply_bound_input_disabled(el, d),
                                );
                            });
                            continue;
                        }
                    }

                    let is_base = matches!(
                        attr_name,
                        "w" | "width"
                            | "max_w"
                            | "maxWidth"
                            | "max_width"
                            | "min_w"
                            | "minWidth"
                            | "min_width"
                            | "h"
                            | "height"
                            | "max_h"
                            | "maxHeight"
                            | "max_height"
                            | "min_h"
                            | "minHeight"
                            | "min_height"
                            | "p"
                            | "padding"
                            | "mx"
                            | "my"
                            | "px"
                            | "py"
                            | "pt"
                            | "pr"
                            | "pb"
                            | "pl"
                            | "mt"
                            | "mr"
                            | "mb"
                            | "ml"
                            | "rounded"
                            | "bg"
                            | "color"
                            | "text_color"
                            | "font_weight"
                            | "text_weight"
                            | "flex_grow"
                            | "flex_shrink"
                            | "grow"
                            | "shrink"
                            | "flex_basis"
                            | "absolute"
                            | "relative"
                            | "top"
                            | "right"
                            | "bottom"
                            | "left"
                            | "inset"
                            | "z_index"
                            | "cols"
                            | "grid_cols"
                            | "rows"
                            | "grid_rows"
                            | "col_span"
                            | "grid_column_span"
                            | "row_span"
                            | "grid_row_span"
                            | "align_self"
                            | "text_size"
                            | "leading"
                            | "text_leading"
                            | "line_height"
                            | "lineHeight"
                            | "underline"
                            | "text_decoration"
                            | "textDecoration"
                            | "text_decoration_line"
                            | "textDecorationLine"
                            | "text_decoration_style"
                            | "textDecorationStyle"
                            | "text_decoration_color"
                            | "textDecorationColor"
                            | "text_decoration_thickness"
                            | "textDecorationThickness"
                            | "items"
                            | "wrap"
                            | "gap"
                            | "border_style"
                            | "borderStyle"
                            | "border_color"
                            | "borderColor"
                            | "border_width"
                            | "borderWidth"
                            | "outline_style"
                            | "outlineStyle"
                            | "outline_color"
                            | "outlineColor"
                            | "outline_width"
                            | "outlineWidth"
                            | "shadow"
                            | "box_shadow"
                            | "boxShadow"
                            | "box-shadow"
                            | "box_shadow_color"
                            | "boxShadowColor"
                            | "box_shadow_blur"
                            | "boxShadowBlur"
                            | "box_shadow_offset_x"
                            | "boxShadowOffsetX"
                            | "box_shadow_offset_y"
                            | "boxShadowOffsetY"
                            | "box_shadow_spread"
                            | "boxShadowSpread"
                            | "box_shadow_inset"
                            | "boxShadowInset"
                            | "text_shadow"
                            | "textShadow"
                            | "text-shadow"
                            | "text_shadow_color"
                            | "textShadowColor"
                            | "text_shadow_blur"
                            | "textShadowBlur"
                            | "text_shadow_offset_x"
                            | "textShadowOffsetX"
                            | "text_shadow_offset_y"
                            | "textShadowOffsetY"
                            | "text_align"
                            | "class"
                            | "className"
                            | "tw"
                            | "uno"
                    );

                    if is_base {
                        // Dynamic class / color base attrs → bind on resulting Element.
                        if !in_reactive_boundary && expr_reads_get(value) {
                            let idx = *node_counter;
                            *node_counter += 1;
                            if matches!(attr_name, "class" | "className" | "tw" | "uno") {
                                chrome_binds.push(quote! {
                                    __rui_el = rui::app::reactive::bind_class(
                                        __rui_el,
                                        format!("{}:{}:bind_class#{}", file!(), line!(), #idx),
                                        move || (#value).to_string(),
                                    );
                                });
                                continue;
                            }
                            if attr_name == "bg" {
                                chrome_binds.push(quote! {
                                    __rui_el = rui::app::reactive::bind_prop(
                                        __rui_el,
                                        format!("{}:{}:bind_bg#{}", file!(), line!(), #idx),
                                        move || {
                                            let __v = (#value).to_string();
                                            rui::core::tree::parse_color(__v.as_str())
                                        },
                                        |el, color| {
                                            if el.style.bg_color == color {
                                                rui::core::tree::LayoutStatus::Clean
                                            } else {
                                                el.style.bg_color = color;
                                                rui::core::tree::LayoutStatus::StyleChanged
                                            }
                                        },
                                    );
                                });
                                continue;
                            }
                            if matches!(attr_name, "color" | "text_color") {
                                chrome_binds.push(quote! {
                                    __rui_el = rui::app::reactive::bind_prop(
                                        __rui_el,
                                        format!("{}:{}:bind_color#{}", file!(), line!(), #idx),
                                        move || {
                                            let __v = (#value).to_string();
                                            rui::core::tree::parse_text_color(__v.as_str())
                                        },
                                        |el, color| {
                                            if el.text_style.color == color {
                                                rui::core::tree::LayoutStatus::Clean
                                            } else {
                                                el.text_style.color = color;
                                                rui::core::tree::LayoutStatus::StyleChanged
                                            }
                                        },
                                    );
                                });
                                continue;
                            }
                            if matches!(attr_name, "border_color" | "borderColor") {
                                chrome_binds.push(quote! {
                                    __rui_el = rui::app::reactive::bind_prop(
                                        __rui_el,
                                        format!("{}:{}:bind_border_color#{}", file!(), line!(), #idx),
                                        move || {
                                            let __v = (#value).to_string();
                                            rui::core::tree::parse_color(__v.as_str())
                                        },
                                        |el, color| {
                                            if el.style.border_color == color {
                                                rui::core::tree::LayoutStatus::Clean
                                            } else {
                                                el.style.border_color = color;
                                                rui::core::tree::LayoutStatus::StyleChanged
                                            }
                                        },
                                    );
                                });
                                continue;
                            }
                        }
                        if attr_name == "class"
                            || attr_name == "className"
                            || attr_name == "tw"
                            || attr_name == "uno"
                        {
                            // Apply via Element path (shared runtime catalog) after the component
                            // builds — BaseComponentBuilder cannot consume `with_class_list`.
                            for warn in parse_utility_unknown_warnings(value) {
                                base_calls.push(warn);
                            }
                            if let Some(lit) = class_lit_str(value) {
                                chrome_binds.push(quote! {
                                    __rui_el = rui::style::with_class_list(__rui_el, #lit);
                                });
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
                                _ => {
                                    quote! { b = b.leading(rui::core::tree::IntoF32::into_f32(#value)); }
                                }
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
                        } else if matches!(
                            attr_name,
                            "on_click" | "onClick" | "on_change" | "onChange"
                        ) {
                            quote! { Some(rui::core::tree::Handler::from(rui::core::tree::ZeroArgFn(#value))) }
                        } else if !in_reactive_boundary && expr_reads_get(value) {
                            // Avoid subscribing the parent effect during props construction.
                            quote! { Some(rui::core::reactivity::untrack(|| (#value).into())) }
                        } else {
                            quote! { Some((#value).into()) }
                        };
                        prop_names.push(final_attr);
                        prop_values.push(final_value);
                    }
                }

                if name_str == "Input" && !prop_names.iter().any(|p| *p == "id") {
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
                        expanded_children.push(expand_node(
                            child,
                            node_counter,
                            in_reactive_boundary,
                        ));
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
                            })
                        };
                    }
                }

                let rest_ts = if let Some(spread) = spread_expr {
                    quote! { ..#spread }
                } else {
                    quote! { ..Default::default() }
                };

                let ref_bind_ts = if let Some(ref_target) = ref_expr {
                    let idx = *node_counter;
                    *node_counter += 1;
                    quote! {
                        if __rui_el.template_id.is_none() {
                            __rui_el = __rui_el.template_id(
                                format!("{}:{}:ref#{}", file!(), line!(), #idx),
                            );
                        }
                        rui::core::tree::bind_element_ref(&(#ref_target), &__rui_el);
                    }
                } else {
                    quote! {}
                };

                if chrome_binds.is_empty() && ref_expr.is_none() {
                    return quote! {
                        {
                            #[allow(clippy::needless_update)]
                            let __rui_comp = #name_ident(#props_ident {
                                base: #base_expr,
                                children: #children_expr,
                                #(#prop_names: #prop_values,)*
                                #rest_ts
                            });
                            __rui_comp
                        }
                    };
                }
                return quote! {
                    {
                        #[allow(clippy::needless_update)]
                        let mut __rui_el: rui::core::tree::Element = #name_ident(#props_ident {
                            base: #base_expr,
                            children: #children_expr,
                            #(#prop_names: #prop_values,)*
                            #rest_ts
                        }).into();
                        #(#chrome_binds)*
                        #ref_bind_ts
                        __rui_el
                    }
                };
            } else {
                match name_str.as_str() {
                    "VStack" => quote! { rui::primitives::vstack() },
                    "HStack" => quote! { rui::primitives::hstack() },
                    // Grid 为 GStack 的 HTML/Solid 风格别名。
                    "GStack" | "Grid" => quote! { rui::primitives::gstack() },
                    "Box" | "Rect" => quote! { rui::primitives::r#box() },
                    "Interactive" => {
                        quote! { rui::primitives::interactive(rui::core::tree::Element::new()) }
                    }
                    "Text" => quote! { rui::primitives::text("") },
                    _ => quote! { rui::core::tree::Element::new() },
                }
            };

            let mut pending_icon_bind: Option<proc_macro2::TokenStream> = None;
            if name_str == "Icon" {
                let mut icon_code = "ea0e".to_string(); // fallback to an album icon
                let mut dynamic_name: Option<&syn::Expr> = None;
                for (attr, value) in &el.attributes {
                    if *attr == "name" {
                        if !in_reactive_boundary && expr_reads_get(value) {
                            dynamic_name = Some(value);
                            continue;
                        }
                        if let syn::Expr::Lit(expr_lit) = value
                            && let syn::Lit::Str(lit_str) = &expr_lit.lit
                        {
                            let name_str = lit_str.value();
                            let json_str = include_str!("../../rui/assets/lucide.json");
                            static ICONS: std::sync::OnceLock<
                                std::collections::HashMap<String, String>,
                            > = std::sync::OnceLock::new();
                            let map = ICONS
                                .get_or_init(|| serde_json::from_str(json_str).unwrap_or_default());
                            if let Some(code) = map.get(&name_str) {
                                icon_code = code.clone();
                            } else {
                                panic!(
                                    "Icon '{}' not found in lucide.json. Ensure the name is correct.",
                                    name_str
                                );
                            }
                        }
                    }
                }

                if let Some(name_expr) = dynamic_name {
                    let idx = *node_counter;
                    *node_counter += 1;
                    // Initial glyph via untrack; subsequent updates via bind_attr.
                    element = quote! {
                        {
                            let __glyph = rui::core::reactivity::untrack(|| {
                                let __n = (#name_expr).to_string();
                                rui::utils::lucide_glyph(__n.as_str())
                                    .unwrap_or_else(|| "\u{ea0e}".to_string())
                            });
                            rui::utils::text_node::build_icon_text_node(__glyph)
                        }
                    };
                    pending_icon_bind = Some(quote! {
                        __rui_el = rui::app::reactive::bind_attr(
                            __rui_el,
                            format!("{}:{}:bind_icon_name#{}", file!(), line!(), #idx),
                            move || (#name_expr).to_string(),
                            |el, name| rui::utils::apply_bound_icon_name(el, name),
                        );
                    });
                } else if let Ok(cp) = u32::from_str_radix(&icon_code, 16)
                    && let Some(c) = std::char::from_u32(cp)
                {
                    let c_str = c.to_string();
                    element = quote! { rui::utils::text_node::build_icon_text_node(#c_str) };
                }
            }

            let mut utility_calls: Vec<proc_macro2::TokenStream> = Vec::new();
            let mut dynamic_binds: Vec<proc_macro2::TokenStream> = Vec::new();
            let mut unknown_attr_warnings: Vec<proc_macro2::TokenStream> = Vec::new();
            if let Some(icon_bind) = pending_icon_bind {
                dynamic_binds.push(icon_bind);
            }
            for (attr, value) in &el.attributes {
                let attr_str = attr.to_string();
                if matches!(attr_str.as_str(), "class" | "className" | "tw" | "uno") {
                    if !in_reactive_boundary && expr_reads_get(value) {
                        let idx = *node_counter;
                        *node_counter += 1;
                        dynamic_binds.push(quote! {
                            __rui_el = rui::app::reactive::bind_class(
                                __rui_el,
                                format!("{}:{}:bind_class#{}", file!(), line!(), #idx),
                                move || (#value).to_string(),
                            );
                        });
                    } else {
                        utility_calls.extend(parse_utility_calls(value));
                    }
                    continue;
                }
                if attr_str == "ref" {
                    let idx = *node_counter;
                    *node_counter += 1;
                    dynamic_binds.push(quote! {
                        if __rui_el.template_id.is_none() {
                            __rui_el = __rui_el.template_id(
                                format!("{}:{}:ref#{}", file!(), line!(), #idx),
                            );
                        }
                        rui::core::tree::bind_element_ref(&(#value), &__rui_el);
                    });
                    continue;
                }
                if attr_str == "id"
                    && matches!(
                        name_str.as_str(),
                        "ScrollArea"
                            | "ScrollViewport"
                            | "Box"
                            | "Rect"
                            | "VStack"
                            | "HStack"
                            | "GStack"
                            | "Grid"
                    )
                {
                    element = quote! { #element.id(#value) };
                    continue;
                }
                if matches!(attr_str.as_str(), "scroll_id" | "scrollId") {
                    element = quote! { #element.scroll_id(#value) };
                    continue;
                }
                match attr_str.as_str() {
                    "w" | "width" => element = apply_min_max_size_expr(element, value, "w"),
                    "max_w" | "maxWidth" | "max_width" => {
                        element = quote! { #element.max_w(#value) }
                    }
                    "min_w" | "minWidth" | "min_width" => {
                        element = quote! { #element.min_w(#value) }
                    }
                    "inline_size" | "inlineSize" => {
                        element = apply_min_max_size_expr(element, value, "w")
                    }
                    "h" | "height" => element = apply_min_max_size_expr(element, value, "h"),
                    "max_h" | "maxHeight" | "max_height" => {
                        element = quote! { #element.max_h(#value) }
                    }
                    "min_h" | "minHeight" | "min_height" => {
                        element = quote! { #element.min_h(#value) }
                    }
                    "block_size" | "blockSize" => {
                        element = apply_min_max_size_expr(element, value, "h")
                    }
                    "bg" => {
                        if !in_reactive_boundary && expr_reads_get(value) {
                            let idx = *node_counter;
                            *node_counter += 1;
                            dynamic_binds.push(quote! {
                                __rui_el = rui::app::reactive::bind_prop(
                                    __rui_el,
                                    format!("{}:{}:bind_bg#{}", file!(), line!(), #idx),
                                    move || {
                                        let __v = (#value).to_string();
                                        rui::core::tree::parse_color(__v.as_str())
                                    },
                                    |el, color| {
                                        if el.style.bg_color == color {
                                            rui::core::tree::LayoutStatus::Clean
                                        } else {
                                            el.style.bg_color = color;
                                            rui::core::tree::LayoutStatus::StyleChanged
                                        }
                                    },
                                );
                            });
                        } else {
                            element = quote! { #element.bg(rui::core::tree::parse_color(#value)) };
                        }
                    }
                    "border_style" | "borderStyle" => {
                        element = quote! { #element.border_style(#value) }
                    }
                    "border_color" | "borderColor" => {
                        if !in_reactive_boundary && expr_reads_get(value) {
                            let idx = *node_counter;
                            *node_counter += 1;
                            dynamic_binds.push(quote! {
                                __rui_el = rui::app::reactive::bind_prop(
                                    __rui_el,
                                    format!("{}:{}:bind_border_color#{}", file!(), line!(), #idx),
                                    move || {
                                        let __v = (#value).to_string();
                                        rui::core::tree::parse_color(__v.as_str())
                                    },
                                    |el, color| {
                                        if el.style.border_color == color {
                                            rui::core::tree::LayoutStatus::Clean
                                        } else {
                                            el.style.border_color = color;
                                            rui::core::tree::LayoutStatus::StyleChanged
                                        }
                                    },
                                );
                            });
                        } else {
                            element = quote! { #element.border_color(rui::core::tree::parse_color(#value)) };
                        }
                    }
                    "border_width" | "borderWidth" => {
                        element = quote! { #element.border_width(rui::core::tree::IntoF32::into_f32(#value)) }
                    }
                    "outline_style" | "outlineStyle" => {
                        element = quote! { #element.outline_style(#value) }
                    }
                    "outline_color" | "outlineColor" => {
                        element =
                            quote! { #element.outline_color(rui::core::tree::parse_color(#value)) }
                    }
                    "outline_width" | "outlineWidth" => {
                        element = quote! { #element.outline_width(rui::core::tree::IntoF32::into_f32(#value)) }
                    }
                    "color" | "text_color" => {
                        if !in_reactive_boundary && expr_reads_get(value) {
                            let idx = *node_counter;
                            *node_counter += 1;
                            dynamic_binds.push(quote! {
                                __rui_el = rui::app::reactive::bind_prop(
                                    __rui_el,
                                    format!("{}:{}:bind_color#{}", file!(), line!(), #idx),
                                    move || {
                                        let __v = (#value).to_string();
                                        rui::core::tree::parse_text_color(__v.as_str())
                                    },
                                    |el, color| {
                                        if el.text_style.color == color {
                                            rui::core::tree::LayoutStatus::Clean
                                        } else {
                                            el.text_style.color = color;
                                            rui::core::tree::LayoutStatus::StyleChanged
                                        }
                                    },
                                );
                            });
                        } else {
                            element = quote! { #element.text_color(rui::core::tree::parse_text_color(#value)) };
                        }
                    }
                    "font_weight" | "text_weight" => {
                        element = quote! { #element.font_weight(rui::core::tree::IntoF32::into_f32(#value)) }
                    }
                    "underline" => element = quote! { #element.underline(#value) },
                    "text_decoration" | "textDecoration" | "decoration" => {
                        element = quote! { #element.text_decoration(#value) }
                    }
                    "text_decoration_line" | "textDecorationLine" => {
                        element = quote! { #element.text_decoration_line(#value) }
                    }
                    "text_size" | "textSize" | "size" if name_str == "Label" => {
                        element = quote! { #element.text_size(#value) }
                    }
                    "gap" => element = quote! { #element.gap(#value) },
                    "items" | "align" => element = quote! { #element.items(#value) },
                    "justify" => element = quote! { #element.justify(#value) },
                    "wrap" => element = quote! { #element.wrap(#value) },
                    "text_decoration_style" | "textDecorationStyle" => {
                        element = quote! { #element.text_decoration_style(#value) }
                    }
                    "text_decoration_color" | "textDecorationColor" => {
                        element = quote! { #element.text_decoration_color(rui::core::tree::parse_text_color(#value)) }
                    }
                    "text_decoration_thickness" | "textDecorationThickness" => {
                        element = quote! { #element.text_decoration_thickness(rui::core::tree::IntoF32::into_f32(#value)) }
                    }
                    "text_underline_offset" | "textUnderlineOffset" => {
                        element = quote! { #element.text_underline_offset(rui::core::tree::IntoF32::into_f32(#value)) }
                    }
                    "line_height" | "lineHeight" | "lh" | "text_line_height" => {
                        if is_custom {
                            element = quote! { #element.line_height(rui::core::tree::IntoF32::into_f32(#value)) }
                        } else {
                            element = quote! { #element.text_line_height(rui::core::tree::IntoF32::into_f32(#value)) }
                        }
                    }
                    "leading" | "text_leading" => {
                        let s = if let syn::Expr::Lit(expr_lit) = value {
                            if let syn::Lit::Str(lit_str) = &expr_lit.lit {
                                Some(lit_str.value())
                            } else {
                                None
                            }
                        } else {
                            None
                        };
                        let s = s.as_deref().unwrap_or("");
                        if s == "none" {
                            if is_custom {
                                element = quote! { #element.leading_none() }
                            } else {
                                element = quote! { #element.text_leading_none() }
                            }
                        } else if s == "tight" {
                            if is_custom {
                                element = quote! { #element.leading_tight() }
                            } else {
                                element = quote! { #element.text_leading_tight() }
                            }
                        } else if s == "normal" {
                            if is_custom {
                                element = quote! { #element.leading_normal() }
                            } else {
                                element = quote! { #element.text_leading_normal() }
                            }
                        } else if s == "relaxed" {
                            if is_custom {
                                element = quote! { #element.leading_relaxed() }
                            } else {
                                element = quote! { #element.text_leading_relaxed() }
                            }
                        } else if is_custom {
                            element = quote! { #element.leading(rui::core::tree::IntoF32::into_f32(#value)) }
                        } else {
                            element = quote! { #element.text_leading(rui::core::tree::IntoF32::into_f32(#value)) }
                        }
                    }
                    "size" => {
                        if is_custom || name_str == "Button" || name_str == "Input" {
                            element = quote! { #element.size(#value) }
                        } else {
                            element = quote! { #element.text_size(rui::core::tree::IntoF32::into_f32(#value)) }
                        }
                    }
                    "text_size" | "font_size" => {
                        element = quote! { #element.text_size(rui::core::tree::IntoF32::into_f32(#value)) }
                    }
                    "p" | "padding" => element = quote! { #element.p(#value) },
                    "px" | "padding_x" | "paddingX" => element = quote! { #element.px(#value) },
                    "py" | "padding_y" | "paddingY" => element = quote! { #element.py(#value) },
                    "pt" | "padding_top" | "paddingTop" => element = quote! { #element.pt(#value) },
                    "pr" | "padding_right" | "paddingRight" => {
                        element = quote! { #element.pr(#value) }
                    }
                    "pb" | "padding_bottom" | "paddingBottom" => {
                        element = quote! { #element.pb(#value) }
                    }
                    "pl" | "padding_left" | "paddingLeft" => {
                        element = quote! { #element.pl(#value) }
                    }
                    "padding_inline" | "paddingInline" => {
                        element = quote! { #element.padding_inline(#value) }
                    }
                    "padding_block" | "paddingBlock" => {
                        element = quote! { #element.padding_block(#value) }
                    }
                    "padding_inline_start" | "paddingInlineStart" => {
                        element = quote! { #element.padding_inline_start(#value) }
                    }
                    "padding_inline_end" | "paddingInlineEnd" => {
                        element = quote! { #element.padding_inline_end(#value) }
                    }
                    "padding_block_start" | "paddingBlockStart" => {
                        element = quote! { #element.padding_block_start(#value) }
                    }
                    "padding_block_end" | "paddingBlockEnd" => {
                        element = quote! { #element.padding_block_end(#value) }
                    }
                    "m" | "margin" => element = quote! { #element.m(#value) },
                    "overflow" => element = quote! { #element.overflow(#value) },
                    "overflow_x" | "overflowX" => element = quote! { #element.overflow_x(#value) },
                    "overflow_y" | "overflowY" => element = quote! { #element.overflow_y(#value) },
                    "scroll_behavior" | "scrollBehavior" => {
                        element = quote! { #element.scroll_behavior(#value) }
                    }
                    "hover_mode" | "hoverMode" => element = quote! { #element.hover_mode(#value) },
                    "scrollbar_visibility" | "scrollbarVisibility" => {
                        element = quote! { #element.scrollbar_visibility(#value) }
                    }
                    "scrollbar_inset" | "scrollbarInset" => {
                        element = quote! { #element.scrollbar_inset(#value) }
                    }
                    "scrollbar_inset_x" | "scrollbarInsetX" => {
                        element = quote! { #element.scrollbar_inset_x(#value) }
                    }
                    "scrollbar_inset_y" | "scrollbarInsetY" => {
                        element = quote! { #element.scrollbar_inset_y(#value) }
                    }
                    "scrollbar_y_side" | "scrollbarYSide" => {
                        element = quote! { #element.scrollbar_y_side(#value) }
                    }
                    "scrollbar_x_side" | "scrollbarXSide" => {
                        element = quote! { #element.scrollbar_x_side(#value) }
                    }
                    "scrollbar_thickness" | "scrollbarThickness" => {
                        element = quote! { #element.scrollbar_thickness(rui::core::tree::IntoF32::into_f32(#value)) }
                    }
                    "scrollbar_variant" | "scrollbarVariant" => {
                        element = quote! { #element.scrollbar_variant(#value) }
                    }
                    "scrollbar_track_color" | "scrollbarTrackColor" => {
                        element = quote! { #element.scrollbar_track_color(#value) }
                    }
                    "scrollbar_thumb_color" | "scrollbarThumbColor" => {
                        element = quote! { #element.scrollbar_thumb_color(#value) }
                    }
                    "mx" | "margin_x" | "marginX" => element = quote! { #element.mx(#value) },
                    "my" | "margin_y" | "marginY" => element = quote! { #element.my(#value) },
                    "mt" | "margin_top" | "marginTop" => element = quote! { #element.mt(#value) },
                    "mr" | "margin_right" | "marginRight" => {
                        element = quote! { #element.mr(#value) }
                    }
                    "mb" | "margin_bottom" | "marginBottom" => {
                        element = quote! { #element.mb(#value) }
                    }
                    "ml" | "margin_left" | "marginLeft" => element = quote! { #element.ml(#value) },
                    "margin_inline" | "marginInline" => {
                        element = quote! { #element.margin_inline(#value) }
                    }
                    "margin_block" | "marginBlock" => {
                        element = quote! { #element.margin_block(#value) }
                    }
                    "margin_inline_start" | "marginInlineStart" => {
                        element = quote! { #element.margin_inline_start(#value) }
                    }
                    "margin_inline_end" | "marginInlineEnd" => {
                        element = quote! { #element.margin_inline_end(#value) }
                    }
                    "margin_block_start" | "marginBlockStart" => {
                        element = quote! { #element.margin_block_start(#value) }
                    }
                    "margin_block_end" | "marginBlockEnd" => {
                        element = quote! { #element.margin_block_end(#value) }
                    }
                    "rounded" | "radius" | "border_radius" | "borderRadius" => {
                        element =
                            quote! { #element.rounded(rui::core::tree::IntoF32::into_f32(#value)) }
                    }
                    "shadow" => {
                        element = quote! { #element.shadow([0.0, 0.0, 0.0, 0.5], rui::core::tree::IntoF32::into_f32(#value)) }
                    }
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
                    "text_shadow" => element = quote! { #element.text_shadow(#value) },
                    "textShadow" | "text-shadow" => {
                        element = quote! { #element.text_shadow(#value) }
                    }
                    "text_shadow_color" | "textShadowColor" => {
                        element = quote! { #element.text_shadow_color(rui::core::tree::parse_text_color(#value)) }
                    }
                    "text_shadow_blur" | "textShadowBlur" => {
                        element = quote! { #element.text_shadow_blur(rui::core::tree::IntoF32::into_f32(#value)) }
                    }
                    "text_shadow_offset_x" | "textShadowOffsetX" => {
                        element = quote! { #element.text_shadow_offset_x(rui::core::tree::IntoF32::into_f32(#value)) }
                    }
                    "text_shadow_offset_y" | "textShadowOffsetY" => {
                        element = quote! { #element.text_shadow_offset_y(rui::core::tree::IntoF32::into_f32(#value)) }
                    }
                    "text_align" => element = quote! { #element.text_align(#value) },
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
                        element =
                            quote! { #element.track_color(rui::core::tree::parse_color(#value)) }
                    }
                    "thumb_color" => {
                        element =
                            quote! { #element.thumb_color(rui::core::tree::parse_color(#value)) }
                    }
                    "disabled" => element = quote! { #element.disabled(#value) },
                    "placeholder" => element = quote! { #element.placeholder(#value) },
                    "invalid" => element = quote! { #element.invalid(#value) },

                    "grow" | "flex_grow" | "flexGrow" => {
                        element = quote! { #element.flex_grow(rui::core::tree::IntoF32Raw::into_f32_raw(#value)) }
                    }
                    "shrink" | "flex_shrink" | "flexShrink" => {
                        element = quote! { #element.flex_shrink(rui::core::tree::IntoF32Raw::into_f32_raw(#value)) }
                    }
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
                    "align_self" => element = quote! { #element.align_self_kw(#value) },
                    "flex_basis" => element = quote! { #element.flex_basis(#value) },

                    "absolute" => element = quote! { #element.absolute(#value) },
                    "relative" => element = quote! { #element.relative(#value) },
                    "top" => element = quote! { #element.top(#value) },
                    "right" => element = quote! { #element.right(#value) },
                    "bottom" => element = quote! { #element.bottom(#value) },
                    "left" => element = quote! { #element.left(#value) },
                    "inset" => element = quote! { #element.inset(#value) },
                    "z_index" | "zIndex" => {
                        if let Some(z) = expr_as_i32(value) {
                            element = quote! { #element.z_index(#z) };
                        }
                    }

                    // Consumed by a dedicated pre-pass above, so reaching the fallback here would
                    // be a false "unknown attribute" report.
                    "name" if name_str == "Icon" => {}
                    _ => {
                        let msg = format!("unknown rui attribute `{attr_str}` on `<{name_str}>`");
                        let span = attr.span();
                        unknown_attr_warnings.push(quote_spanned! { span =>
                            {
                                #[deprecated(note = #msg)]
                                #[allow(dead_code)]
                                fn __rui_unknown_attr() {}
                                __rui_unknown_attr();
                            }
                        });
                    }
                }
            }

            for child in &el.children {
                match child {
                    RsxNode::Expr(_expr) => {
                        let expanded_child = expand_node(child, node_counter, in_reactive_boundary);
                        element = quote! {
                            {
                                use rui::core::tree::IntoElementList;
                                #element.children((#expanded_child).into_element_list())
                            }
                        };
                    }
                    RsxNode::Element(_) => {
                        let expanded_child = expand_node(child, node_counter, in_reactive_boundary);
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

            if !utility_calls.is_empty()
                || !dynamic_binds.is_empty()
                || !unknown_attr_warnings.is_empty()
            {
                element = quote! {
                    {
                        #(#unknown_attr_warnings)*
                        let mut __rui_el: rui::core::tree::Element = (#element).into();
                        #(#utility_calls)*
                        #(#dynamic_binds)*
                        __rui_el
                    }
                };
            }

            element
        }
    }
}

fn class_lit_str(value: &Expr) -> Option<syn::LitStr> {
    let Expr::Lit(expr_lit) = value else {
        return None;
    };
    let syn::Lit::Str(lit_str) = &expr_lit.lit else {
        return None;
    };
    Some(lit_str.clone())
}

fn parse_utility_unknown_warnings(value: &Expr) -> Vec<proc_macro2::TokenStream> {
    let Some(lit_str) = class_lit_str(value) else {
        return Vec::new();
    };
    let raw = lit_str.value();
    let span = lit_str.span();
    let mut out = Vec::new();
    for token in raw.split_whitespace() {
        let parsed = rui_style_tokens::parse_class_token(token);
        if !rui_style_tokens::is_known_utility(&parsed.utility) {
            let msg = format!("unknown rui utility class `{token}`");
            out.push(quote_spanned! { span =>
                {
                    #[deprecated(note = #msg)]
                    #[allow(dead_code)]
                    fn __rui_unknown_utility_class() {}
                    __rui_unknown_utility_class();
                }
            });
        }
    }
    out
}

fn parse_utility_calls(value: &Expr) -> Vec<proc_macro2::TokenStream> {
    let mut out = parse_utility_unknown_warnings(value);
    if let Some(lit) = class_lit_str(value) {
        out.push(quote! {
            __rui_el = rui::style::with_class_list(__rui_el, #lit);
        });
    }
    out
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

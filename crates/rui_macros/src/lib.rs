use std::sync::atomic::{AtomicUsize, Ordering};

use proc_macro::TokenStream;
use quote::{quote, quote_spanned};
use syn::parse::{Parse, ParseStream};
use syn::visit::Visit;
use syn::{Expr, Ident, Token, braced};

/// RSX 中省略 `id` 的 `<Input>`：编译期为每个标签分配序号，生成稳定字面量，避免多个 Input 共用空 `id` 导致信号冲突。
static INPUT_RSX_ANON_SEQ: AtomicUsize = AtomicUsize::new(0);
/// 同 Input：省略 `id` 的 `<Checkbox>` 仍生成唯一实例键（focus / Label `for`）；非受控布尔态走 Owner 作用域（WP-3.7）。
static CHECKBOX_RSX_ANON_SEQ: AtomicUsize = AtomicUsize::new(0);
/// 同 Checkbox：省略 `id` 的 `<Switch>` 生成唯一实例键。
static SWITCH_RSX_ANON_SEQ: AtomicUsize = AtomicUsize::new(0);

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

/// CSS length / distance（`sideOffset` / blur / height…）：`4` ≡ `"4px"`；无单位字符串 = rem。
fn quote_length_f32(e: &Expr) -> proc_macro2::TokenStream {
    quote! { rui::core::tree::IntoF32::into_f32(#e) }
}

/// 无单位纯数值（字重 / opacity / flex / leading 倍率 / progress 分数…）。
fn quote_unitless_f32(e: &Expr) -> proc_macro2::TokenStream {
    quote! { rui::core::tree::IntoF32Raw::into_f32_raw(#e) }
}

/// HTML-ish bool attrs: `true` / `"true"` / `"1"` / `"yes"` / `"on"` (and false counterparts).
fn parse_bool_str(raw: &str) -> Option<bool> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "true" | "1" | "yes" | "on" => Some(true),
        "false" | "0" | "no" | "off" => Some(false),
        _ => None,
    }
}

fn expr_as_bool_compat(value: &Expr) -> Option<bool> {
    match value {
        Expr::Lit(l) => match &l.lit {
            syn::Lit::Bool(b) => Some(b.value),
            syn::Lit::Str(s) => parse_bool_str(&s.value()),
            syn::Lit::Int(i) => match i.base10_parse::<i64>().ok()? {
                0 => Some(false),
                1 => Some(true),
                _ => None,
            },
            _ => None,
        },
        _ => None,
    }
}

fn is_svg_tag(name: &str) -> bool {
    svg_tag_canonical(name).is_some()
}

/// Map RSX tag → canonical SVG element name (excludes script / foreignObject).
fn svg_tag_canonical(name: &str) -> Option<&'static str> {
    Some(match name {
        "svg" | "Svg" => "svg",
        "defs" | "Defs" => "defs",
        "use" | "Use" => "use",
        "clipPath" | "ClipPath" => "clipPath",
        "linearGradient" | "LinearGradient" => "linearGradient",
        "radialGradient" | "RadialGradient" => "radialGradient",
        "symbol" | "Symbol" => "symbol",
        "stop" | "Stop" => "stop",
        "animate" | "Animate" => "animate",
        "animateTransform" | "AnimateTransform" => "animateTransform",
        "animateMotion" | "AnimateMotion" => "animateMotion",
        "mpath" | "Mpath" | "MPath" => "mpath",
        "set" | "Set" => "set",
        "circle" | "Circle" => "circle",
        "ellipse" | "Ellipse" => "ellipse",
        "path" | "Path" => "path",
        "rect" | "Rect" => "rect",
        "line" | "Line" => "line",
        "polyline" | "Polyline" => "polyline",
        "polygon" | "Polygon" => "polygon",
        "g" => "g",
        // Lowercase-only where PascalCase is a UI component (`Text`/`Image`/`Switch`/`Marker`/…).
        "text" => "text",
        "tspan" | "Tspan" | "TSpan" => "tspan",
        "textPath" | "TextPath" => "textPath",
        "image" => "image",
        "mask" | "Mask" => "mask",
        "pattern" | "Pattern" => "pattern",
        "marker" => "marker",
        "filter" | "Filter" => "filter",
        "title" | "Title" => "title",
        "desc" | "Desc" => "desc",
        "metadata" | "Metadata" => "metadata",
        "switch" => "switch",
        "a" => "a",
        "view" => "view",
        "style" => "style",
        "feBlend" | "FeBlend" => "feBlend",
        "feColorMatrix" | "FeColorMatrix" => "feColorMatrix",
        "feComponentTransfer" | "FeComponentTransfer" => "feComponentTransfer",
        "feComposite" | "FeComposite" => "feComposite",
        "feConvolveMatrix" | "FeConvolveMatrix" => "feConvolveMatrix",
        "feDiffuseLighting" | "FeDiffuseLighting" => "feDiffuseLighting",
        "feDisplacementMap" | "FeDisplacementMap" => "feDisplacementMap",
        "feDistantLight" | "FeDistantLight" => "feDistantLight",
        "feDropShadow" | "FeDropShadow" => "feDropShadow",
        "feFlood" | "FeFlood" => "feFlood",
        "feFuncA" | "FeFuncA" => "feFuncA",
        "feFuncB" | "FeFuncB" => "feFuncB",
        "feFuncG" | "FeFuncG" => "feFuncG",
        "feFuncR" | "FeFuncR" => "feFuncR",
        "feGaussianBlur" | "FeGaussianBlur" => "feGaussianBlur",
        "feImage" | "FeImage" => "feImage",
        "feMerge" | "FeMerge" => "feMerge",
        "feMergeNode" | "FeMergeNode" => "feMergeNode",
        "feMorphology" | "FeMorphology" => "feMorphology",
        "feOffset" | "FeOffset" => "feOffset",
        "fePointLight" | "FePointLight" => "fePointLight",
        "feSpecularLighting" | "FeSpecularLighting" => "feSpecularLighting",
        "feSpotLight" | "FeSpotLight" => "feSpotLight",
        "feTile" | "FeTile" => "feTile",
        "feTurbulence" | "FeTurbulence" => "feTurbulence",
        _ => return None,
    })
}

fn svg_attr_emit_name(attr: &str) -> String {
    match attr {
        "xlink_href" => "xlink:href".to_string(),
        other => other.replace('_', "-"),
    }
}

/// After the first attr Ident, consume `-ident` segments (HTML kebab) and return a
/// Rust-friendly name (`stroke-width` → `stroke_width`).
fn finish_kebab_attr_name(input: ParseStream, first: Ident) -> syn::Result<Ident> {
    if !input.peek(Token![-]) {
        return Ok(first);
    }
    let span = first.span();
    let mut parts = vec![first.to_string()];
    while input.peek(Token![-]) {
        input.parse::<Token![-]>()?;
        let next: Ident = input.parse()?;
        parts.push(next.to_string());
    }
    Ok(Ident::new(&parts.join("_"), span))
}

fn is_canvas_tag(name: &str) -> bool {
    matches!(name, "canvas" | "Canvas")
}

/// Valueless RSX flags: `<DragProvider scroll-lock-all>` ≡ `scroll_lock="all"`.
fn is_scroll_lock_flag_attr(name: &str) -> bool {
    matches!(
        name,
        "scroll_lock_all"
            | "scroll_lock_x"
            | "scroll_lock_y"
            | "scroll_lock_none"
            | "scroll_lock_auto"
    )
}

fn is_boolish_attr(name: &str) -> bool {
    matches!(
        name,
        "clearable"
            | "password_toggle"
            | "passwordToggle"
            | "number_steppers"
            | "numberSteppers"
            | "disabled"
            | "invalid"
            | "absolute"
            | "relative"
            | "checked"
            | "open"
            | "active"
            | "is_active"
            | "isActive"
            | "default_open"
            | "defaultOpen"
            | "required"
            | "readonly"
            | "read_only"
            | "readOnly"
            | "multiple"
            | "selected"
            | "default"
            | "underline"
            | "box_shadow_inset"
            | "boxShadowInset"
            | "with_handle"
            | "withHandle"
            | "auto_scroll"
            | "autoScroll"
            | "scroll_anchor"
            | "scrollAnchor"
            | "clickable"
            | "inertia"
    )
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

        // Bare text children: `<Button>Dialog</Button>` / `<DialogTrigger>Edit Profile</DialogTrigger>`
        // (consecutive idents joined with spaces). Not used when the next token is `<`.
        if input.peek(Ident) {
            let first: Ident = input.parse()?;
            let span = first.span();
            let mut parts = vec![first.to_string()];
            while input.peek(Ident) {
                let next: Ident = input.parse()?;
                parts.push(next.to_string());
            }
            let joined = parts.join(" ");
            let lit = syn::LitStr::new(&joined, span);
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
                        syn::Error::new(
                            e.span(),
                            format!("Failed to parse spread `..{{...}}`: {}", e),
                        )
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
            } else if input.peek(Token![loop]) {
                // Carousel `loop={true}`；`loop` 是关键字。
                input.parse::<Token![loop]>().map_err(|e| {
                    syn::Error::new(e.span(), format!("Failed at keyword attr loop: {}", e))
                })?;
                syn::Ident::new("loop", proc_macro2::Span::call_site())
            } else if input.peek(Token![for]) {
                // Solid `for={id}` → Label prop `for_target`（`r#for` 仍可作为 Builder 别名）。
                input.parse::<Token![for]>().map_err(|e| {
                    syn::Error::new(e.span(), format!("Failed at keyword attr for: {}", e))
                })?;
                syn::Ident::new("for", proc_macro2::Span::call_site())
            } else {
                let first: Ident = input
                    .parse()
                    .map_err(|e| syn::Error::new(e.span(), format!("Failed at attr_name {}", e)))?;
                finish_kebab_attr_name(input, first)?
            };
            let attr_value: Expr = if input.peek(Token![=]) {
                input.parse::<Token![=]>().map_err(|e| {
                    syn::Error::new(e.span(), format!("Failed at = after attr {}", attr_name))
                })?;
                if input.peek(syn::token::Brace) {
                    let content;
                    braced!(content in input);
                    content.parse().map_err(|e| {
                        syn::Error::new(
                            e.span(),
                            format!("Failed to parse braced attribute '{}': {}", attr_name, e),
                        )
                    })?
                } else {
                    let lit: syn::Lit = input.parse().map_err(|e| {
                        syn::Error::new(e.span(), format!("Expected literal: {}", e))
                    })?;
                    syn::Expr::Lit(syn::ExprLit {
                        attrs: Vec::new(),
                        lit,
                    })
                }
            } else if is_scroll_lock_flag_attr(&attr_name.to_string()) {
                Expr::Lit(syn::ExprLit {
                    attrs: Vec::new(),
                    lit: syn::Lit::Bool(syn::LitBool::new(true, attr_name.span())),
                })
            } else {
                return Err(syn::Error::new(
                    attr_name.span(),
                    format!("Failed at = after attr {}", attr_name),
                ));
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

            wrap_ui_expansion(expanded, template_hash_lit).into()
        }
        Err(err) => err.to_compile_error().into(),
    }
}

fn wrap_ui_expansion(
    expanded: proc_macro2::TokenStream,
    template_hash_lit: syn::LitStr,
) -> proc_macro2::TokenStream {
    // `build_ui_root` keeps the RSX expansion off the caller's MSVC-debug prologue.
    // Keep an explicit component `id` (Sidebar / ScrollArea / …): overwriting it with
    // `file:line` made splitting `ui!` across files change identity and broke lookups.
    quote! {
        rui::core::tree::build_ui_root(|| {
            let mut __rui_root: rui::core::tree::Element = (#expanded).into();
            __rui_root = __rui_root.template_hash(#template_hash_lit);
            if __rui_root.template_id.is_none() {
                __rui_root = __rui_root.template_id(format!("{}:{}:{}", file!(), line!(), column!()));
            }
            __rui_root
        })
    }
}

/// A marker attribute for functional components.
/// It automatically generates a Props struct based on the function arguments.
///
/// `#[component(reactive)]` wraps the body in [`reactive_component`] (requires `id: String`).
///
/// Field-level Solid alignment:
/// - Each invocation runs under [`component_root`] / instance Owner (WP-3.14). Sibling
///   `provide_context` calls do not overwrite each other.
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

    let mut merge_fields = Vec::new();

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
            merge_fields.push(merge_prop_field(&crate_path, name, &name_str));

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
        let children_ident = syn::Ident::new("children", proc_macro2::Span::call_site());
        merge_fields.push(merge_prop_field(&crate_path, &children_ident, "children"));
    }

    // Add base prop for styling/layout
    fields.push(quote! { pub base: Option<Box<dyn FnOnce(#crate_path::components::base::BaseComponentBuilder) -> #crate_path::components::base::BaseComponentBuilder>> });
    field_names.push(syn::Ident::new("base", proc_macro2::Span::call_site()));
    field_unwrapping.push(quote! { let mut base = base; });
    let base_ident = syn::Ident::new("base", proc_macro2::Span::call_site());
    merge_fields.push(merge_prop_field(&crate_path, &base_ident, "base"));

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
            #crate_path::core::reactivity::component_root(|| {
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
                        __el = #crate_path::style::apply_stylesheet_rules_tree(__el);
                        if let Some(base_fn) = base.take() {
                            let builder = #crate_path::components::base::BaseComponentBuilder::new(__el);
                            __el = base_fn(builder).el;
                        }
                        __el
                    },
                )
            })
        }
    } else {
        quote! {
            #crate_path::core::reactivity::component_root(|| {
                #(#field_unwrapping)*
                #merge_base_stmt
                let mut __el = { #body };
                __el = #crate_path::style::apply_stylesheet_rules_tree(__el);
                #post_apply_base_stmt
                __el
            })
        }
    };

    let expanded = quote! {
        #(#fn_docs)*
        #[derive(Default)]
        #vis struct #props_name #impl_generics #where_clause {
            #(#fields),*
        }

        impl #impl_generics #crate_path::app::props::MergeProps for #props_name #ty_generics #where_clause {
            fn merge_with(self, overlay: Self) -> Self {
                Self {
                    #(#merge_fields,)*
                }
            }
        }

        #(#fn_docs)*
        #[inline(never)]
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

fn merge_prop_field(
    crate_path: &proc_macro2::TokenStream,
    name: &syn::Ident,
    name_str: &str,
) -> proc_macro2::TokenStream {
    if name_str == "id" {
        quote! {
            #name: if overlay.#name.is_empty() {
                self.#name
            } else {
                overlay.#name
            }
        }
    } else if name_str == "base" || name_str == "base_builder" {
        quote! {
            #name: #crate_path::app::props::merge_base_prop(self.#name, overlay.#name)
        }
    } else {
        quote! {
            #name: overlay.#name.or(self.#name)
        }
    }
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
/// - ignores `.get(...)` with arguments (e.g. `HashMap::get`);
/// - scans inside macro args (e.g. `format!("…{}", count.get())`) — syn's default visit
///   does not walk macro token trees, which previously skipped auto `bind_text` lowering
///   and left naked `.get()` under `isolate_reads` (gallery section body) with no subscriber.
fn expr_reads_get(expr: &Expr) -> bool {
    use syn::parse::Parser;
    use syn::punctuated::Punctuated;

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
                Expr::Unary(u) if matches!(u.op, syn::UnOp::Deref(_)) => Self::receiver_ok(&u.expr),
                Expr::MethodCall(m) => {
                    // Allow chains like `self.count.get()` / `state.foo.get()`.
                    Self::receiver_ok(&m.receiver)
                }
                _ => false,
            }
        }

        fn receiver_looks_like_thread_local(expr: &Expr) -> bool {
            let Expr::Path(p) = expr else {
                return false;
            };
            let Some(seg) = p.path.segments.last() else {
                return false;
            };
            let s = seg.ident.to_string();
            s.len() > 1 && s.chars().all(|c| c.is_ascii_uppercase() || c == '_')
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
            // Signal::with is a tracked read. Skip ALL_CAPS receivers (`THREAD.with`) so
            // `thread_local!` is not treated as a UI signal.
            if node.method == "with"
                && node.args.len() == 1
                && Self::receiver_ok(&node.receiver)
                && !Self::receiver_looks_like_thread_local(&node.receiver)
            {
                self.found = true;
                return;
            }
            syn::visit::visit_expr_method_call(self, node);
        }

        fn visit_expr_macro(&mut self, node: &syn::ExprMacro) {
            if self.found {
                return;
            }
            // `format!("…{}", sig.get())`, `vec![a.get()]`, etc.: parse comma-separated exprs.
            let tokens = node.mac.tokens.clone();
            if let Ok(args) =
                Punctuated::<Expr, syn::Token![,]>::parse_terminated.parse2(tokens.clone())
            {
                for arg in &args {
                    self.visit_expr(arg);
                    if self.found {
                        return;
                    }
                }
            } else if let Ok(inner) = syn::parse2::<Expr>(tokens) {
                self.visit_expr(&inner);
                if self.found {
                    return;
                }
            }
            syn::visit::visit_expr_macro(self, node);
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
            if expr_reads_get(expr) {
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
            // HTML / Solid 风格别名 → 规范组件名（`<Img>`/`<img>` → `Image`）。
            let name_str = match el.name.to_string().as_str() {
                "Img" | "img" => "Image".to_string(),
                other => other.to_string(),
            };
            let select_is_compound =
                name_str == "Select" && !el.attributes.iter().any(|(k, _)| k == "options");
            let combobox_is_compound =
                name_str == "Combobox" && !el.attributes.iter().any(|(k, _)| k == "options");
            let command_is_compound = name_str == "Command"
                && !el.children.is_empty()
                && !el.attributes.iter().any(|(k, _)| k == "actions");
            let radio_group_is_compound =
                name_str == "RadioGroup" && !el.attributes.iter().any(|(k, _)| k == "options");
            let toggle_group_is_compound =
                name_str == "ToggleGroup" && !el.attributes.iter().any(|(k, _)| k == "options");
            let tabs_is_compound =
                name_str == "Tabs" && !el.attributes.iter().any(|(k, _)| k == "count");
            let accordion_is_compound = name_str == "Accordion";
            let questionnaire_is_compound = name_str == "Questionnaire";
            let collapsible_is_compound = name_str == "Collapsible";
            let message_scroller_is_compound = name_str == "MessageScrollerProvider";
            let input_otp_is_compound = name_str == "InputOTP";
            let tree_view_is_compound = name_str == "TreeView";
            let carousel_is_compound = matches!(
                name_str.as_str(),
                "Carousel" | "CarouselSingle" | "CarouselMulti" | "CarouselStack"
            );
            // Presentational shells that only need `*Props` (no scope): Pagination / Navbar /
            // Badge / Card / ButtonGroup / Breadcrumb / Kbd / KbdGroup / Skeleton / Empty / Alert / Item / Attachment / List / Table* / MessageScroller (parts) — use the generic
            // PascalCase expansion below; do not add them to this special-case list.
            // ToggleGroup without `options` is compound (scope before Item); with `options` uses ToggleGroupProps.
            // MessageScrollerProvider is compound (scope before Viewport / Button).
            if name_str == "Reactive"
                || name_str == "Show"
                || name_str == "Popover"
                || name_str == "PopoverContent"
                || name_str == "AnchoredOverlay"
                || name_str == "AnchoredOverlayContent"
                || name_str == "ViewportModal"
                || name_str == "ViewportModalContent"
                || name_str == "DragProvider"
                || name_str == "Draggable"
                || name_str == "Droppable"
                || name_str == "DragOverlay"
                || name_str == "DragHandle"
                || name_str == "DragIgnore"
                || name_str == "SortableList"
                || name_str == "SortableItem"
                || name_str == "HoverCard"
                || name_str == "HoverCardContent"
                || name_str == "Progress"
                || name_str == "Stepper"
                || name_str == "Timeline"
                || name_str == "Bubble"
                || name_str == "Avatar"
                || name_str == "Accordion"
                || name_str == "Questionnaire"
                || name_str == "Collapsible"
                || tree_view_is_compound
                || input_otp_is_compound
                || name_str == "ResizablePanelGroup"
                || name_str == "Dialog"
                || name_str == "DialogContent"
                || name_str == "AlertDialog"
                || name_str == "AlertDialogContent"
                || name_str == "Sheet"
                || name_str == "SheetContent"
                || name_str == "Drawer"
                || name_str == "DrawerContent"
                || name_str == "CommandDialog"
                || name_str == "CommandOverlay"
                || command_is_compound
                || name_str == "Tooltip"
                || name_str == "TooltipContent"
                || name_str == "SidebarProvider"
                || name_str == "ContextMenu"
                || name_str == "ContextMenuContent"
                || name_str == "ContextMenuSub"
                || name_str == "ContextMenuSubContent"
                || name_str == "ContextMenuRadioGroup"
                || name_str == "DropdownMenu"
                || name_str == "DropdownMenuContent"
                || name_str == "DropdownMenuSub"
                || name_str == "DropdownMenuSubContent"
                || name_str == "DropdownMenuRadioGroup"
                || name_str == "Menubar"
                || name_str == "MenubarMenu"
                || name_str == "MenubarContent"
                || name_str == "MenubarSub"
                || name_str == "MenubarSubContent"
                || name_str == "MenubarRadioGroup"
                || name_str == "NavigationMenu"
                || name_str == "NavigationMenuItem"
                || name_str == "NavigationMenuContent"
                || select_is_compound
                || name_str == "SelectContent"
                || combobox_is_compound
                || name_str == "ComboboxContent"
                || radio_group_is_compound
                || toggle_group_is_compound
                || tabs_is_compound
                || name_str == "TabsContent"
                || name_str == "AccordionContent"
                || name_str == "AccordionItem"
                || name_str == "QuestionnaireItem"
                || name_str == "CollapsibleContent"
                || message_scroller_is_compound
                || name_str == "TreeViewItem"
                || carousel_is_compound
                || name_str == "ForKeyed"
                || name_str == "For"
                || name_str == "Match"
                || name_str == "MatchArm"
                || name_str == "When"
                || name_str == "Fragment"
                || name_str == "Dynamic"
            {
                let get_attr = |name: &str| -> Option<&Expr> {
                    el.attributes
                        .iter()
                        .find(|(k, _)| k == &syn::Ident::new(name, proc_macro2::Span::call_site()))
                        .map(|(_, v)| v)
                };

                if name_str == "Progress" {
                    // Provide context *before* Label/Value (eager props order); Track 由 scope 注入。
                    let id_expr = get_attr("id");
                    let value_sig = get_attr("value_signal").or_else(|| get_attr("valueSignal"));
                    let value_lit = get_attr("value");
                    let class_expr = get_attr("class");
                    let orientation_expr = get_attr("orientation");
                    let idx = *node_counter;
                    *node_counter += 1;
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
                                let mut __list: Vec<rui::core::tree::Element> = Vec::new();
                                #( __list.extend(rui::core::tree::build_ui_child(|| (#expanded_children))); )*
                                let mut __frag = rui::core::tree::Element::new();
                                __frag.set_attr("rui_fragment", "1");
                                __frag.children(__list)
                            }
                        }
                    };
                    let id_ts = if let Some(id) = id_expr {
                        quote! { (#id).to_string() }
                    } else {
                        quote! { format!("{}:{}:Progress#{}", file!(), line!(), #idx) }
                    };
                    let value_ts = if let Some(sig) = value_sig {
                        quote! { Some(#sig) }
                    } else {
                        quote! { None }
                    };
                    let default_ts = if let Some(v) = value_lit {
                        quote_unitless_f32(v)
                    } else {
                        quote! { 0.0f32 }
                    };
                    let class_ts = if let Some(c) = class_expr {
                        quote! { Some((#c).to_string()) }
                    } else {
                        quote! { None }
                    };
                    let orientation_ts = if let Some(o) = orientation_expr {
                        quote! { (#o).to_string() }
                    } else {
                        quote! { "horizontal".to_string() }
                    };
                    return quote! {
                        rui::components::progress_scope(
                            #id_ts,
                            #value_ts,
                            #default_ts,
                            #orientation_ts,
                            #class_ts,
                            || #child_el,
                        )
                    };
                }

                if name_str == "Stepper" {
                    // Provide context *before* Item / Indicator / Title (eager props order).
                    let id_expr = get_attr("id");
                    let current_sig = get_attr("current_signal")
                        .or_else(|| get_attr("currentSignal"))
                        .or_else(|| get_attr("current"));
                    let default_expr = get_attr("default_step").or_else(|| get_attr("defaultStep"));
                    let class_expr = get_attr("class");
                    let orientation_expr = get_attr("orientation");
                    let clickable_expr = get_attr("clickable");
                    let idx = *node_counter;
                    *node_counter += 1;
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
                                let mut __list: Vec<rui::core::tree::Element> = Vec::new();
                                #( __list.extend(rui::core::tree::build_ui_child(|| (#expanded_children))); )*
                                let mut __frag = rui::core::tree::Element::new();
                                __frag.set_attr("rui_fragment", "1");
                                __frag.children(__list)
                            }
                        }
                    };
                    let id_ts = if let Some(id) = id_expr {
                        quote! { (#id).to_string() }
                    } else {
                        quote! { format!("{}:{}:Stepper#{}", file!(), line!(), #idx) }
                    };
                    let current_ts = if let Some(sig) = current_sig {
                        quote! { Some(#sig) }
                    } else {
                        quote! { None }
                    };
                    let default_ts = if let Some(d) = default_expr {
                        if let Some(n) = expr_as_u16(d) {
                            let n = n as usize;
                            quote! { #n }
                        } else {
                            quote! { (#d) as usize }
                        }
                    } else {
                        quote! { 0usize }
                    };
                    let class_ts = if let Some(c) = class_expr {
                        quote! { Some((#c).to_string()) }
                    } else {
                        quote! { None }
                    };
                    let orientation_ts = if let Some(o) = orientation_expr {
                        quote! { (#o).to_string() }
                    } else {
                        quote! { "horizontal".to_string() }
                    };
                    let clickable_ts = if let Some(c) = clickable_expr {
                        if let Some(b) = expr_as_bool_compat(c) {
                            quote! { #b }
                        } else {
                            quote! { rui::core::tree::parse_bool_like(#c) }
                        }
                    } else {
                        quote! { true }
                    };
                    return quote! {
                        rui::components::stepper_scope(
                            #id_ts,
                            #current_ts,
                            #default_ts,
                            #orientation_ts,
                            #clickable_ts,
                            #class_ts,
                            || #child_el,
                        )
                    };
                }

                if name_str == "Timeline" {
                    let id_expr = get_attr("id");
                    let active_sig = get_attr("active_item_signal")
                        .or_else(|| get_attr("activeItemSignal"))
                        .or_else(|| get_attr("active_item"))
                        .or_else(|| get_attr("activeItem"))
                        .or_else(|| get_attr("active"));
                    let default_expr =
                        get_attr("default_active").or_else(|| get_attr("defaultActive"));
                    let class_expr = get_attr("class");
                    let bullet_expr = get_attr("bullet_size").or_else(|| get_attr("bulletSize"));
                    let line_expr = get_attr("line_size").or_else(|| get_attr("lineSize"));
                    let idx = *node_counter;
                    *node_counter += 1;
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
                                let mut __list: Vec<rui::core::tree::Element> = Vec::new();
                                #( __list.extend(rui::core::tree::build_ui_child(|| (#expanded_children))); )*
                                let mut __frag = rui::core::tree::Element::new();
                                __frag.set_attr("rui_fragment", "1");
                                __frag.children(__list)
                            }
                        }
                    };
                    let id_ts = if let Some(id) = id_expr {
                        quote! { (#id).to_string() }
                    } else {
                        quote! { format!("{}:{}:Timeline#{}", file!(), line!(), #idx) }
                    };
                    let (active_ts, default_ts) = if let Some(e) = active_sig {
                        if let Some(n) = expr_as_i32(e) {
                            (quote! { None }, quote! { #n })
                        } else {
                            let fallback = if let Some(d) = default_expr {
                                if let Some(n) = expr_as_i32(d) {
                                    quote! { #n }
                                } else {
                                    quote! { (#d) as i32 }
                                }
                            } else {
                                quote! { -1i32 }
                            };
                            (quote! { Some(#e) }, fallback)
                        }
                    } else if let Some(d) = default_expr {
                        if let Some(n) = expr_as_i32(d) {
                            (quote! { None }, quote! { #n })
                        } else {
                            (quote! { None }, quote! { (#d) as i32 })
                        }
                    } else {
                        (quote! { None }, quote! { -1i32 })
                    };
                    let class_ts = if let Some(c) = class_expr {
                        quote! { Some((#c).to_string()) }
                    } else {
                        quote! { None }
                    };
                    let bullet_ts = if let Some(v) = bullet_expr {
                        quote_unitless_f32(v)
                    } else {
                        quote! { 20.0f32 }
                    };
                    let line_ts = if let Some(v) = line_expr {
                        quote_unitless_f32(v)
                    } else {
                        quote! { 2.0f32 }
                    };
                    return quote! {
                        rui::components::timeline_scope(
                            #id_ts,
                            #active_ts,
                            #default_ts,
                            #bullet_ts,
                            #line_ts,
                            #class_ts,
                            || #child_el,
                        )
                    };
                }

                if name_str == "Bubble" {
                    let variant_expr = get_attr("variant");
                    let align_expr = get_attr("align");
                    let class_expr = get_attr("class");
                    *node_counter += 1;
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
                                let mut __list: Vec<rui::core::tree::Element> = Vec::new();
                                #( __list.extend(rui::core::tree::build_ui_child(|| (#expanded_children))); )*
                                let mut __frag = rui::core::tree::Element::new();
                                __frag.set_attr("rui_fragment", "1");
                                __frag.children(__list)
                            }
                        }
                    };
                    let variant_ts = if let Some(v) = variant_expr {
                        quote! { rui::components::BubbleVariant::from(#v) }
                    } else {
                        quote! { rui::components::BubbleVariant::Default }
                    };
                    let align_ts = if let Some(a) = align_expr {
                        quote! { rui::components::BubbleAlign::from(#a) }
                    } else {
                        quote! { rui::components::BubbleAlign::Start }
                    };
                    let class_ts = if let Some(c) = class_expr {
                        quote! { Some((#c).to_string()) }
                    } else {
                        quote! { None }
                    };
                    return quote! {
                        rui::components::bubble_scope(
                            #variant_ts,
                            #align_ts,
                            #class_ts,
                            || #child_el,
                        )
                    };
                }

                if name_str == "Avatar" {
                    let id_expr = get_attr("id");
                    let size_expr = get_attr("size");
                    let shape_expr = get_attr("shape");
                    let class_expr = get_attr("class");
                    let idx = *node_counter;
                    *node_counter += 1;
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
                                let mut __list: Vec<rui::core::tree::Element> = Vec::new();
                                #( __list.extend(rui::core::tree::build_ui_child(|| (#expanded_children))); )*
                                let mut __frag = rui::core::tree::Element::new();
                                __frag.set_attr("rui_fragment", "1");
                                __frag.children(__list)
                            }
                        }
                    };
                    let id_ts = if let Some(id) = id_expr {
                        quote! { (#id).to_string() }
                    } else {
                        quote! { format!("{}:{}:Avatar#{}", file!(), line!(), #idx) }
                    };
                    let size_ts = if let Some(s) = size_expr {
                        quote! { rui::components::AvatarSize::from(#s) }
                    } else {
                        quote! { rui::components::AvatarSize::Default }
                    };
                    let shape_ts = if let Some(s) = shape_expr {
                        quote! { rui::components::AvatarShape::from(#s) }
                    } else {
                        quote! { rui::components::AvatarShape::Circle }
                    };
                    let class_ts = if let Some(c) = class_expr {
                        quote! { Some((#c).to_string()) }
                    } else {
                        quote! { None }
                    };
                    return quote! {
                        rui::components::avatar_scope(
                            #id_ts,
                            #size_ts,
                            #shape_ts,
                            #class_ts,
                            || #child_el,
                        )
                    };
                }

                if accordion_is_compound {
                    // ShadCN compound Accordion：先 provide context 再构建 Item/Trigger/Content。
                    let id_expr = get_attr("id");
                    let value_expr = get_attr("value").or_else(|| get_attr("valueSignal"));
                    let default_expr =
                        get_attr("default_value").or_else(|| get_attr("defaultValue"));
                    let multiple_expr = get_attr("multiple");
                    let disabled_expr = get_attr("disabled");
                    let class_expr = get_attr("class");
                    let idx = *node_counter;
                    *node_counter += 1;
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
                                let mut __list: Vec<rui::core::tree::Element> = Vec::new();
                                #( __list.extend(rui::core::tree::build_ui_child(|| (#expanded_children))); )*
                                let mut __frag = rui::core::tree::Element::new();
                                __frag.set_attr("rui_fragment", "1");
                                __frag.children(__list)
                            }
                        }
                    };
                    let id_ts = if let Some(id) = id_expr {
                        quote! { (#id).to_string() }
                    } else {
                        quote! { format!("{}:{}:Accordion#{}", file!(), line!(), #idx) }
                    };
                    let value_ts = if let Some(value) = value_expr {
                        quote! { Some(#value) }
                    } else {
                        quote! { None }
                    };
                    let default_ts = if let Some(d) = default_expr {
                        quote! { Some(#d) }
                    } else {
                        quote! { None }
                    };
                    let multiple_ts = if let Some(m) = multiple_expr {
                        if let Some(b) = expr_as_bool_compat(m) {
                            quote! { #b }
                        } else {
                            quote! { rui::core::tree::parse_bool_like(#m) }
                        }
                    } else {
                        quote! { false }
                    };
                    let disabled_ts = if let Some(d) = disabled_expr {
                        if let Some(b) = expr_as_bool_compat(d) {
                            quote! { #b }
                        } else {
                            quote! { rui::core::tree::parse_bool_like(#d) }
                        }
                    } else {
                        quote! { false }
                    };
                    let class_ts = if let Some(c) = class_expr {
                        quote! { Some((#c).to_string()) }
                    } else {
                        quote! { None }
                    };
                    return quote! {
                        rui::components::accordion_scope(
                            #id_ts,
                            #value_ts,
                            #default_ts.unwrap_or_default(),
                            #multiple_ts,
                            #disabled_ts,
                            #class_ts,
                            || #child_el,
                        )
                    };
                }

                if questionnaire_is_compound {
                    // ShadCN compound Questionnaire: provide context before Progress / Item / Actions.
                    let id_expr = get_attr("id");
                    let item_expr = get_attr("item")
                        .or_else(|| get_attr("item_signal"))
                        .or_else(|| get_attr("itemSignal"));
                    let default_expr = get_attr("default_item").or_else(|| get_attr("defaultItem"));
                    let shortcuts_expr = get_attr("shortcuts");
                    let items_expr = get_attr("items");
                    let class_expr = get_attr("class");
                    let on_submit_expr = get_attr("on_submit").or_else(|| get_attr("onSubmit"));
                    let on_item_change_expr =
                        get_attr("on_item_change").or_else(|| get_attr("onItemChange"));
                    let idx = *node_counter;
                    *node_counter += 1;
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
                                let mut __list: Vec<rui::core::tree::Element> = Vec::new();
                                #( __list.extend(rui::core::tree::build_ui_child(|| (#expanded_children))); )*
                                let mut __frag = rui::core::tree::Element::new();
                                __frag.set_attr("rui_fragment", "1");
                                __frag.children(__list)
                            }
                        }
                    };
                    let id_ts = if let Some(id) = id_expr {
                        quote! { (#id).to_string() }
                    } else {
                        quote! { format!("{}:{}:Questionnaire#{}", file!(), line!(), #idx) }
                    };
                    let item_ts = if let Some(item) = item_expr {
                        quote! { Some(#item) }
                    } else {
                        quote! { None }
                    };
                    let default_ts = if let Some(d) = default_expr {
                        quote! { (#d).to_string() }
                    } else {
                        quote! { String::new() }
                    };
                    let shortcuts_ts = if let Some(s) = shortcuts_expr {
                        quote! { rui::components::QuestionnaireShortcuts::parse(&(#s).to_string()) }
                    } else {
                        quote! { rui::components::QuestionnaireShortcuts::None }
                    };
                    let items_ts = if let Some(i) = items_expr {
                        quote! { #i }
                    } else {
                        quote! { Vec::new() }
                    };
                    let class_ts = if let Some(c) = class_expr {
                        quote! { Some((#c).to_string()) }
                    } else {
                        quote! { None }
                    };
                    let on_submit_ts = if let Some(s) = on_submit_expr {
                        quote! { Some(rui::components::questionnaire_submit_fn(#s)) }
                    } else {
                        quote! { None }
                    };
                    let on_item_change_ts = if let Some(s) = on_item_change_expr {
                        quote! { Some(rui::components::questionnaire_item_change_fn(#s)) }
                    } else {
                        quote! { None }
                    };
                    return quote! {
                        rui::components::questionnaire_scope(
                            #id_ts,
                            #item_ts,
                            #default_ts,
                            #shortcuts_ts,
                            #items_ts,
                            #class_ts,
                            #on_submit_ts,
                            #on_item_change_ts,
                            || #child_el,
                        )
                    };
                }

                if tree_view_is_compound {
                    let id_expr = get_attr("id");
                    let variant_expr = get_attr("variant");
                    let expanded_expr =
                        get_attr("expanded_signal").or_else(|| get_attr("expandedSignal"));
                    let expanded_storage_key_expr =
                        get_attr("expanded_storage_key").or_else(|| get_attr("expandedStorageKey"));
                    let row_height_expr = get_attr("row_height").or_else(|| get_attr("rowHeight"));
                    let height_expr = get_attr("height").or_else(|| get_attr("h"));
                    let idx = *node_counter;
                    *node_counter += 1;
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
                                let mut __list: Vec<rui::core::tree::Element> = Vec::new();
                                #( __list.extend(rui::core::tree::build_ui_child(|| (#expanded_children))); )*
                                let mut __frag = rui::core::tree::Element::new();
                                __frag.set_attr("rui_fragment", "1");
                                __frag.children(__list)
                            }
                        }
                    };
                    let id_ts = if let Some(id) = id_expr {
                        quote! { (#id).to_string() }
                    } else {
                        quote! { format!("{}:{}:TreeView#{}", file!(), line!(), #idx) }
                    };
                    let variant_ts = if let Some(v) = variant_expr {
                        quote! { rui::components::TreeViewVariant::from((#v).to_string()) }
                    } else {
                        quote! { rui::components::TreeViewVariant::default() }
                    };
                    let expanded_ts = if let Some(exp) = expanded_expr {
                        quote! { Some(#exp) }
                    } else {
                        quote! { None }
                    };
                    let expanded_storage_key_ts = if let Some(k) = expanded_storage_key_expr {
                        quote! { Some((#k).to_string()) }
                    } else {
                        quote! { None }
                    };
                    let class_expr = get_attr("class");
                    let class_ts = if let Some(c) = class_expr {
                        quote! { Some((#c).to_string()) }
                    } else {
                        quote! { None }
                    };
                    let exclusive_expr = get_attr("exclusive");
                    let exclusive_ts = if let Some(e) = exclusive_expr {
                        if let Some(b) = expr_as_bool_compat(e) {
                            quote! { Some(#b) }
                        } else {
                            quote! { Some(rui::core::tree::parse_bool_like(#e)) }
                        }
                    } else {
                        quote! { None }
                    };
                    let chrome_expr = get_attr("chrome");
                    let chrome_ts = if let Some(c) = chrome_expr {
                        quote! { rui::components::TreeViewChrome::from((#c).to_string()) }
                    } else {
                        quote! { rui::components::TreeViewChrome::Default }
                    };
                    let selected_expr =
                        get_attr("selected_signal").or_else(|| get_attr("selectedSignal"));
                    let selected_ts = if let Some(s) = selected_expr {
                        quote! { Some(#s) }
                    } else {
                        quote! { None }
                    };
                    let row_height_ts = if let Some(rh) = row_height_expr {
                        let v = quote_unitless_f32(rh);
                        quote! { #v }
                    } else {
                        quote! { 32.0f32 }
                    };
                    let height_ts = if let Some(h) = height_expr {
                        let v = quote_unitless_f32(h);
                        quote! { Some(#v) }
                    } else {
                        quote! { None }
                    };
                    return quote! {
                        rui::components::tree_view_scope(
                            #id_ts,
                            #variant_ts,
                            #expanded_ts,
                            #expanded_storage_key_ts,
                            #class_ts,
                            #row_height_ts,
                            #height_ts,
                            #exclusive_ts,
                            #chrome_ts,
                            #selected_ts,
                            None,
                            ::std::vec::Vec::new(),
                            || #child_el,
                        )
                    };
                }

                if collapsible_is_compound {
                    // ShadCN compound Collapsible：先 provide context 再构建 Trigger/Content。
                    let id_expr = get_attr("id");
                    let open_expr = get_attr("open")
                        .or_else(|| get_attr("open_signal"))
                        .or_else(|| get_attr("openSignal"));
                    let default_expr = get_attr("default_open").or_else(|| get_attr("defaultOpen"));
                    let disabled_expr = get_attr("disabled");
                    let class_expr = get_attr("class");
                    let idx = *node_counter;
                    *node_counter += 1;
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
                                let mut __list: Vec<rui::core::tree::Element> = Vec::new();
                                #( __list.extend(rui::core::tree::build_ui_child(|| (#expanded_children))); )*
                                let mut __frag = rui::core::tree::Element::new();
                                __frag.set_attr("rui_fragment", "1");
                                __frag.children(__list)
                            }
                        }
                    };
                    let id_ts = if let Some(id) = id_expr {
                        quote! { (#id).to_string() }
                    } else {
                        quote! { format!("{}:{}:Collapsible#{}", file!(), line!(), #idx) }
                    };
                    let open_ts = if let Some(open) = open_expr {
                        quote! { Some(#open) }
                    } else {
                        quote! { None }
                    };
                    let default_ts = if let Some(d) = default_expr {
                        if let Some(b) = expr_as_bool_compat(d) {
                            quote! { #b }
                        } else {
                            quote! { rui::core::tree::parse_bool_like(#d) }
                        }
                    } else {
                        quote! { false }
                    };
                    let disabled_ts = if let Some(d) = disabled_expr {
                        if let Some(b) = expr_as_bool_compat(d) {
                            quote! { #b }
                        } else {
                            quote! { rui::core::tree::parse_bool_like(#d) }
                        }
                    } else {
                        quote! { false }
                    };
                    let class_ts = if let Some(c) = class_expr {
                        quote! { Some((#c).to_string()) }
                    } else {
                        quote! { None }
                    };
                    let exclusive_expr = get_attr("exclusive");
                    let exclusive_ts = if let Some(e) = exclusive_expr {
                        if let Some(b) = expr_as_bool_compat(e) {
                            quote! { #b }
                        } else {
                            quote! { rui::core::tree::parse_bool_like(#e) }
                        }
                    } else {
                        quote! { false }
                    };
                    let group_expr = get_attr("group");
                    let group_ts = if let Some(g) = group_expr {
                        quote! { Some((#g).to_string()) }
                    } else {
                        quote! { None }
                    };
                    return quote! {
                        rui::components::collapsible_scope(
                            #id_ts,
                            #open_ts,
                            #default_ts,
                            #disabled_ts,
                            #class_ts,
                            #exclusive_ts,
                            #group_ts,
                            || #child_el,
                        )
                    };
                }

                if message_scroller_is_compound {
                    // ShadCN compound MessageScroller: provide context before Viewport / Button.
                    let id_expr = get_attr("id");
                    let auto_expr = get_attr("auto_scroll").or_else(|| get_attr("autoScroll"));
                    let pos_expr = get_attr("default_scroll_position")
                        .or_else(|| get_attr("defaultScrollPosition"));
                    let threshold_expr = get_attr("scroll_edge_threshold")
                        .or_else(|| get_attr("scrollEdgeThreshold"));
                    let peek_expr = get_attr("scroll_previous_item_peek")
                        .or_else(|| get_attr("scrollPreviousItemPeek"));
                    let class_expr = get_attr("class");
                    let idx = *node_counter;
                    *node_counter += 1;
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
                                let mut __list: Vec<rui::core::tree::Element> = Vec::new();
                                #( __list.extend(rui::core::tree::build_ui_child(|| (#expanded_children))); )*
                                let mut __frag = rui::core::tree::Element::new();
                                __frag.set_attr("rui_fragment", "1");
                                __frag.children(__list)
                            }
                        }
                    };
                    let id_ts = if let Some(id) = id_expr {
                        quote! { (#id).to_string() }
                    } else {
                        quote! { format!("{}:{}:MessageScrollerProvider#{}", file!(), line!(), #idx) }
                    };
                    let auto_ts = if let Some(a) = auto_expr {
                        if let Some(b) = expr_as_bool_compat(a) {
                            quote! { #b }
                        } else {
                            quote! { rui::core::tree::parse_bool_like(#a) }
                        }
                    } else {
                        quote! { true }
                    };
                    let pos_ts = if let Some(p) = pos_expr {
                        quote! { (#p).to_string() }
                    } else {
                        quote! { "end".to_string() }
                    };
                    let threshold_ts = if let Some(t) = threshold_expr {
                        let v = quote_unitless_f32(t);
                        quote! { #v }
                    } else {
                        quote! { 8.0_f32 }
                    };
                    let peek_ts = if let Some(p) = peek_expr {
                        let v = quote_unitless_f32(p);
                        quote! { #v }
                    } else {
                        quote! { 48.0_f32 }
                    };
                    let class_ts = if let Some(c) = class_expr {
                        quote! { Some((#c).to_string()) }
                    } else {
                        quote! { None }
                    };
                    return quote! {
                        rui::components::message_scroller_scope(
                            #id_ts,
                            #auto_ts,
                            #pos_ts,
                            #threshold_ts,
                            #peek_ts,
                            #class_ts,
                            || #child_el,
                        )
                    };
                }

                if input_otp_is_compound {
                    // ShadCN compound InputOTP：先 provide context 再构建 Group/Slot/Separator。
                    let id_expr = get_attr("id");
                    let max_expr = get_attr("max_length").or_else(|| get_attr("maxLength"));
                    let value_expr = get_attr("value")
                        .or_else(|| get_attr("value_signal"))
                        .or_else(|| get_attr("valueSignal"));
                    let disabled_expr = get_attr("disabled");
                    let invalid_expr = get_attr("invalid");
                    let pattern_expr = get_attr("pattern");
                    let class_expr = get_attr("class");
                    let idx = *node_counter;
                    *node_counter += 1;
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
                                let mut __list: Vec<rui::core::tree::Element> = Vec::new();
                                #( __list.extend(rui::core::tree::build_ui_child(|| (#expanded_children))); )*
                                let mut __frag = rui::core::tree::Element::new();
                                __frag.set_attr("rui_fragment", "1");
                                __frag.children(__list)
                            }
                        }
                    };
                    let id_ts = if let Some(id) = id_expr {
                        quote! { (#id).to_string() }
                    } else {
                        quote! { format!("{}:{}:InputOTP#{}", file!(), line!(), #idx) }
                    };
                    let max_ts = if let Some(m) = max_expr {
                        quote! { (#m) as usize }
                    } else {
                        quote! { 6usize }
                    };
                    let value_ts = if let Some(v) = value_expr {
                        quote! { Some(#v) }
                    } else {
                        quote! { None }
                    };
                    let disabled_ts = if let Some(d) = disabled_expr {
                        if let Some(b) = expr_as_bool_compat(d) {
                            quote! { #b }
                        } else {
                            quote! { rui::core::tree::parse_bool_like(#d) }
                        }
                    } else {
                        quote! { false }
                    };
                    let invalid_ts = if let Some(d) = invalid_expr {
                        if let Some(b) = expr_as_bool_compat(d) {
                            quote! { #b }
                        } else {
                            quote! { rui::core::tree::parse_bool_like(#d) }
                        }
                    } else {
                        quote! { false }
                    };
                    let pattern_ts = if let Some(p) = pattern_expr {
                        quote! {
                            rui::components::InputOtpPattern::parse(&(#p).to_string())
                        }
                    } else {
                        quote! { rui::components::InputOtpPattern::Digits }
                    };
                    let class_ts = if let Some(c) = class_expr {
                        quote! { Some((#c).to_string()) }
                    } else {
                        quote! { None }
                    };
                    return quote! {
                        rui::components::input_otp_scope(
                            #id_ts,
                            #max_ts,
                            #value_ts,
                            #disabled_ts,
                            #invalid_ts,
                            #pattern_ts,
                            #class_ts,
                            || #child_el,
                        )
                    };
                }

                if name_str == "ResizablePanelGroup" {
                    let id_expr = get_attr("id");
                    let ori_expr = get_attr("orientation");
                    let class_expr = get_attr("class");
                    let h_expr = get_attr("h").or_else(|| get_attr("height"));
                    let sizes_expr = get_attr("sizes")
                        .or_else(|| get_attr("sizes_signal"))
                        .or_else(|| get_attr("sizesSignal"));
                    let idx = *node_counter;
                    *node_counter += 1;
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
                                let mut __list: Vec<rui::core::tree::Element> = Vec::new();
                                #( __list.extend(rui::core::tree::build_ui_child(|| (#expanded_children))); )*
                                let mut __frag = rui::core::tree::Element::new();
                                __frag.set_attr("rui_fragment", "1");
                                __frag.children(__list)
                            }
                        }
                    };
                    let id_ts = if let Some(id) = id_expr {
                        quote! { (#id).to_string() }
                    } else {
                        quote! { format!("{}:{}:ResizablePanelGroup#{}", file!(), line!(), #idx) }
                    };
                    let ori_ts = if let Some(o) = ori_expr {
                        quote! { (#o).to_string() }
                    } else {
                        quote! { "horizontal".to_string() }
                    };
                    let class_ts = if let Some(c) = class_expr {
                        quote! { Some((#c).to_string()) }
                    } else {
                        quote! { None }
                    };
                    let height_ts = if let Some(h) = h_expr {
                        let px = quote_length_f32(h);
                        quote! { Some(#px) }
                    } else {
                        quote! { None }
                    };
                    let sizes_ts = if let Some(s) = sizes_expr {
                        quote! { Some(#s) }
                    } else {
                        quote! { None }
                    };
                    return quote! {
                        rui::components::resizable_panel_group_scope(
                            #id_ts,
                            #ori_ts,
                            #class_ts,
                            #height_ts,
                            #sizes_ts,
                            || #child_el,
                        )
                    };
                }

                if name_str == "Popover" {
                    // Provide context *before* Trigger/Content (eager props order).
                    let id_expr = get_attr("id");
                    let open_expr = get_attr("open").or_else(|| get_attr("openSignal"));
                    let idx = *node_counter;
                    *node_counter += 1;
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
                        // Fragment so Trigger can sit beside Content without an extra column.
                        quote! {
                            {
                                let mut __list: Vec<rui::core::tree::Element> = Vec::new();
                                #( __list.extend(rui::core::tree::build_ui_child(|| (#expanded_children))); )*
                                let mut __frag = rui::core::tree::Element::new();
                                __frag.set_attr("rui_fragment", "1");
                                __frag.children(__list)
                            }
                        }
                    };
                    let id_ts = if let Some(id) = id_expr {
                        quote! { (#id).to_string() }
                    } else {
                        quote! { format!("{}:{}:Popover#{}", file!(), line!(), #idx) }
                    };
                    let open_ts = if let Some(open) = open_expr {
                        quote! { Some(#open) }
                    } else {
                        quote! { None }
                    };
                    return quote! {
                        rui::components::popover_scope(#id_ts, #open_ts, || #child_el)
                    };
                }

                if name_str == "AnchoredOverlay" {
                    let id_expr = get_attr("id");
                    let open_expr = get_attr("open").or_else(|| get_attr("openSignal"));
                    let idx = *node_counter;
                    *node_counter += 1;
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
                                let mut __list: Vec<rui::core::tree::Element> = Vec::new();
                                #( __list.extend(rui::core::tree::build_ui_child(|| (#expanded_children))); )*
                                let mut __frag = rui::core::tree::Element::new();
                                __frag.set_attr("rui_fragment", "1");
                                __frag.children(__list)
                            }
                        }
                    };
                    let id_ts = if let Some(id) = id_expr {
                        quote! { (#id).to_string() }
                    } else {
                        quote! { format!("{}:{}:AnchoredOverlay#{}", file!(), line!(), #idx) }
                    };
                    let open_ts = if let Some(open) = open_expr {
                        quote! { Some(#open) }
                    } else {
                        quote! { None }
                    };
                    return quote! {
                        rui::utils::anchored_overlay::anchored_overlay_scope(#id_ts, #open_ts, || #child_el)
                    };
                }

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
                        .map(|e| quote! { rui::utils::drag::Axis::parse(&(#e)) })
                        .unwrap_or_else(|| quote! { rui::utils::drag::Axis::Xy });
                    let restrict_end_only = get_attr("restrict_end_only")
                        .or_else(|| get_attr("restrictEndOnly"))
                        .map(|e| quote! { #e })
                        .unwrap_or_else(|| quote! { false });
                    let restrict_anchor = get_attr("restrict_anchor")
                        .or_else(|| get_attr("restrictAnchor"))
                        .map(|e| quote! { rui::utils::drag::RestrictAnchor::parse(&(#e)) })
                        .unwrap_or_else(|| quote! { rui::utils::drag::RestrictAnchor::FULL });
                    let restrict_overflow = get_attr("restrict_overflow")
                        .or_else(|| get_attr("restrictOverflow"))
                        .map(|e| quote! { rui::utils::drag::RestrictOverflow::parse(&(#e)) })
                        .unwrap_or_else(|| quote! { rui::utils::drag::RestrictOverflow::Visible });
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
                        .map(|e| quote! { rui::utils::drag::Activation::parse(&(#e)) })
                        .unwrap_or_else(|| quote! { rui::utils::drag::Activation::And });
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
                                let mut __list: Vec<rui::core::tree::Element> = Vec::new();
                                #( __list.extend(rui::core::tree::build_ui_child(|| (#expanded_children))); )*
                                let mut __frag = rui::core::tree::Element::new();
                                __frag.set_attr("rui_fragment", "1");
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
                            let mut __b = rui::utils::drag::drag_scope(#id_ts);
                            __b = __b.collision(rui::utils::drag::CollisionAlg::parse(&(#collision)));
                            __b = __b.axis(rui::utils::drag::Axis::parse(&(#axis)));
                            __b = __b.start_axis(#start_axis);
                            __b = __b.threshold(#threshold);
                            __b = __b.restrict(rui::utils::drag::Restrict::parse(&(#restrict)));
                            __b = __b.restrict_end_only(#restrict_end_only);
                            __b = __b.restrict_anchor(#restrict_anchor);
                            __b = __b.restrict_overflow(#restrict_overflow);
                            __b = __b.delay_ms(#delay_ms);
                            __b = __b.activation(#activation);
                            __b = __b.empty_insert(#empty_insert);
                            __b = __b.autoscroll(#autoscroll);
                            #scroll_sensitivity_ts
                            #scroll_speed_ts
                            __b = __b.scroll_lock(rui::utils::drag::ScrollLock::parse(&(#scroll_lock)));
                            __b = __b.animation(#animation);
                            __b = __b.sort_mode(rui::utils::drag::SortMode::parse(&(#sort_mode)));
                            __b = __b.multi_drag(#multi_drag);
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

                if name_str == "ViewportModal" {
                    let id_expr = get_attr("id");
                    let open_expr = get_attr("open").or_else(|| get_attr("openSignal"));
                    let overlay_color = get_attr("overlay_color")
                        .or_else(|| get_attr("overlayColor"))
                        .map(|e| quote! { Some((#e).to_string()) })
                        .unwrap_or_else(|| quote! { None });
                    let overlay_opacity = get_attr("overlay_opacity")
                        .or_else(|| get_attr("overlayOpacity"))
                        .map(|e| {
                            let v = quote_unitless_f32(e);
                            quote! { Some(#v) }
                        })
                        .unwrap_or_else(|| quote! { None });
                    let overlay_blur = get_attr("overlay_blur")
                        .or_else(|| get_attr("overlayBlur"))
                        .or_else(|| get_attr("backdrop_blur"))
                        .or_else(|| get_attr("backdropBlur"))
                        .map(|e| {
                            let v = quote_length_f32(e);
                            quote! { Some(#v) }
                        })
                        .unwrap_or_else(|| quote! { None });
                    let close_on_click = get_attr("close_on_click")
                        .or_else(|| get_attr("closeOnClick"))
                        .map(|e| quote! { #e })
                        .unwrap_or_else(|| quote! { true });
                    let idx = *node_counter;
                    *node_counter += 1;
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
                                let mut __list: Vec<rui::core::tree::Element> = Vec::new();
                                #( __list.extend(rui::core::tree::build_ui_child(|| (#expanded_children))); )*
                                let mut __frag = rui::core::tree::Element::new();
                                __frag.set_attr("rui_fragment", "1");
                                __frag.children(__list)
                            }
                        }
                    };
                    let id_ts = if let Some(id) = id_expr {
                        quote! { (#id).to_string() }
                    } else {
                        quote! { format!("{}:{}:ViewportModal#{}", file!(), line!(), #idx) }
                    };
                    let open_ts = if let Some(open) = open_expr {
                        quote! { Some(#open) }
                    } else {
                        quote! { None }
                    };
                    return quote! {
                        rui::utils::viewport_modal::viewport_modal_scope(
                            #id_ts,
                            #open_ts,
                            rui::utils::viewport_modal::ShellOverlayOpts {
                                color: #overlay_color,
                                opacity: #overlay_opacity,
                                backdrop_blur: #overlay_blur,
                                close_on_click: #close_on_click,
                            },
                            || #child_el,
                        )
                    };
                }

                if name_str == "HoverCard" {
                    // Provide context *before* Trigger/Content (eager props order).
                    let id_expr = get_attr("id");
                    let open_expr = get_attr("open").or_else(|| get_attr("openSignal"));
                    let idx = *node_counter;
                    *node_counter += 1;
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
                        // Fragment so Trigger can sit beside Content without an extra column.
                        quote! {
                            {
                                let mut __list: Vec<rui::core::tree::Element> = Vec::new();
                                #( __list.extend(rui::core::tree::build_ui_child(|| (#expanded_children))); )*
                                let mut __frag = rui::core::tree::Element::new();
                                __frag.set_attr("rui_fragment", "1");
                                __frag.children(__list)
                            }
                        }
                    };
                    let id_ts = if let Some(id) = id_expr {
                        quote! { (#id).to_string() }
                    } else {
                        quote! { format!("{}:{}:HoverCard#{}", file!(), line!(), #idx) }
                    };
                    let open_ts = if let Some(open) = open_expr {
                        quote! { Some(#open) }
                    } else {
                        quote! { None }
                    };
                    return quote! {
                        rui::components::hover_card_scope(#id_ts, #open_ts, || #child_el)
                    };
                }

                if name_str == "Dialog" {
                    // Provide context *before* building Trigger/Content (eager props order).
                    let id_expr = get_attr("id");
                    let open_expr = get_attr("open");
                    let idx = *node_counter;
                    *node_counter += 1;
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
                        // Fragment so Trigger can sit in parent HStack/VStack without an extra column.
                        quote! {
                            {
                                let mut __list: Vec<rui::core::tree::Element> = Vec::new();
                                #( __list.extend(rui::core::tree::build_ui_child(|| (#expanded_children))); )*
                                let mut __frag = rui::core::tree::Element::new();
                                __frag.set_attr("rui_fragment", "1");
                                __frag.children(__list)
                            }
                        }
                    };
                    let id_ts = if let Some(id) = id_expr {
                        quote! { (#id).to_string() }
                    } else {
                        quote! { format!("{}:{}:Dialog#{}", file!(), line!(), #idx) }
                    };
                    let open_ts = if let Some(open) = open_expr {
                        quote! { Some(#open) }
                    } else {
                        quote! { None }
                    };
                    return quote! {
                        rui::components::dialog_scope(#id_ts, #open_ts, || #child_el)
                    };
                }

                if name_str == "AlertDialog" {
                    let id_expr = get_attr("id");
                    let open_expr = get_attr("open");
                    let idx = *node_counter;
                    *node_counter += 1;
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
                                let mut __list: Vec<rui::core::tree::Element> = Vec::new();
                                #( __list.extend(rui::core::tree::build_ui_child(|| (#expanded_children))); )*
                                let mut __frag = rui::core::tree::Element::new();
                                __frag.set_attr("rui_fragment", "1");
                                __frag.children(__list)
                            }
                        }
                    };
                    let id_ts = if let Some(id) = id_expr {
                        quote! { (#id).to_string() }
                    } else {
                        quote! { format!("{}:{}:AlertDialog#{}", file!(), line!(), #idx) }
                    };
                    let open_ts = if let Some(open) = open_expr {
                        quote! { Some(#open) }
                    } else {
                        quote! { None }
                    };
                    return quote! {
                        rui::components::alert_dialog_scope(#id_ts, #open_ts, || #child_el)
                    };
                }

                if name_str == "Sheet" {
                    // Provide context *before* building Trigger/Content (eager props order).
                    let id_expr = get_attr("id");
                    let open_expr = get_attr("open");
                    let idx = *node_counter;
                    *node_counter += 1;
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
                        // Fragment so Trigger can sit in parent HStack/VStack without an extra column.
                        quote! {
                            {
                                let mut __list: Vec<rui::core::tree::Element> = Vec::new();
                                #( __list.extend(rui::core::tree::build_ui_child(|| (#expanded_children))); )*
                                let mut __frag = rui::core::tree::Element::new();
                                __frag.set_attr("rui_fragment", "1");
                                __frag.children(__list)
                            }
                        }
                    };
                    let id_ts = if let Some(id) = id_expr {
                        quote! { (#id).to_string() }
                    } else {
                        quote! { format!("{}:{}:Sheet#{}", file!(), line!(), #idx) }
                    };
                    let open_ts = if let Some(open) = open_expr {
                        quote! { Some(#open) }
                    } else {
                        quote! { None }
                    };
                    return quote! {
                        rui::components::sheet_scope(#id_ts, #open_ts, || #child_el)
                    };
                }

                if name_str == "Drawer" {
                    let id_expr = get_attr("id");
                    let open_expr = get_attr("open");
                    let idx = *node_counter;
                    *node_counter += 1;
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
                                let mut __list: Vec<rui::core::tree::Element> = Vec::new();
                                #( __list.extend(rui::core::tree::build_ui_child(|| (#expanded_children))); )*
                                let mut __frag = rui::core::tree::Element::new();
                                __frag.set_attr("rui_fragment", "1");
                                __frag.children(__list)
                            }
                        }
                    };
                    let id_ts = if let Some(id) = id_expr {
                        quote! { (#id).to_string() }
                    } else {
                        quote! { format!("{}:{}:Drawer#{}", file!(), line!(), #idx) }
                    };
                    let open_ts = if let Some(open) = open_expr {
                        quote! { Some(#open) }
                    } else {
                        quote! { None }
                    };
                    return quote! {
                        rui::components::drawer_scope(#id_ts, #open_ts, || #child_el)
                    };
                }

                if name_str == "CommandDialog" {
                    let id_expr = get_attr("id");
                    let open_expr = get_attr("open");
                    let placeholder_expr = get_attr("placeholder");
                    let show_close = get_attr("show_close")
                        .or_else(|| get_attr("showClose"))
                        .or_else(|| get_attr("showCloseButton"))
                        .map(|e| quote! { #e })
                        .unwrap_or_else(|| quote! { false });
                    let idx = *node_counter;
                    *node_counter += 1;
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
                                let mut __list: Vec<rui::core::tree::Element> = Vec::new();
                                #( __list.extend(rui::core::tree::build_ui_child(|| (#expanded_children))); )*
                                let mut __frag = rui::core::tree::Element::new();
                                __frag.set_attr("rui_fragment", "1");
                                __frag.children(__list)
                            }
                        }
                    };
                    let id_ts = if let Some(id) = id_expr {
                        quote! { (#id).to_string() }
                    } else {
                        quote! { format!("{}:{}:CommandDialog#{}", file!(), line!(), #idx) }
                    };
                    let open_ts = if let Some(open) = open_expr {
                        quote! { Some(#open) }
                    } else {
                        quote! { None }
                    };
                    let placeholder_ts = if let Some(p) = placeholder_expr {
                        quote! { (#p).to_string() }
                    } else {
                        quote! { "Type a command or search...".to_string() }
                    };
                    return quote! {
                        rui::components::command_dialog_scope(
                            #id_ts,
                            #open_ts,
                            #placeholder_ts,
                            #show_close,
                            || #child_el,
                        )
                    };
                }

                if name_str == "CommandOverlay" {
                    let id_expr = get_attr("id");
                    let open_expr = get_attr("open");
                    let placeholder_expr = get_attr("placeholder");
                    let idx = *node_counter;
                    *node_counter += 1;
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
                                let mut __list: Vec<rui::core::tree::Element> = Vec::new();
                                #( __list.extend(rui::core::tree::build_ui_child(|| (#expanded_children))); )*
                                let mut __frag = rui::core::tree::Element::new();
                                __frag.set_attr("rui_fragment", "1");
                                __frag.children(__list)
                            }
                        }
                    };
                    let id_ts = if let Some(id) = id_expr {
                        quote! { (#id).to_string() }
                    } else {
                        quote! { format!("{}:{}:CommandOverlay#{}", file!(), line!(), #idx) }
                    };
                    let open_ts = if let Some(open) = open_expr {
                        quote! { Some(#open) }
                    } else {
                        quote! { None }
                    };
                    let placeholder_ts = if let Some(p) = placeholder_expr {
                        quote! { (#p).to_string() }
                    } else {
                        quote! { "Type a command or search...".to_string() }
                    };
                    return quote! {
                        rui::components::command_overlay_scope(
                            #id_ts,
                            #open_ts,
                            #placeholder_ts,
                            || #child_el,
                        )
                    };
                }

                if command_is_compound {
                    let id_expr = get_attr("id");
                    let open_expr = get_attr("open");
                    let placeholder_expr = get_attr("placeholder");
                    let idx = *node_counter;
                    *node_counter += 1;
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
                                let mut __list: Vec<rui::core::tree::Element> = Vec::new();
                                #( __list.extend(rui::core::tree::build_ui_child(|| (#expanded_children))); )*
                                let mut __frag = rui::core::tree::Element::new();
                                __frag.set_attr("rui_fragment", "1");
                                __frag.children(__list)
                            }
                        }
                    };
                    let id_ts = if let Some(id) = id_expr {
                        quote! { (#id).to_string() }
                    } else {
                        quote! { format!("{}:{}:Command#{}", file!(), line!(), #idx) }
                    };
                    let open_ts = if let Some(open) = open_expr {
                        quote! { Some(#open) }
                    } else {
                        quote! { None }
                    };
                    let placeholder_ts = if let Some(p) = placeholder_expr {
                        quote! { (#p).to_string() }
                    } else {
                        quote! { "Type a command or search...".to_string() }
                    };
                    return quote! {
                        rui::components::command_scope(
                            #id_ts,
                            #open_ts,
                            #placeholder_ts,
                            rui::components::CommandSurface::Inline,
                            || #child_el,
                        )
                    };
                }

                if name_str == "Tooltip" {
                    // Provide context *before* Trigger/Content (eager props order).
                    let id_expr = get_attr("id");
                    let open_expr = get_attr("open");
                    let idx = *node_counter;
                    *node_counter += 1;
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
                                let mut __list: Vec<rui::core::tree::Element> = Vec::new();
                                #( __list.extend(rui::core::tree::build_ui_child(|| (#expanded_children))); )*
                                let mut __frag = rui::core::tree::Element::new();
                                __frag.set_attr("rui_fragment", "1");
                                __frag.children(__list)
                            }
                        }
                    };
                    let id_ts = if let Some(id) = id_expr {
                        quote! { (#id).to_string() }
                    } else {
                        quote! { format!("{}:{}:Tooltip#{}", file!(), line!(), #idx) }
                    };
                    let open_ts = if let Some(open) = open_expr {
                        quote! { Some(#open) }
                    } else {
                        quote! { None }
                    };
                    return quote! {
                        rui::components::tooltip_scope(#id_ts, #open_ts, || #child_el)
                    };
                }

                if name_str == "SidebarProvider" {
                    // Provide context *before* Sidebar / SidebarInset (eager props would miss ctx).
                    let id_expr = get_attr("id");
                    let open_expr = get_attr("open");
                    let default_open_expr =
                        get_attr("defaultOpen").or_else(|| get_attr("default_open"));
                    let variant_expr = get_attr("variant");
                    let class_expr = get_attr("class");
                    let idx = *node_counter;
                    *node_counter += 1;
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
                                let mut __list: Vec<rui::core::tree::Element> = Vec::new();
                                #( __list.extend(rui::core::tree::build_ui_child(|| (#expanded_children))); )*
                                let mut __frag = rui::core::tree::Element::new();
                                __frag.set_attr("rui_fragment", "1");
                                __frag.children(__list)
                            }
                        }
                    };
                    let id_ts = if let Some(id) = id_expr {
                        quote! { (#id).to_string() }
                    } else {
                        quote! { format!("{}:{}:SidebarProvider#{}", file!(), line!(), #idx) }
                    };
                    let open_ts = if let Some(open) = open_expr {
                        quote! { Some(#open) }
                    } else {
                        quote! { None }
                    };
                    let default_open_ts = if let Some(v) = default_open_expr {
                        quote! { #v }
                    } else {
                        quote! { true }
                    };
                    let variant_ts = if let Some(v) = variant_expr {
                        quote! { Some((#v).to_string()) }
                    } else {
                        quote! { None }
                    };
                    let class_ts = if let Some(c) = class_expr {
                        quote! { Some((#c).to_string()) }
                    } else {
                        quote! { None }
                    };
                    return quote! {
                        {
                            let __rui_sidebar_variant: Option<String> = #variant_ts;
                            rui::components::sidebar_provider_scope(
                                #id_ts,
                                #open_ts,
                                #default_open_ts,
                                __rui_sidebar_variant.as_deref(),
                                #class_ts,
                                || #child_el,
                            )
                        }
                    };
                }

                if name_str == "ContextMenu" {
                    // Provide context *before* Trigger/Content (eager props order).
                    let id_expr = get_attr("id");
                    let open_expr = get_attr("open");
                    let idx = *node_counter;
                    *node_counter += 1;
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
                                let mut __list: Vec<rui::core::tree::Element> = Vec::new();
                                #( __list.extend(rui::core::tree::build_ui_child(|| (#expanded_children))); )*
                                let mut __frag = rui::core::tree::Element::new();
                                __frag.set_attr("rui_fragment", "1");
                                __frag.children(__list)
                            }
                        }
                    };
                    let id_ts = if let Some(id) = id_expr {
                        quote! { (#id).to_string() }
                    } else {
                        quote! { format!("{}:{}:ContextMenu#{}", file!(), line!(), #idx) }
                    };
                    let open_ts = if let Some(open) = open_expr {
                        quote! { Some(#open) }
                    } else {
                        quote! { None }
                    };
                    return quote! {
                        rui::components::context_menu_scope(#id_ts, #open_ts, || #child_el)
                    };
                }

                if name_str == "DropdownMenu" {
                    // Provide context *before* Trigger/Content (eager props order).
                    let id_expr = get_attr("id");
                    let open_expr = get_attr("open");
                    let idx = *node_counter;
                    *node_counter += 1;
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
                                let mut __list: Vec<rui::core::tree::Element> = Vec::new();
                                #( __list.extend(rui::core::tree::build_ui_child(|| (#expanded_children))); )*
                                let mut __frag = rui::core::tree::Element::new();
                                __frag.set_attr("rui_fragment", "1");
                                __frag.children(__list)
                            }
                        }
                    };
                    let id_ts = if let Some(id) = id_expr {
                        quote! { (#id).to_string() }
                    } else {
                        quote! { format!("{}:{}:DropdownMenu#{}", file!(), line!(), #idx) }
                    };
                    let open_ts = if let Some(open) = open_expr {
                        quote! { Some(#open) }
                    } else {
                        quote! { None }
                    };
                    return quote! {
                        rui::components::dropdown_menu_scope(#id_ts, #open_ts, || #child_el)
                    };
                }

                if name_str == "Menubar" {
                    let id_expr = get_attr("id");
                    let idx = *node_counter;
                    *node_counter += 1;
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
                                let mut __list: Vec<rui::core::tree::Element> = Vec::new();
                                #( __list.extend(rui::core::tree::build_ui_child(|| (#expanded_children))); )*
                                let mut __frag = rui::core::tree::Element::new();
                                __frag.set_attr("rui_fragment", "1");
                                __frag.children(__list)
                            }
                        }
                    };
                    let id_ts = if let Some(id) = id_expr {
                        quote! { (#id).to_string() }
                    } else {
                        quote! { format!("{}:{}:Menubar#{}", file!(), line!(), #idx) }
                    };
                    return quote! {
                        rui::components::menubar_scope(#id_ts, || #child_el)
                    };
                }

                if name_str == "NavigationMenu" {
                    let id_expr = get_attr("id");
                    let viewport_expr = get_attr("viewport");
                    let idx = *node_counter;
                    *node_counter += 1;
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
                                let mut __list: Vec<rui::core::tree::Element> = Vec::new();
                                #( __list.extend(rui::core::tree::build_ui_child(|| (#expanded_children))); )*
                                let mut __frag = rui::core::tree::Element::new();
                                __frag.set_attr("rui_fragment", "1");
                                __frag.children(__list)
                            }
                        }
                    };
                    let id_ts = if let Some(id) = id_expr {
                        quote! { (#id).to_string() }
                    } else {
                        quote! { format!("{}:{}:NavigationMenu#{}", file!(), line!(), #idx) }
                    };
                    let viewport_ts = if let Some(v) = viewport_expr {
                        quote! {
                            {
                                let __v = (#v);
                                let __s = format!("{}", __v).to_ascii_lowercase();
                                !(__s == "false" || __s == "0" || __s == "no" || __s == "off")
                            }
                        }
                    } else {
                        quote! { true }
                    };
                    return quote! {
                        rui::components::navigation_menu_scope(#id_ts, #viewport_ts, || #child_el)
                    };
                }

                if name_str == "NavigationMenuItem" {
                    let value_expr = get_attr("value");
                    let class_expr = get_attr("class");
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
                                let mut __list: Vec<rui::core::tree::Element> = Vec::new();
                                #( __list.extend(rui::core::tree::build_ui_child(|| (#expanded_children))); )*
                                let mut __frag = rui::core::tree::Element::new();
                                __frag.set_attr("rui_fragment", "1");
                                __frag.children(__list)
                            }
                        }
                    };
                    let value_ts = if let Some(v) = value_expr {
                        quote! { Some((#v).to_string()) }
                    } else {
                        quote! { None }
                    };
                    let class_ts = if let Some(c) = class_expr {
                        quote! { Some((#c).to_string()) }
                    } else {
                        quote! { None }
                    };
                    return quote! {
                        rui::components::navigation_menu_item(#value_ts, #class_ts, || #child_el)
                    };
                }

                if name_str == "MenubarMenu" {
                    let id_expr = get_attr("id");
                    let open_expr = get_attr("open");
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
                                let mut __list: Vec<rui::core::tree::Element> = Vec::new();
                                #( __list.extend(rui::core::tree::build_ui_child(|| (#expanded_children))); )*
                                let mut __frag = rui::core::tree::Element::new();
                                __frag.set_attr("rui_fragment", "1");
                                __frag.children(__list)
                            }
                        }
                    };
                    let id_ts = if let Some(id) = id_expr {
                        quote! { Some((#id).to_string()) }
                    } else {
                        quote! { None }
                    };
                    let open_ts = if let Some(open) = open_expr {
                        quote! { Some(#open) }
                    } else {
                        quote! { None }
                    };
                    return quote! {
                        rui::components::menubar_menu(#id_ts, #open_ts, || #child_el)
                    };
                }

                if select_is_compound {
                    // ShadCN compound Select: provide context before Trigger/Content.
                    let id_expr = get_attr("id");
                    let open_expr = get_attr("open");
                    let value_expr = get_attr("value");
                    let placeholder_expr = get_attr("placeholder");
                    let class_expr = get_attr("class");
                    let content_class_expr =
                        get_attr("content_class").or_else(|| get_attr("contentClass"));
                    let scroll_selected_expr =
                        get_attr("scroll_selected").or_else(|| get_attr("scrollSelected"));
                    let idx = *node_counter;
                    *node_counter += 1;
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
                                let mut __list: Vec<rui::core::tree::Element> = Vec::new();
                                #( __list.extend(rui::core::tree::build_ui_child(|| (#expanded_children))); )*
                                let mut __frag = rui::core::tree::Element::new();
                                __frag.set_attr("rui_fragment", "1");
                                __frag.children(__list)
                            }
                        }
                    };
                    let id_ts = if let Some(id) = id_expr {
                        quote! { (#id).to_string() }
                    } else {
                        quote! { format!("{}:{}:Select#{}", file!(), line!(), #idx) }
                    };
                    let open_ts = if let Some(open) = open_expr {
                        quote! { Some(#open) }
                    } else {
                        quote! { None }
                    };
                    let value_ts = if let Some(value) = value_expr {
                        quote! { Some(#value) }
                    } else {
                        quote! { None }
                    };
                    let placeholder_ts = if let Some(ph) = placeholder_expr {
                        quote! { (#ph).to_string() }
                    } else {
                        quote! { "Select…".to_string() }
                    };
                    let class_ts = if let Some(c) = class_expr {
                        quote! { (#c).to_string() }
                    } else {
                        quote! { String::new() }
                    };
                    let content_class_ts = if let Some(c) = content_class_expr {
                        quote! { (#c).to_string() }
                    } else {
                        quote! { String::new() }
                    };
                    let scroll_selected_ts = if let Some(s) = scroll_selected_expr {
                        quote! { (#s).to_string() }
                    } else {
                        quote! { String::new() }
                    };
                    return quote! {
                        rui::components::select_scope(
                            #id_ts,
                            #open_ts,
                            #value_ts,
                            #placeholder_ts,
                            #class_ts,
                            #content_class_ts,
                            #scroll_selected_ts,
                            || #child_el,
                        )
                    };
                }

                if radio_group_is_compound {
                    // solid-ui / ShadCN compound RadioGroup：先 provide context 再构建 Item。
                    let id_expr = get_attr("id");
                    let value_expr = get_attr("value").or_else(|| get_attr("selected"));
                    let default_expr =
                        get_attr("default_value").or_else(|| get_attr("defaultValue"));
                    let disabled_expr = get_attr("disabled");
                    let idx = *node_counter;
                    *node_counter += 1;
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
                                let mut __list: Vec<rui::core::tree::Element> = Vec::new();
                                #( __list.extend(rui::core::tree::build_ui_child(|| (#expanded_children))); )*
                                let mut __frag = rui::core::tree::Element::new();
                                __frag.set_attr("rui_fragment", "1");
                                __frag.children(__list)
                            }
                        }
                    };
                    let id_ts = if let Some(id) = id_expr {
                        quote! { (#id).to_string() }
                    } else {
                        quote! { format!("{}:{}:RadioGroup#{}", file!(), line!(), #idx) }
                    };
                    let value_ts = if let Some(value) = value_expr {
                        quote! { Some(#value) }
                    } else {
                        quote! { None }
                    };
                    let default_ts = if let Some(d) = default_expr {
                        quote! { (#d).to_string() }
                    } else {
                        quote! { String::new() }
                    };
                    let disabled_ts = if let Some(d) = disabled_expr {
                        if let Some(b) = expr_as_bool_compat(d) {
                            quote! { #b }
                        } else {
                            quote! { rui::core::tree::parse_bool_like(#d) }
                        }
                    } else {
                        quote! { false }
                    };
                    return quote! {
                        rui::components::radio_group_scope(
                            #id_ts,
                            #value_ts,
                            #default_ts,
                            #disabled_ts,
                            || #child_el,
                        )
                    };
                }

                if toggle_group_is_compound {
                    // ShadCN compound ToggleGroup: provide context before ToggleGroupItem.
                    let id_expr = get_attr("id");
                    let value_expr = get_attr("value");
                    let values_expr = get_attr("values")
                        .or_else(|| get_attr("value_multi"))
                        .or_else(|| get_attr("valueMulti"));
                    let default_expr =
                        get_attr("default_value").or_else(|| get_attr("defaultValue"));
                    let type_expr = get_attr("type").or_else(|| get_attr("selection"));
                    let multiple_expr = get_attr("multiple");
                    let disabled_expr = get_attr("disabled");
                    let variant_expr = get_attr("variant");
                    let size_expr = get_attr("size");
                    let orientation_expr = get_attr("orientation");
                    let spacing_expr = get_attr("spacing");
                    let joined_expr = get_attr("joined");
                    let layout_expr = get_attr("layout");
                    let required_expr = get_attr("required");
                    let class_expr = get_attr("class");
                    let idx = *node_counter;
                    *node_counter += 1;
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
                                let mut __list: Vec<rui::core::tree::Element> = Vec::new();
                                #( __list.extend(rui::core::tree::build_ui_child(|| (#expanded_children))); )*
                                let mut __frag = rui::core::tree::Element::new();
                                __frag.set_attr("rui_fragment", "1");
                                __frag.children(__list)
                            }
                        }
                    };
                    let id_ts = if let Some(id) = id_expr {
                        quote! { (#id).to_string() }
                    } else {
                        quote! { format!("{}:{}:ToggleGroup#{}", file!(), line!(), #idx) }
                    };
                    let value_ts = if let Some(value) = value_expr {
                        quote! { Some(#value) }
                    } else {
                        quote! { None }
                    };
                    let values_ts = if let Some(values) = values_expr {
                        quote! { Some(#values) }
                    } else {
                        quote! { None }
                    };
                    let default_ts = if let Some(d) = default_expr {
                        quote! { (#d).to_string() }
                    } else {
                        quote! { String::new() }
                    };
                    let multiple_ts = if let Some(t) = type_expr {
                        quote! { (#t).to_string().eq_ignore_ascii_case("multiple") }
                    } else if let Some(m) = multiple_expr {
                        if let Some(b) = expr_as_bool_compat(m) {
                            quote! { #b }
                        } else {
                            quote! { rui::core::tree::parse_bool_like(#m) }
                        }
                    } else {
                        quote! { false }
                    };
                    let disabled_ts = if let Some(d) = disabled_expr {
                        if let Some(b) = expr_as_bool_compat(d) {
                            quote! { #b }
                        } else {
                            quote! { rui::core::tree::parse_bool_like(#d) }
                        }
                    } else {
                        quote! { false }
                    };
                    let variant_ts = if let Some(v) = variant_expr {
                        quote! { (#v).to_string() }
                    } else {
                        quote! { "default".to_string() }
                    };
                    let size_ts = if let Some(s) = size_expr {
                        quote! { (#s).to_string() }
                    } else {
                        quote! { "default".to_string() }
                    };
                    let orientation_ts = if let Some(o) = orientation_expr {
                        quote! { (#o).to_string() }
                    } else {
                        quote! { "horizontal".to_string() }
                    };
                    let spacing_ts = if let Some(s) = spacing_expr {
                        quote! { rui::components::parse_toggle_group_spacing(#s) }
                    } else {
                        quote! { 2u8 }
                    };
                    let joined_ts = if let Some(j) = joined_expr {
                        if let Some(b) = expr_as_bool_compat(j) {
                            quote! { #b }
                        } else {
                            quote! { rui::core::tree::parse_bool_like(#j) }
                        }
                    } else if let Some(l) = layout_expr {
                        quote! { rui::components::ToggleGroupLayout::from((#l).to_string()).is_joined() }
                    } else {
                        quote! { false }
                    };
                    let required_ts = if let Some(r) = required_expr {
                        if let Some(b) = expr_as_bool_compat(r) {
                            quote! { #b }
                        } else {
                            quote! { rui::core::tree::parse_bool_like(#r) }
                        }
                    } else {
                        quote! { false }
                    };
                    let class_ts = if let Some(c) = class_expr {
                        quote! { Some((#c).to_string()) }
                    } else {
                        quote! { None }
                    };
                    return quote! {
                        rui::components::toggle_group_scope(
                            #id_ts,
                            #value_ts,
                            #values_ts,
                            #default_ts,
                            #multiple_ts,
                            #disabled_ts,
                            #variant_ts,
                            #size_ts,
                            #orientation_ts,
                            #spacing_ts,
                            #joined_ts,
                            false,
                            #required_ts,
                            #class_ts,
                            || #child_el,
                        )
                    };
                }

                if tabs_is_compound {
                    // solid-ui / ShadCN compound Tabs：先 provide context 再构建 List/Trigger/Content。
                    let id_expr = get_attr("id");
                    let value_expr = get_attr("value");
                    let default_expr =
                        get_attr("default_value").or_else(|| get_attr("defaultValue"));
                    let orientation_expr = get_attr("orientation");
                    let disabled_expr = get_attr("disabled");
                    let idx = *node_counter;
                    *node_counter += 1;
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
                                let mut __list: Vec<rui::core::tree::Element> = Vec::new();
                                #( __list.extend(rui::core::tree::build_ui_child(|| (#expanded_children))); )*
                                let mut __frag = rui::core::tree::Element::new();
                                __frag.set_attr("rui_fragment", "1");
                                __frag.children(__list)
                            }
                        }
                    };
                    let id_ts = if let Some(id) = id_expr {
                        quote! { (#id).to_string() }
                    } else {
                        quote! { format!("{}:{}:Tabs#{}", file!(), line!(), #idx) }
                    };
                    let value_ts = if let Some(value) = value_expr {
                        quote! { Some(#value) }
                    } else {
                        quote! { None }
                    };
                    let default_ts = if let Some(d) = default_expr {
                        quote! { (#d).to_string() }
                    } else {
                        quote! { String::new() }
                    };
                    let orientation_ts = if let Some(o) = orientation_expr {
                        quote! { (#o).to_string() }
                    } else {
                        quote! { "horizontal".to_string() }
                    };
                    let disabled_ts = if let Some(d) = disabled_expr {
                        if let Some(b) = expr_as_bool_compat(d) {
                            quote! { #b }
                        } else {
                            quote! { rui::core::tree::parse_bool_like(#d) }
                        }
                    } else {
                        quote! { false }
                    };
                    return quote! {
                        rui::components::tabs_scope(
                            #id_ts,
                            #value_ts,
                            #default_ts,
                            #orientation_ts,
                            #disabled_ts,
                            || #child_el,
                        )
                    };
                }

                if carousel_is_compound {
                    // solid-ui / ShadCN compound Carousel：先 provide context 再构建 Content/Item/Prev/Next。
                    let id_expr = get_attr("id");
                    let index_expr = get_attr("index")
                        .or_else(|| get_attr("index_signal"))
                        .or_else(|| get_attr("indexSignal"));
                    let orientation_expr = get_attr("orientation");
                    let loop_expr = get_attr("loop")
                        .or_else(|| get_attr("loop_slides"))
                        .or_else(|| get_attr("loopSlides"));
                    let seamless_expr = get_attr("seamless")
                        .or_else(|| get_attr("seamless_loop"))
                        .or_else(|| get_attr("seamlessLoop"));
                    let snap_expr = get_attr("snap");
                    let per_view_expr = get_attr("per_view")
                        .or_else(|| get_attr("perView"))
                        .or_else(|| get_attr("slides_per_view"))
                        .or_else(|| get_attr("slidesPerView"));
                    let autoplay_expr = get_attr("autoplay")
                        .or_else(|| get_attr("autoplay_ms"))
                        .or_else(|| get_attr("autoplayMs"));
                    let controls_expr = get_attr("controls")
                        .or_else(|| get_attr("controls_placement"))
                        .or_else(|| get_attr("controlsPlacement"));
                    let scroll_by_expr = get_attr("scroll_by")
                        .or_else(|| get_attr("scrollBy"))
                        .or_else(|| get_attr("step"));
                    let end_align_expr = get_attr("end_align")
                        .or_else(|| get_attr("endAlign"))
                        .or_else(|| get_attr("page_end"))
                        .or_else(|| get_attr("pageEnd"));
                    let page_snap_expr = get_attr("page_snap")
                        .or_else(|| get_attr("pageSnap"))
                        .or_else(|| get_attr("carry_peek"))
                        .or_else(|| get_attr("carryPeek"));
                    let item_sizing_expr = get_attr("item_sizing")
                        .or_else(|| get_attr("itemSizing"))
                        .or_else(|| get_attr("sizing"));
                    let item_width_expr = get_attr("item_width")
                        .or_else(|| get_attr("itemWidth"))
                        .or_else(|| get_attr("slide_width"))
                        .or_else(|| get_attr("slideWidth"));
                    let dots_expr = get_attr("dots")
                        .or_else(|| get_attr("show_dots"))
                        .or_else(|| get_attr("showDots"));
                    let stack_sides_expr = get_attr("stack_sides")
                        .or_else(|| get_attr("stackSides"))
                        .or_else(|| get_attr("stack_depth"))
                        .or_else(|| get_attr("stackDepth"));
                    let variant_expr = get_attr("variant").or_else(|| get_attr("kind"));
                    // 专用标签锁定变体；统一 `<Carousel>` 读 `variant`/`kind`（默认 single）。
                    let (variant_default, default_per_view) = match name_str.as_str() {
                        "CarouselSingle" => ("single", quote! { 1.0f32 }),
                        "CarouselMulti" => ("multi", quote! { 2.2f32 }),
                        "CarouselStack" => ("stack", quote! { 1.0f32 }),
                        // 统一入口：缺省 perView 传 0，由 carousel_scope 按 variant 填默认。
                        "Carousel" => ("single", quote! { 0.0f32 }),
                        _ => ("single", quote! { 1.0f32 }),
                    };
                    let pinned_variant = !matches!(name_str.as_str(), "Carousel");
                    let idx = *node_counter;
                    *node_counter += 1;
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
                                let mut __list: Vec<rui::core::tree::Element> = Vec::new();
                                #( __list.extend(rui::core::tree::build_ui_child(|| (#expanded_children))); )*
                                let mut __frag = rui::core::tree::Element::new();
                                __frag.set_attr("rui_fragment", "1");
                                __frag.children(__list)
                            }
                        }
                    };
                    let id_ts = if let Some(id) = id_expr {
                        quote! { (#id).to_string() }
                    } else {
                        quote! { format!("{}:{}:Carousel#{}", file!(), line!(), #idx) }
                    };
                    let index_ts = if let Some(index) = index_expr {
                        quote! { Some(#index) }
                    } else {
                        quote! { None }
                    };
                    let orientation_ts = if let Some(o) = orientation_expr {
                        quote! { (#o).to_string() }
                    } else {
                        quote! { "horizontal".to_string() }
                    };
                    let loop_ts = if let Some(d) = loop_expr {
                        if let Some(b) = expr_as_bool_compat(d) {
                            quote! { #b }
                        } else {
                            quote! { rui::core::tree::parse_bool_like(#d) }
                        }
                    } else {
                        quote! { true }
                    };
                    let seamless_ts = if let Some(d) = seamless_expr {
                        if let Some(b) = expr_as_bool_compat(d) {
                            quote! { Some(#b) }
                        } else {
                            quote! { Some(rui::core::tree::parse_bool_like(#d)) }
                        }
                    } else {
                        quote! { None }
                    };
                    let snap_ts = if let Some(d) = snap_expr {
                        if let Some(b) = expr_as_bool_compat(d) {
                            quote! { #b }
                        } else {
                            quote! { rui::core::tree::parse_bool_like(#d) }
                        }
                    } else {
                        quote! { true }
                    };
                    let per_view_ts = if let Some(p) = per_view_expr {
                        quote_unitless_f32(p)
                    } else {
                        default_per_view
                    };
                    let autoplay_ts = if let Some(a) = autoplay_expr {
                        quote! { (#a) as u64 }
                    } else {
                        quote! { 0u64 }
                    };
                    let controls_ts = if let Some(c) = controls_expr {
                        quote! { (#c).to_string() }
                    } else {
                        quote! { "outside".to_string() }
                    };
                    let scroll_by_ts = if let Some(s) = scroll_by_expr {
                        quote! { (#s).to_string() }
                    } else {
                        quote! { "slide".to_string() }
                    };
                    let kind_ts = if pinned_variant {
                        let v = variant_default;
                        quote! { #v.to_string() }
                    } else if let Some(v) = variant_expr {
                        quote! { (#v).to_string() }
                    } else {
                        let v = variant_default;
                        quote! { #v.to_string() }
                    };
                    let end_align_ts = if let Some(e) = end_align_expr {
                        quote! { (#e).to_string() }
                    } else {
                        quote! { "flush".to_string() }
                    };
                    let page_snap_ts = if let Some(p) = page_snap_expr {
                        // carryPeek={true|false} → peek|viewport；或 pageSnap="peek"|"viewport"
                        if let Some(b) = expr_as_bool_compat(p) {
                            if b {
                                quote! { "peek".to_string() }
                            } else {
                                quote! { "viewport".to_string() }
                            }
                        } else {
                            quote! { (#p).to_string() }
                        }
                    } else {
                        quote! { "peek".to_string() }
                    };
                    let item_sizing_ts = if let Some(s) = item_sizing_expr {
                        quote! { (#s).to_string() }
                    } else {
                        quote! { "fraction".to_string() }
                    };
                    let item_width_ts = if let Some(w) = item_width_expr {
                        quote! { (#w).to_string() }
                    } else {
                        quote! { "12rem".to_string() }
                    };
                    let dots_ts = if let Some(d) = dots_expr {
                        if let Some(b) = expr_as_bool_compat(d) {
                            quote! { Some(#b) }
                        } else {
                            quote! { Some(rui::core::tree::parse_bool_like(#d)) }
                        }
                    } else {
                        quote! { None }
                    };
                    let stack_sides_ts = if let Some(s) = stack_sides_expr {
                        quote! { (#s) as u32 }
                    } else {
                        quote! { 1u32 }
                    };
                    return quote! {
                        rui::components::carousel_scope(
                            #id_ts,
                            #index_ts,
                            #orientation_ts,
                            #loop_ts,
                            #seamless_ts,
                            #snap_ts,
                            #per_view_ts,
                            #autoplay_ts,
                            #controls_ts,
                            #scroll_by_ts,
                            #kind_ts,
                            #end_align_ts,
                            #page_snap_ts,
                            #item_sizing_ts,
                            #item_width_ts,
                            #dots_ts,
                            #stack_sides_ts,
                            || #child_el,
                        )
                    };
                }

                if name_str == "TabsContent" {
                    let Some(value_expr) = get_attr("value") else {
                        return quote! { compile_error!("<TabsContent> requires `value={...}`"); };
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
                                let mut __list: Vec<rui::core::tree::Element> = Vec::new();
                                #( __list.extend(rui::core::tree::build_ui_child(|| (#expanded_children))); )*
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
                        rui::components::tabs_content((#value_expr).to_string(), move || #child_el)
                    };
                }

                if name_str == "AccordionContent" {
                    let mut expanded_children = Vec::new();
                    for child in &el.children {
                        expanded_children.push(expand_node(child, node_counter, true));
                    }
                    let child_el = if expanded_children.is_empty() {
                        quote! { rui::core::tree::Element::new() }
                    } else {
                        quote! {
                            {
                                let mut __list: Vec<rui::core::tree::Element> = Vec::new();
                                #( __list.extend(rui::core::tree::build_ui_child(|| (#expanded_children))); )*
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
                        rui::components::accordion_content(move || #child_el)
                    };
                }

                if name_str == "CollapsibleContent" {
                    let mut expanded_children = Vec::new();
                    for child in &el.children {
                        expanded_children.push(expand_node(child, node_counter, true));
                    }
                    let child_el = if expanded_children.is_empty() {
                        quote! { rui::core::tree::Element::new() }
                    } else {
                        quote! {
                            {
                                let mut __list: Vec<rui::core::tree::Element> = Vec::new();
                                #( __list.extend(rui::core::tree::build_ui_child(|| (#expanded_children))); )*
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
                        rui::components::collapsible_content(move || #child_el)
                    };
                }

                if name_str == "TreeViewItem" {
                    let Some(id_expr) = get_attr("id") else {
                        return quote! { compile_error!("<TreeViewItem> requires `id={...}`"); };
                    };
                    let label_expr = get_attr("label");
                    let disabled_expr = get_attr("disabled");
                    let class_expr = get_attr("class");
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
                                let mut __list: Vec<rui::core::tree::Element> = Vec::new();
                                #( __list.extend(rui::core::tree::build_ui_child(|| (#expanded_children))); )*
                                let mut __frag = rui::core::tree::Element::new();
                                __frag.set_attr("rui_fragment", "1");
                                __frag.children(__list)
                            }
                        }
                    };
                    let id_ts = quote! { (#id_expr).to_string() };
                    let label_ts = if let Some(l) = label_expr {
                        quote! { (#l).to_string() }
                    } else {
                        quote! { (#id_expr).to_string() }
                    };
                    let disabled_ts = if let Some(d) = disabled_expr {
                        if let Some(b) = expr_as_bool_compat(d) {
                            quote! { #b }
                        } else {
                            quote! { rui::core::tree::parse_bool_like(#d) }
                        }
                    } else {
                        quote! { false }
                    };
                    let class_ts = if let Some(c) = class_expr {
                        quote! { Some((#c).to_string()) }
                    } else {
                        quote! { None }
                    };
                    return quote! {
                        rui::components::tree_view_item_scope(
                            #id_ts,
                            #label_ts,
                            #disabled_ts,
                            #class_ts,
                            || #child_el,
                        )
                    };
                }

                if name_str == "AccordionItem" {
                    let Some(value_expr) = get_attr("value") else {
                        return quote! { compile_error!("<AccordionItem> requires `value={...}`"); };
                    };
                    let disabled_expr = get_attr("disabled");
                    let class_expr = get_attr("class");
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
                                let mut __list: Vec<rui::core::tree::Element> = Vec::new();
                                #( __list.extend(rui::core::tree::build_ui_child(|| (#expanded_children))); )*
                                let mut __frag = rui::core::tree::Element::new();
                                __frag.set_attr("rui_fragment", "1");
                                __frag.children(__list)
                            }
                        }
                    };
                    let value_ts = quote! { (#value_expr).to_string() };
                    let disabled_ts = if let Some(d) = disabled_expr {
                        if let Some(b) = expr_as_bool_compat(d) {
                            quote! { #b }
                        } else {
                            quote! { rui::core::tree::parse_bool_like(#d) }
                        }
                    } else {
                        quote! { false }
                    };
                    let class_ts = if let Some(c) = class_expr {
                        quote! { Some((#c).to_string()) }
                    } else {
                        quote! { None }
                    };
                    return quote! {
                        rui::components::accordion_item_scope(
                            #value_ts,
                            #disabled_ts,
                            #class_ts,
                            || #child_el,
                        )
                    };
                }

                if name_str == "QuestionnaireItem" {
                    let Some(name_expr) = get_attr("name") else {
                        return quote! {
                            compile_error!("<QuestionnaireItem> requires `name={...}`");
                        };
                    };
                    let required_expr = get_attr("required");
                    let multiple_expr = get_attr("multiple");
                    let disabled_expr = get_attr("disabled");
                    let invalid_expr = get_attr("invalid");
                    let class_expr = get_attr("class");
                    let on_status_expr =
                        get_attr("on_status_change").or_else(|| get_attr("onStatusChange"));
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
                                let mut __list: Vec<rui::core::tree::Element> = Vec::new();
                                #( __list.extend(rui::core::tree::build_ui_child(|| (#expanded_children))); )*
                                let mut __frag = rui::core::tree::Element::new();
                                __frag.set_attr("rui_fragment", "1");
                                __frag.children(__list)
                            }
                        }
                    };
                    let name_ts = quote! { (#name_expr).to_string() };
                    let required_ts = if let Some(v) = required_expr {
                        if let Some(b) = expr_as_bool_compat(v) {
                            quote! { #b }
                        } else {
                            quote! { rui::core::tree::parse_bool_like(#v) }
                        }
                    } else {
                        quote! { false }
                    };
                    let multiple_ts = if let Some(v) = multiple_expr {
                        if let Some(b) = expr_as_bool_compat(v) {
                            quote! { #b }
                        } else {
                            quote! { rui::core::tree::parse_bool_like(#v) }
                        }
                    } else {
                        quote! { false }
                    };
                    let disabled_ts = if let Some(d) = disabled_expr {
                        if let Some(b) = expr_as_bool_compat(d) {
                            quote! { #b }
                        } else {
                            quote! { rui::core::tree::parse_bool_like(#d) }
                        }
                    } else {
                        quote! { false }
                    };
                    let invalid_ts = if let Some(v) = invalid_expr {
                        if let Some(b) = expr_as_bool_compat(v) {
                            quote! { #b }
                        } else {
                            quote! { rui::core::tree::parse_bool_like(#v) }
                        }
                    } else {
                        quote! { false }
                    };
                    let class_ts = if let Some(c) = class_expr {
                        quote! { Some((#c).to_string()) }
                    } else {
                        quote! { None }
                    };
                    let on_status_ts = if let Some(s) = on_status_expr {
                        quote! { Some(rui::components::questionnaire_status_change_fn(#s)) }
                    } else {
                        quote! { None }
                    };
                    return quote! {
                        rui::components::questionnaire_item_scope(
                            #name_ts,
                            #required_ts,
                            #multiple_ts,
                            #disabled_ts,
                            #invalid_ts,
                            #class_ts,
                            #on_status_ts,
                            || #child_el,
                        )
                    };
                }

                if name_str == "DialogContent" {
                    let show_close = get_attr("show_close")
                        .or_else(|| get_attr("showClose"))
                        .map(|e| quote! { #e })
                        .unwrap_or_else(|| quote! { true });
                    let overlay_color = get_attr("overlay_color")
                        .or_else(|| get_attr("overlayColor"))
                        .map(|e| quote! { Some((#e).to_string()) })
                        .unwrap_or_else(|| quote! { None });
                    let overlay_opacity = get_attr("overlay_opacity")
                        .or_else(|| get_attr("overlayOpacity"))
                        .map(|e| {
                            let v = quote_unitless_f32(e);
                            quote! { Some(#v) }
                        })
                        .unwrap_or_else(|| quote! { None });
                    let close_on_overlay = get_attr("close_on_overlay_click")
                        .or_else(|| get_attr("closeOnOverlayClick"))
                        .map(|e| quote! { #e })
                        .unwrap_or_else(|| quote! { true });
                    let overlay_blur = get_attr("overlay_blur")
                        .or_else(|| get_attr("overlayBlur"))
                        .or_else(|| get_attr("backdrop_blur"))
                        .or_else(|| get_attr("backdropBlur"))
                        .map(|e| {
                            let v = quote_length_f32(e);
                            quote! { Some(#v) }
                        })
                        .unwrap_or_else(|| quote! { None });
                    let class = get_attr("class")
                        .or_else(|| get_attr("className"))
                        .map(|e| quote! { Some((#e).to_string()) })
                        .unwrap_or_else(|| quote! { None });
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
                                let mut __list: Vec<rui::core::tree::Element> = Vec::new();
                                #( __list.extend(rui::core::tree::build_ui_child(|| (#expanded_children))); )*
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
                        rui::components::dialog_content(
                            rui::components::DialogContentOpts {
                                show_close: #show_close,
                                overlay_color: #overlay_color,
                                overlay_opacity: #overlay_opacity,
                                close_on_overlay_click: #close_on_overlay,
                                overlay_blur: #overlay_blur,
                                hug_content: false,
                                class: #class,
                                style: None,
                            },
                            move || #child_el,
                        )
                    };
                }

                if name_str == "AlertDialogContent" {
                    let overlay_color = get_attr("overlay_color")
                        .or_else(|| get_attr("overlayColor"))
                        .map(|e| quote! { Some((#e).to_string()) })
                        .unwrap_or_else(|| quote! { None });
                    let overlay_opacity = get_attr("overlay_opacity")
                        .or_else(|| get_attr("overlayOpacity"))
                        .map(|e| {
                            let v = quote_unitless_f32(e);
                            quote! { Some(#v) }
                        })
                        .unwrap_or_else(|| quote! { None });
                    let close_on_overlay = get_attr("close_on_overlay_click")
                        .or_else(|| get_attr("closeOnOverlayClick"))
                        .map(|e| quote! { #e })
                        .unwrap_or_else(|| quote! { false });
                    let overlay_blur = get_attr("overlay_blur")
                        .or_else(|| get_attr("overlayBlur"))
                        .or_else(|| get_attr("backdrop_blur"))
                        .or_else(|| get_attr("backdropBlur"))
                        .map(|e| {
                            let v = quote_length_f32(e);
                            quote! { Some(#v) }
                        })
                        .unwrap_or_else(|| quote! { None });
                    let size = get_attr("size")
                        .map(|e| quote! { rui::components::AlertDialogSize::from(#e) })
                        .unwrap_or_else(|| {
                            quote! { rui::components::AlertDialogSize::Default }
                        });
                    let variant_static = get_attr("variant")
                        .filter(|e| !expr_reads_get(e))
                        .map(|e| quote! { rui::components::AlertDialogVariant::from(#e) })
                        .unwrap_or_else(|| {
                            quote! { rui::components::AlertDialogVariant::Default }
                        });
                    let filled_static = get_attr("filled")
                        .filter(|e| !expr_reads_get(e))
                        .map(|e| quote! { #e })
                        .unwrap_or_else(|| quote! { false });
                    let chrome_static = get_attr("chrome")
                        .filter(|e| !expr_reads_get(e))
                        .map(|e| quote! { #e })
                        .unwrap_or_else(|| quote! { true });
                    let variant_signal = get_attr("variant_signal")
                        .or_else(|| get_attr("variantSignal"))
                        .map(|e| quote! { Some(#e) })
                        .unwrap_or_else(|| quote! { None });
                    let chrome_signal = get_attr("chrome_signal")
                        .or_else(|| get_attr("chromeSignal"))
                        .map(|e| quote! { Some(#e) })
                        .unwrap_or_else(|| quote! { None });
                    let class = get_attr("class")
                        .or_else(|| get_attr("className"))
                        .map(|e| quote! { Some((#e).to_string()) })
                        .unwrap_or_else(|| quote! { None });
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
                                let mut __list: Vec<rui::core::tree::Element> = Vec::new();
                                #( __list.extend(rui::core::tree::build_ui_child(|| (#expanded_children))); )*
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
                        rui::components::alert_dialog_content(
                            rui::components::AlertDialogContentOpts {
                                overlay_color: #overlay_color,
                                overlay_opacity: #overlay_opacity,
                                close_on_overlay_click: #close_on_overlay,
                                overlay_blur: #overlay_blur,
                                size: #size,
                                variant: #variant_static,
                                variant_signal: #variant_signal,
                                filled: #filled_static,
                                chrome: #chrome_static,
                                chrome_signal: #chrome_signal,
                                class: #class,
                                style: None,
                            },
                            move || #child_el,
                        )
                    };
                }

                if name_str == "SheetContent" {
                    let show_close = get_attr("show_close")
                        .or_else(|| get_attr("showClose"))
                        .or_else(|| get_attr("showCloseButton"))
                        .map(|e| quote! { #e })
                        .unwrap_or_else(|| quote! { true });
                    let side = get_attr("side")
                        .map(|e| quote! { rui::components::SheetSide::from(#e) })
                        .unwrap_or_else(|| {
                            quote! { rui::components::SheetSide::Right }
                        });
                    let overlay_color = get_attr("overlay_color")
                        .or_else(|| get_attr("overlayColor"))
                        .map(|e| quote! { Some((#e).to_string()) })
                        .unwrap_or_else(|| quote! { None });
                    let overlay_opacity = get_attr("overlay_opacity")
                        .or_else(|| get_attr("overlayOpacity"))
                        .map(|e| {
                            let v = quote_unitless_f32(e);
                            quote! { Some(#v) }
                        })
                        .unwrap_or_else(|| quote! { None });
                    let close_on_overlay = get_attr("close_on_overlay_click")
                        .or_else(|| get_attr("closeOnOverlayClick"))
                        .map(|e| quote! { #e })
                        .unwrap_or_else(|| quote! { true });
                    let overlay_blur = get_attr("overlay_blur")
                        .or_else(|| get_attr("overlayBlur"))
                        .or_else(|| get_attr("backdrop_blur"))
                        .or_else(|| get_attr("backdropBlur"))
                        .map(|e| {
                            let v = quote_length_f32(e);
                            quote! { Some(#v) }
                        })
                        .unwrap_or_else(|| quote! { None });
                    let width = get_attr("w")
                        .or_else(|| get_attr("width"))
                        .map(|e| quote! { Some((#e).to_string()) })
                        .unwrap_or_else(|| quote! { None });
                    let height = get_attr("h")
                        .or_else(|| get_attr("height"))
                        .map(|e| quote! { Some((#e).to_string()) })
                        .unwrap_or_else(|| quote! { None });
                    let class = get_attr("class")
                        .or_else(|| get_attr("className"))
                        .map(|e| quote! { Some((#e).to_string()) })
                        .unwrap_or_else(|| quote! { None });
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
                                let mut __list: Vec<rui::core::tree::Element> = Vec::new();
                                #( __list.extend(rui::core::tree::build_ui_child(|| (#expanded_children))); )*
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
                        rui::components::sheet_content(
                            rui::components::SheetContentOpts {
                                side: #side,
                                show_close: #show_close,
                                overlay_color: #overlay_color,
                                overlay_opacity: #overlay_opacity,
                                close_on_overlay_click: #close_on_overlay,
                                overlay_blur: #overlay_blur,
                                width: #width,
                                height: #height,
                                class: #class,
                                style: None,
                            },
                            move || #child_el,
                        )
                    };
                }

                if name_str == "DrawerContent" {
                    let show_close = get_attr("show_close")
                        .or_else(|| get_attr("showClose"))
                        .or_else(|| get_attr("showCloseButton"))
                        .map(|e| quote! { #e })
                        .unwrap_or_else(|| quote! { false });
                    let show_swipe_handle = get_attr("show_swipe_handle")
                        .or_else(|| get_attr("showSwipeHandle"))
                        .map(|e| quote! { #e })
                        .unwrap_or_else(|| quote! { false });
                    let snap_to_sequential_point = get_attr("snap_to_sequential_point")
                        .or_else(|| get_attr("snapToSequentialPoint"))
                        .map(|e| quote! { #e })
                        .unwrap_or_else(|| quote! { false });
                    let scale_background = get_attr("scale_background")
                        .or_else(|| get_attr("scaleBackground"))
                        .or_else(|| get_attr("shouldScaleBackground"))
                        .map(|e| quote! { #e })
                        .unwrap_or_else(|| quote! { false });
                    let class = get_attr("class")
                        .or_else(|| get_attr("className"))
                        .map(|e| quote! { Some((#e).to_string()) })
                        .unwrap_or_else(|| quote! { None });
                    let modal = get_attr("modal")
                        .map(|e| quote! { #e })
                        .unwrap_or_else(|| quote! { true });
                    let direction = get_attr("direction")
                        .or_else(|| get_attr("swipe_direction"))
                        .or_else(|| get_attr("swipeDirection"))
                        .or_else(|| get_attr("side"))
                        .map(|e| quote! { rui::components::DrawerDirection::from(#e) })
                        .unwrap_or_else(|| {
                            quote! { rui::components::DrawerDirection::Down }
                        });
                    let overlay_color = get_attr("overlay_color")
                        .or_else(|| get_attr("overlayColor"))
                        .map(|e| quote! { Some((#e).to_string()) })
                        .unwrap_or_else(|| quote! { None });
                    let overlay_opacity = get_attr("overlay_opacity")
                        .or_else(|| get_attr("overlayOpacity"))
                        .map(|e| {
                            let v = quote_unitless_f32(e);
                            quote! { Some(#v) }
                        })
                        .unwrap_or_else(|| quote! { None });
                    let close_on_overlay = get_attr("close_on_overlay_click")
                        .or_else(|| get_attr("closeOnOverlayClick"))
                        .map(|e| quote! { #e })
                        .unwrap_or_else(|| quote! { true });
                    let overlay_blur = get_attr("overlay_blur")
                        .or_else(|| get_attr("overlayBlur"))
                        .or_else(|| get_attr("backdrop_blur"))
                        .or_else(|| get_attr("backdropBlur"))
                        .map(|e| {
                            let v = quote_length_f32(e);
                            quote! { Some(#v) }
                        })
                        .unwrap_or_else(|| quote! { None });
                    let width = get_attr("w")
                        .or_else(|| get_attr("width"))
                        .map(|e| quote! { Some((#e).to_string()) })
                        .unwrap_or_else(|| quote! { None });
                    let height = get_attr("h")
                        .or_else(|| get_attr("height"))
                        .map(|e| quote! { Some((#e).to_string()) })
                        .unwrap_or_else(|| quote! { None });
                    let snap_points = get_attr("snap_points")
                        .or_else(|| get_attr("snapPoints"))
                        .map(|e| {
                            quote! { rui::components::drawer::coerce_snap_points(#e) }
                        })
                        .unwrap_or_else(
                            || quote! { vec![rui::components::DrawerSnapPoint::Fraction(1.0)] },
                        );
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
                                let mut __list: Vec<rui::core::tree::Element> = Vec::new();
                                #( __list.extend(rui::core::tree::build_ui_child(|| (#expanded_children))); )*
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
                        rui::components::drawer_content(
                            rui::components::DrawerContentOpts {
                                direction: #direction,
                                show_close: #show_close,
                                show_swipe_handle: #show_swipe_handle,
                                modal: #modal,
                                overlay_color: #overlay_color,
                                overlay_opacity: #overlay_opacity,
                                close_on_overlay_click: #close_on_overlay,
                                overlay_blur: #overlay_blur,
                                width: #width,
                                height: #height,
                                snap_points: #snap_points,
                                snap_to_sequential_point: #snap_to_sequential_point,
                                scale_background: #scale_background,
                                class: #class,
                                style: None,
                            },
                            move || #child_el,
                        )
                    };
                }

                if name_str == "TooltipContent" {
                    let side = get_attr("side")
                        .or_else(|| get_attr("placement"))
                        .map(|e| quote! { (#e).to_string() })
                        .unwrap_or_else(|| quote! { "bottom".to_string() });
                    let class = get_attr("class")
                        .map(|e| quote! { Some((#e).to_string()) })
                        .unwrap_or_else(|| quote! { None });
                    let mut expanded_children = Vec::new();
                    for child in &el.children {
                        // Body is rebuilt inside tooltip_content's reactive_view.
                        expanded_children.push(expand_node(child, node_counter, true));
                    }
                    let child_el = if expanded_children.is_empty() {
                        quote! { rui::core::tree::Element::new() }
                    } else {
                        quote! {
                            {
                                let mut __list: Vec<rui::core::tree::Element> = Vec::new();
                                #( __list.extend(rui::core::tree::build_ui_child(|| (#expanded_children))); )*
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
                        rui::components::tooltip_content(
                            rui::components::TooltipContentOpts {
                                side: #side,
                                class: #class,
                                style: None,
                            },
                            move || #child_el,
                        )
                    };
                }

                if name_str == "PopoverContent" {
                    let side = get_attr("side")
                        .or_else(|| get_attr("placement"))
                        .map(|e| quote! { (#e).to_string() })
                        .unwrap_or_else(|| quote! { "bottom".to_string() });
                    let align = get_attr("align")
                        .map(|e| quote! { (#e).to_string() })
                        .unwrap_or_else(|| quote! { "center".to_string() });
                    let side_offset = get_attr("side_offset")
                        .or_else(|| get_attr("sideOffset"))
                        .map(|e| quote_length_f32(e))
                        .unwrap_or_else(|| quote! { 4.0f32 });
                    let class = get_attr("class")
                        .map(|e| quote! { Some((#e).to_string()) })
                        .unwrap_or_else(|| quote! { None });
                    let strategy = get_attr("strategy")
                        .map(|e| quote! { Some((#e).to_string()) })
                        .unwrap_or_else(|| quote! { None });
                    let width = get_attr("width")
                        .or_else(|| get_attr("w"))
                        .map(|e| quote! { Some((#e).to_string()) })
                        .unwrap_or_else(|| quote! { None });
                    let mut expanded_children = Vec::new();
                    for child in &el.children {
                        // Body is rebuilt inside popover_content's reactive_anchor.
                        expanded_children.push(expand_node(child, node_counter, true));
                    }
                    let child_el = if expanded_children.is_empty() {
                        quote! { rui::core::tree::Element::new() }
                    } else {
                        quote! {
                            {
                                let mut __list: Vec<rui::core::tree::Element> = Vec::new();
                                #( __list.extend(rui::core::tree::build_ui_child(|| (#expanded_children))); )*
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
                        rui::components::popover_content(
                            rui::components::PopoverContentOpts {
                                side: #side,
                                align: #align,
                                side_offset: #side_offset,
                                class: #class,
                                strategy: #strategy,
                                width: #width,
                                style: None,
                            },
                            move || #child_el,
                        )
                    };
                }

                if name_str == "AnchoredOverlayContent" {
                    let placement_mode = get_attr("placement")
                        .map(|e| quote! { (#e).to_string() })
                        .unwrap_or_else(|| quote! { "trigger".to_string() });
                    let side = get_attr("side")
                        .map(|e| quote! { (#e).to_string() })
                        .unwrap_or_else(|| quote! { "bottom".to_string() });
                    let align = get_attr("align")
                        .map(|e| quote! { (#e).to_string() })
                        .unwrap_or_else(|| quote! { "start".to_string() });
                    let side_offset = get_attr("side_offset")
                        .or_else(|| get_attr("sideOffset"))
                        .map(|e| quote_length_f32(e))
                        .unwrap_or_else(|| quote! { 4.0f32 });
                    let focus_id = get_attr("focus_id")
                        .or_else(|| get_attr("focusId"))
                        .or_else(|| get_attr("triggerId"))
                        .map(|e| quote! { (#e).to_string() })
                        .unwrap_or_else(|| quote! { String::new() });
                    let strategy = get_attr("strategy")
                        .map(|e| quote! { (#e).to_string() })
                        .unwrap_or_else(|| quote! { String::new() });
                    let pos = get_attr("pos")
                        .map(|e| quote! { Some(#e) })
                        .unwrap_or_else(|| quote! { None });
                    let x = get_attr("x")
                        .map(|e| quote_length_f32(e))
                        .unwrap_or_else(|| quote! { 0.0f32 });
                    let y = get_attr("y")
                        .map(|e| quote_length_f32(e))
                        .unwrap_or_else(|| quote! { 0.0f32 });
                    let gap = get_attr("gap")
                        .map(|e| quote_length_f32(e))
                        .unwrap_or_else(|| quote! { 0.0f32 });
                    let kind = get_attr("kind")
                        .map(|e| {
                            quote! {
                                rui::utils::anchored_overlay::parse_overlay_kind(&(#e).to_string())
                            }
                        })
                        .unwrap_or_else(|| {
                            quote! { rui::app::portal_overlay::OverlayKind::Popover }
                        });
                    let motion = get_attr("motion")
                        .map(|e| {
                            quote! {
                                rui::utils::anchored_overlay::parse_floating_motion(&(#e).to_string())
                            }
                        })
                        .unwrap_or_else(|| {
                            quote! { rui::utils::anchored_overlay::FloatingMotionKind::Fade }
                        });
                    let mode = get_attr("mode")
                        .map(|e| {
                            quote! {
                                rui::utils::anchored_overlay::parse_overlay_mode(&(#e).to_string())
                            }
                        })
                        .unwrap_or_else(|| {
                            quote! { rui::app::portal_overlay::OverlayMode::Stacked }
                        });
                    let match_trigger = get_attr("match_trigger_width")
                        .or_else(|| get_attr("matchTriggerWidth"))
                        .map(|e| quote! { (#e) })
                        .unwrap_or_else(|| quote! { false });
                    let max_h = get_attr("max_h")
                        .or_else(|| get_attr("maxH"))
                        .map(|e| quote! { Some((#e).to_string()) })
                        .unwrap_or_else(|| quote! { None });
                    let width = get_attr("width")
                        .or_else(|| get_attr("w"))
                        .map(|e| quote! { Some((#e).to_string()) })
                        .unwrap_or_else(|| quote! { None });
                    let mut expanded_children = Vec::new();
                    for child in &el.children {
                        expanded_children.push(expand_node(child, node_counter, true));
                    }
                    let child_el = if expanded_children.is_empty() {
                        quote! { rui::core::tree::Element::new() }
                    } else {
                        quote! {
                            {
                                let mut __list: Vec<rui::core::tree::Element> = Vec::new();
                                #( __list.extend(rui::core::tree::build_ui_child(|| (#expanded_children))); )*
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
                        rui::utils::anchored_overlay::anchored_overlay_content_from_rsx(
                            {
                                let mut __opts =
                                    rui::utils::anchored_overlay::AnchoredOverlayContentOpts::default();
                                __opts.placement = None;
                                __opts.placement_mode = #placement_mode;
                                __opts.side = #side;
                                __opts.align = #align;
                                __opts.side_offset = #side_offset;
                                __opts.focus_id = #focus_id;
                                __opts.strategy = #strategy;
                                __opts.pos = #pos;
                                __opts.x = #x;
                                __opts.y = #y;
                                __opts.gap = #gap;
                                __opts.kind = #kind;
                                __opts.motion = #motion;
                                __opts.mode = #mode;
                                __opts.match_trigger_width = #match_trigger;
                                __opts.max_h = #max_h;
                                __opts.width = #width;
                                __opts
                            },
                            move || #child_el,
                        )
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
                            quote! { Some(rui::core::tree::IntoOpacity::into_opacity(#e)) }
                        })
                        .unwrap_or_else(|| quote! { None });
                    let source_opacity = get_attr("source_opacity")
                        .or_else(|| get_attr("sourceOpacity"))
                        .map(|e| {
                            quote! { Some(rui::core::tree::IntoOpacity::into_opacity(#e)) }
                        })
                        .unwrap_or_else(|| quote! { None });
                    let stack = get_attr("stack")
                        .map(|e| quote! { rui::utils::drag::DragStack::from(#e) })
                        .unwrap_or_else(|| quote! { rui::utils::drag::DragStack::Auto });
                    let restrict = get_attr("restrict")
                        .map(|e| {
                            quote! { Some(rui::utils::drag::Restrict::parse(&(#e).to_string())) }
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
                        quote! { rui::core::tree::Element::new() }
                    } else {
                        quote! {
                            {
                                let mut __list: Vec<rui::core::tree::Element> = Vec::new();
                                #( __list.extend(rui::core::tree::build_ui_child(|| (#expanded_children))); )*
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
                        rui::utils::drag::draggable_from_rsx(
                            rui::utils::drag::DraggableRsxOpts {
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
                                rui::utils::drag::DropOverlap::parse(&(#e).to_string()),
                            );
                        }
                    } else {
                        quote! {}
                    };
                    let drop_checker = get_attr("drop_checker").or_else(|| get_attr("dropChecker"));
                    let on_drop_activate =
                        get_attr("on_drop_activate").or_else(|| get_attr("onDropActivate"));
                    let on_drop_deactivate =
                        get_attr("on_drop_deactivate").or_else(|| get_attr("onDropDeactivate"));
                    let on_drag_enter =
                        get_attr("on_drag_enter").or_else(|| get_attr("onDragEnter"));
                    let on_drag_leave =
                        get_attr("on_drag_leave").or_else(|| get_attr("onDragLeave"));
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
                                rui::core::tree::IntoOpacity::into_opacity(#e),
                            );
                        }
                    } else {
                        quote! {}
                    };
                    let direction_attr = get_attr("direction");
                    let direction_ts = if let Some(e) = direction_attr {
                        quote! {
                            __db = __db.direction(rui::utils::drag::Axis::parse(&(#e)));
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
                        quote! { rui::core::tree::Element::new() }
                    } else {
                        quote! {
                            {
                                let mut __list: Vec<rui::core::tree::Element> = Vec::new();
                                #( __list.extend(rui::core::tree::build_ui_child(|| (#expanded_children))); )*
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
                    let id_ts = if let Some(id) = id_expr {
                        quote! { (#id).to_string() }
                    } else {
                        quote! { format!("{}:{}:{}#{}", file!(), line!(), #name_str, #idx) }
                    };
                    return quote! {
                        {
                            let mut __db = rui::utils::drag::droppable(#id_ts)
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
                                let mut __list: Vec<rui::core::tree::Element> = Vec::new();
                                #( __list.extend(rui::core::tree::build_ui_child(|| (#expanded_children))); )*
                                if __list.len() == 1 {
                                    __list.remove(0)
                                } else {
                                    let mut __v = rui::primitives::vstack();
                                    __v = __v.children(__list);
                                    rui::core::tree::Element::from(__v)
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
                            __rui_el = __rui_el.opacity(#op);
                        });
                    }
                    if let Some(rot) = get_attr("rotate") {
                        style_stmts.push(quote! {
                            __rui_el = __rui_el.rotate(#rot);
                        });
                    }
                    if let Some(bg) = get_attr("bg") {
                        style_stmts.push(quote! {
                            __rui_el = __rui_el.bg(rui::core::tree::parse_color(#bg));
                        });
                    }
                    if let Some(rounded) = get_attr("rounded")
                        .or_else(|| get_attr("radius"))
                        .or_else(|| get_attr("border_radius"))
                        .or_else(|| get_attr("borderRadius"))
                    {
                        style_stmts.push(quote! {
                            __rui_el = __rui_el.rounded(rui::core::tree::IntoF32::into_f32(#rounded));
                        });
                    }
                    if let Some(shadow) = get_attr("box_shadow").or_else(|| get_attr("boxShadow")) {
                        style_stmts.push(quote! {
                            __rui_el = __rui_el.box_shadow(#shadow);
                        });
                    }
                    if let Some(p) = get_attr("p") {
                        style_stmts.push(quote! {
                            __rui_el = __rui_el.p(#p);
                        });
                    }
                    if let Some(w) = get_attr("w").or_else(|| get_attr("width")) {
                        style_stmts.push(quote! {
                            __rui_el = __rui_el.w(#w);
                        });
                    }
                    if let Some(h) = get_attr("h").or_else(|| get_attr("height")) {
                        style_stmts.push(quote! {
                            __rui_el = __rui_el.h(#h);
                        });
                    }
                    let style_ts = if style_stmts.is_empty() {
                        quote! {}
                    } else {
                        quote! {
                            .style(move |__rui_el| {
                                let mut __rui_el = __rui_el;
                                #(#style_stmts)*
                                __rui_el
                            })
                        }
                    };
                    let source_opacity_ts = if let Some(v) =
                        get_attr("source_opacity").or_else(|| get_attr("sourceOpacity"))
                    {
                        quote! { .source_opacity(#v) }
                    } else {
                        quote! {}
                    };
                    let align_ts = if let Some(v) = get_attr("align") {
                        quote! { .align(rui::utils::drag::OverlayAlign::parse(&(#v))) }
                    } else {
                        quote! {}
                    };
                    return quote! {
                        rui::core::tree::Element::from(
                            rui::utils::drag::drag_overlay()
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
                        quote! { rui::core::tree::Element::new() }
                    } else {
                        quote! {
                            {
                                let mut __list: Vec<rui::core::tree::Element> = Vec::new();
                                #( __list.extend(rui::core::tree::build_ui_child(|| (#expanded_children))); )*
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
                        rui::utils::drag::drag_handle_from_rsx(|| #child_el)
                    };
                }

                if name_str == "DragIgnore" {
                    let mut expanded_children = Vec::new();
                    for child in &el.children {
                        expanded_children.push(expand_node(child, node_counter, true));
                    }
                    let child_el = if expanded_children.is_empty() {
                        quote! { rui::core::tree::Element::new() }
                    } else {
                        quote! {
                            {
                                let mut __list: Vec<rui::core::tree::Element> = Vec::new();
                                #( __list.extend(rui::core::tree::build_ui_child(|| (#expanded_children))); )*
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
                        rui::utils::drag::drag_ignore_from_rsx(|| #child_el)
                    };
                }

                if name_str == "ViewportModalContent" {
                    let motion = get_attr("motion")
                        .map(|e| {
                            quote! {
                                rui::utils::viewport_modal::parse_modal_motion(&(#e).to_string())
                            }
                        })
                        .unwrap_or_else(|| {
                            quote! { rui::utils::viewport_modal::ModalSurfaceMotion::DialogFade }
                        });
                    let raw_stage = get_attr("raw_stage")
                        .or_else(|| get_attr("rawStage"))
                        .map(|e| quote! { #e })
                        .unwrap_or_else(|| quote! { false });
                    let mut expanded_children = Vec::new();
                    for child in &el.children {
                        expanded_children.push(expand_node(child, node_counter, true));
                    }
                    let child_el = if expanded_children.is_empty() {
                        quote! { rui::core::tree::Element::new() }
                    } else {
                        quote! {
                            {
                                let mut __list: Vec<rui::core::tree::Element> = Vec::new();
                                #( __list.extend(rui::core::tree::build_ui_child(|| (#expanded_children))); )*
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
                        rui::utils::viewport_modal::viewport_modal_content_from_rsx(
                            rui::utils::viewport_modal::ViewportModalContentOpts {
                                motion: #motion,
                                raw_stage: #raw_stage,
                            },
                            move || #child_el,
                        )
                    };
                }

                if name_str == "HoverCardContent" {
                    let side = get_attr("side")
                        .or_else(|| get_attr("placement"))
                        .map(|e| quote! { (#e).to_string() })
                        .unwrap_or_else(|| quote! { "bottom".to_string() });
                    let align = get_attr("align")
                        .map(|e| quote! { (#e).to_string() })
                        .unwrap_or_else(|| quote! { "center".to_string() });
                    let side_offset = get_attr("side_offset")
                        .or_else(|| get_attr("sideOffset"))
                        .map(|e| quote_length_f32(e))
                        .unwrap_or_else(|| quote! { 4.0f32 });
                    let class = get_attr("class")
                        .map(|e| quote! { Some((#e).to_string()) })
                        .unwrap_or_else(|| quote! { None });
                    let width = get_attr("width")
                        .or_else(|| get_attr("w"))
                        .map(|e| quote! { Some((#e).to_string()) })
                        .unwrap_or_else(|| quote! { None });
                    let mut expanded_children = Vec::new();
                    for child in &el.children {
                        // Body is rebuilt inside hover_card_content's reactive_anchor.
                        expanded_children.push(expand_node(child, node_counter, true));
                    }
                    let child_el = if expanded_children.is_empty() {
                        quote! { rui::core::tree::Element::new() }
                    } else {
                        quote! {
                            {
                                let mut __list: Vec<rui::core::tree::Element> = Vec::new();
                                #( __list.extend(rui::core::tree::build_ui_child(|| (#expanded_children))); )*
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
                        rui::components::hover_card_content(
                            rui::components::HoverCardContentOpts {
                                side: #side,
                                align: #align,
                                side_offset: #side_offset,
                                class: #class,
                                width: #width,
                                style: None,
                            },
                            move || #child_el,
                        )
                    };
                }

                if name_str == "ContextMenuSub" {
                    // Provide sub context *before* SubTrigger / SubContent (eager props order).
                    let id_expr = get_attr("id");
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
                                let mut __list: Vec<rui::core::tree::Element> = Vec::new();
                                #( __list.extend(rui::core::tree::build_ui_child(|| (#expanded_children))); )*
                                if __list.len() == 1 {
                                    __list.remove(0)
                                } else {
                                    let mut __frag = rui::core::tree::Element::new();
                                    __frag.set_attr("rui_fragment", "1");
                                    __frag.children(__list)
                                }
                            }
                        }
                    };
                    let id_ts = if let Some(id) = id_expr {
                        quote! { Some((#id).to_string()) }
                    } else {
                        quote! { None }
                    };
                    return quote! {
                        rui::components::context_menu_sub(#id_ts, || #child_el)
                    };
                }

                if name_str == "ContextMenuRadioGroup" {
                    let id_expr = get_attr("id");
                    let value_expr = get_attr("value");
                    let default_expr =
                        get_attr("default_value").or_else(|| get_attr("defaultValue"));
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
                                let mut __list: Vec<rui::core::tree::Element> = Vec::new();
                                #( __list.extend(rui::core::tree::build_ui_child(|| (#expanded_children))); )*
                                if __list.len() == 1 {
                                    __list.remove(0)
                                } else {
                                    let mut __frag = rui::core::tree::Element::new();
                                    __frag.set_attr("rui_fragment", "1");
                                    __frag.children(__list)
                                }
                            }
                        }
                    };
                    let id_ts = if let Some(id) = id_expr {
                        quote! { Some((#id).to_string()) }
                    } else {
                        quote! { None }
                    };
                    let value_ts = if let Some(v) = value_expr {
                        quote! { Some(#v) }
                    } else {
                        quote! { None }
                    };
                    let default_ts = if let Some(d) = default_expr {
                        quote! { Some((#d).to_string()) }
                    } else {
                        quote! { None }
                    };
                    return quote! {
                        rui::components::context_menu_radio_group(
                            #id_ts,
                            #value_ts,
                            #default_ts,
                            || #child_el,
                        )
                    };
                }

                if name_str == "DropdownMenuSub" {
                    let id_expr = get_attr("id");
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
                                let mut __list: Vec<rui::core::tree::Element> = Vec::new();
                                #( __list.extend(rui::core::tree::build_ui_child(|| (#expanded_children))); )*
                                if __list.len() == 1 {
                                    __list.remove(0)
                                } else {
                                    let mut __frag = rui::core::tree::Element::new();
                                    __frag.set_attr("rui_fragment", "1");
                                    __frag.children(__list)
                                }
                            }
                        }
                    };
                    let id_ts = if let Some(id) = id_expr {
                        quote! { Some((#id).to_string()) }
                    } else {
                        quote! { None }
                    };
                    return quote! {
                        rui::components::dropdown_menu_sub(#id_ts, || #child_el)
                    };
                }

                if name_str == "DropdownMenuRadioGroup" {
                    let id_expr = get_attr("id");
                    let value_expr = get_attr("value");
                    let default_expr =
                        get_attr("default_value").or_else(|| get_attr("defaultValue"));
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
                                let mut __list: Vec<rui::core::tree::Element> = Vec::new();
                                #( __list.extend(rui::core::tree::build_ui_child(|| (#expanded_children))); )*
                                if __list.len() == 1 {
                                    __list.remove(0)
                                } else {
                                    let mut __frag = rui::core::tree::Element::new();
                                    __frag.set_attr("rui_fragment", "1");
                                    __frag.children(__list)
                                }
                            }
                        }
                    };
                    let id_ts = if let Some(id) = id_expr {
                        quote! { Some((#id).to_string()) }
                    } else {
                        quote! { None }
                    };
                    let value_ts = if let Some(v) = value_expr {
                        quote! { Some(#v) }
                    } else {
                        quote! { None }
                    };
                    let default_ts = if let Some(d) = default_expr {
                        quote! { Some((#d).to_string()) }
                    } else {
                        quote! { None }
                    };
                    return quote! {
                        rui::components::dropdown_menu_radio_group(
                            #id_ts,
                            #value_ts,
                            #default_ts,
                            || #child_el,
                        )
                    };
                }

                if name_str == "MenubarSub" {
                    let id_expr = get_attr("id");
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
                                let mut __list: Vec<rui::core::tree::Element> = Vec::new();
                                #( __list.extend(rui::core::tree::build_ui_child(|| (#expanded_children))); )*
                                if __list.len() == 1 {
                                    __list.remove(0)
                                } else {
                                    let mut __frag = rui::core::tree::Element::new();
                                    __frag.set_attr("rui_fragment", "1");
                                    __frag.children(__list)
                                }
                            }
                        }
                    };
                    let id_ts = if let Some(id) = id_expr {
                        quote! { Some((#id).to_string()) }
                    } else {
                        quote! { None }
                    };
                    return quote! {
                        rui::components::menubar_sub(#id_ts, || #child_el)
                    };
                }

                if name_str == "MenubarRadioGroup" {
                    let id_expr = get_attr("id");
                    let value_expr = get_attr("value");
                    let default_expr =
                        get_attr("default_value").or_else(|| get_attr("defaultValue"));
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
                                let mut __list: Vec<rui::core::tree::Element> = Vec::new();
                                #( __list.extend(rui::core::tree::build_ui_child(|| (#expanded_children))); )*
                                if __list.len() == 1 {
                                    __list.remove(0)
                                } else {
                                    let mut __frag = rui::core::tree::Element::new();
                                    __frag.set_attr("rui_fragment", "1");
                                    __frag.children(__list)
                                }
                            }
                        }
                    };
                    let id_ts = if let Some(id) = id_expr {
                        quote! { Some((#id).to_string()) }
                    } else {
                        quote! { None }
                    };
                    let value_ts = if let Some(v) = value_expr {
                        quote! { Some(#v) }
                    } else {
                        quote! { None }
                    };
                    let default_ts = if let Some(d) = default_expr {
                        quote! { Some((#d).to_string()) }
                    } else {
                        quote! { None }
                    };
                    return quote! {
                        rui::components::menubar_radio_group(
                            #id_ts,
                            #value_ts,
                            #default_ts,
                            || #child_el,
                        )
                    };
                }

                if name_str == "ContextMenuContent" {
                    let placement = get_attr("placement")
                        .or_else(|| get_attr("side"))
                        .map(|e| quote! { (#e).to_string() })
                        .unwrap_or_else(|| quote! { "bottom-start".to_string() });
                    let class = get_attr("class")
                        .map(|e| quote! { Some((#e).to_string()) })
                        .unwrap_or_else(|| quote! { None });
                    let mut expanded_children = Vec::new();
                    for child in &el.children {
                        // Body is rebuilt inside context_menu_content's reactive_anchor.
                        expanded_children.push(expand_node(child, node_counter, true));
                    }
                    let child_el = if expanded_children.is_empty() {
                        quote! { rui::core::tree::Element::new() }
                    } else {
                        quote! {
                            {
                                let mut __list: Vec<rui::core::tree::Element> = Vec::new();
                                #( __list.extend(rui::core::tree::build_ui_child(|| (#expanded_children))); )*
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
                        rui::components::context_menu_content(
                            rui::components::ContextMenuContentOpts {
                                placement: #placement,
                                class: #class,
                                style: None,
                            },
                            move || #child_el,
                        )
                    };
                }

                if name_str == "DropdownMenuContent" {
                    let placement = get_attr("placement")
                        .or_else(|| get_attr("side"))
                        .map(|e| quote! { (#e).to_string() })
                        .unwrap_or_else(|| quote! { "bottom-start".to_string() });
                    let class = get_attr("class")
                        .map(|e| quote! { Some((#e).to_string()) })
                        .unwrap_or_else(|| quote! { None });
                    let mut expanded_children = Vec::new();
                    for child in &el.children {
                        expanded_children.push(expand_node(child, node_counter, true));
                    }
                    let child_el = if expanded_children.is_empty() {
                        quote! { rui::core::tree::Element::new() }
                    } else {
                        quote! {
                            {
                                let mut __list: Vec<rui::core::tree::Element> = Vec::new();
                                #( __list.extend(rui::core::tree::build_ui_child(|| (#expanded_children))); )*
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
                        rui::components::dropdown_menu_content(
                            rui::components::DropdownMenuContentOpts::new(#placement, #class),
                            move || #child_el,
                        )
                    };
                }

                if name_str == "MenubarContent" {
                    let placement = get_attr("placement")
                        .or_else(|| get_attr("side"))
                        .map(|e| quote! { (#e).to_string() })
                        .unwrap_or_else(|| quote! { "bottom-start".to_string() });
                    let class = get_attr("class")
                        .map(|e| quote! { Some((#e).to_string()) })
                        .unwrap_or_else(|| quote! { None });
                    let mut expanded_children = Vec::new();
                    for child in &el.children {
                        expanded_children.push(expand_node(child, node_counter, true));
                    }
                    let child_el = if expanded_children.is_empty() {
                        quote! { rui::core::tree::Element::new() }
                    } else {
                        quote! {
                            {
                                let mut __list: Vec<rui::core::tree::Element> = Vec::new();
                                #( __list.extend(rui::core::tree::build_ui_child(|| (#expanded_children))); )*
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
                        rui::components::menubar_content(
                            rui::components::MenubarContentOpts {
                                placement: #placement,
                                class: #class,
                                style: None,
                            },
                            move || #child_el,
                        )
                    };
                }

                if name_str == "NavigationMenuContent" {
                    let class = get_attr("class")
                        .map(|e| quote! { Some((#e).to_string()) })
                        .unwrap_or_else(|| quote! { None });
                    let mut expanded_children = Vec::new();
                    for child in &el.children {
                        expanded_children.push(expand_node(child, node_counter, true));
                    }
                    let child_el = if expanded_children.is_empty() {
                        quote! { rui::core::tree::Element::new() }
                    } else {
                        quote! {
                            {
                                let mut __list: Vec<rui::core::tree::Element> = Vec::new();
                                #( __list.extend(rui::core::tree::build_ui_child(|| (#expanded_children))); )*
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
                        rui::components::navigation_menu_content(
                            rui::components::NavigationMenuContentOpts {
                                class: #class,
                                style: None,
                            },
                            move || #child_el,
                        )
                    };
                }

                if name_str == "ContextMenuSubContent" {
                    let class = get_attr("class")
                        .map(|e| quote! { Some((#e).to_string()) })
                        .unwrap_or_else(|| quote! { None });
                    let mut expanded_children = Vec::new();
                    for child in &el.children {
                        expanded_children.push(expand_node(child, node_counter, true));
                    }
                    let child_el = if expanded_children.is_empty() {
                        quote! { rui::core::tree::Element::new() }
                    } else {
                        quote! {
                            {
                                let mut __list: Vec<rui::core::tree::Element> = Vec::new();
                                #( __list.extend(rui::core::tree::build_ui_child(|| (#expanded_children))); )*
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
                        rui::components::context_menu_sub_content(
                            #class,
                            move || #child_el,
                        )
                    };
                }

                if name_str == "DropdownMenuSubContent" {
                    let class = get_attr("class")
                        .map(|e| quote! { Some((#e).to_string()) })
                        .unwrap_or_else(|| quote! { None });
                    let mut expanded_children = Vec::new();
                    for child in &el.children {
                        expanded_children.push(expand_node(child, node_counter, true));
                    }
                    let child_el = if expanded_children.is_empty() {
                        quote! { rui::core::tree::Element::new() }
                    } else {
                        quote! {
                            {
                                let mut __list: Vec<rui::core::tree::Element> = Vec::new();
                                #( __list.extend(rui::core::tree::build_ui_child(|| (#expanded_children))); )*
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
                        rui::components::dropdown_menu_sub_content(
                            #class,
                            move || #child_el,
                        )
                    };
                }

                if name_str == "MenubarSubContent" {
                    let class = get_attr("class")
                        .map(|e| quote! { Some((#e).to_string()) })
                        .unwrap_or_else(|| quote! { None });
                    let mut expanded_children = Vec::new();
                    for child in &el.children {
                        expanded_children.push(expand_node(child, node_counter, true));
                    }
                    let child_el = if expanded_children.is_empty() {
                        quote! { rui::core::tree::Element::new() }
                    } else {
                        quote! {
                            {
                                let mut __list: Vec<rui::core::tree::Element> = Vec::new();
                                #( __list.extend(rui::core::tree::build_ui_child(|| (#expanded_children))); )*
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
                        rui::components::menubar_sub_content(
                            #class,
                            move || #child_el,
                        )
                    };
                }

                if name_str == "SelectContent" {
                    let class = get_attr("class")
                        .map(|e| quote! { Some((#e).to_string()) })
                        .unwrap_or_else(|| quote! { None });
                    let match_trigger = get_attr("match_trigger")
                        .or_else(|| get_attr("matchTrigger"))
                        .map(|e| quote! { Some(#e) })
                        .unwrap_or_else(|| quote! { None });
                    let menu_width = get_attr("menu_width")
                        .or_else(|| get_attr("menuWidth"))
                        .map(|e| quote! { Some((#e).to_string()) })
                        .unwrap_or_else(|| quote! { None });
                    let menu_height = get_attr("menu_height")
                        .or_else(|| get_attr("menuHeight"))
                        .map(|e| quote! { Some((#e).to_string()) })
                        .unwrap_or_else(|| quote! { None });
                    let min_height = get_attr("min_height")
                        .or_else(|| get_attr("minHeight"))
                        .or_else(|| get_attr("min_h"))
                        .map(|e| quote! { Some((#e).to_string()) })
                        .unwrap_or_else(|| quote! { None });
                    let max_height = get_attr("max_height")
                        .or_else(|| get_attr("maxHeight"))
                        .or_else(|| get_attr("max_h"))
                        .map(|e| quote! { Some((#e).to_string()) })
                        .unwrap_or_else(|| quote! { None });
                    let placement = get_attr("placement")
                        .map(|e| quote! { Some(#e) })
                        .unwrap_or_else(|| quote! { None });
                    let scroll_selected = get_attr("scroll_selected")
                        .or_else(|| get_attr("scrollSelected"))
                        .map(|e| quote! { Some((#e).to_string()) })
                        .unwrap_or_else(|| quote! { None });
                    let mut expanded_children = Vec::new();
                    for child in &el.children {
                        // Body is rendered inside SelectContent's reactive_view.
                        expanded_children.push(expand_node(child, node_counter, true));
                    }
                    let child_el = if expanded_children.is_empty() {
                        quote! { rui::core::tree::Element::new() }
                    } else {
                        quote! {
                            {
                                let mut __list: Vec<rui::core::tree::Element> = Vec::new();
                                #( __list.extend(rui::core::tree::build_ui_child(|| (#expanded_children))); )*
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
                        rui::components::select_content(
                            rui::components::SelectContentOpts {
                                class: #class,
                                match_trigger: #match_trigger,
                                menu_width: #menu_width,
                                menu_height: #menu_height,
                                min_height: #min_height,
                                max_height: #max_height,
                                placement: #placement,
                                scroll_selected: #scroll_selected,
                                style: None,
                            },
                            move || #child_el,
                        )
                    };
                }

                if combobox_is_compound {
                    // ShadCN compound Combobox: provide context before Input/Content.
                    let id_expr = get_attr("id");
                    let open_expr = get_attr("open");
                    let value_expr = get_attr("value");
                    let placeholder_expr = get_attr("placeholder");
                    let clearable_expr = get_attr("clearable");
                    let class_expr = get_attr("class");
                    let content_class_expr =
                        get_attr("content_class").or_else(|| get_attr("contentClass"));
                    let idx = *node_counter;
                    *node_counter += 1;
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
                                let mut __list: Vec<rui::core::tree::Element> = Vec::new();
                                #( __list.extend(rui::core::tree::build_ui_child(|| (#expanded_children))); )*
                                let mut __frag = rui::core::tree::Element::new();
                                __frag.set_attr("rui_fragment", "1");
                                __frag.children(__list)
                            }
                        }
                    };
                    let id_ts = if let Some(id) = id_expr {
                        quote! { (#id).to_string() }
                    } else {
                        quote! { format!("{}:{}:Combobox#{}", file!(), line!(), #idx) }
                    };
                    let open_ts = if let Some(open) = open_expr {
                        quote! { Some(#open) }
                    } else {
                        quote! { None }
                    };
                    let value_ts = if let Some(value) = value_expr {
                        quote! { Some(#value) }
                    } else {
                        quote! { None }
                    };
                    let placeholder_ts = if let Some(ph) = placeholder_expr {
                        quote! { (#ph).to_string() }
                    } else {
                        quote! { "Search…".to_string() }
                    };
                    let clearable_ts = if let Some(c) = clearable_expr {
                        quote! { rui::core::tree::parse_bool_like(#c) }
                    } else {
                        quote! { false }
                    };
                    let class_ts = if let Some(c) = class_expr {
                        quote! { (#c).to_string() }
                    } else {
                        quote! { String::new() }
                    };
                    let content_class_ts = if let Some(c) = content_class_expr {
                        quote! { (#c).to_string() }
                    } else {
                        quote! { String::new() }
                    };
                    return quote! {
                        rui::components::combobox_scope(
                            #id_ts,
                            #open_ts,
                            #value_ts,
                            #placeholder_ts,
                            #clearable_ts,
                            #class_ts,
                            #content_class_ts,
                            || #child_el,
                        )
                    };
                }

                if name_str == "ComboboxContent" {
                    let class = get_attr("class")
                        .map(|e| quote! { Some((#e).to_string()) })
                        .unwrap_or_else(|| quote! { None });
                    let match_trigger = get_attr("match_trigger")
                        .or_else(|| get_attr("matchTrigger"))
                        .map(|e| quote! { Some(#e) })
                        .unwrap_or_else(|| quote! { None });
                    let menu_width = get_attr("menu_width")
                        .or_else(|| get_attr("menuWidth"))
                        .map(|e| quote! { Some((#e).to_string()) })
                        .unwrap_or_else(|| quote! { None });
                    let menu_height = get_attr("menu_height")
                        .or_else(|| get_attr("menuHeight"))
                        .map(|e| quote! { Some((#e).to_string()) })
                        .unwrap_or_else(|| quote! { None });
                    let min_height = get_attr("min_height")
                        .or_else(|| get_attr("minHeight"))
                        .or_else(|| get_attr("min_h"))
                        .map(|e| quote! { Some((#e).to_string()) })
                        .unwrap_or_else(|| quote! { None });
                    let max_height = get_attr("max_height")
                        .or_else(|| get_attr("maxHeight"))
                        .or_else(|| get_attr("max_h"))
                        .map(|e| quote! { Some((#e).to_string()) })
                        .unwrap_or_else(|| quote! { None });
                    let placement = get_attr("placement")
                        .map(|e| quote! { Some(#e) })
                        .unwrap_or_else(|| quote! { None });
                    let mut expanded_children = Vec::new();
                    for child in &el.children {
                        // Body is rendered inside ComboboxContent's reactive_anchor.
                        expanded_children.push(expand_node(child, node_counter, true));
                    }
                    let child_el = if expanded_children.is_empty() {
                        quote! { rui::core::tree::Element::new() }
                    } else {
                        quote! {
                            {
                                let mut __list: Vec<rui::core::tree::Element> = Vec::new();
                                #( __list.extend(rui::core::tree::build_ui_child(|| (#expanded_children))); )*
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
                        rui::components::combobox_content(
                            rui::components::ComboboxContentOpts {
                                class: #class,
                                match_trigger: #match_trigger,
                                menu_width: #menu_width,
                                menu_height: #menu_height,
                                min_height: #min_height,
                                max_height: #max_height,
                                placement: #placement,
                                style: None,
                            },
                            move || #child_el,
                        )
                    };
                }

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
                            let mut __list: Vec<rui::core::tree::Element> = Vec::new();
                            #( __list.extend(rui::core::tree::build_ui_child(|| (#expanded_children))); )*
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
                            #( __list.extend(rui::core::tree::build_ui_child(|| (#expanded_children))); )*
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
                                let mut __list: Vec<rui::core::tree::Element> = Vec::new();
                                #( __list.extend(rui::core::tree::build_ui_child(|| (#expanded_children))); )*
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
                                let mut __list: Vec<rui::core::tree::Element> = Vec::new();
                                #( __list.extend(rui::core::tree::build_ui_child(|| (#expanded_children))); )*
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

                if name_str == "MatchArm" || name_str == "When" {
                    return quote! {
                        compile_error!("<MatchArm>/<When> must be a child of <Match>")
                    };
                }

                if name_str == "Match" {
                    // Same contract as `<For scope={...}>`: an explicit island id must not drift
                    // with `file:line` (hot reload / gallery section body).
                    let scope_ts =
                        if let Some(scope_expr) = get_attr("scope").or_else(|| get_attr("id")) {
                            quote! { #scope_expr }
                        } else {
                            let idx = *node_counter;
                            *node_counter += 1;
                            quote! { format!("{}:{}:Match#{}", file!(), line!(), #idx) }
                        };
                    let maybe_fallback_ts = get_attr("fallback").map(|e| quote! { #e });
                    let maybe_key = get_attr("key");

                    let mut expand_kids = |children: &[RsxNode]| -> proc_macro2::TokenStream {
                        let mut expanded_children = Vec::new();
                        for child in children {
                            expanded_children.push(expand_node(
                                child,
                                node_counter,
                                in_reactive_boundary,
                            ));
                        }
                        if expanded_children.is_empty() {
                            quote! { rui::core::tree::Element::new() }
                        } else {
                            quote! {
                                {
                                    let mut __list: Vec<rui::core::tree::Element> = Vec::new();
                                    #( __list.extend(rui::core::tree::build_ui_child(|| (#expanded_children))); )*
                                    if __list.len() == 1 {
                                        __list.remove(0)
                                    } else {
                                        let mut __v = rui::primitives::vstack();
                                        __v = __v.children(__list);
                                        rui::core::tree::Element::from(__v)
                                    }
                                }
                            }
                        }
                    };

                    if let Some(key_expr) = maybe_key {
                        if el.children.len() != 1 {
                            return quote! {
                                compile_error!("<Match key={...}> expects a single closure child `{ |k| ... }`")
                            };
                        }
                        let render_ts = match &el.children[0] {
                            RsxNode::Expr(e) => quote! { #e },
                            _ => {
                                return quote! {
                                    compile_error!("<Match key={...}> expects closure child expression")
                                };
                            }
                        };
                        return quote! {
                            rui::app::reactive::switch_reactive(
                                #scope_ts,
                                move || { #key_expr },
                                #render_ts,
                            )
                        };
                    }

                    let mut arm_when = Vec::new();
                    let mut arm_body = Vec::new();
                    for child in &el.children {
                        let RsxNode::Element(arm) = child else {
                            return quote! {
                                compile_error!("<Match> without `key` expects <MatchArm when={...}> / <When when={...}> children")
                            };
                        };
                        let arm_name = arm.name.to_string();
                        if arm_name != "MatchArm" && arm_name != "When" {
                            return quote! {
                                compile_error!("<Match> children must be <MatchArm> or <When>")
                            };
                        }
                        let Some((_, when_expr)) = arm.attributes.iter().find(|(k, _)| k == "when")
                        else {
                            return quote! {
                                compile_error!("<MatchArm>/<When> requires `when={...}`")
                            };
                        };
                        arm_when.push(quote! { #when_expr });
                        arm_body.push(expand_kids(&arm.children));
                    }
                    let fallback_el = if let Some(fb) = maybe_fallback_ts {
                        quote! { #fb }
                    } else {
                        quote! { rui::core::tree::Element::new() }
                    };
                    return quote! {
                        rui::app::reactive::match_when_reactive(
                            #scope_ts,
                            move || {
                                #( if { #arm_when } { return { #arm_body }; } )*
                                #fallback_el
                            },
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
                                let mut __list: Vec<rui::core::tree::Element> = Vec::new();
                                #( __list.extend(rui::core::tree::build_ui_child(|| (#expanded_children))); )*
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
                let item_ident_for_key = maybe_item_ident
                    .as_ref()
                    .and_then(|v| v.as_ref().ok())
                    .cloned()
                    .or_else(|| Some(syn::Ident::new("item", proc_macro2::Span::call_site())));
                // I-7 via ui!: `each={…get()}` + `key` → item-level owners (`for_keyed_reactive`).
                // Empty source + `fallback` still keeps item owners for non-empty updates.
                if each_is_reactive && let Some(key_ts) = maybe_key_ts.as_ref() {
                    let key_fn = if let Some(item_ident) = item_ident_for_key.clone() {
                        quote! {
                            |__rui_for_item| {
                                let #item_ident = __rui_for_item;
                                #key_ts
                            }
                        }
                    } else {
                        quote! { #key_ts }
                    };
                    if let Some(fallback_ts) = maybe_fallback_ts.as_ref() {
                        return quote! {
                            rui::app::reactive::for_keyed_reactive_with_fallback(
                                #scope_ts,
                                move || {
                                    (#each_expr).into_iter().collect::<Vec<_>>()
                                },
                                #key_fn,
                                #render_ts,
                                move || {
                                    use rui::core::tree::IntoElementList;
                                    (#fallback_ts).into_element_list()
                                },
                            )
                        };
                    }
                    return quote! {
                        rui::app::reactive::for_keyed_reactive(
                            #scope_ts,
                            move || {
                                (#each_expr).into_iter().collect::<Vec<_>>()
                            },
                            #key_fn,
                            #render_ts,
                        )
                    };
                }
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
                    if let Some(item_ident) = item_ident_for_key {
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
                    "Icon",
                    "AnimatedIcon",
                    "Box",
                    "Text",
                    "Interactive",
                    "Svg",
                    "Circle",
                    "Ellipse",
                    "Path",
                    "Rect",
                    "Line",
                    "Polyline",
                    "Polygon",
                    "Canvas",
                    "Scene3d",
                    "Defs",
                    "Use",
                    "ClipPath",
                    "LinearGradient",
                    "RadialGradient",
                    "Symbol",
                    "Stop",
                    "Animate",
                    "AnimateTransform",
                    "AnimateMotion",
                    "Set",
                    "Mask",
                    "Pattern",
                    "Filter",
                    "Title",
                    "Desc",
                    "Metadata",
                    "Tspan",
                    "TSpan",
                    "TextPath",
                    "FeGaussianBlur",
                    "FeDropShadow",
                    "FeOffset",
                    "FeBlend",
                    "FeColorMatrix",
                    "FeFlood",
                    "FeMerge",
                    "FeMergeNode",
                ]
                .contains(&name_str.as_str());

            let mut element = if is_custom {
                // Alias tags (`Img`/`img`) must call the canonical `Image` / `ImageProps`.
                let name_ident = if matches!(el.name.to_string().as_str(), "Img" | "img") {
                    syn::Ident::new("Image", el.name.span())
                } else {
                    el.name.clone()
                };
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

                    // SidebarMenuButton `isActive={sig.get() == id}` ≡ builder
                    // `bind_prop(..., apply_bound_sidebar_menu_button_active)` (live chrome, no
                    // sidebar rebuild). Plain `untrack` snapshot is not equivalent.
                    if name_str == "SidebarMenuButton"
                        && matches!(attr_name, "isActive" | "is_active")
                        && expr_reads_get(value)
                    {
                        let idx = *node_counter;
                        *node_counter += 1;
                        prop_names.push(syn::Ident::new("is_active", attr.span()));
                        prop_values.push(quote! {
                            Some(rui::core::reactivity::untrack(|| #value))
                        });
                        chrome_binds.push(quote! {
                            __rui_el = rui::app::reactive::bind_attr(
                                __rui_el,
                                format!("{}:{}:bind_sidebar_menu_button_active#{}", file!(), line!(), #idx),
                                move || #value,
                                |el, active| {
                                    rui::components::apply_bound_sidebar_menu_button_active(el, active)
                                },
                            );
                        });
                        continue;
                    }

                    // Button / Badge / Avatar / Bubble / Input chrome attrs → bind_attr (Solid-style, no rebuild).
                    if matches!(
                        name_str.as_str(),
                        "Button"
                            | "Badge"
                            | "Avatar"
                            | "AvatarBadge"
                            | "Bubble"
                            | "Alert"
                            | "Item"
                            | "Attachment"
                            | "Input"
                            | "TableRow"
                            | "TableHead"
                            | "TableCell"
                            | "Marker"
                            | "Message"
                    ) && matches!(
                        attr_name,
                        "variant"
                            | "disabled"
                            | "invalid"
                            | "size"
                            | "shape"
                            | "align"
                            | "state"
                            | "orientation"
                    ) && expr_reads_get(value)
                    {
                        let idx = *node_counter;
                        *node_counter += 1;
                        if name_str == "Bubble" && attr_name == "variant" {
                            prop_names.push(attr.clone());
                            prop_values.push(quote! {
                                Some(rui::core::reactivity::untrack(|| (#value).to_string()).into())
                            });
                            chrome_binds.push(quote! {
                                __rui_el = rui::app::reactive::bind_attr(
                                    __rui_el,
                                    format!("{}:{}:bind_bubble_variant#{}", file!(), line!(), #idx),
                                    move || (#value).to_string(),
                                    |el, v| rui::components::apply_bound_bubble_variant(el, v),
                                );
                            });
                            continue;
                        }
                        if name_str == "Bubble" && attr_name == "align" {
                            prop_names.push(attr.clone());
                            prop_values.push(quote! {
                                Some(rui::core::reactivity::untrack(|| (#value).to_string()).into())
                            });
                            chrome_binds.push(quote! {
                                __rui_el = rui::app::reactive::bind_attr(
                                    __rui_el,
                                    format!("{}:{}:bind_bubble_align#{}", file!(), line!(), #idx),
                                    move || (#value).to_string(),
                                    |el, v| rui::components::apply_bound_bubble_align(el, v),
                                );
                            });
                            continue;
                        }
                        if name_str == "Message" && attr_name == "align" {
                            prop_names.push(attr.clone());
                            prop_values.push(quote! {
                                Some(rui::core::reactivity::untrack(|| (#value).to_string()).into())
                            });
                            chrome_binds.push(quote! {
                                __rui_el = rui::app::reactive::bind_attr(
                                    __rui_el,
                                    format!("{}:{}:bind_message_align#{}", file!(), line!(), #idx),
                                    move || (#value).to_string(),
                                    |el, v| rui::components::apply_bound_message_align(el, v),
                                );
                            });
                            continue;
                        }
                        if name_str == "Alert" && attr_name == "variant" {
                            prop_names.push(attr.clone());
                            prop_values.push(quote! {
                                Some(rui::core::reactivity::untrack(|| (#value).to_string()).into())
                            });
                            chrome_binds.push(quote! {
                                __rui_el = rui::app::reactive::bind_attr(
                                    __rui_el,
                                    format!("{}:{}:bind_alert_variant#{}", file!(), line!(), #idx),
                                    move || (#value).to_string(),
                                    |el, v| rui::components::apply_bound_alert_variant(el, v),
                                );
                            });
                            continue;
                        }
                        if name_str == "Alert" && attr_name == "chrome" {
                            if expr_reads_get(value) {
                                let idx = *node_counter;
                                *node_counter += 1;
                                chrome_binds.push(quote! {
                                    __rui_el = rui::app::reactive::bind_attr(
                                        __rui_el,
                                        format!("{}:{}:bind_alert_chrome#{}", file!(), line!(), #idx),
                                        move || #value,
                                        |el, enabled| {
                                            rui::components::apply_bound_alert_chrome(el, enabled)
                                        },
                                    );
                                });
                            } else {
                                prop_names.push(attr.clone());
                                prop_values.push(quote! { Some(#value) });
                            }
                            continue;
                        }
                        if name_str == "Marker" && attr_name == "variant" {
                            prop_names.push(attr.clone());
                            prop_values.push(quote! {
                                Some(rui::core::reactivity::untrack(|| (#value).to_string()).into())
                            });
                            chrome_binds.push(quote! {
                                __rui_el = rui::app::reactive::bind_attr(
                                    __rui_el,
                                    format!("{}:{}:bind_marker_variant#{}", file!(), line!(), #idx),
                                    move || (#value).to_string(),
                                    |el, v| rui::components::apply_bound_marker_variant(el, v),
                                );
                            });
                            continue;
                        }
                        if name_str == "Item" && attr_name == "variant" {
                            prop_names.push(attr.clone());
                            prop_values.push(quote! {
                                Some(rui::core::reactivity::untrack(|| (#value).to_string()).into())
                            });
                            chrome_binds.push(quote! {
                                __rui_el = rui::app::reactive::bind_attr(
                                    __rui_el,
                                    format!("{}:{}:bind_item_variant#{}", file!(), line!(), #idx),
                                    move || (#value).to_string(),
                                    |el, v| rui::components::apply_bound_item_variant(el, v),
                                );
                            });
                            continue;
                        }
                        if name_str == "Item" && attr_name == "size" {
                            prop_names.push(attr.clone());
                            prop_values.push(quote! {
                                Some(rui::core::reactivity::untrack(|| (#value).to_string()).into())
                            });
                            chrome_binds.push(quote! {
                                __rui_el = rui::app::reactive::bind_attr(
                                    __rui_el,
                                    format!("{}:{}:bind_item_size#{}", file!(), line!(), #idx),
                                    move || (#value).to_string(),
                                    |el, v| rui::components::apply_bound_item_size(el, v),
                                );
                            });
                            continue;
                        }
                        if name_str == "ItemMedia" && attr_name == "variant" {
                            prop_names.push(attr.clone());
                            prop_values.push(quote! {
                                Some(rui::core::reactivity::untrack(|| (#value).to_string()).into())
                            });
                            chrome_binds.push(quote! {
                                __rui_el = rui::app::reactive::bind_attr(
                                    __rui_el,
                                    format!("{}:{}:bind_item_media_variant#{}", file!(), line!(), #idx),
                                    move || (#value).to_string(),
                                    |el, v| rui::components::apply_bound_item_media_variant(el, v),
                                );
                            });
                            continue;
                        }
                        if name_str == "Attachment" && attr_name == "state" {
                            prop_names.push(attr.clone());
                            prop_values.push(quote! {
                                Some(rui::core::reactivity::untrack(|| (#value).to_string()).into())
                            });
                            chrome_binds.push(quote! {
                                __rui_el = rui::app::reactive::bind_attr(
                                    __rui_el,
                                    format!("{}:{}:bind_attachment_state#{}", file!(), line!(), #idx),
                                    move || (#value).to_string(),
                                    |el, v| rui::components::apply_bound_attachment_state(el, v),
                                );
                            });
                            continue;
                        }
                        if name_str == "Attachment" && attr_name == "size" {
                            prop_names.push(attr.clone());
                            prop_values.push(quote! {
                                Some(rui::core::reactivity::untrack(|| (#value).to_string()).into())
                            });
                            chrome_binds.push(quote! {
                                __rui_el = rui::app::reactive::bind_attr(
                                    __rui_el,
                                    format!("{}:{}:bind_attachment_size#{}", file!(), line!(), #idx),
                                    move || (#value).to_string(),
                                    |el, v| rui::components::apply_bound_attachment_size(el, v),
                                );
                            });
                            continue;
                        }
                        if name_str == "Attachment" && attr_name == "orientation" {
                            prop_names.push(attr.clone());
                            prop_values.push(quote! {
                                Some(rui::core::reactivity::untrack(|| (#value).to_string()).into())
                            });
                            chrome_binds.push(quote! {
                                __rui_el = rui::app::reactive::bind_attr(
                                    __rui_el,
                                    format!("{}:{}:bind_attachment_orientation#{}", file!(), line!(), #idx),
                                    move || (#value).to_string(),
                                    |el, v| rui::components::apply_bound_attachment_orientation(el, v),
                                );
                            });
                            continue;
                        }
                        if name_str == "AttachmentMedia" && attr_name == "variant" {
                            prop_names.push(attr.clone());
                            prop_values.push(quote! {
                                Some(rui::core::reactivity::untrack(|| (#value).to_string()).into())
                            });
                            chrome_binds.push(quote! {
                                __rui_el = rui::app::reactive::bind_attr(
                                    __rui_el,
                                    format!("{}:{}:bind_attachment_media_variant#{}", file!(), line!(), #idx),
                                    move || (#value).to_string(),
                                    |el, v| rui::components::apply_bound_attachment_media_variant(el, v),
                                );
                            });
                            continue;
                        }
                        if name_str == "TableRow" && attr_name == "selected" {
                            prop_names.push(attr.clone());
                            prop_values.push(quote! {
                                Some(rui::core::reactivity::untrack(|| #value))
                            });
                            chrome_binds.push(quote! {
                                __rui_el = rui::app::reactive::bind_attr(
                                    __rui_el,
                                    format!("{}:{}:bind_table_row_selected#{}", file!(), line!(), #idx),
                                    move || #value,
                                    |el, v| rui::components::apply_bound_table_row_selected(el, v),
                                );
                            });
                            continue;
                        }
                        if (name_str == "TableHead" || name_str == "TableCell")
                            && attr_name == "align"
                        {
                            prop_names.push(attr.clone());
                            let is_reactive = expr_reads_get(value);
                            prop_values.push(if is_reactive {
                                quote! {
                                    Some(rui::core::reactivity::untrack(|| (#value).to_string()).into())
                                }
                            } else {
                                quote! { Some((#value).to_string()) }
                            });
                            if is_reactive {
                                chrome_binds.push(quote! {
                                    __rui_el = rui::app::reactive::bind_attr(
                                        __rui_el,
                                        format!("{}:{}:bind_table_align#{}", file!(), line!(), #idx),
                                        move || (#value).to_string(),
                                        |el, v| rui::components::apply_bound_table_align(el, v),
                                    );
                                });
                            }
                            continue;
                        }
                        if name_str == "Badge" && attr_name == "chrome" {
                            if expr_reads_get(value) {
                                let idx = *node_counter;
                                *node_counter += 1;
                                chrome_binds.push(quote! {
                                    __rui_el = rui::app::reactive::bind_attr(
                                        __rui_el,
                                        format!("{}:{}:bind_badge_chrome#{}", file!(), line!(), #idx),
                                        move || #value,
                                        |el, enabled| {
                                            rui::components::apply_bound_badge_chrome(el, enabled)
                                        },
                                    );
                                });
                            } else {
                                prop_names.push(attr.clone());
                                prop_values.push(quote! { Some(#value) });
                            }
                            continue;
                        }
                        if name_str == "Avatar" && attr_name == "size" {
                            prop_names.push(attr.clone());
                            prop_values.push(quote! {
                                Some(rui::core::reactivity::untrack(|| (#value).to_string()).into())
                            });
                            chrome_binds.push(quote! {
                                __rui_el = rui::app::reactive::bind_attr(
                                    __rui_el,
                                    format!("{}:{}:bind_avatar_size#{}", file!(), line!(), #idx),
                                    move || (#value).to_string(),
                                    |el, v| rui::components::apply_bound_avatar_size(el, v),
                                );
                            });
                            continue;
                        }
                        if name_str == "Avatar" && attr_name == "shape" {
                            prop_names.push(attr.clone());
                            prop_values.push(quote! {
                                Some(rui::core::reactivity::untrack(|| (#value).to_string()).into())
                            });
                            chrome_binds.push(quote! {
                                __rui_el = rui::app::reactive::bind_attr(
                                    __rui_el,
                                    format!("{}:{}:bind_avatar_shape#{}", file!(), line!(), #idx),
                                    move || (#value).to_string(),
                                    |el, v| rui::components::apply_bound_avatar_shape(el, v),
                                );
                            });
                            continue;
                        }
                        if matches!(name_str.as_str(), "Badge" | "AvatarBadge")
                            && attr_name == "variant"
                        {
                            prop_names.push(attr.clone());
                            prop_values.push(quote! {
                                Some(rui::core::reactivity::untrack(|| (#value).to_string()).into())
                            });
                            let bind_key = if name_str == "AvatarBadge" {
                                "avatarbadge"
                            } else {
                                "badge"
                            };
                            let apply_fn = if name_str == "AvatarBadge" {
                                quote! { rui::components::apply_bound_avatar_badge_variant }
                            } else {
                                quote! { rui::components::apply_bound_badge_variant }
                            };
                            chrome_binds.push(quote! {
                                __rui_el = rui::app::reactive::bind_attr(
                                    __rui_el,
                                    format!("{}:{}:bind_{}_variant#{}", file!(), line!(), #bind_key, #idx),
                                    move || (#value).to_string(),
                                    |el, v| #apply_fn(el, v),
                                );
                            });
                            continue;
                        }
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

                    // Chart `h`/`height` 是绘图区高度（轴标签在外），不能当 base 钳死整棵 vstack，
                    // 否则 BarChart 标签会溢到下一张图的标题上。
                    let chart_plot_height = matches!(
                        name_str.as_str(),
                        "BarChart" | "LineChart" | "AreaChart" | "Chart"
                    ) && matches!(attr_name, "h" | "height");

                    // Textarea: `wrap` = soft|off（单行溢出），不是 flex-wrap；
                    // `rows` = 可视行数，不是 grid-template-rows。
                    let textarea_own_prop =
                        name_str == "Textarea" && matches!(attr_name, "wrap" | "rows");

                    // TableHead/TableCell：`w` / `align` 是列 prop，不是根节点 base。
                    let table_cell_own_prop =
                        matches!(name_str.as_str(), "TableHead" | "TableCell")
                            && matches!(attr_name, "w" | "width" | "align");

                    // Toaster `position` is the Sonner anchor (`top-center`…), not CSS `position`.
                    let toaster_own_prop = name_str == "Toaster"
                        && matches!(attr_name, "position" | "variant" | "overflow" | "limit");

                    let is_base = !chart_plot_height
                        && !textarea_own_prop
                        && !table_cell_own_prop
                        && !toaster_own_prop
                        && matches!(
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
                                | "aspect_ratio"
                                | "aspect-ratio"
                                | "aspectRatio"
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
                                | "font_family"
                                | "text_font"
                                | "flex_grow"
                                | "flex_shrink"
                                | "grow"
                                | "shrink"
                                | "aria_label"
                                | "aria-label"
                                | "aria_labelledby"
                                | "aria-labelledby"
                                | "accesskit_name"
                                | "accesskit_role"
                                | "accesskit_id"
                                | "flex_basis"
                                | "absolute"
                                | "relative"
                                | "position"
                                | "pointer_events"
                                | "pointerEvents"
                                | "cursor"
                                | "top"
                                | "right"
                                | "bottom"
                                | "left"
                                | "inset"
                                | "z_index"
                                | "zIndex"
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
                                | "gap_x"
                                | "gapX"
                                | "gap_y"
                                | "gapY"
                                | "column_gap"
                                | "columnGap"
                                | "row_gap"
                                | "rowGap"
                                | "border_style"
                                | "borderStyle"
                                | "border_color"
                                | "borderColor"
                                | "border_width"
                                | "borderWidth"
                                | "border_top"
                                | "borderTop"
                                | "border_right"
                                | "borderRight"
                                | "border_bottom"
                                | "borderBottom"
                                | "border_left"
                                | "borderLeft"
                                | "border_top_width"
                                | "borderTopWidth"
                                | "border_right_width"
                                | "borderRightWidth"
                                | "border_bottom_width"
                                | "borderBottomWidth"
                                | "border_left_width"
                                | "borderLeftWidth"
                                | "border_top_color"
                                | "borderTopColor"
                                | "border_right_color"
                                | "borderRightColor"
                                | "border_bottom_color"
                                | "borderBottomColor"
                                | "border_left_color"
                                | "borderLeftColor"
                                | "outline_style"
                                | "outlineStyle"
                                | "outline_color"
                                | "outlineColor"
                                | "outline_width"
                                | "outlineWidth"
                                | "outline_offset"
                                | "outlineOffset"
                                | "outline"
                                | "transition"
                                | "transition_property"
                                | "transitionProperty"
                                | "transition_duration"
                                | "transitionDuration"
                                | "transition_delay"
                                | "transitionDelay"
                                | "transition_timing_function"
                                | "transitionTimingFunction"
                                | "animation"
                                | "animation_name"
                                | "animationName"
                                | "animation_duration"
                                | "animationDuration"
                                | "animation_delay"
                                | "animationDelay"
                                | "animation_timing_function"
                                | "animationTimingFunction"
                                | "animation_iteration_count"
                                | "animationIterationCount"
                                | "animation_direction"
                                | "animationDirection"
                                | "animation_fill_mode"
                                | "animationFillMode"
                                | "animation_play_state"
                                | "animationPlayState"
                                | "animation_composition"
                                | "animationComposition"
                                | "animation_timeline"
                                | "animationTimeline"
                                | "animation_range"
                                | "animationRange"
                                | "animation_range_start"
                                | "animationRangeStart"
                                | "animation_range_end"
                                | "animationRangeEnd"
                                | "clip_path"
                                | "clipPath"
                                | "clip_path_name"
                                | "clipPathName"
                                | "offset_path"
                                | "offsetPath"
                                | "offset_distance"
                                | "offsetDistance"
                                | "offset_rotate"
                                | "offsetRotate"
                                | "shadow"
                                | "box_shadow"
                                | "boxShadow"
                                | "box-shadow"
                                | "box_shadow_color"
                                | "boxShadowColor"
                                | "box_shadow_blur"
                                | "boxShadowBlur"
                                | "backdrop_blur"
                                | "backdropBlur"
                                | "filter"
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
                        // a11y attrs apply on the finished Element (not BaseComponentBuilder).
                        if matches!(attr_name, "aria_label" | "aria-label") {
                            chrome_binds.push(quote! {
                                __rui_el = __rui_el.aria_label(#value);
                            });
                            continue;
                        }
                        if matches!(attr_name, "aria_labelledby" | "aria-labelledby") {
                            chrome_binds.push(quote! {
                                __rui_el = __rui_el.aria_labelledby(#value);
                            });
                            continue;
                        }
                        if attr_name == "accesskit_name" {
                            chrome_binds.push(quote! {
                                __rui_el = __rui_el.accesskit_name(#value);
                            });
                            continue;
                        }
                        if attr_name == "accesskit_role" {
                            chrome_binds.push(quote! {
                                __rui_el = __rui_el.accesskit_role(#value);
                            });
                            continue;
                        }
                        if attr_name == "accesskit_id" {
                            chrome_binds.push(quote! {
                                __rui_el = __rui_el.accesskit_id(#value);
                            });
                            continue;
                        }
                        // Dynamic class / color base attrs → bind on resulting Element.
                        if expr_reads_get(value) {
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
                                        |el, color| rui::style::apply_user_bg(el, color),
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
                                        |el, color| rui::style::apply_user_fg(el, color),
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
                                        |el, color| rui::style::apply_user_border_color(el, color),
                                    );
                                });
                                continue;
                            }
                            // Dynamic size axes → LayoutChanged bind (WP-2.3 chip widen, etc.).
                            if matches!(
                                attr_name,
                                "w" | "width"
                                    | "inline_size"
                                    | "inlineSize"
                                    | "h"
                                    | "height"
                                    | "block_size"
                                    | "blockSize"
                                    | "min_w"
                                    | "minWidth"
                                    | "min_width"
                                    | "min_h"
                                    | "minHeight"
                                    | "min_height"
                                    | "max_w"
                                    | "maxWidth"
                                    | "max_width"
                                    | "max_h"
                                    | "maxHeight"
                                    | "max_height"
                            ) {
                                let axis = attr_name;
                                chrome_binds.push(quote! {
                                    __rui_el = rui::app::reactive::bind_prop(
                                        __rui_el,
                                        format!("{}:{}:bind_size_{}#{}", file!(), line!(), #axis, #idx),
                                        move || {
                                            rui::core::tree::IntoCssDim::into_css_dim(#value)
                                        },
                                        |el, dim| {
                                            rui::app::reactive::apply_bound_size(el, #axis, dim)
                                        },
                                    );
                                });
                                continue;
                            }
                            if matches!(attr_name, "aspect_ratio" | "aspect-ratio" | "aspectRatio")
                            {
                                chrome_binds.push(quote! {
                                    __rui_el = rui::app::reactive::bind_prop(
                                        __rui_el,
                                        format!("{}:{}:bind_aspect_ratio#{}", file!(), line!(), #idx),
                                        move || (#value).to_string(),
                                        |el, s: String| {
                                            let next =
                                                rui::core::tree::parse_css_aspect_ratio(s.as_str());
                                            if el.style.aspect_ratio == next {
                                                rui::core::tree::LayoutStatus::Clean
                                            } else {
                                                el.style.aspect_ratio = next;
                                                rui::core::tree::LayoutStatus::LayoutChanged
                                            }
                                        },
                                    );
                                });
                                continue;
                            }
                            if matches!(attr_name, "animation_play_state" | "animationPlayState") {
                                chrome_binds.push(quote! {
                                    __rui_el = rui::app::reactive::bind_prop(
                                        __rui_el,
                                        format!(
                                            "{}:{}:bind_animation_play_state#{}",
                                            file!(),
                                            line!(),
                                            #idx
                                        ),
                                        move || (#value).to_string(),
                                        |el, v| {
                                            rui::app::reactive::apply_bound_animation_play_state(
                                                el, v,
                                            )
                                        },
                                    );
                                });
                                continue;
                            }
                            if matches!(attr_name, "z_index" | "zIndex") {
                                chrome_binds.push(quote! {
                                    __rui_el = rui::app::reactive::bind_prop(
                                        __rui_el,
                                        format!("{}:{}:bind_z_index#{}", file!(), line!(), #idx),
                                        move || (#value) as i32,
                                        |el, z| rui::app::reactive::apply_bound_z_index(el, z),
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
                            chrome_binds.push(quote_apply_class_list(value));
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
                                Some("snug") => quote! { b = b.leading_snug(); },
                                Some("normal") => quote! { b = b.leading_normal(); },
                                Some("relaxed") => quote! { b = b.leading_relaxed(); },
                                Some("loose") => quote! { b = b.leading_loose(); },
                                _ => {
                                    quote! { b = b.leading(rui::core::tree::IntoF32Raw::into_f32_raw(#value)); }
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
                                "text_font" => "font_family",
                                "textDecoration" => "text_decoration",
                                "textDecorationLine" => "text_decoration_line",
                                "textDecorationStyle" => "text_decoration_style",
                                "textDecorationColor" => "text_decoration_color",
                                "textDecorationThickness" => "text_decoration_thickness",
                                "lineHeight" => "line_height",
                                "grow" => "flex_grow",
                                "shrink" => "flex_shrink",
                                "gapX" => "gap_x",
                                "gapY" => "gap_y",
                                "columnGap" | "column_gap" => "column_gap",
                                "rowGap" | "row_gap" => "row_gap",
                                "scroll_behavior" | "scrollBehavior" => "scroll_behavior",
                                "borderStyle" => "border_style",
                                "borderColor" => "border_color",
                                "borderWidth" => "border_width",
                                "borderTop" | "border_top" | "borderTopWidth" => "border_top_width",
                                "borderRight" | "border_right" | "borderRightWidth" => {
                                    "border_right_width"
                                }
                                "borderBottom" | "border_bottom" | "borderBottomWidth" => {
                                    "border_bottom_width"
                                }
                                "borderLeft" | "border_left" | "borderLeftWidth" => {
                                    "border_left_width"
                                }
                                "borderTopColor" => "border_top_color",
                                "borderRightColor" => "border_right_color",
                                "borderBottomColor" => "border_bottom_color",
                                "borderLeftColor" => "border_left_color",
                                "outlineStyle" => "outline_style",
                                "outlineColor" => "outline_color",
                                "outlineWidth" => "outline_width",
                                "outlineOffset" => "outline_offset",
                                "transitionProperty" => "transition_property",
                                "transitionDuration" => "transition_duration",
                                "transitionDelay" => "transition_delay",
                                "transitionTimingFunction" => "transition_timing_function",
                                "animationName" => "animation_name",
                                "animationDuration" => "animation_duration",
                                "animationDelay" => "animation_delay",
                                "animationTimingFunction" => "animation_timing_function",
                                "animationIterationCount" => "animation_iteration_count",
                                "animationDirection" => "animation_direction",
                                "animationFillMode" => "animation_fill_mode",
                                "animationPlayState" => "animation_play_state",
                                "animationComposition" => "animation_composition",
                                "animationTimeline" => "animation_timeline",
                                "animationRange" => "animation_range",
                                "animationRangeStart" => "animation_range_start",
                                "animationRangeEnd" => "animation_range_end",
                                "boxShadow" | "box-shadow" => "box_shadow",
                                "boxShadowColor" => "box_shadow_color",
                                "boxShadowBlur" => "box_shadow_blur",
                                "backdropBlur" => "backdrop_blur",
                                "boxShadowOffsetX" => "box_shadow_offset_x",
                                "boxShadowOffsetY" => "box_shadow_offset_y",
                                "boxShadowSpread" => "box_shadow_spread",
                                "boxShadowInset" => "box_shadow_inset",
                                "textShadow" | "text-shadow" => "text_shadow",
                                "textShadowColor" => "text_shadow_color",
                                "textShadowBlur" => "text_shadow_blur",
                                "textShadowOffsetX" => "text_shadow_offset_x",
                                "textShadowOffsetY" => "text_shadow_offset_y",
                                "pointerEvents" => "pointer_events",
                                "aspect-ratio" | "aspectRatio" => "aspect_ratio",
                                "zIndex" => "z_index",
                                _ => attr_name,
                            };
                            let method_ident = syn::Ident::new(method, attr.span());
                            let final_value = match method {
                                "bg" | "border" | "shadow" => {
                                    quote! { rui::core::tree::parse_color(#value) }
                                }
                                "border_color"
                                | "border_top_color"
                                | "border_right_color"
                                | "border_bottom_color"
                                | "border_left_color"
                                | "outline_color"
                                | "box_shadow_color" => {
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
                                "box_shadow" | "text_shadow" | "filter" | "border_style"
                                | "outline_style" => {
                                    quote! { #value }
                                }
                                "absolute" | "relative" | "underline" | "box_shadow_inset" => {
                                    if let Some(b) = expr_as_bool_compat(value) {
                                        quote! { #b }
                                    } else {
                                        quote! { rui::core::tree::parse_bool_like(#value) }
                                    }
                                }
                                _ => quote! { #value },
                            };
                            base_calls.push(quote! { b = b.#method_ident(#final_value); });
                        }
                    } else {
                        // Checkbox / Switch `checked` 是 `Option<Signal<bool>>`，不能走全局 boolish。
                        if (name_str == "Checkbox" || name_str == "Switch")
                            && matches!(attr_name, "checked" | "checked_signal" | "checkedSignal")
                        {
                            prop_names.push(syn::Ident::new("checked", attr.span()));
                            prop_values.push(quote! { Some(#value) });
                            continue;
                        }

                        let final_attr = if name_str == "Input" && attr_str == "type" {
                            // HTML `type` → 组件 prop `input_type`（避免 Rust 关键字）。
                            syn::Ident::new("input_type", attr.span())
                        } else if name_str == "Label" && attr_str == "for" {
                            // Solid `for` → `for_target`（原生 GUI 命名；`r#for` 仍可作为 Builder 别名）。
                            syn::Ident::new("for_target", attr.span())
                        } else if name_str == "ToggleGroup" && attr_str == "type" {
                            // ShadCN `type="single|multiple"` → prop `selection`（避免 Rust 关键字）。
                            syn::Ident::new("selection", attr.span())
                        } else if matches!(name_str.as_str(), "TableHead" | "TableCell")
                            && matches!(attr_str.as_str(), "width")
                        {
                            syn::Ident::new("w", attr.span())
                        } else {
                            match attr_str.as_str() {
                                "onClick" => syn::Ident::new("on_click", attr.span()),
                                "onChange" => syn::Ident::new("on_change", attr.span()),
                                // ShadCN / HTML 别名
                                "defaultChecked" => syn::Ident::new("default_checked", attr.span()),
                                "defaultValue" => syn::Ident::new("default_value", attr.span()),
                                "onCheckedChange" => syn::Ident::new("on_click", attr.span()),
                                "ariaInvalid" | "aria_invalid" => {
                                    syn::Ident::new("invalid", attr.span())
                                }
                                "passwordToggle" => syn::Ident::new("password_toggle", attr.span()),
                                "numberSteppers" => syn::Ident::new("number_steppers", attr.span()),
                                "maxLength" => syn::Ident::new("max_length", attr.span()),
                                // Calendar / DatePicker / ColorPicker / Command（Aria + Reader）
                                "captionLayout" => syn::Ident::new("caption_layout", attr.span()),
                                "valueSignal" => syn::Ident::new("value_signal", attr.span()),
                                "currentSignal" => syn::Ident::new("current_signal", attr.span()),
                                "defaultStep" => syn::Ident::new("default_step", attr.span()),
                                "activeItem" => syn::Ident::new("active_item", attr.span()),
                                "activeItemSignal" => {
                                    syn::Ident::new("active_item_signal", attr.span())
                                }
                                "defaultActive" => syn::Ident::new("default_active", attr.span()),
                                "bulletSize" => syn::Ident::new("bullet_size", attr.span()),
                                "lineSize" => syn::Ident::new("line_size", attr.span()),
                                "showAlpha" => syn::Ident::new("show_alpha", attr.span()),
                                "showAnimation" => syn::Ident::new("show_animation", attr.span()),
                                "scrollSelected" => syn::Ident::new("scroll_selected", attr.span()),
                                // Popover / Tooltip / HoverCard / floating
                                "openSignal" => syn::Ident::new("open_signal", attr.span()),
                                "sideOffset" => syn::Ident::new("side_offset", attr.span()),
                                "alignOffset" => syn::Ident::new("align_offset", attr.span()),
                                "anchorFocusId" => syn::Ident::new("anchor_focus_id", attr.span()),
                                // HoverCard Trigger：`delay` ≡ `openDelay`（对位 PreviewCard demo）
                                "openDelay" => syn::Ident::new("open_delay", attr.span()),
                                "closeDelay" => syn::Ident::new("close_delay", attr.span()),
                                "delay" => syn::Ident::new("open_delay", attr.span()),
                                // Resizable
                                "withHandle" => syn::Ident::new("with_handle", attr.span()),
                                "defaultSize" => syn::Ident::new("default_size", attr.span()),
                                "minSize" => syn::Ident::new("min_size", attr.span()),
                                "maxSize" => syn::Ident::new("max_size", attr.span()),
                                "sizesSignal" => syn::Ident::new("sizes", attr.span()),
                                // Sidebar（ShadCN base）
                                "defaultOpen" => syn::Ident::new("default_open", attr.span()),
                                "defaultItem" => syn::Ident::new("default_item", attr.span()),
                                "itemSignal" => syn::Ident::new("item_signal", attr.span()),
                                "onItemChange" => syn::Ident::new("on_item_change", attr.span()),
                                "onSubmit" => syn::Ident::new("on_submit", attr.span()),
                                "onStatusChange" => {
                                    syn::Ident::new("on_status_change", attr.span())
                                }
                                "isActive" => syn::Ident::new("is_active", attr.span()),
                                // MessageScroller (ShadCN base)
                                "autoScroll" => syn::Ident::new("auto_scroll", attr.span()),
                                "defaultScrollPosition" => {
                                    syn::Ident::new("default_scroll_position", attr.span())
                                }
                                "scrollEdgeThreshold" => {
                                    syn::Ident::new("scroll_edge_threshold", attr.span())
                                }
                                "scrollPreviousItemPeek" => {
                                    syn::Ident::new("scroll_previous_item_peek", attr.span())
                                }
                                "messageId" => syn::Ident::new("message_id", attr.span()),
                                "scrollAnchor" => syn::Ident::new("scroll_anchor", attr.span()),
                                // Solid `for`（非 Label 组件仍走 raw ident；Label 已在上方映射为 `for_target`）
                                "for" => proc_macro2::Ident::new_raw("for", attr.span()),
                                _ => attr.clone(),
                            }
                        };
                        let final_attr_name = final_attr.to_string();
                        let final_value = if attr_str == "id" {
                            // RSX string attrs are `&str`; component `id` is `String`.
                            quote! { (#value).into() }
                        } else if attr_name == "children" {
                            quote! { #value }
                        } else if matches!(
                            attr_name,
                            "on_click" | "onClick" | "on_change" | "onChange" | "onCheckedChange"
                        ) {
                            quote! { Some(rui::core::tree::Handler::from(rui::core::tree::ZeroArgFn(#value))) }
                        } else if is_boolish_attr(attr_name)
                            || matches!(
                                final_attr_name.as_str(),
                                "default_checked" | "invalid" | "disabled"
                            )
                        {
                            // 字面量 / `"true"` 仍走 bool 解析；非 bool 类型 props
                            //（如 Chart `selected: ChartSelected`）走 `.into()`，
                            // 避免全局 boolish 名单把 Option<usize> 误判成 bool。
                            if let Some(b) = expr_as_bool_compat(value) {
                                quote! { Some(#b) }
                            } else if matches!(
                                value,
                                syn::Expr::Lit(syn::ExprLit {
                                    lit: syn::Lit::Str(_),
                                    ..
                                })
                            ) {
                                quote! { Some(rui::core::tree::parse_bool_like(#value)) }
                            } else if expr_reads_get(value) {
                                quote! { Some(rui::core::reactivity::untrack(|| (#value).into())) }
                            } else {
                                quote! { Some((#value).into()) }
                            }
                        } else if matches!(final_attr_name.as_str(), "open_delay" | "close_delay") {
                            // Unsuffixed `{10}` → u64（不必写 `10_u64`）；与 Carousel autoplay 一致。
                            if expr_reads_get(value) {
                                quote! { Some(rui::core::reactivity::untrack(|| (#value) as u64)) }
                            } else {
                                quote! { Some((#value) as u64) }
                            }
                        } else if matches!(final_attr_name.as_str(), "side_offset" | "align_offset")
                        {
                            // Length：`4` ≡ `"4px"`；无单位字符串 = rem。
                            let v = quote_length_f32(value);
                            if expr_reads_get(value) {
                                quote! {
                                    Some(rui::core::reactivity::untrack(|| { #v }))
                                }
                            } else {
                                quote! { Some(#v) }
                            }
                        } else if expr_reads_get(value) {
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

                if name_str == "Checkbox" && !prop_names.iter().any(|p| *p == "id") {
                    let seq = CHECKBOX_RSX_ANON_SEQ.fetch_add(1, Ordering::Relaxed);
                    prop_names.push(syn::Ident::new("id", name_ident.span()));
                    prop_values.push(quote! {
                        format!("__rui_checkbox_auto_{}", #seq)
                    });
                }

                if name_str == "Switch" && !prop_names.iter().any(|p| *p == "id") {
                    let seq = SWITCH_RSX_ANON_SEQ.fetch_add(1, Ordering::Relaxed);
                    prop_names.push(syn::Ident::new("id", name_ident.span()));
                    prop_values.push(quote! {
                        format!("__rui_switch_auto_{}", #seq)
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
                    // Multi-child → `rui_fragment` (single-root children slot). Layout
                    // primitives HStack/VStack/GStack use the primitive path below.
                    // Runtime unpacks via `rui::utils::take_packed` / `take_fragment`.
                    if expanded_children.len() == 1 {
                        let first = &expanded_children[0];
                        children_expr = quote! {
                            Some({
                                let mut __list = rui::core::tree::build_ui_child(|| (#first));
                                if __list.len() == 1 {
                                    __list.remove(0)
                                } else {
                                    let mut __frag = rui::core::tree::Element::new();
                                    __frag.set_attr("rui_fragment", "1");
                                    __frag.children(__list)
                                }
                            })
                        };
                    } else {
                        children_expr = quote! {
                            Some({
                                let mut __list: Vec<rui::core::tree::Element> = Vec::new();
                                #( __list.extend(rui::core::tree::build_ui_child(|| (#expanded_children))); )*
                                if __list.len() == 1 {
                                    __list.remove(0)
                                } else {
                                    let mut __frag = rui::core::tree::Element::new();
                                    __frag.set_attr("rui_fragment", "1");
                                    __frag.children(__list)
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
                                children: rui::utils::children_none_if_vacuous(#children_expr),
                                #(#prop_names: #prop_values,)*
                                #rest_ts
                            });
                            rui::style::apply_stylesheet_rules_tree(__rui_comp)
                        }
                    };
                }
                return quote! {
                    {
                        #[allow(clippy::needless_update)]
                        let mut __rui_el: rui::core::tree::Element = #name_ident(#props_ident {
                            base: #base_expr,
                            children: rui::utils::children_none_if_vacuous(#children_expr),
                            #(#prop_names: #prop_values,)*
                            #rest_ts
                        }).into();
                        __rui_el = rui::style::apply_stylesheet_rules_tree(__rui_el);
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
                    // Layout container. `Rect` is SVG — use `<Box>`.
                    "Box" => quote! { rui::primitives::r#box() },
                    "Interactive" => {
                        quote! { rui::primitives::interactive(rui::core::tree::Element::new()) }
                    }
                    "Text" => quote! { rui::primitives::text("") },
                    "svg" | "Svg" => quote! { rui::primitives::svg() },
                    "circle" | "Circle" => quote! { rui::primitives::svg::circle() },
                    "ellipse" | "Ellipse" => quote! { rui::primitives::svg::ellipse() },
                    "path" | "Path" => quote! { rui::primitives::svg::path() },
                    "rect" | "Rect" => quote! { rui::primitives::svg::rect() },
                    "line" | "Line" => quote! { rui::primitives::svg::line() },
                    "polyline" | "Polyline" => quote! { rui::primitives::svg::polyline() },
                    "polygon" | "Polygon" => quote! { rui::primitives::svg::polygon() },
                    "g" => quote! { rui::primitives::svg::g() },
                    "defs" | "Defs" => quote! { rui::primitives::svg::defs() },
                    "use" | "Use" => quote! { rui::primitives::svg::svg_use() },
                    "clipPath" | "ClipPath" => quote! { rui::primitives::svg::clip_path() },
                    "linearGradient" | "LinearGradient" => {
                        quote! { rui::primitives::svg::linear_gradient() }
                    }
                    "radialGradient" | "RadialGradient" => {
                        quote! { rui::primitives::svg::radial_gradient() }
                    }
                    "symbol" | "Symbol" => quote! { rui::primitives::svg::symbol() },
                    "stop" | "Stop" => quote! { rui::primitives::svg::stop() },
                    "animate" | "Animate" => quote! { rui::primitives::svg::animate() },
                    "animateTransform" | "AnimateTransform" => {
                        quote! { rui::primitives::svg::animate_transform() }
                    }
                    "animateMotion" | "AnimateMotion" => {
                        quote! { rui::primitives::svg::animate_motion() }
                    }
                    "mpath" | "Mpath" | "MPath" => quote! { rui::primitives::svg::mpath() },
                    "set" | "Set" => quote! { rui::primitives::svg::set_el() },
                    "text" => quote! { rui::primitives::svg::svg_text() },
                    "tspan" | "Tspan" | "TSpan" => quote! { rui::primitives::svg::tspan() },
                    "textPath" | "TextPath" => quote! { rui::primitives::svg::text_path() },
                    "image" => quote! { rui::primitives::svg::image() },
                    "mask" | "Mask" => quote! { rui::primitives::svg::mask() },
                    "pattern" | "Pattern" => quote! { rui::primitives::svg::pattern() },
                    "marker" => quote! { rui::primitives::svg::marker() },
                    "filter" | "Filter" => quote! { rui::primitives::svg::filter() },
                    "title" | "Title" => quote! { rui::primitives::svg::title() },
                    "desc" | "Desc" => quote! { rui::primitives::svg::desc() },
                    "feGaussianBlur" | "FeGaussianBlur" => {
                        quote! { rui::primitives::svg::fe_gaussian_blur() }
                    }
                    "feDropShadow" | "FeDropShadow" => {
                        quote! { rui::primitives::svg::fe_drop_shadow() }
                    }
                    "feOffset" | "FeOffset" => quote! { rui::primitives::svg::fe_offset() },
                    "feBlend" | "FeBlend" => quote! { rui::primitives::svg::fe_blend() },
                    "feColorMatrix" | "FeColorMatrix" => {
                        quote! { rui::primitives::svg::fe_color_matrix() }
                    }
                    "feFlood" | "FeFlood" => quote! { rui::primitives::svg::fe_flood() },
                    "feMerge" | "FeMerge" => quote! { rui::primitives::svg::fe_merge() },
                    "feMergeNode" | "FeMergeNode" => {
                        quote! { rui::primitives::svg::fe_merge_node() }
                    }
                    "canvas" | "Canvas" => quote! { rui::primitives::canvas() },
                    "scene3d" | "Scene3d" => quote! { rui::primitives::scene3d() },
                    other => {
                        if let Some(tag) = svg_tag_canonical(other) {
                            quote! { rui::primitives::svg::svg_el(#tag) }
                        } else {
                            quote! { rui::core::tree::Element::new() }
                        }
                    }
                }
            };

            let mut pending_icon_bind: Option<proc_macro2::TokenStream> = None;
            if name_str == "Icon" {
                let mut static_icon_name: Option<String> = None;
                let mut dynamic_name: Option<&syn::Expr> = None;
                let mut static_src: Option<String> = None;
                let mut dynamic_src: Option<&syn::Expr> = None;
                for (attr, value) in &el.attributes {
                    let key = attr.to_string();
                    if key == "name" {
                        if let syn::Expr::Lit(expr_lit) = value
                            && let syn::Lit::Str(lit_str) = &expr_lit.lit
                            && !expr_reads_get(value)
                        {
                            static_icon_name = Some(lit_str.value());
                        } else {
                            dynamic_name = Some(value);
                        }
                    } else if key == "src" {
                        if let syn::Expr::Lit(expr_lit) = value
                            && let syn::Lit::Str(lit_str) = &expr_lit.lit
                            && !expr_reads_get(value)
                        {
                            static_src = Some(lit_str.value());
                        } else {
                            dynamic_src = Some(value);
                        }
                    }
                }

                if let Some(src_expr) = dynamic_src {
                    let idx = *node_counter;
                    *node_counter += 1;
                    element = quote! {
                        {
                            let __src = rui::core::reactivity::untrack(|| (#src_expr).to_string());
                            rui::utils::build_icon_from_src(__src)
                        }
                    };
                    pending_icon_bind = Some(quote! {
                        __rui_el = rui::app::reactive::bind_attr(
                            __rui_el,
                            format!("{}:{}:bind_icon_src#{}", file!(), line!(), #idx),
                            move || (#src_expr).to_string(),
                            |el, src| rui::utils::apply_bound_icon_src(el, src),
                        );
                    });
                } else if let Some(src) = static_src {
                    element = quote! { rui::utils::build_icon_from_src(#src) };
                } else if let Some(name_expr) = dynamic_name {
                    let idx = *node_counter;
                    *node_counter += 1;
                    element = quote! {
                        {
                            let __n = rui::core::reactivity::untrack(|| (#name_expr).to_string());
                            rui::utils::build_icon_named(__n)
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
                } else if let Some(iname) = static_icon_name {
                    element = quote! { rui::utils::build_icon_named(#iname) };
                } else {
                    element = quote! {
                        rui::utils::text_node::build_icon_text_node("\u{ea0e}")
                    };
                }
            }

            if name_str == "AnimatedIcon" {
                let mut name_ts = quote! { "heart" };
                let mut size_ts = quote! { 24.0_f32 };
                let mut controlled_ts = quote! { false };
                let mut pointer_events_ts: Option<proc_macro2::TokenStream> = None;
                for (attr, value) in &el.attributes {
                    match attr.to_string().as_str() {
                        "name" => {
                            if let syn::Expr::Lit(expr_lit) = value
                                && let syn::Lit::Str(lit_str) = &expr_lit.lit
                            {
                                let n = lit_str.value();
                                name_ts = quote! { #n };
                            } else {
                                name_ts = quote! { (#value).to_string() };
                            }
                        }
                        "size" => {
                            size_ts = quote! { rui::core::tree::IntoF32::into_f32(#value) };
                        }
                        "controlled" => {
                            controlled_ts = quote! { #value };
                        }
                        "pointer_events" | "pointerEvents" | "pointer-events" => {
                            pointer_events_ts = Some(quote! { #value });
                        }
                        _ => {}
                    }
                }
                let pe_chain = if let Some(pe) = pointer_events_ts {
                    quote! { __b = __b.pointer_events(#pe); }
                } else {
                    quote! {}
                };
                element = quote! {
                    {
                        let mut __b = rui::utils::animated_icon(#name_ts)
                            .size_px(#size_ts);
                        if #controlled_ts {
                            __b = __b.controlled(true);
                        }
                        #pe_chain
                        rui::core::tree::Element::from(__b)
                    }
                };
            }

            if matches!(
                name_str.as_str(),
                "Box" | "VStack" | "HStack" | "GStack" | "Grid"
            ) {
                let mut handle_expr: Option<&syn::Expr> = None;
                let mut index_expr: Option<&syn::Expr> = None;
                for (attr, value) in &el.attributes {
                    match attr.to_string().as_str() {
                        "drag_reorder_handle" | "dragReorderHandle" => {
                            handle_expr = Some(value);
                        }
                        "drag_reorder_index" | "dragReorderIndex" => {
                            index_expr = Some(value);
                        }
                        _ => {}
                    }
                }
                if let Some(handle) = handle_expr {
                    if let syn::Expr::Tuple(tuple) = handle {
                        if tuple.elems.len() == 2 {
                            let list = &tuple.elems[0];
                            let index = &tuple.elems[1];
                            element = quote! {
                                #element.drag_reorder_handle(#list, (#index) as usize)
                            };
                        } else {
                            return quote! {
                                compile_error!(
                                    "`drag_reorder_handle={(list_id, index)}` needs exactly two tuple elements"
                                )
                            };
                        }
                    } else if let Some(index) = index_expr {
                        element = quote! {
                            #element.drag_reorder_handle(#handle, (#index) as usize)
                        };
                    } else {
                        return quote! {
                            compile_error!(
                                "`drag_reorder_handle` requires `(list_id, index)` or a sibling `drag_reorder_index`"
                            )
                        };
                    }
                } else if index_expr.is_some() {
                    return quote! {
                        compile_error!("`drag_reorder_index` requires `drag_reorder_handle`")
                    };
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
                    if expr_reads_get(value) {
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
                if is_svg_tag(&name_str) {
                    match attr_str.as_str() {
                        "viewBox" | "view_box" => {
                            element = quote! { #element.view_box(#value) };
                            continue;
                        }
                        "cx" => {
                            element = quote! { #element.cx(#value) };
                            continue;
                        }
                        "cy" => {
                            element = quote! { #element.cy(#value) };
                            continue;
                        }
                        "r" => {
                            element = quote! { #element.r(#value) };
                            continue;
                        }
                        "rx" => {
                            element = quote! { #element.rx(#value) };
                            continue;
                        }
                        "ry" => {
                            element = quote! { #element.ry(#value) };
                            continue;
                        }
                        "d" => {
                            element = quote! { #element.d(#value) };
                            continue;
                        }
                        "x" => {
                            element = quote! { #element.x(#value) };
                            continue;
                        }
                        "y" => {
                            element = quote! { #element.y(#value) };
                            continue;
                        }
                        "x1" => {
                            element = quote! { #element.x1(#value) };
                            continue;
                        }
                        "y1" => {
                            element = quote! { #element.y1(#value) };
                            continue;
                        }
                        "x2" => {
                            element = quote! { #element.x2(#value) };
                            continue;
                        }
                        "y2" => {
                            element = quote! { #element.y2(#value) };
                            continue;
                        }
                        "points" => {
                            element = quote! { #element.points(#value) };
                            continue;
                        }
                        "fill" => {
                            element = quote! { #element.fill(#value) };
                            continue;
                        }
                        "stroke" => {
                            element = quote! { #element.stroke(#value) };
                            continue;
                        }
                        "stroke_width" | "strokeWidth" => {
                            element = quote! { #element.stroke_width(#value) };
                            continue;
                        }
                        "stroke_linecap" | "strokeLinecap" => {
                            element = quote! { #element.stroke_linecap(#value) };
                            continue;
                        }
                        "stroke_linejoin" | "strokeLinejoin" => {
                            element = quote! { #element.stroke_linejoin(#value) };
                            continue;
                        }
                        "fill_rule" | "fillRule" => {
                            element = quote! { #element.fill_rule(#value) };
                            continue;
                        }
                        "id" => {
                            element = quote! { #element.id(#value) };
                            continue;
                        }
                        "clip_path" | "clipPath" => {
                            element = quote! { #element.clip_path(#value) };
                            continue;
                        }
                        "href" | "xlink:href" => {
                            element = quote! { #element.href(#value) };
                            continue;
                        }
                        "offset" => {
                            element = quote! { #element.offset(#value) };
                            continue;
                        }
                        "stop_color" | "stopColor" => {
                            element = quote! { #element.stop_color(#value) };
                            continue;
                        }
                        "stop_opacity" | "stopOpacity" => {
                            element = quote! { #element.stop_opacity(#value) };
                            continue;
                        }
                        "gradient_units" | "gradientUnits" => {
                            element = quote! { #element.gradient_units(#value) };
                            continue;
                        }
                        "attribute_name" | "attributeName" => {
                            element = quote! { #element.attribute_name(#value) };
                            continue;
                        }
                        "from" => {
                            element = quote! { #element.from_val(#value) };
                            continue;
                        }
                        "to" => {
                            element = quote! { #element.to_val(#value) };
                            continue;
                        }
                        "dur" => {
                            element = quote! { #element.dur(#value) };
                            continue;
                        }
                        "repeat_count" | "repeatCount" => {
                            element = quote! { #element.repeat_count(#value) };
                            continue;
                        }
                        "begin" => {
                            element = quote! { #element.begin(#value) };
                            continue;
                        }
                        "type" => {
                            element = quote! { #element.anim_type(#value) };
                            continue;
                        }
                        "transform" => {
                            element = quote! { #element.transform(#value) };
                            continue;
                        }
                        "opacity" => {
                            element = quote! { #element.opacity(#value) };
                            continue;
                        }
                        "font_size" | "fontSize" => {
                            element = quote! { #element.font_size(#value) };
                            continue;
                        }
                        "text_anchor" | "textAnchor" => {
                            element = quote! { #element.text_anchor(#value) };
                            continue;
                        }
                        "fill_opacity" | "fillOpacity" => {
                            element = quote! { #element.fill_opacity(#value) };
                            continue;
                        }
                        "stroke_opacity" | "strokeOpacity" => {
                            element = quote! { #element.stroke_opacity(#value) };
                            continue;
                        }
                        "stroke_dasharray" | "strokeDasharray" => {
                            element = quote! { #element.stroke_dasharray(#value) };
                            continue;
                        }
                        "mask" => {
                            element = quote! { #element.mask_attr(#value) };
                            continue;
                        }
                        "filter" => {
                            element = quote! { #element.filter_attr(#value) };
                            continue;
                        }
                        _ => {
                            // `svg` root uses SvgBuilder (w/h/…), not SvgShapeBuilder::attr.
                            if matches!(name_str.as_str(), "svg" | "Svg") {
                                // fall through to general attribute handlers
                            } else {
                                let emit_name = svg_attr_emit_name(&attr_str);
                                element = quote! {
                                    #element.attr(#emit_name, (#value).to_string())
                                };
                                continue;
                            }
                        }
                    }
                }
                if is_canvas_tag(&name_str) && attr_str.as_str() == "paint" {
                    element = quote! { #element.paint(#value) };
                    continue;
                }
                match attr_str.as_str() {
                    "w" | "width" => {
                        if expr_reads_get(value) && parse_min_max_call(value).is_none() {
                            let idx = *node_counter;
                            *node_counter += 1;
                            let axis = "w";
                            dynamic_binds.push(quote! {
                                __rui_el = rui::app::reactive::bind_prop(
                                    __rui_el,
                                    format!("{}:{}:bind_size_w#{}", file!(), line!(), #idx),
                                    move || {
                                        rui::core::tree::IntoCssDim::into_css_dim(#value)
                                    },
                                    |el, dim| rui::app::reactive::apply_bound_size(el, #axis, dim),
                                );
                            });
                        } else {
                            element = apply_min_max_size_expr(element, value, "w");
                        }
                    }
                    "inline_size" | "inlineSize" => {
                        if expr_reads_get(value) && parse_min_max_call(value).is_none() {
                            let idx = *node_counter;
                            *node_counter += 1;
                            let axis = "inline_size";
                            dynamic_binds.push(quote! {
                                __rui_el = rui::app::reactive::bind_prop(
                                    __rui_el,
                                    format!("{}:{}:bind_size_inline#{}", file!(), line!(), #idx),
                                    move || {
                                        rui::core::tree::IntoCssDim::into_css_dim(#value)
                                    },
                                    |el, dim| rui::app::reactive::apply_bound_size(el, #axis, dim),
                                );
                            });
                        } else {
                            element = quote! { #element.inline_size(#value) };
                        }
                    }
                    "max_w" | "maxWidth" | "max_width" => {
                        if expr_reads_get(value) {
                            let idx = *node_counter;
                            *node_counter += 1;
                            let axis = "max_w";
                            dynamic_binds.push(quote! {
                                __rui_el = rui::app::reactive::bind_prop(
                                    __rui_el,
                                    format!("{}:{}:bind_size_max_w#{}", file!(), line!(), #idx),
                                    move || {
                                        rui::core::tree::IntoCssDim::into_css_dim(#value)
                                    },
                                    |el, dim| rui::app::reactive::apply_bound_size(el, #axis, dim),
                                );
                            });
                        } else {
                            element = quote! { #element.max_w(#value) };
                        }
                    }
                    "min_w" | "minWidth" | "min_width" => {
                        if expr_reads_get(value) {
                            let idx = *node_counter;
                            *node_counter += 1;
                            let axis = "min_w";
                            dynamic_binds.push(quote! {
                                __rui_el = rui::app::reactive::bind_prop(
                                    __rui_el,
                                    format!("{}:{}:bind_size_min_w#{}", file!(), line!(), #idx),
                                    move || {
                                        rui::core::tree::IntoCssDim::into_css_dim(#value)
                                    },
                                    |el, dim| rui::app::reactive::apply_bound_size(el, #axis, dim),
                                );
                            });
                        } else {
                            element = quote! { #element.min_w(#value) };
                        }
                    }
                    "h" | "height" => {
                        if expr_reads_get(value) && parse_min_max_call(value).is_none() {
                            let idx = *node_counter;
                            *node_counter += 1;
                            let axis = "h";
                            dynamic_binds.push(quote! {
                                __rui_el = rui::app::reactive::bind_prop(
                                    __rui_el,
                                    format!("{}:{}:bind_size_h#{}", file!(), line!(), #idx),
                                    move || {
                                        rui::core::tree::IntoCssDim::into_css_dim(#value)
                                    },
                                    |el, dim| rui::app::reactive::apply_bound_size(el, #axis, dim),
                                );
                            });
                        } else {
                            element = apply_min_max_size_expr(element, value, "h");
                        }
                    }
                    "block_size" | "blockSize" => {
                        if expr_reads_get(value) && parse_min_max_call(value).is_none() {
                            let idx = *node_counter;
                            *node_counter += 1;
                            let axis = "block_size";
                            dynamic_binds.push(quote! {
                                __rui_el = rui::app::reactive::bind_prop(
                                    __rui_el,
                                    format!("{}:{}:bind_size_block#{}", file!(), line!(), #idx),
                                    move || {
                                        rui::core::tree::IntoCssDim::into_css_dim(#value)
                                    },
                                    |el, dim| rui::app::reactive::apply_bound_size(el, #axis, dim),
                                );
                            });
                        } else {
                            element = quote! { #element.block_size(#value) };
                        }
                    }
                    "max_h" | "maxHeight" | "max_height" => {
                        if expr_reads_get(value) {
                            let idx = *node_counter;
                            *node_counter += 1;
                            let axis = "max_h";
                            dynamic_binds.push(quote! {
                                __rui_el = rui::app::reactive::bind_prop(
                                    __rui_el,
                                    format!("{}:{}:bind_size_max_h#{}", file!(), line!(), #idx),
                                    move || {
                                        rui::core::tree::IntoCssDim::into_css_dim(#value)
                                    },
                                    |el, dim| rui::app::reactive::apply_bound_size(el, #axis, dim),
                                );
                            });
                        } else {
                            element = quote! { #element.max_h(#value) };
                        }
                    }
                    "min_h" | "minHeight" | "min_height" => {
                        if expr_reads_get(value) {
                            let idx = *node_counter;
                            *node_counter += 1;
                            let axis = "min_h";
                            dynamic_binds.push(quote! {
                                __rui_el = rui::app::reactive::bind_prop(
                                    __rui_el,
                                    format!("{}:{}:bind_size_min_h#{}", file!(), line!(), #idx),
                                    move || {
                                        rui::core::tree::IntoCssDim::into_css_dim(#value)
                                    },
                                    |el, dim| rui::app::reactive::apply_bound_size(el, #axis, dim),
                                );
                            });
                        } else {
                            element = quote! { #element.min_h(#value) };
                        }
                    }
                    "opacity" => {
                        element = quote! { #element.opacity(#value) };
                    }
                    "bg" => {
                        if expr_reads_get(value) {
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
                        if expr_reads_get(value) {
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
                        element = quote! { #element.border_width(#value) }
                    }
                    "border_top_width" | "borderTopWidth" | "border_top" | "borderTop" => {
                        element = quote! { #element.border_top_width(rui::core::tree::IntoF32::into_f32(#value)) }
                    }
                    "border_right_width" | "borderRightWidth" | "border_right" | "borderRight" => {
                        element = quote! { #element.border_right_width(rui::core::tree::IntoF32::into_f32(#value)) }
                    }
                    "border_bottom_width"
                    | "borderBottomWidth"
                    | "border_bottom"
                    | "borderBottom" => {
                        element = quote! { #element.border_bottom_width(rui::core::tree::IntoF32::into_f32(#value)) }
                    }
                    "border_left_width" | "borderLeftWidth" | "border_left" | "borderLeft" => {
                        element = quote! { #element.border_left_width(rui::core::tree::IntoF32::into_f32(#value)) }
                    }
                    "border_top_color" | "borderTopColor" => {
                        element = quote! { #element.border_top_color(rui::core::tree::parse_color(#value)) }
                    }
                    "border_right_color" | "borderRightColor" => {
                        element = quote! { #element.border_right_color(rui::core::tree::parse_color(#value)) }
                    }
                    "border_bottom_color" | "borderBottomColor" => {
                        element = quote! { #element.border_bottom_color(rui::core::tree::parse_color(#value)) }
                    }
                    "border_left_color" | "borderLeftColor" => {
                        element = quote! { #element.border_left_color(rui::core::tree::parse_color(#value)) }
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
                    "outline_offset" | "outlineOffset" => {
                        element = quote! { #element.outline_offset(rui::core::tree::IntoF32::into_f32(#value)) }
                    }
                    "outline" => element = quote! { #element.apply_outline_shorthand(#value) },
                    "transition" => element = quote! { #element.transition(#value) },
                    "transition_property" => {
                        element = quote! { #element.transition_property(#value) }
                    }
                    "transition_duration" => {
                        element = quote! { #element.transition_duration(#value) }
                    }
                    "transition_delay" => element = quote! { #element.transition_delay(#value) },
                    "transition_timing_function" => {
                        element = quote! { #element.transition_timing_function(#value) }
                    }
                    "animation" => element = quote! { #element.animation(#value) },
                    "animation_name" | "animationName" => {
                        element = quote! { #element.animation_name(#value) }
                    }
                    "animation_duration" | "animationDuration" => {
                        element = quote! { #element.animation_duration(#value) }
                    }
                    "animation_delay" | "animationDelay" => {
                        element = quote! { #element.animation_delay(#value) }
                    }
                    "animation_timing_function" | "animationTimingFunction" => {
                        element = quote! { #element.animation_timing_function(#value) }
                    }
                    "animation_iteration_count" | "animationIterationCount" => {
                        element = quote! { #element.animation_iteration_count(#value) }
                    }
                    "animation_direction" | "animationDirection" => {
                        element = quote! { #element.animation_direction(#value) }
                    }
                    "animation_fill_mode" | "animationFillMode" => {
                        element = quote! { #element.animation_fill_mode(#value) }
                    }
                    "animation_play_state" | "animationPlayState" => {
                        if expr_reads_get(value) {
                            let idx = *node_counter;
                            *node_counter += 1;
                            dynamic_binds.push(quote! {
                                __rui_el = rui::app::reactive::bind_prop(
                                    __rui_el,
                                    format!(
                                        "{}:{}:bind_animation_play_state#{}",
                                        file!(),
                                        line!(),
                                        #idx
                                    ),
                                    move || (#value).to_string(),
                                    |el, v| {
                                        rui::app::reactive::apply_bound_animation_play_state(el, v)
                                    },
                                );
                            });
                        } else {
                            element = quote! { #element.animation_play_state(#value) }
                        }
                    }
                    "animation_composition" | "animationComposition" => {
                        element = quote! { #element.animation_composition(#value) }
                    }
                    "animation_timeline" | "animationTimeline" => {
                        element = quote! { #element.animation_timeline(#value) }
                    }
                    "animation_range" | "animationRange" => {
                        element = quote! { #element.animation_range(#value) }
                    }
                    "animation_range_start" | "animationRangeStart" => {
                        element = quote! { #element.animation_range_start(#value) }
                    }
                    "animation_range_end" | "animationRangeEnd" => {
                        element = quote! { #element.animation_range_end(#value) }
                    }
                    "color" | "text_color" => {
                        if expr_reads_get(value) {
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
                        // Unitless（Tailwind `font-[600]`）；走 IntoF32Raw，勿用长度 rem 路径。
                        element = quote! {
                            #element.font_weight(rui::core::tree::IntoF32Raw::into_f32_raw(#value))
                        }
                    }
                    "font_family" | "text_font" => {
                        element = quote! { #element.font_family(#value) }
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
                    "gap_x" | "gapX" | "column_gap" | "columnGap" => {
                        element = quote! { #element.column_gap(#value) }
                    }
                    "gap_y" | "gapY" | "row_gap" | "rowGap" => {
                        element = quote! { #element.row_gap(#value) }
                    }
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
                        } else if s == "snug" {
                            if is_custom {
                                element = quote! { #element.leading_snug() }
                            } else {
                                element = quote! { #element.text_leading_snug() }
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
                        } else if s == "loose" {
                            if is_custom {
                                element = quote! { #element.leading_loose() }
                            } else {
                                element = quote! { #element.text_leading_loose() }
                            }
                        } else if is_custom {
                            let v = quote_unitless_f32(value);
                            element = quote! { #element.leading(#v) }
                        } else {
                            let v = quote_unitless_f32(value);
                            element = quote! { #element.text_leading(#v) }
                        }
                    }
                    "size" => {
                        if name_str == "AnimatedIcon" {
                            // Consumed by AnimatedIcon pre-pass (builder size_px).
                        } else if name_str == "Icon" {
                            element = quote! {
                                rui::utils::with_icon_size(
                                    #element,
                                    rui::core::tree::IntoF32::into_f32(#value),
                                )
                            }
                        } else if is_custom || name_str == "Button" || name_str == "Input" {
                            element = quote! { #element.size(#value) }
                        } else {
                            element = quote! { #element.text_size(rui::core::tree::IntoF32::into_f32(#value)) }
                        }
                    }
                    "controlled" if name_str == "AnimatedIcon" => {}
                    "pointer_events" | "pointerEvents" | "pointer-events"
                        if name_str == "AnimatedIcon" => {}
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
                    "writing_mode" | "writingMode" => {
                        element = quote! { #element.writing_mode(#value) }
                    }
                    "overflow_x" | "overflowX" => element = quote! { #element.overflow_x(#value) },
                    "overflow_y" | "overflowY" => element = quote! { #element.overflow_y(#value) },
                    "aspect_ratio" | "aspect-ratio" | "aspectRatio" => {
                        if expr_reads_get(value) {
                            let idx = *node_counter;
                            *node_counter += 1;
                            dynamic_binds.push(quote! {
                                __rui_el = rui::app::reactive::bind_prop(
                                    __rui_el,
                                    format!("{}:{}:bind_aspect_ratio#{}", file!(), line!(), #idx),
                                    move || (#value).to_string(),
                                    |el, s: String| {
                                        let next =
                                            rui::core::tree::parse_css_aspect_ratio(s.as_str());
                                        if el.style.aspect_ratio == next {
                                            rui::core::tree::LayoutStatus::Clean
                                        } else {
                                            el.style.aspect_ratio = next;
                                            rui::core::tree::LayoutStatus::LayoutChanged
                                        }
                                    },
                                );
                            });
                        } else {
                            element = quote! { #element.aspect_ratio(#value) };
                        }
                    }
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
                    "scrollbar_track_border_color" | "scrollbarTrackBorderColor" => {
                        element = quote! { #element.scrollbar_track_border_color(#value) }
                    }
                    "scrollbar_thumb_border_color" | "scrollbarThumbBorderColor" => {
                        element = quote! { #element.scrollbar_thumb_border_color(#value) }
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
                    "backdrop_blur" | "backdropBlur" => {
                        element = quote! { #element.backdrop_blur(rui::core::tree::IntoF32::into_f32(#value)) }
                    }
                    "filter" => element = quote! { #element.filter(#value) },
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
                    "on_mousedown" | "onMouseDown" => {
                        element = quote! { #element.on_mousedown(#value) }
                    }
                    "on_mouseup" | "onMouseUp" => element = quote! { #element.on_mouseup(#value) },
                    "on_mousemove" | "onMouseMove" => {
                        element = quote! { #element.on_mousemove(#value) }
                    }
                    "on_mouseover" | "onMouseOver" => {
                        element = quote! { #element.on_mouseover(#value) }
                    }
                    "on_mouseout" | "onMouseOut" => {
                        element = quote! { #element.on_mouseout(#value) }
                    }
                    "on_dblclick" | "onDblClick" | "on_double_click" | "onDoubleClick" => {
                        element = quote! { #element.on_dblclick(#value) }
                    }
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
                    "track_border_color" | "trackBorderColor" => {
                        element = quote! {
                            #element.track_border_color(rui::core::tree::parse_color(#value))
                        }
                    }
                    "thumb_border_color" | "thumbBorderColor" => {
                        element = quote! {
                            #element.thumb_border_color(rui::core::tree::parse_color(#value))
                        }
                    }
                    "disabled" => element = quote! { #element.disabled(#value) },
                    "placeholder" => element = quote! { #element.placeholder(#value) },
                    "invalid" => element = quote! { #element.invalid(#value) },
                    "aria_label" | "aria-label" => element = quote! { #element.aria_label(#value) },
                    "aria_labelledby" | "aria-labelledby" => {
                        element = quote! { #element.aria_labelledby(#value) }
                    }
                    "accesskit_name" => element = quote! { #element.accesskit_name(#value) },
                    "accesskit_role" => element = quote! { #element.accesskit_role(#value) },
                    "accesskit_id" => element = quote! { #element.accesskit_id(#value) },

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

                    "absolute" => {
                        if let Some(b) = expr_as_bool_compat(value) {
                            element = quote! { #element.absolute(#b) };
                        } else {
                            element = quote! {
                                #element.absolute(rui::core::tree::parse_bool_like(#value))
                            };
                        }
                    }
                    "relative" => {
                        if let Some(b) = expr_as_bool_compat(value) {
                            element = quote! { #element.relative(#b) };
                        } else {
                            element = quote! {
                                #element.relative(rui::core::tree::parse_bool_like(#value))
                            };
                        }
                    }
                    "pointer_events" | "pointerEvents" => {
                        element = quote! { #element.pointer_events(#value) }
                    }
                    "position" => element = quote! { #element.position(#value) },
                    "cursor" => element = quote! { #element.cursor(#value) },
                    "top" => element = quote! { #element.top(#value) },
                    "right" => element = quote! { #element.right(#value) },
                    "bottom" => element = quote! { #element.bottom(#value) },
                    "left" => element = quote! { #element.left(#value) },
                    "inset" => element = quote! { #element.inset(#value) },
                    "inset_inline" | "insetInline" => {
                        element = quote! { #element.inset_inline(#value) }
                    }
                    "inset_block" | "insetBlock" => {
                        element = quote! { #element.inset_block(#value) }
                    }
                    "z_index" | "zIndex" => {
                        if expr_reads_get(value) {
                            let idx = *node_counter;
                            *node_counter += 1;
                            dynamic_binds.push(quote! {
                                __rui_el = rui::app::reactive::bind_prop(
                                    __rui_el,
                                    format!("{}:{}:bind_z_index#{}", file!(), line!(), #idx),
                                    move || (#value) as i32,
                                    |el, z| rui::app::reactive::apply_bound_z_index(el, z),
                                );
                            });
                        } else if let Some(z) = expr_as_i32(value) {
                            element = quote! { #element.z_index(#z) };
                        } else {
                            element = quote! { #element.z_index((#value) as i32) };
                        }
                    }

                    "css_transform" | "cssTransform" => {
                        element = quote! { #element.css_transform(#value) }
                    }
                    "clip_path" | "clipPath" | "clip-path" => {
                        element = quote! { #element.clip_path(#value) }
                    }
                    "clip_path_name" | "clipPathName" => {
                        element = quote! { #element.clip_path_name(#value) }
                    }
                    "offset_path" | "offsetPath" | "offset-path" => {
                        element = quote! { #element.offset_path(#value) }
                    }
                    "offset_distance" | "offsetDistance" | "offset-distance" => {
                        element = quote! { #element.offset_distance(#value) }
                    }
                    "offset_rotate" | "offsetRotate" | "offset-rotate" => {
                        element = quote! { #element.offset_rotate(#value) }
                    }
                    "transform_origin" | "transformOrigin" | "transform-origin" => {
                        element = quote! { #element.transform_origin(#value) }
                    }
                    "transform_style" | "transformStyle" | "transform-style" => {
                        element = quote! { #element.transform_style(#value) }
                    }
                    "transform_box" | "transformBox" | "transform-box" => {
                        element = quote! { #element.transform_box(#value) }
                    }
                    // Consumed by a dedicated pre-pass above, so reaching the fallback here would
                    // be a false "unknown attribute" report.
                    "name" if name_str == "Icon" || name_str == "AnimatedIcon" => {}
                    "src" if name_str == "Icon" => {}
                    "drag_reorder_handle"
                    | "dragReorderHandle"
                    | "drag_reorder_index"
                    | "dragReorderIndex" => {}
                    _ => {
                        if expr_reads_get(value) {
                            let idx = *node_counter;
                            *node_counter += 1;
                            let key = attr_str.clone();
                            dynamic_binds.push(quote! {
                                __rui_el = rui::app::reactive::bind_attr(
                                    __rui_el,
                                    format!("{}:{}:bind_attr:{}#{}", file!(), line!(), #key, #idx),
                                    move || (#value).to_string(),
                                    |el, v| rui::app::reactive::apply_bound_attr(el, #key, v),
                                );
                            });
                        } else {
                            let msg =
                                format!("unknown rui attribute `{attr_str}` on `<{name_str}>`");
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
            }

            let mut kid_extends = Vec::new();
            for child in &el.children {
                match child {
                    RsxNode::Expr(expr)
                        if is_svg_tag(&name_str) && name_str != "svg" && name_str != "Svg" =>
                    {
                        let text_tags = matches!(
                            name_str.as_str(),
                            "text"
                                | "tspan"
                                | "Tspan"
                                | "TSpan"
                                | "textPath"
                                | "TextPath"
                                | "title"
                                | "Title"
                                | "desc"
                                | "Desc"
                        );
                        let is_str_lit = matches!(
                            expr,
                            syn::Expr::Lit(syn::ExprLit {
                                lit: syn::Lit::Str(_),
                                ..
                            })
                        );
                        if is_str_lit {
                            element = quote! { #element.text_content(#expr) };
                        } else if text_tags {
                            element = quote! { #element.text_content((#expr).to_string()) };
                        } else {
                            let expanded_child =
                                expand_node(child, node_counter, in_reactive_boundary);
                            kid_extends.push(quote! {
                                __rui_kids.extend(rui::core::tree::build_ui_child(|| (#expanded_child)));
                            });
                        }
                    }
                    RsxNode::Expr(_) | RsxNode::Element(_) => {
                        let expanded_child = expand_node(child, node_counter, in_reactive_boundary);
                        kid_extends.push(quote! {
                            __rui_kids.extend(rui::core::tree::build_ui_child(|| (#expanded_child)));
                        });
                    }
                }
            }
            if !kid_extends.is_empty() {
                element = quote! {
                    {
                        let mut __rui_kids: Vec<rui::core::tree::Element> = Vec::new();
                        #(#kid_extends)*
                        #element.children(__rui_kids)
                    }
                };
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

/// `class="lit"` → `with_class_list`; `class={expr}` → same, via `AsRef<str>` (build-time snapshot).
/// Reactive `class={sig.get()}` is handled by `bind_class` before this runs.
fn quote_apply_class_list(value: &Expr) -> proc_macro2::TokenStream {
    if let Some(lit) = class_lit_str(value) {
        quote! {
            __rui_el = rui::style::with_class_list(__rui_el, #lit);
        }
    } else {
        quote! {
            {
                let __rui_class = #value;
                __rui_el = rui::style::with_class_list(
                    __rui_el,
                    ::std::convert::AsRef::<str>::as_ref(&__rui_class),
                );
            }
        }
    }
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
    out.push(quote_apply_class_list(value));
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

#[cfg(test)]
mod rsx_lowering_tests {
    use super::*;

    fn expand(src: &str) -> String {
        let root: RsxNode = syn::parse_str(src).unwrap_or_else(|e| panic!("{e} in {src}"));
        let mut c = 0usize;
        expand_node(&root, &mut c, false).to_string()
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
        let s =
            expand(r#"<DragProvider id="p" snap_fn={f} snap={g} snap_targets={t}></DragProvider>"#);
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
        let s = expand(
            r#"<DragProvider id="p"><DragOverlay rotate={45.0}></DragOverlay></DragProvider>"#,
        );
        assert!(s.contains("rotate"), "{s}");
        assert!(s.contains("style"), "{s}");
    }

    #[test]
    fn sortable_item_sets_also_droppable() {
        let s = expand(
            r#"<SortableItem id="row" index={i} list="todo"><Label>"r"</Label></SortableItem>"#,
        );
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
        let s =
            expand(r#"<Draggable id="a" restrict="corners-arena"><Label>"x"</Label></Draggable>"#);
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
        let s = expand(
            r#"<Draggable id="a"><DragIgnore><Button>"x"</Button></DragIgnore></Draggable>"#,
        );
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
        let s =
            expand(r#"<DragProvider id="p" sort_mode="swap" multi_drag={true}></DragProvider>"#);
        let compact = s.replace(' ', "");
        assert!(compact.contains("SortMode::parse"), "{s}");
        assert!(compact.contains("\"swap\""), "{s}");
        assert!(compact.contains("multi_drag(true)"), "{s}");
        let camel =
            expand(r#"<DragProvider id="p" sortMode="swap" multiDrag={true}></DragProvider>"#);
        let compact_camel = camel.replace(' ', "");
        assert!(compact_camel.contains("SortMode::parse"), "{camel}");
        assert!(compact_camel.contains("multi_drag(true)"), "{camel}");
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
        let s = expand(
            r#"<DragProvider id="p" scroll_sensitivity={30} scroll_speed={12}></DragProvider>"#,
        );
        assert!(s.contains("scroll_sensitivity"), "{s}");
        assert!(s.contains("scroll_speed"), "{s}");
        let camel = expand(
            r#"<DragProvider id="p" scrollSensitivity={28} scrollSpeed={10}></DragProvider>"#,
        );
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
        let s =
            expand(r#"<SortableList id="todo" direction="x"><Label>"x"</Label></SortableList>"#);
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
        let s =
            expand(r#"<DragProvider id="p" sensor_id="kbd" sensor={|_| false}></DragProvider>"#);
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
        let expanded = quote! { rui::core::tree::Element::new() };
        let hash = syn::LitStr::new("h", proc_macro2::Span::call_site());
        let s = wrap_ui_expansion(expanded, hash).to_string();
        assert!(s.contains("build_ui_root"), "{s}");
        assert!(s.contains("is_none"), "{s}");
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
        let blur =
            expand(r#"<DialogContent overlayBlur="4px" overlayOpacity=0.5></DialogContent>"#);
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
}

use std::sync::atomic::{AtomicUsize, Ordering};

use proc_macro::TokenStream;
use quote::{quote, quote_spanned};
use syn::parse::{Parse, ParseStream, Parser};
use syn::visit::Visit;
use syn::{Expr, Ident, Token, braced};

/// RSX 中省略 `id` 的 `<Input>`：编译期为每个标签分配序号，生成稳定字面量，避免多个 Input 共用空 `id` 导致信号冲突。
static INPUT_RSX_ANON_SEQ: AtomicUsize = AtomicUsize::new(0);
/// 同 Input：省略 `id` 的 `<Checkbox>` 仍生成唯一实例键（focus / Label `for`）；非受控布尔态走 Owner 作用域（WP-3.7）。
static CHECKBOX_RSX_ANON_SEQ: AtomicUsize = AtomicUsize::new(0);
/// 同 Checkbox：省略 `id` 的 `<Switch>` 生成唯一实例键。
static SWITCH_RSX_ANON_SEQ: AtomicUsize = AtomicUsize::new(0);
/// 同 Input：省略 `id` 的 `<Button>` 生成稳定实例键（focus / hover / template_id）。
/// Runtime `btn-N` 每次 rebuild 递增，会拆掉 AlertDialog 退场快照。
static BUTTON_RSX_ANON_SEQ: AtomicUsize = AtomicUsize::new(0);

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

/// CSS length / distance（`sideOffset` / blur / height…）：Rust 数值 `{4}` ≡ `"4px"`；无单位字符串 `"4"` = rem。
fn quote_length_f32(e: &Expr) -> proc_macro2::TokenStream {
    quote! { jadix::core::tree::IntoF32::into_f32(#e) }
}

/// 无单位纯数值（字重 / opacity / flex / leading 倍率 / progress 分数…）。
fn quote_unitless_f32(e: &Expr) -> proc_macro2::TokenStream {
    quote! { jadix::core::tree::IntoF32Raw::into_f32_raw(#e) }
}

/// Overlay `*ContentOpts.panel` / SubContent chrome from RSX `class` (typed to avoid E0282).
fn quote_slot_chrome(class: &proc_macro2::TokenStream) -> proc_macro2::TokenStream {
    quote! {{
        let __jadix_sc_class: Option<String> = #class;
        jadix::components::base::SlotChrome::from_opts(__jadix_sc_class, None)
    }}
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

fn is_shader_tag(name: &str) -> bool {
    matches!(name, "shader" | "Shader")
}

/// Valueless RSX flags: `<DragProvider scroll-lock-all>` ≡ `scroll_lock="all"`.
/// Drag edge-autoscroll only — not page wheel / pan / fling lock.
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

/// RSX `{ … }` child: a single expression, or a statement block (`throw_x(); view()`).
///
/// The outer braces are RSX delimiters (stripped). A complete inner expr is kept as-is
/// (`n.get()`, `{ |k| k }`, `{ { stmt; elem } }`). Leftover tokens after a partial expr
/// (typically `;`) are wrapped into a Rust block so `{ throw_x(); view() }` parses.
fn parse_rsx_brace_child(content: ParseStream) -> syn::Result<Expr> {
    use syn::parse::discouraged::Speculative;

    if content.is_empty() {
        return Err(content.error("empty `{}` expression child"));
    }
    let fork = content.fork();
    if let Ok(expr) = fork.parse::<Expr>() {
        if fork.is_empty() {
            content.advance_to(&fork);
            return Ok(expr);
        }
    }
    let inner: proc_macro2::TokenStream = content.parse()?;
    syn::parse2(quote! {{ #inner }})
        .map_err(|e| syn::Error::new(e.span(), format!("RSX `{{ … }}` statement block: {e}")))
}

impl Parse for RsxNode {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        // If it starts with `{`, it's an expression (or a statement block).
        if input.peek(syn::token::Brace) {
            let content;
            braced!(content in input);
            let expr = parse_rsx_brace_child(&content)?;
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
/// into `jadix::core::tree::Element` builder calls.
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
        jadix::core::tree::build_ui_root(|| {
            let mut __jadix_root: jadix::core::tree::Element = (#expanded).into();
            __jadix_root = __jadix_root.template_hash(#template_hash_lit);
            let __jadix_source = format!("{}:{}:{}", file!(), line!(), column!());
            if __jadix_root.template_id.is_none() {
                __jadix_root = __jadix_root.template_id(__jadix_source.clone());
            }
            __jadix_root.set_attr("jadix_ui_template_id", __jadix_source);
            __jadix_root
        })
    }
}

/// **可选糖**：少写一层 `launch` / `app().mount` 调用。默认教程请直接用
/// [`jadix::launch`] 或 [`jadix::app::app`]`.mount`（见 [`jadix::prelude`]）。
///
/// ```ignore
/// // 等价于 launch(|| ui! { <App title="My App" font_size={18}>…</App> })
/// #[jadix::main]
/// fn main() {
///     ui! {
///         <App title="My App" font_size={18}>
///             <Button>"保存"</Button>
///         </App>
///     }
/// }
///
/// // 等价于 app().title(…).mount(|| Root())；勿与框架 `<App>` 文档壳同名
/// #[jadix::main(title = "My App", font_size = 18)]
/// #[component]
/// fn Root() -> Element {
///     ui! { <Button>"保存"</Button> }
/// }
/// ```
///
/// `fn main`：最后一个表达式是每帧 UI 树；前面语句只执行一次。
/// 非 `main` 函数：生成 `fn main()` 并调用 [`jadix::launch`] 或 [`jadix::app::app`]`.mount`。
/// 包 `#[component]` 时，`#[jadix::main]` 写在外层。
#[proc_macro_attribute]
pub fn main(attr: TokenStream, item: TokenStream) -> TokenStream {
    match expand_jadix_main(attr.into(), item.into()) {
        Ok(tokens) => tokens.into(),
        Err(err) => err.to_compile_error().into(),
    }
}

struct JadixMainAttr {
    title: Option<syn::LitStr>,
    width: Option<syn::Expr>,
    height: Option<syn::Expr>,
    font: Option<syn::LitStr>,
    font_size: Option<syn::Expr>,
    font_weight: Option<syn::Expr>,
    frameless: bool,
    transparent: bool,
    crate_path: syn::Path,
}

fn parse_jadix_main_attr(attr: proc_macro2::TokenStream) -> syn::Result<JadixMainAttr> {
    let mut parsed = JadixMainAttr {
        title: None,
        width: None,
        height: None,
        font: None,
        font_size: None,
        font_weight: None,
        frameless: false,
        transparent: false,
        crate_path: syn::parse_quote!(::jadix),
    };
    if attr.is_empty() {
        return Ok(parsed);
    }
    let parser = syn::meta::parser(|meta| {
        if meta.path.is_ident("title") {
            parsed.title = Some(meta.value()?.parse()?);
        } else if meta.path.is_ident("width") {
            parsed.width = Some(meta.value()?.parse()?);
        } else if meta.path.is_ident("height") {
            parsed.height = Some(meta.value()?.parse()?);
        } else if meta.path.is_ident("font") || meta.path.is_ident("font_family") {
            parsed.font = Some(meta.value()?.parse()?);
        } else if meta.path.is_ident("font_size") {
            parsed.font_size = Some(meta.value()?.parse()?);
        } else if meta.path.is_ident("font_weight") {
            parsed.font_weight = Some(meta.value()?.parse()?);
        } else if meta.path.is_ident("frameless") {
            parsed.frameless = true;
        } else if meta.path.is_ident("transparent") {
            parsed.transparent = true;
        } else if meta.path.is_ident("crate_path") {
            let value = meta.value()?;
            let s: syn::LitStr = value.parse()?;
            parsed.crate_path = s.parse()?;
        } else {
            return Err(meta.error(
                "unknown #[jadix::main] argument; expected title, width, height, font, font_size, font_weight, frameless, transparent, crate_path",
            ));
        }
        Ok(())
    });
    parser.parse2(attr)?;
    Ok(parsed)
}

impl JadixMainAttr {
    fn has_window_config(&self) -> bool {
        self.title.is_some()
            || self.width.is_some()
            || self.height.is_some()
            || self.font.is_some()
            || self.font_size.is_some()
            || self.font_weight.is_some()
            || self.frameless
            || self.transparent
    }

    fn boot(
        &self,
        crate_path: &syn::Path,
        root: proc_macro2::TokenStream,
    ) -> proc_macro2::TokenStream {
        if !self.has_window_config() {
            return quote! { #crate_path::launch(#root) };
        }
        let mut chain = quote! { #crate_path::app::app() };
        if let Some(title) = &self.title {
            chain = quote! { #chain.title(#title) };
        }
        match (&self.width, &self.height) {
            (Some(w), Some(h)) => {
                chain = quote! { #chain.size(#w as f64, #h as f64) };
            }
            (Some(w), None) => {
                chain = quote! { #chain.width(#w as f64) };
            }
            (None, Some(h)) => {
                chain = quote! { #chain.height(#h as f64) };
            }
            (None, None) => {}
        }
        if let Some(font) = &self.font {
            chain = quote! { #chain.font(#font) };
        }
        if let Some(size) = &self.font_size {
            chain = quote! { #chain.font_size(#size) };
        }
        if let Some(weight) = &self.font_weight {
            chain = quote! { #chain.font_weight(#weight as u16) };
        }
        if self.frameless {
            chain = quote! { #chain.frameless() };
        }
        if self.transparent {
            chain = quote! { #chain.transparent(true) };
        }
        quote! { #chain.mount(#root) }
    }
}

fn stmt_as_root_expr(stmt: syn::Stmt) -> syn::Result<syn::Expr> {
    match stmt {
        syn::Stmt::Expr(expr, _) => Ok(expr),
        syn::Stmt::Macro(m) => Ok(syn::Expr::Macro(syn::ExprMacro {
            attrs: m.attrs,
            mac: m.mac,
        })),
        other => Err(syn::Error::new_spanned(
            other,
            "#[jadix::main] fn main() 的最后一个表达式必须是 UI 树，例如 `ui! { <App>...</App> }`",
        )),
    }
}

fn expand_jadix_main(
    attr: proc_macro2::TokenStream,
    item: proc_macro2::TokenStream,
) -> syn::Result<proc_macro2::TokenStream> {
    let args = parse_jadix_main_attr(attr)?;
    let func: syn::ItemFn = syn::parse2(item)?;
    if func.sig.asyncness.is_some() {
        return Err(syn::Error::new_spanned(
            func.sig.asyncness,
            "#[jadix::main] does not wrap async fn; run setup then `launch(|| ui! { <App/> })`",
        ));
    }
    let crate_path = &args.crate_path;
    let name = &func.sig.ident;
    let is_component = func.attrs.iter().any(|a| a.path().is_ident("component"));

    if name == "main" {
        let mut stmts = func.block.stmts.clone();
        let last = stmts.pop().ok_or_else(|| {
            syn::Error::new_spanned(
                &func.sig.ident,
                "#[jadix::main] fn main() 需要以 UI 表达式结尾，例如 `ui! { <App>...</App> }`",
            )
        })?;
        let last_expr = stmt_as_root_expr(last)?;
        let boot = args.boot(crate_path, quote! { || #last_expr });
        let vis = &func.vis;
        let sig = &func.sig;
        let attrs = &func.attrs;
        Ok(quote! {
            #(#attrs)*
            #vis #sig {
                #(#stmts)*
                #boot
            }
        })
    } else {
        let root = if is_component {
            quote! { || #name(::core::default::Default::default()) }
        } else {
            quote! { #name }
        };
        let boot = args.boot(crate_path, root);
        Ok(quote! {
            #func
            fn main() {
                #boot
            }
        })
    }
}

#[cfg(test)]
mod jadix_main_expand_tests {
    use super::*;

    fn compact(tokens: proc_macro2::TokenStream) -> String {
        tokens
            .to_string()
            .chars()
            .filter(|c| !c.is_whitespace())
            .collect()
    }

    #[test]
    fn main_fn_last_expr_goes_to_launch() {
        let tokens = expand_jadix_main(
            quote! {},
            quote! {
                fn main() {
                    setup();
                    ui! { <App></App> }
                }
            },
        )
        .unwrap();
        let s = compact(tokens);
        assert!(s.contains("setup();"), "{s}");
        assert!(s.contains("::jadix::launch"), "{s}");
        assert!(s.contains("ui!"), "{s}");
        assert!(s.contains("<App></App>"), "{s}");
    }

    #[test]
    fn main_attr_window_uses_app_mount() {
        let tokens = expand_jadix_main(
            quote! { title = "Hi", font_size = 18 },
            quote! {
                fn main() {
                    ui! { <Button>"x"</Button> }
                }
            },
        )
        .unwrap();
        let s = compact(tokens);
        assert!(s.contains("::jadix::app::app()"), "{s}");
        assert!(s.contains(".title(\"Hi\")"), "{s}");
        assert!(s.contains(".font_size(18)"), "{s}");
        assert!(s.contains(".mount("), "{s}");
    }

    #[test]
    fn component_root_generates_main_and_default_props() {
        let tokens = expand_jadix_main(
            quote! {},
            quote! {
                #[component]
                fn App() -> Element {
                    el
                }
            },
        )
        .unwrap();
        let s = compact(tokens);
        assert!(s.contains("fnmain()"), "{s}");
        assert!(s.contains("Default::default()"), "{s}");
        assert!(s.contains("::jadix::launch"), "{s}");
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
/// - Default (non-`reactive`) bodies run under [`isolate_reads`] so a naked `signal.get()`
///   does **not** subscribe a parent effect (D2 / N.3). Set `JADIX_COMPONENT_ISOLATE_DEFAULT=0`
///   to roll back. Prefer `ui!` auto `bind_*` / `<Show>` / `<For>` — isolate discards a
///   missed bind; it is not itself a bind.
/// - `Signal` / closure / `Element` props are **excluded** from the PartialEq snapshot (static
///   props only trigger subtree refresh).
/// - `#[component(reactive)]` already isolates via `reactive_component`; do not double-wrap.
///   `reactive_component_live` stays un-isolated.
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
    let mut crate_path = quote! { jadix };
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
                let mut __el = #crate_path::core::reactivity::component_isolated_body(|| { #body });
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

/// `id` / `scope` for reactive islands. Empty / whitespace string literals are
/// treated as omitted (`element_identity.md` — empty string is not an identity).
fn rsx_island_anchor(
    id_expr: Option<&Expr>,
    kind: &str,
    node_counter: &mut usize,
) -> proc_macro2::TokenStream {
    let use_generated = match id_expr {
        None => true,
        Some(expr) => expr_is_blank_str_lit(expr),
    };
    if use_generated {
        let idx = *node_counter;
        *node_counter += 1;
        quote! { format!("{}:{}:{}#{}", file!(), line!(), #kind, #idx) }
    } else {
        let expr = id_expr.expect("non-generated island id has an expr");
        quote! { #expr }
    }
}

fn expr_is_blank_str_lit(expr: &Expr) -> bool {
    match expr {
        Expr::Lit(syn::ExprLit {
            lit: syn::Lit::Str(s),
            ..
        }) => s.value().trim().is_empty(),
        Expr::Group(g) => expr_is_blank_str_lit(&g.expr),
        Expr::Paren(p) => expr_is_blank_str_lit(&p.expr),
        _ => false,
    }
}

/// Best-effort AST heuristic for Solid-style signal reads.
///
/// Tightened vs a bare zero-arg `.get()` scan:
/// - receiver must be a path / field / index / paren / reference chain (not a call result);
/// - `.peek()` / `.get_untracked()` are non-subscribing (ignored — different method names);
/// - ignores `.get(...)` with arguments (e.g. `HashMap::get`);
/// - does not walk closures (event handlers, `untrack(|| …)`, `move ||`);
/// - skips `untrack(...)` call arguments;
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
                Expr::Cast(c) => Self::receiver_ok(&c.expr),
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

        fn path_ends_with_ident(expr: &Expr, name: &str) -> bool {
            match expr {
                Expr::Path(p) => p.path.segments.last().is_some_and(|s| s.ident == name),
                Expr::Paren(p) => Self::path_ends_with_ident(&p.expr, name),
                Expr::Group(g) => Self::path_ends_with_ident(&g.expr, name),
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

        fn visit_expr_closure(&mut self, _node: &syn::ExprClosure) {
            // Event handlers / `untrack(|| …)` / `move ||` compute: inner `.get()` is
            // not a reason to emit a leaf bind on the outer expression.
        }

        fn visit_expr_call(&mut self, node: &syn::ExprCall) {
            if self.found {
                return;
            }
            if Self::path_ends_with_ident(&node.func, "untrack") {
                return;
            }
            syn::visit::visit_expr_call(self, node);
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

fn peel_expr(expr: &Expr) -> &Expr {
    match expr {
        Expr::Paren(p) => peel_expr(&p.expr),
        Expr::Group(g) => peel_expr(&g.expr),
        other => other,
    }
}

fn expr_contains_ui_macro(expr: &Expr) -> bool {
    struct Finder {
        found: bool,
    }
    impl Visit<'_> for Finder {
        fn visit_expr_macro(&mut self, node: &syn::ExprMacro) {
            if self.found {
                return;
            }
            if node
                .mac
                .path
                .segments
                .last()
                .is_some_and(|s| s.ident == "ui")
            {
                self.found = true;
                return;
            }
            syn::visit::visit_expr_macro(self, node);
        }
    }
    let mut finder = Finder { found: false };
    finder.visit_expr(expr);
    finder.found
}

/// Tracked `if sig.get() { ui!(…) } else { … }` as a child must be `<Show>` / `<Match>`.
fn expr_is_reactive_structural_branch(expr: &Expr) -> bool {
    match peel_expr(expr) {
        Expr::If(_) | Expr::Match(_) => expr_reads_get(expr) && expr_contains_ui_macro(expr),
        _ => false,
    }
}

fn expand_node(
    node: &RsxNode,
    node_counter: &mut usize,
    in_reactive_boundary: bool,
) -> proc_macro2::TokenStream {
    match node {
        RsxNode::Expr(expr) => {
            if expr_is_reactive_structural_branch(expr) {
                return quote! {
                    compile_error!(
                        "tracked `if` / `match` that builds UI must use `<Show>` / `<Match>`, not a child `{ if sig.get() { ui!(…) } }`"
                    )
                };
            }
            // Solid-style: dynamic text children that read `.get()` become in-place bindings
            // instead of subscribing the surrounding (often root) effect.
            if expr_reads_get(expr) {
                let idx = *node_counter;
                *node_counter += 1;
                return quote! {
                    jadix::app::reactive::bind_text(
                        format!("{}:{}:bind#{}", file!(), line!(), #idx),
                        move || {
                            let __jadix_text = #expr;
                            __jadix_text.to_string()
                        },
                    )
                };
            }
            quote! { #expr }
        }
        RsxNode::Element(el) => {
            // HTML / Solid 风格别名 → 规范名（`<Img>`/`<img>` → `Image`；`<div>`/`<Div>` → `Box`）。
            let name_str = match el.name.to_string().as_str() {
                "Img" | "img" => "Image".to_string(),
                "audio" => "Audio".to_string(),
                "video" => "Video".to_string(),
                "div" | "Div" => "Box".to_string(),
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
                || name_str == "ErrorBoundary"
                || name_str == "Suspense"
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
                    return quote! {{
                        let __jadix_prg_class: Option<String> = #class_ts;
                        jadix::components::base::SlotChrome::from_opts(__jadix_prg_class, None).apply(
                            jadix::components::progress_scope(
                                #id_ts,
                                #value_ts,
                                #default_ts,
                                #orientation_ts,
                                || #child_el,
                            )
                        )
                    }};
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
                            quote! { jadix::core::tree::parse_bool_like(#c) }
                        }
                    } else {
                        quote! { true }
                    };
                    return quote! {
                        jadix::components::stepper_scope(
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
                        quote! { 32.0f32 }
                    };
                    let line_ts = if let Some(v) = line_expr {
                        quote_unitless_f32(v)
                    } else {
                        quote! { 2.0f32 }
                    };
                    return quote! {
                        jadix::components::timeline_scope(
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
                    let variant_ts = if let Some(v) = variant_expr {
                        quote! { jadix::components::BubbleVariant::from(#v) }
                    } else {
                        quote! { jadix::components::BubbleVariant::Default }
                    };
                    let align_ts = if let Some(a) = align_expr {
                        quote! { jadix::components::BubbleAlign::from(#a) }
                    } else {
                        quote! { jadix::components::BubbleAlign::Start }
                    };
                    let class_ts = if let Some(c) = class_expr {
                        quote! { Some((#c).to_string()) }
                    } else {
                        quote! { None }
                    };
                    return quote! {{
                        let __jadix_bub_class: Option<String> = #class_ts;
                        jadix::components::base::SlotChrome::from_opts(__jadix_bub_class, None).apply(
                            jadix::components::bubble_scope(
                                #variant_ts,
                                #align_ts,
                                || #child_el,
                            )
                        )
                    }};
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
                        quote! { format!("{}:{}:Avatar#{}", file!(), line!(), #idx) }
                    };
                    let size_ts = if let Some(s) = size_expr {
                        quote! { jadix::components::AvatarSize::from(#s) }
                    } else {
                        quote! { jadix::components::AvatarSize::Default }
                    };
                    let shape_ts = if let Some(s) = shape_expr {
                        quote! { jadix::components::AvatarShape::from(#s) }
                    } else {
                        quote! { jadix::components::AvatarShape::Circle }
                    };
                    let class_ts = if let Some(c) = class_expr {
                        quote! { Some((#c).to_string()) }
                    } else {
                        quote! { None }
                    };
                    return quote! {
                        jadix::components::avatar_scope(
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
                            quote! { jadix::core::tree::parse_bool_like(#m) }
                        }
                    } else {
                        quote! { false }
                    };
                    let disabled_ts = if let Some(d) = disabled_expr {
                        if let Some(b) = expr_as_bool_compat(d) {
                            quote! { #b }
                        } else {
                            quote! { jadix::core::tree::parse_bool_like(#d) }
                        }
                    } else {
                        quote! { false }
                    };
                    let class_ts = if let Some(c) = class_expr {
                        quote! { Some((#c).to_string()) }
                    } else {
                        quote! { None }
                    };
                    return quote! {{
                        let __jadix_acc_class: Option<String> = #class_ts;
                        jadix::components::base::SlotChrome::from_opts(__jadix_acc_class, None).apply(
                            jadix::components::accordion_scope(
                                #id_ts,
                                #value_ts,
                                #default_ts.unwrap_or_default(),
                                #multiple_ts,
                                #disabled_ts,
                                || #child_el,
                            )
                        )
                    }};
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
                        quote! { jadix::components::QuestionnaireShortcuts::parse(&(#s).to_string()) }
                    } else {
                        quote! { jadix::components::QuestionnaireShortcuts::None }
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
                        quote! { Some(jadix::components::questionnaire_submit_fn(#s)) }
                    } else {
                        quote! { None }
                    };
                    let on_item_change_ts = if let Some(s) = on_item_change_expr {
                        quote! { Some(jadix::components::questionnaire_item_change_fn(#s)) }
                    } else {
                        quote! { None }
                    };
                    return quote! {{
                        let __jadix_q_class: Option<String> = #class_ts;
                        jadix::components::base::SlotChrome::from_opts(__jadix_q_class, None).apply(
                            jadix::components::questionnaire_scope(
                                #id_ts,
                                #item_ts,
                                #default_ts,
                                #shortcuts_ts,
                                #items_ts,
                                #on_submit_ts,
                                #on_item_change_ts,
                                || #child_el,
                            )
                        )
                    }};
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
                        quote! { format!("{}:{}:TreeView#{}", file!(), line!(), #idx) }
                    };
                    let variant_ts = if let Some(v) = variant_expr {
                        quote! { jadix::components::TreeViewVariant::from((#v).to_string()) }
                    } else {
                        quote! { jadix::components::TreeViewVariant::default() }
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
                            quote! { Some(jadix::core::tree::parse_bool_like(#e)) }
                        }
                    } else {
                        quote! { None }
                    };
                    let chrome_expr = get_attr("chrome");
                    let chrome_ts = if let Some(c) = chrome_expr {
                        quote! { jadix::components::TreeViewChrome::from((#c).to_string()) }
                    } else {
                        quote! { jadix::components::TreeViewChrome::Default }
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
                    return quote! {{
                        let __jadix_tv_class: Option<String> = #class_ts;
                        jadix::components::base::SlotChrome::from_opts(__jadix_tv_class, None).apply(
                            jadix::components::tree_view_scope(
                                #id_ts,
                                #variant_ts,
                                #expanded_ts,
                                #expanded_storage_key_ts,
                                #row_height_ts,
                                #height_ts,
                                #exclusive_ts,
                                #chrome_ts,
                                #selected_ts,
                                None,
                                ::std::vec::Vec::new(),
                                || #child_el,
                            )
                        )
                    }};
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
                            quote! { jadix::core::tree::parse_bool_like(#d) }
                        }
                    } else {
                        quote! { false }
                    };
                    let disabled_ts = if let Some(d) = disabled_expr {
                        if let Some(b) = expr_as_bool_compat(d) {
                            quote! { #b }
                        } else {
                            quote! { jadix::core::tree::parse_bool_like(#d) }
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
                            quote! { jadix::core::tree::parse_bool_like(#e) }
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
                    return quote! {{
                        let __jadix_col_class: Option<String> = #class_ts;
                        jadix::components::base::SlotChrome::from_opts(__jadix_col_class, None).apply(
                            jadix::components::collapsible_scope(
                                #id_ts,
                                #open_ts,
                                #default_ts,
                                #disabled_ts,
                                #exclusive_ts,
                                #group_ts,
                                || #child_el,
                            )
                        )
                    }};
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
                        quote! { format!("{}:{}:MessageScrollerProvider#{}", file!(), line!(), #idx) }
                    };
                    let auto_ts = if let Some(a) = auto_expr {
                        if let Some(b) = expr_as_bool_compat(a) {
                            quote! { #b }
                        } else {
                            quote! { jadix::core::tree::parse_bool_like(#a) }
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
                    return quote! {{
                        let __jadix_ms_class: Option<String> = #class_ts;
                        jadix::components::base::SlotChrome::from_opts(__jadix_ms_class, None).apply(
                            jadix::components::message_scroller_scope(
                                #id_ts,
                                #auto_ts,
                                #pos_ts,
                                #threshold_ts,
                                #peek_ts,
                                || #child_el,
                            )
                        )
                    }};
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
                            quote! { jadix::core::tree::parse_bool_like(#d) }
                        }
                    } else {
                        quote! { false }
                    };
                    let invalid_ts = if let Some(d) = invalid_expr {
                        if let Some(b) = expr_as_bool_compat(d) {
                            quote! { #b }
                        } else {
                            quote! { jadix::core::tree::parse_bool_like(#d) }
                        }
                    } else {
                        quote! { false }
                    };
                    let pattern_ts = if let Some(p) = pattern_expr {
                        quote! {
                            jadix::components::InputOtpPattern::parse(&(#p).to_string())
                        }
                    } else {
                        quote! { jadix::components::InputOtpPattern::Digits }
                    };
                    let class_ts = if let Some(c) = class_expr {
                        quote! { Some((#c).to_string()) }
                    } else {
                        quote! { None }
                    };
                    return quote! {{
                        let __jadix_otp_class: Option<String> = #class_ts;
                        jadix::components::base::SlotChrome::from_opts(__jadix_otp_class, None).apply(
                            jadix::components::input_otp_scope(
                                #id_ts,
                                #max_ts,
                                #value_ts,
                                #disabled_ts,
                                #invalid_ts,
                                #pattern_ts,
                                || #child_el,
                            )
                        )
                    }};
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
                    return quote! {{
                        let __jadix_rz_class: Option<String> = #class_ts;
                        jadix::components::base::SlotChrome::from_opts(__jadix_rz_class, None).apply(
                            jadix::components::resizable_panel_group_scope(
                                #id_ts,
                                #ori_ts,
                                #height_ts,
                                #sizes_ts,
                                || #child_el,
                            )
                        )
                    }};
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
                        quote! { jadix::core::tree::Element::new() }
                    } else {
                        // Fragment so Trigger can sit beside Content without an extra column.
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
                        quote! { format!("{}:{}:Popover#{}", file!(), line!(), #idx) }
                    };
                    let open_ts = if let Some(open) = open_expr {
                        quote! { Some(#open) }
                    } else {
                        quote! { None }
                    };
                    return quote! {
                        jadix::components::popover_scope(#id_ts, #open_ts, || #child_el)
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
                        quote! { format!("{}:{}:AnchoredOverlay#{}", file!(), line!(), #idx) }
                    };
                    let open_ts = if let Some(open) = open_expr {
                        quote! { Some(#open) }
                    } else {
                        quote! { None }
                    };
                    return quote! {
                        jadix::utils::anchored_overlay::anchored_overlay_scope(#id_ts, #open_ts, || #child_el)
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
                        .map(|e| quote! { jadix::utils::drag::Axis::parse(&(#e)) })
                        .unwrap_or_else(|| quote! { jadix::utils::drag::Axis::Xy });
                    let restrict_end_only = get_attr("restrict_end_only")
                        .or_else(|| get_attr("restrictEndOnly"))
                        .map(|e| quote! { #e })
                        .unwrap_or_else(|| quote! { false });
                    let restrict_anchor = get_attr("restrict_anchor")
                        .or_else(|| get_attr("restrictAnchor"))
                        .map(|e| quote! { jadix::utils::drag::RestrictAnchor::parse(&(#e)) })
                        .unwrap_or_else(|| quote! { jadix::utils::drag::RestrictAnchor::FULL });
                    let restrict_overflow = get_attr("restrict_overflow")
                        .or_else(|| get_attr("restrictOverflow"))
                        .map(|e| quote! { jadix::utils::drag::RestrictOverflow::parse(&(#e)) })
                        .unwrap_or_else(
                            || quote! { jadix::utils::drag::RestrictOverflow::Visible },
                        );
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
                        .map(|e| quote! { jadix::utils::drag::Activation::parse(&(#e)) })
                        .unwrap_or_else(|| quote! { jadix::utils::drag::Activation::And });
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
                        expanded_children.push(expand_node(
                            child,
                            node_counter,
                            in_reactive_boundary,
                        ));
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
                            let mut __b = jadix::utils::drag::drag_scope(#id_ts);
                            __b = __b.collision(jadix::utils::drag::CollisionAlg::parse(&(#collision)));
                            __b = __b.axis(jadix::utils::drag::Axis::parse(&(#axis)));
                            __b = __b.start_axis(#start_axis);
                            __b = __b.threshold(#threshold);
                            __b = __b.restrict(jadix::utils::drag::Restrict::parse(&(#restrict)));
                            __b = __b.restrict_end_only(#restrict_end_only);
                            __b = __b.restrict_anchor(#restrict_anchor);
                            __b = __b.restrict_overflow(#restrict_overflow);
                            __b = __b.delay_ms(#delay_ms);
                            __b = __b.activation(#activation);
                            __b = __b.empty_insert(#empty_insert);
                            __b = __b.autoscroll(#autoscroll);
                            #scroll_sensitivity_ts
                            #scroll_speed_ts
                            __b = __b.scroll_lock(jadix::utils::drag::ScrollLock::parse(&(#scroll_lock)));
                            __b = __b.animation(#animation);
                            __b = __b.sort_mode(jadix::utils::drag::SortMode::parse(&(#sort_mode)));
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
                        quote! { format!("{}:{}:ViewportModal#{}", file!(), line!(), #idx) }
                    };
                    let open_ts = if let Some(open) = open_expr {
                        quote! { Some(#open) }
                    } else {
                        quote! { None }
                    };
                    return quote! {
                        jadix::utils::viewport_modal::viewport_modal_scope(
                            #id_ts,
                            #open_ts,
                            jadix::utils::viewport_modal::ShellOverlayOpts {
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
                        quote! { jadix::core::tree::Element::new() }
                    } else {
                        // Fragment so Trigger can sit beside Content without an extra column.
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
                        quote! { format!("{}:{}:HoverCard#{}", file!(), line!(), #idx) }
                    };
                    let open_ts = if let Some(open) = open_expr {
                        quote! { Some(#open) }
                    } else {
                        quote! { None }
                    };
                    return quote! {
                        jadix::components::hover_card_scope(#id_ts, #open_ts, || #child_el)
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
                        quote! { jadix::core::tree::Element::new() }
                    } else {
                        // Fragment so Trigger can sit in parent HStack/VStack without an extra column.
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
                        quote! { format!("{}:{}:Dialog#{}", file!(), line!(), #idx) }
                    };
                    let open_ts = if let Some(open) = open_expr {
                        quote! { Some(#open) }
                    } else {
                        quote! { None }
                    };
                    return quote! {
                        jadix::components::dialog_scope(#id_ts, #open_ts, || #child_el)
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
                        quote! { format!("{}:{}:AlertDialog#{}", file!(), line!(), #idx) }
                    };
                    let open_ts = if let Some(open) = open_expr {
                        quote! { Some(#open) }
                    } else {
                        quote! { None }
                    };
                    return quote! {
                        jadix::components::alert_dialog_scope(#id_ts, #open_ts, || #child_el)
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
                        quote! { jadix::core::tree::Element::new() }
                    } else {
                        // Fragment so Trigger can sit in parent HStack/VStack without an extra column.
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
                        quote! { format!("{}:{}:Sheet#{}", file!(), line!(), #idx) }
                    };
                    let open_ts = if let Some(open) = open_expr {
                        quote! { Some(#open) }
                    } else {
                        quote! { None }
                    };
                    return quote! {
                        jadix::components::sheet_scope(#id_ts, #open_ts, || #child_el)
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
                        quote! { format!("{}:{}:Drawer#{}", file!(), line!(), #idx) }
                    };
                    let open_ts = if let Some(open) = open_expr {
                        quote! { Some(#open) }
                    } else {
                        quote! { None }
                    };
                    return quote! {
                        jadix::components::drawer_scope(#id_ts, #open_ts, || #child_el)
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
                        jadix::components::command_dialog_scope(
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
                        jadix::components::command_overlay_scope(
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
                        jadix::components::command_scope(
                            #id_ts,
                            #open_ts,
                            #placeholder_ts,
                            jadix::components::CommandSurface::Inline,
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
                        quote! { format!("{}:{}:Tooltip#{}", file!(), line!(), #idx) }
                    };
                    let open_ts = if let Some(open) = open_expr {
                        quote! { Some(#open) }
                    } else {
                        quote! { None }
                    };
                    return quote! {
                        jadix::components::tooltip_scope(#id_ts, #open_ts, || #child_el)
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
                            let __jadix_sidebar_variant: Option<String> = #variant_ts;
                            let mut __jadix_el = jadix::components::sidebar_provider_scope(
                                #id_ts,
                                #open_ts,
                                #default_open_ts,
                                __jadix_sidebar_variant.as_deref(),
                                || #child_el,
                            );
                            // Typed Option: bare `if let Some = None` is E0282 in gallery `ui!`.
                            let __jadix_sidebar_class: Option<String> = #class_ts;
                            if let Some(ref __c) = __jadix_sidebar_class {
                                if !__c.trim().is_empty() {
                                    __jadix_el = jadix::style::with_class_list(__jadix_el, __c);
                                }
                            }
                            __jadix_el
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
                        quote! { format!("{}:{}:ContextMenu#{}", file!(), line!(), #idx) }
                    };
                    let open_ts = if let Some(open) = open_expr {
                        quote! { Some(#open) }
                    } else {
                        quote! { None }
                    };
                    return quote! {
                        jadix::components::context_menu_scope(#id_ts, #open_ts, || #child_el)
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
                        quote! { format!("{}:{}:DropdownMenu#{}", file!(), line!(), #idx) }
                    };
                    let open_ts = if let Some(open) = open_expr {
                        quote! { Some(#open) }
                    } else {
                        quote! { None }
                    };
                    return quote! {
                        jadix::components::dropdown_menu_scope(#id_ts, #open_ts, || #child_el)
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
                        quote! { format!("{}:{}:Menubar#{}", file!(), line!(), #idx) }
                    };
                    return quote! {
                        jadix::components::menubar_scope(#id_ts, || #child_el)
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
                        jadix::components::navigation_menu_scope(#id_ts, #viewport_ts, || #child_el)
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
                    return quote! {{
                        let __jadix_nmi_class: Option<String> = #class_ts;
                        jadix::components::base::SlotChrome::from_opts(__jadix_nmi_class, None).apply(
                            jadix::components::navigation_menu_item(#value_ts, || #child_el)
                        )
                    }};
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
                        jadix::components::menubar_menu(#id_ts, #open_ts, || #child_el)
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
                        quote! { Some((#c).to_string()) }
                    } else {
                        quote! { None }
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
                    return quote! {{
                        let __jadix_sel_class: Option<String> = #class_ts;
                        jadix::components::base::SlotChrome::from_opts(__jadix_sel_class, None).apply(
                            jadix::components::select_scope(
                                #id_ts,
                                #open_ts,
                                #value_ts,
                                #placeholder_ts,
                                String::new(),
                                #content_class_ts,
                                #scroll_selected_ts,
                                || #child_el,
                            )
                        )
                    }};
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
                            quote! { jadix::core::tree::parse_bool_like(#d) }
                        }
                    } else {
                        quote! { false }
                    };
                    return quote! {
                        jadix::components::radio_group_scope(
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
                            quote! { jadix::core::tree::parse_bool_like(#m) }
                        }
                    } else {
                        quote! { false }
                    };
                    let disabled_ts = if let Some(d) = disabled_expr {
                        if let Some(b) = expr_as_bool_compat(d) {
                            quote! { #b }
                        } else {
                            quote! { jadix::core::tree::parse_bool_like(#d) }
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
                        quote! { jadix::components::parse_toggle_group_spacing(#s) }
                    } else {
                        quote! { 2u8 }
                    };
                    let joined_ts = if let Some(j) = joined_expr {
                        if let Some(b) = expr_as_bool_compat(j) {
                            quote! { #b }
                        } else {
                            quote! { jadix::core::tree::parse_bool_like(#j) }
                        }
                    } else if let Some(l) = layout_expr {
                        quote! { jadix::components::ToggleGroupLayout::from((#l).to_string()).is_joined() }
                    } else {
                        quote! { false }
                    };
                    let required_ts = if let Some(r) = required_expr {
                        if let Some(b) = expr_as_bool_compat(r) {
                            quote! { #b }
                        } else {
                            quote! { jadix::core::tree::parse_bool_like(#r) }
                        }
                    } else {
                        quote! { false }
                    };
                    let class_ts = if let Some(c) = class_expr {
                        quote! { Some((#c).to_string()) }
                    } else {
                        quote! { None }
                    };
                    return quote! {{
                        let __jadix_tg_class: Option<String> = #class_ts;
                        jadix::components::base::SlotChrome::from_opts(__jadix_tg_class, None).apply(
                            jadix::components::toggle_group_scope(
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
                                || #child_el,
                            )
                        )
                    }};
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
                            quote! { jadix::core::tree::parse_bool_like(#d) }
                        }
                    } else {
                        quote! { false }
                    };
                    return quote! {
                        jadix::components::tabs_scope(
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
                            quote! { jadix::core::tree::parse_bool_like(#d) }
                        }
                    } else {
                        quote! { true }
                    };
                    let seamless_ts = if let Some(d) = seamless_expr {
                        if let Some(b) = expr_as_bool_compat(d) {
                            quote! { Some(#b) }
                        } else {
                            quote! { Some(jadix::core::tree::parse_bool_like(#d)) }
                        }
                    } else {
                        quote! { None }
                    };
                    let snap_ts = if let Some(d) = snap_expr {
                        if let Some(b) = expr_as_bool_compat(d) {
                            quote! { #b }
                        } else {
                            quote! { jadix::core::tree::parse_bool_like(#d) }
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
                            quote! { Some(jadix::core::tree::parse_bool_like(#d)) }
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
                        jadix::components::carousel_scope(
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
                        jadix::components::tabs_content((#value_expr).to_string(), move || #child_el)
                    };
                }

                if name_str == "AccordionContent" {
                    let class_expr = get_attr("class");
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
                    let class_ts = if let Some(c) = class_expr {
                        quote! { Some((#c).to_string()) }
                    } else {
                        quote! { None }
                    };
                    return quote! {{
                        let __jadix_accc_class: Option<String> = #class_ts;
                        jadix::components::base::SlotChrome::from_opts(__jadix_accc_class, None).apply(
                            jadix::components::accordion_content(move || #child_el)
                        )
                    }};
                }

                if name_str == "CollapsibleContent" {
                    let class_expr = get_attr("class");
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
                    let class_ts = if let Some(c) = class_expr {
                        quote! { Some((#c).to_string()) }
                    } else {
                        quote! { None }
                    };
                    return quote! {{
                        let __jadix_colc_class: Option<String> = #class_ts;
                        jadix::components::base::SlotChrome::from_opts(__jadix_colc_class, None).apply(
                            jadix::components::collapsible_content(move || #child_el)
                        )
                    }};
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
                            quote! { jadix::core::tree::parse_bool_like(#d) }
                        }
                    } else {
                        quote! { false }
                    };
                    let class_ts = if let Some(c) = class_expr {
                        quote! { Some((#c).to_string()) }
                    } else {
                        quote! { None }
                    };
                    return quote! {{
                        let __jadix_tvi_class: Option<String> = #class_ts;
                        jadix::components::base::SlotChrome::from_opts(__jadix_tvi_class, None).apply(
                            jadix::components::tree_view_item_scope(
                                #id_ts,
                                #label_ts,
                                #disabled_ts,
                                || #child_el,
                            )
                        )
                    }};
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
                    let value_ts = quote! { (#value_expr).to_string() };
                    let disabled_ts = if let Some(d) = disabled_expr {
                        if let Some(b) = expr_as_bool_compat(d) {
                            quote! { #b }
                        } else {
                            quote! { jadix::core::tree::parse_bool_like(#d) }
                        }
                    } else {
                        quote! { false }
                    };
                    let class_ts = if let Some(c) = class_expr {
                        quote! { Some((#c).to_string()) }
                    } else {
                        quote! { None }
                    };
                    return quote! {{
                        let __jadix_acci_class: Option<String> = #class_ts;
                        jadix::components::base::SlotChrome::from_opts(__jadix_acci_class, None).apply(
                            jadix::components::accordion_item_scope(
                                #value_ts,
                                #disabled_ts,
                                || #child_el,
                            )
                        )
                    }};
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
                    let name_ts = quote! { (#name_expr).to_string() };
                    let required_ts = if let Some(v) = required_expr {
                        if let Some(b) = expr_as_bool_compat(v) {
                            quote! { #b }
                        } else {
                            quote! { jadix::core::tree::parse_bool_like(#v) }
                        }
                    } else {
                        quote! { false }
                    };
                    let multiple_ts = if let Some(v) = multiple_expr {
                        if let Some(b) = expr_as_bool_compat(v) {
                            quote! { #b }
                        } else {
                            quote! { jadix::core::tree::parse_bool_like(#v) }
                        }
                    } else {
                        quote! { false }
                    };
                    let disabled_ts = if let Some(d) = disabled_expr {
                        if let Some(b) = expr_as_bool_compat(d) {
                            quote! { #b }
                        } else {
                            quote! { jadix::core::tree::parse_bool_like(#d) }
                        }
                    } else {
                        quote! { false }
                    };
                    let invalid_ts = if let Some(v) = invalid_expr {
                        if let Some(b) = expr_as_bool_compat(v) {
                            quote! { #b }
                        } else {
                            quote! { jadix::core::tree::parse_bool_like(#v) }
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
                        quote! { Some(jadix::components::questionnaire_status_change_fn(#s)) }
                    } else {
                        quote! { None }
                    };
                    return quote! {{
                        let __jadix_qi_class: Option<String> = #class_ts;
                        jadix::components::base::SlotChrome::from_opts(__jadix_qi_class, None).apply(
                            jadix::components::questionnaire_item_scope(
                                #name_ts,
                                #required_ts,
                                #multiple_ts,
                                #disabled_ts,
                                #invalid_ts,
                                #on_status_ts,
                                || #child_el,
                            )
                        )
                    }};
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
                    let panel = quote_slot_chrome(&class);
                    return quote! {
                        jadix::components::dialog_content(
                            jadix::components::DialogContentOpts {
                                show_close: #show_close,
                                overlay_color: #overlay_color,
                                overlay_opacity: #overlay_opacity,
                                close_on_overlay_click: #close_on_overlay,
                                overlay_blur: #overlay_blur,
                                hug_content: false,
                                panel: #panel,
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
                        .map(|e| quote! { jadix::components::AlertDialogSize::from(#e) })
                        .unwrap_or_else(|| {
                            quote! { jadix::components::AlertDialogSize::Default }
                        });
                    let variant_static = get_attr("variant")
                        .filter(|e| !expr_reads_get(e))
                        .map(|e| quote! { jadix::components::AlertDialogVariant::from(#e) })
                        .unwrap_or_else(|| {
                            quote! { jadix::components::AlertDialogVariant::Default }
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
                    let panel = quote_slot_chrome(&class);
                    return quote! {
                        jadix::components::alert_dialog_content(
                            jadix::components::AlertDialogContentOpts {
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
                                panel: #panel,
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
                        .map(|e| quote! { jadix::components::SheetSide::from(#e) })
                        .unwrap_or_else(|| {
                            quote! { jadix::components::SheetSide::Right }
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
                    let panel = quote_slot_chrome(&class);
                    return quote! {
                        jadix::components::sheet_content(
                            jadix::components::SheetContentOpts {
                                side: #side,
                                show_close: #show_close,
                                overlay_color: #overlay_color,
                                overlay_opacity: #overlay_opacity,
                                close_on_overlay_click: #close_on_overlay,
                                overlay_blur: #overlay_blur,
                                width: #width,
                                height: #height,
                                panel: #panel,
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
                        .map(|e| quote! { jadix::components::DrawerDirection::from(#e) })
                        .unwrap_or_else(|| {
                            quote! { jadix::components::DrawerDirection::Down }
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
                            quote! { jadix::components::drawer::coerce_snap_points(#e) }
                        })
                        .unwrap_or_else(
                            || quote! { vec![jadix::components::DrawerSnapPoint::Fraction(1.0)] },
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
                    let panel = quote_slot_chrome(&class);
                    return quote! {
                        jadix::components::drawer_content(
                            jadix::components::DrawerContentOpts {
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
                                panel: #panel,
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
                    let panel = quote_slot_chrome(&class);
                    return quote! {
                        jadix::components::tooltip_content(
                            jadix::components::TooltipContentOpts {
                                side: #side,
                                panel: #panel,
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
                        .unwrap_or_else(|| quote! { jadix::utils::popup::FLOATING_TRIGGER_GAP });
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
                    let lock_page_scroll = get_attr("lock_page_scroll")
                        .or_else(|| get_attr("lockPageScroll"))
                        .map(|e| quote! { #e })
                        .unwrap_or_else(|| quote! { false });
                    let scroll_freeze = get_attr("scroll_freeze")
                        .or_else(|| get_attr("scrollFreeze"))
                        .map(|e| {
                            quote! {
                                Some(jadix::app::portal_overlay::parse_scroll_freeze(&(#e).to_string()))
                            }
                        })
                        .unwrap_or_else(|| quote! { None });
                    let mut expanded_children = Vec::new();
                    for child in &el.children {
                        // Body is rebuilt inside popover_content's reactive_anchor.
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
                    let panel = quote_slot_chrome(&class);
                    return quote! {
                        jadix::components::popover_content(
                            jadix::components::PopoverContentOpts {
                                side: #side,
                                align: #align,
                                side_offset: #side_offset,
                                panel: #panel,
                                strategy: #strategy,
                                width: #width,
                                lock_page_scroll: #lock_page_scroll,
                                scroll_freeze: #scroll_freeze,
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
                        .unwrap_or_else(|| quote! { jadix::utils::popup::FLOATING_TRIGGER_GAP });
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
                                jadix::utils::anchored_overlay::parse_overlay_kind(&(#e).to_string())
                            }
                        })
                        .unwrap_or_else(|| {
                            quote! { jadix::app::portal_overlay::OverlayKind::Popover }
                        });
                    let motion = get_attr("motion")
                        .map(|e| {
                            quote! {
                                jadix::utils::anchored_overlay::parse_floating_motion(&(#e).to_string())
                            }
                        })
                        .unwrap_or_else(|| {
                            quote! { jadix::utils::anchored_overlay::FloatingMotionKind::Fade }
                        });
                    let mode = get_attr("mode")
                        .map(|e| {
                            quote! {
                                jadix::utils::anchored_overlay::parse_overlay_mode(&(#e).to_string())
                            }
                        })
                        .unwrap_or_else(|| {
                            quote! { jadix::app::portal_overlay::OverlayMode::Stacked }
                        });
                    let mode = get_attr("pinned")
                        .map(|e| {
                            quote! {
                                if #e {
                                    jadix::app::portal_overlay::OverlayMode::Pinned
                                } else {
                                    jadix::app::portal_overlay::OverlayMode::Stacked
                                }
                            }
                        })
                        .unwrap_or(mode);
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
                    let lock_page_scroll = get_attr("lock_page_scroll")
                        .or_else(|| get_attr("lockPageScroll"))
                        .map(|e| quote! { Some(#e) })
                        .unwrap_or_else(|| quote! { None });
                    let scroll_freeze = get_attr("scroll_freeze")
                        .or_else(|| get_attr("scrollFreeze"))
                        .map(|e| {
                            quote! {
                                Some(jadix::app::portal_overlay::parse_scroll_freeze(&(#e).to_string()))
                            }
                        })
                        .unwrap_or_else(|| quote! { None });
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
                        jadix::utils::anchored_overlay::anchored_overlay_content_from_rsx(
                            {
                                let mut __opts =
                                    jadix::utils::anchored_overlay::AnchoredOverlayContentOpts::default();
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
                                __opts.lock_page_scroll = #lock_page_scroll;
                                __opts.scroll_freeze = #scroll_freeze;
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
                        .map(|e| quote! { jadix::utils::drag::DragStack::from(#e) })
                        .unwrap_or_else(|| quote! { jadix::utils::drag::DragStack::Auto });
                    let restrict = get_attr("restrict")
                        .map(|e| {
                            quote! { Some(jadix::utils::drag::Restrict::parse(&(#e).to_string())) }
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
                        jadix::utils::drag::draggable_from_rsx(
                            jadix::utils::drag::DraggableRsxOpts {
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
                                jadix::utils::drag::DropOverlap::parse(&(#e).to_string()),
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
                                jadix::core::tree::IntoOpacity::into_opacity(#e),
                            );
                        }
                    } else {
                        quote! {}
                    };
                    let direction_attr = get_attr("direction");
                    let direction_ts = if let Some(e) = direction_attr {
                        quote! {
                            __db = __db.direction(jadix::utils::drag::Axis::parse(&(#e)));
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
                            let mut __db = jadix::utils::drag::droppable(#id_ts)
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
                    let source_opacity_ts = if let Some(v) =
                        get_attr("source_opacity").or_else(|| get_attr("sourceOpacity"))
                    {
                        quote! { .source_opacity(#v) }
                    } else {
                        quote! {}
                    };
                    let align_ts = if let Some(v) = get_attr("align") {
                        quote! { .align(jadix::utils::drag::OverlayAlign::parse(&(#v))) }
                    } else {
                        quote! {}
                    };
                    return quote! {
                        jadix::core::tree::Element::from(
                            jadix::utils::drag::drag_overlay()
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
                        jadix::utils::drag::drag_handle_from_rsx(|| #child_el)
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
                        jadix::utils::drag::drag_ignore_from_rsx(|| #child_el)
                    };
                }

                if name_str == "ViewportModalContent" {
                    let motion = get_attr("motion")
                        .map(|e| {
                            quote! {
                                jadix::utils::viewport_modal::parse_modal_motion(&(#e).to_string())
                            }
                        })
                        .unwrap_or_else(|| {
                            quote! { jadix::utils::viewport_modal::ModalSurfaceMotion::DialogFade }
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
                        jadix::utils::viewport_modal::viewport_modal_content_from_rsx(
                            jadix::utils::viewport_modal::ViewportModalContentOpts {
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
                        .unwrap_or_else(|| quote! { jadix::utils::popup::FLOATING_TRIGGER_GAP });
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
                    let panel = quote_slot_chrome(&class);
                    return quote! {
                        jadix::components::hover_card_content(
                            jadix::components::HoverCardContentOpts {
                                side: #side,
                                align: #align,
                                side_offset: #side_offset,
                                panel: #panel,
                                width: #width,
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
                        quote! { jadix::core::tree::Element::new() }
                    } else {
                        quote! {
                            {
                                let mut __list: Vec<jadix::core::tree::Element> = Vec::new();
                                #( __list.extend(jadix::core::tree::build_ui_child(|| (#expanded_children))); )*
                                if __list.len() == 1 {
                                    __list.remove(0)
                                } else {
                                    let mut __frag = jadix::core::tree::Element::new();
                                    __frag.set_attr("jadix_fragment", "1");
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
                        jadix::components::context_menu_sub(#id_ts, || #child_el)
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
                        quote! { jadix::core::tree::Element::new() }
                    } else {
                        quote! {
                            {
                                let mut __list: Vec<jadix::core::tree::Element> = Vec::new();
                                #( __list.extend(jadix::core::tree::build_ui_child(|| (#expanded_children))); )*
                                if __list.len() == 1 {
                                    __list.remove(0)
                                } else {
                                    let mut __frag = jadix::core::tree::Element::new();
                                    __frag.set_attr("jadix_fragment", "1");
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
                        jadix::components::context_menu_radio_group(
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
                        quote! { jadix::core::tree::Element::new() }
                    } else {
                        quote! {
                            {
                                let mut __list: Vec<jadix::core::tree::Element> = Vec::new();
                                #( __list.extend(jadix::core::tree::build_ui_child(|| (#expanded_children))); )*
                                if __list.len() == 1 {
                                    __list.remove(0)
                                } else {
                                    let mut __frag = jadix::core::tree::Element::new();
                                    __frag.set_attr("jadix_fragment", "1");
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
                        jadix::components::dropdown_menu_sub(#id_ts, || #child_el)
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
                        quote! { jadix::core::tree::Element::new() }
                    } else {
                        quote! {
                            {
                                let mut __list: Vec<jadix::core::tree::Element> = Vec::new();
                                #( __list.extend(jadix::core::tree::build_ui_child(|| (#expanded_children))); )*
                                if __list.len() == 1 {
                                    __list.remove(0)
                                } else {
                                    let mut __frag = jadix::core::tree::Element::new();
                                    __frag.set_attr("jadix_fragment", "1");
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
                        jadix::components::dropdown_menu_radio_group(
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
                        quote! { jadix::core::tree::Element::new() }
                    } else {
                        quote! {
                            {
                                let mut __list: Vec<jadix::core::tree::Element> = Vec::new();
                                #( __list.extend(jadix::core::tree::build_ui_child(|| (#expanded_children))); )*
                                if __list.len() == 1 {
                                    __list.remove(0)
                                } else {
                                    let mut __frag = jadix::core::tree::Element::new();
                                    __frag.set_attr("jadix_fragment", "1");
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
                        jadix::components::menubar_sub(#id_ts, || #child_el)
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
                        quote! { jadix::core::tree::Element::new() }
                    } else {
                        quote! {
                            {
                                let mut __list: Vec<jadix::core::tree::Element> = Vec::new();
                                #( __list.extend(jadix::core::tree::build_ui_child(|| (#expanded_children))); )*
                                if __list.len() == 1 {
                                    __list.remove(0)
                                } else {
                                    let mut __frag = jadix::core::tree::Element::new();
                                    __frag.set_attr("jadix_fragment", "1");
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
                        jadix::components::menubar_radio_group(
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
                    let panel = quote_slot_chrome(&class);
                    return quote! {
                        jadix::components::context_menu_content(
                            jadix::components::ContextMenuContentOpts {
                                placement: #placement,
                                panel: #panel,
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
                        jadix::components::dropdown_menu_content(
                            jadix::components::DropdownMenuContentOpts::new(#placement, #class),
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
                    let panel = quote_slot_chrome(&class);
                    return quote! {
                        jadix::components::menubar_content(
                            jadix::components::MenubarContentOpts {
                                placement: #placement,
                                panel: #panel,
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
                    let panel = quote_slot_chrome(&class);
                    return quote! {
                        jadix::components::navigation_menu_content(
                            jadix::components::NavigationMenuContentOpts {
                                panel: #panel,
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
                    let panel = quote_slot_chrome(&class);
                    return quote! {
                        jadix::components::context_menu_sub_content(
                            #panel,
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
                    let panel = quote_slot_chrome(&class);
                    return quote! {
                        jadix::components::dropdown_menu_sub_content(
                            #panel,
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
                    let panel = quote_slot_chrome(&class);
                    return quote! {
                        jadix::components::menubar_sub_content(
                            #panel,
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
                    let panel = quote_slot_chrome(&class);
                    return quote! {
                        jadix::components::select_content(
                            jadix::components::SelectContentOpts {
                                panel: #panel,
                                match_trigger: #match_trigger,
                                menu_width: #menu_width,
                                menu_height: #menu_height,
                                min_height: #min_height,
                                max_height: #max_height,
                                placement: #placement,
                                scroll_selected: #scroll_selected,
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
                        quote! { jadix::core::tree::parse_bool_like(#c) }
                    } else {
                        quote! { false }
                    };
                    let class_ts = if let Some(c) = class_expr {
                        quote! { Some((#c).to_string()) }
                    } else {
                        quote! { None }
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
                    return quote! {{
                        let __jadix_cb_class: Option<String> = #class_ts;
                        jadix::components::base::SlotChrome::from_opts(__jadix_cb_class, None).apply(
                            jadix::components::combobox_scope(
                                #id_ts,
                                #open_ts,
                                #value_ts,
                                #placeholder_ts,
                                #clearable_ts,
                                String::new(),
                                #content_class_ts,
                                #scroll_selected_ts,
                                || #child_el,
                            )
                        )
                    }};
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
                    let scroll_selected = get_attr("scroll_selected")
                        .or_else(|| get_attr("scrollSelected"))
                        .map(|e| quote! { Some((#e).to_string()) })
                        .unwrap_or_else(|| quote! { None });
                    let mut expanded_children = Vec::new();
                    for child in &el.children {
                        // Body is rendered inside ComboboxContent's reactive_anchor.
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
                    let panel = quote_slot_chrome(&class);
                    return quote! {
                        jadix::components::combobox_content(
                            jadix::components::ComboboxContentOpts {
                                panel: #panel,
                                match_trigger: #match_trigger,
                                menu_width: #menu_width,
                                menu_height: #menu_height,
                                min_height: #min_height,
                                max_height: #max_height,
                                placement: #placement,
                                scroll_selected: #scroll_selected,
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
                    // Flat multi-child wrapper; runtime unpacks via `jadix_fragment`.
                    return quote! {
                        {
                            let mut __list: Vec<jadix::core::tree::Element> = Vec::new();
                            #( __list.extend(jadix::core::tree::build_ui_child(|| (#expanded_children))); )*
                            let mut __frag = jadix::core::tree::Element::new();
                            __frag.set_attr("jadix_fragment", "1");
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
                                use jadix::core::tree::IntoDynamicComponent;
                                (#component_expr).into_dynamic_component()
                            }
                        };
                    }
                    return quote! {
                        {
                            use jadix::core::tree::{IntoDynamicComponent, IntoElementList};
                            let mut __jadix_dyn = (#component_expr).into_dynamic_component();
                            let mut __list: Vec<jadix::core::tree::Element> = Vec::new();
                            #( __list.extend(jadix::core::tree::build_ui_child(|| (#expanded_children))); )*
                            __jadix_dyn = __jadix_dyn.children(__list);
                            __jadix_dyn
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
                        jadix::reactive!(#id_expr, move || { #child_el })
                    };
                }

                if name_str == "ErrorBoundary" {
                    let Some(fallback_expr) = get_attr("fallback") else {
                        return quote! {
                            compile_error!("<ErrorBoundary> requires `fallback={|msg| ...}`")
                        };
                    };
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
                    let id_ts = rsx_island_anchor(
                        get_attr("id").or_else(|| get_attr("scope")),
                        "ErrorBoundary",
                        node_counter,
                    );
                    return quote! {
                        jadix::app::error_boundary(
                            #id_ts,
                            #fallback_expr,
                            move || { #child_el },
                        )
                    };
                }

                if name_str == "Suspense" {
                    let Some(fallback_expr) = get_attr("fallback") else {
                        return quote! {
                            compile_error!("<Suspense> requires `fallback={|| ...}`")
                        };
                    };
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
                    let id_ts = rsx_island_anchor(
                        get_attr("id").or_else(|| get_attr("scope")),
                        "Suspense",
                        node_counter,
                    );
                    return quote! {
                        jadix::app::suspense(
                            #id_ts,
                            #fallback_expr,
                            move || { #child_el },
                        )
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
                    let idx = *node_counter;
                    *node_counter += 1;
                    // Solid-style: when `when` reads `.get()`, subscribe in a dedicated reactive_view.
                    if expr_reads_get(when_expr) {
                        return quote! {
                            jadix::app::reactive::show_reactive(
                                format!("{}:{}:Show#{}", file!(), line!(), #idx),
                                move || { #when_expr },
                                move || { #child_el },
                            )
                        };
                    }
                    return quote! {
                        jadix::app::reactive::show_scoped(
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
                            jadix::app::reactive::switch_reactive(
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
                        quote! { jadix::core::tree::Element::new() }
                    };
                    return quote! {
                        jadix::app::reactive::match_when_reactive(
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
                    quote! {
                        |__jadix_for_item| {
                            let #item_ident = __jadix_for_item;
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
                            |__jadix_for_item| {
                                let #item_ident = __jadix_for_item;
                                #key_ts
                            }
                        }
                    } else {
                        quote! { #key_ts }
                    };
                    if let Some(fallback_ts) = maybe_fallback_ts.as_ref() {
                        return quote! {
                            jadix::app::reactive::for_keyed_reactive_with_fallback(
                                #scope_ts,
                                move || {
                                    (#each_expr).into_iter().collect::<Vec<_>>()
                                },
                                #key_fn,
                                #render_ts,
                                move || {
                                    use jadix::core::tree::IntoElementList;
                                    (#fallback_ts).into_element_list()
                                },
                            )
                        };
                    }
                    return quote! {
                        jadix::app::reactive::for_keyed_reactive(
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
                                use jadix::core::tree::IntoElementList;
                                let __out = #base;
                                if __out.is_empty() { (#fallback_ts).into_element_list() } else { __out }
                            }}
                        } else {
                            quote! {{ #base }}
                        };
                        if each_is_reactive {
                            quote! {
                                jadix::app::reactive::for_reactive(
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
                            jadix::app::reactive::for_keyed_iter(
                                #items_scope_ts,
                                #each_expr,
                                |__jadix_for_item| {
                                    let #item_ident = __jadix_for_item;
                                    #key_ts
                                },
                                #render_ts,
                            )
                        };
                        return emit_for(base, maybe_fallback_ts.clone());
                    } else {
                        let base = quote! {
                            jadix::app::reactive::for_keyed_iter(
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
                        jadix::app::reactive::for_each_iter(
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
                    "MorphIcon",
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
                let name_ident = if matches!(el.name.to_string().as_str(), "Img" | "img" | "audio" | "video") {
                    syn::Ident::new(&name_str, el.name.span())
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
                let mut collect_item_class: Option<&Expr> = None;

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

                    // Element-root hover — not a component prop.
                    if matches!(attr_name, "on_hover" | "onHover") {
                        continue;
                    }

                    // Interactive primitive: live `disabled={sig.get()}` on any #[component].
                    if attr_name == "disabled" && expr_reads_get(value) {
                        let idx = *node_counter;
                        *node_counter += 1;
                        prop_names.push(attr.clone());
                        prop_values.push(quote! {
                            Some(jadix::core::reactivity::untrack(|| #value))
                        });
                        chrome_binds.push(quote! {
                            __jadix_el = jadix::app::reactive::bind_attr(
                                __jadix_el,
                                format!("{}:{}:bind_disabled#{}", file!(), line!(), #idx),
                                move || #value,
                                |el, d| jadix::primitives::interactive::apply_disabled(el, d),
                            );
                        });
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
                            Some(jadix::core::reactivity::untrack(|| #value))
                        });
                        chrome_binds.push(quote! {
                            __jadix_el = jadix::app::reactive::bind_attr(
                                __jadix_el,
                                format!("{}:{}:bind_sidebar_menu_button_active#{}", file!(), line!(), #idx),
                                move || #value,
                                |el, active| {
                                    jadix::components::apply_bound_sidebar_menu_button_active(el, active)
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
                                Some(jadix::core::reactivity::untrack(|| (#value).to_string()).into())
                            });
                            chrome_binds.push(quote! {
                                __jadix_el = jadix::app::reactive::bind_attr(
                                    __jadix_el,
                                    format!("{}:{}:bind_bubble_variant#{}", file!(), line!(), #idx),
                                    move || (#value).to_string(),
                                    |el, v| jadix::components::apply_bound_bubble_variant(el, v),
                                );
                            });
                            continue;
                        }
                        if name_str == "Bubble" && attr_name == "align" {
                            prop_names.push(attr.clone());
                            prop_values.push(quote! {
                                Some(jadix::core::reactivity::untrack(|| (#value).to_string()).into())
                            });
                            chrome_binds.push(quote! {
                                __jadix_el = jadix::app::reactive::bind_attr(
                                    __jadix_el,
                                    format!("{}:{}:bind_bubble_align#{}", file!(), line!(), #idx),
                                    move || (#value).to_string(),
                                    |el, v| jadix::components::apply_bound_bubble_align(el, v),
                                );
                            });
                            continue;
                        }
                        if name_str == "Message" && attr_name == "align" {
                            prop_names.push(attr.clone());
                            prop_values.push(quote! {
                                Some(jadix::core::reactivity::untrack(|| (#value).to_string()).into())
                            });
                            chrome_binds.push(quote! {
                                __jadix_el = jadix::app::reactive::bind_attr(
                                    __jadix_el,
                                    format!("{}:{}:bind_message_align#{}", file!(), line!(), #idx),
                                    move || (#value).to_string(),
                                    |el, v| jadix::components::apply_bound_message_align(el, v),
                                );
                            });
                            continue;
                        }
                        if name_str == "Alert" && attr_name == "variant" {
                            prop_names.push(attr.clone());
                            prop_values.push(quote! {
                                Some(jadix::core::reactivity::untrack(|| (#value).to_string()).into())
                            });
                            chrome_binds.push(quote! {
                                __jadix_el = jadix::app::reactive::bind_attr(
                                    __jadix_el,
                                    format!("{}:{}:bind_alert_variant#{}", file!(), line!(), #idx),
                                    move || (#value).to_string(),
                                    |el, v| jadix::components::apply_bound_alert_variant(el, v),
                                );
                            });
                            continue;
                        }
                        if name_str == "Alert" && attr_name == "chrome" {
                            if expr_reads_get(value) {
                                let idx = *node_counter;
                                *node_counter += 1;
                                chrome_binds.push(quote! {
                                    __jadix_el = jadix::app::reactive::bind_attr(
                                        __jadix_el,
                                        format!("{}:{}:bind_alert_chrome#{}", file!(), line!(), #idx),
                                        move || #value,
                                        |el, enabled| {
                                            jadix::components::apply_bound_alert_chrome(el, enabled)
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
                                Some(jadix::core::reactivity::untrack(|| (#value).to_string()).into())
                            });
                            chrome_binds.push(quote! {
                                __jadix_el = jadix::app::reactive::bind_attr(
                                    __jadix_el,
                                    format!("{}:{}:bind_marker_variant#{}", file!(), line!(), #idx),
                                    move || (#value).to_string(),
                                    |el, v| jadix::components::apply_bound_marker_variant(el, v),
                                );
                            });
                            continue;
                        }
                        if name_str == "Item" && attr_name == "variant" {
                            prop_names.push(attr.clone());
                            prop_values.push(quote! {
                                Some(jadix::core::reactivity::untrack(|| (#value).to_string()).into())
                            });
                            chrome_binds.push(quote! {
                                __jadix_el = jadix::app::reactive::bind_attr(
                                    __jadix_el,
                                    format!("{}:{}:bind_item_variant#{}", file!(), line!(), #idx),
                                    move || (#value).to_string(),
                                    |el, v| jadix::components::apply_bound_item_variant(el, v),
                                );
                            });
                            continue;
                        }
                        if name_str == "Item" && attr_name == "size" {
                            prop_names.push(attr.clone());
                            prop_values.push(quote! {
                                Some(jadix::core::reactivity::untrack(|| (#value).to_string()).into())
                            });
                            chrome_binds.push(quote! {
                                __jadix_el = jadix::app::reactive::bind_attr(
                                    __jadix_el,
                                    format!("{}:{}:bind_item_size#{}", file!(), line!(), #idx),
                                    move || (#value).to_string(),
                                    |el, v| jadix::components::apply_bound_item_size(el, v),
                                );
                            });
                            continue;
                        }
                        if name_str == "ItemMedia" && attr_name == "variant" {
                            prop_names.push(attr.clone());
                            prop_values.push(quote! {
                                Some(jadix::core::reactivity::untrack(|| (#value).to_string()).into())
                            });
                            chrome_binds.push(quote! {
                                __jadix_el = jadix::app::reactive::bind_attr(
                                    __jadix_el,
                                    format!("{}:{}:bind_item_media_variant#{}", file!(), line!(), #idx),
                                    move || (#value).to_string(),
                                    |el, v| jadix::components::apply_bound_item_media_variant(el, v),
                                );
                            });
                            continue;
                        }
                        if name_str == "Attachment" && attr_name == "state" {
                            prop_names.push(attr.clone());
                            prop_values.push(quote! {
                                Some(jadix::core::reactivity::untrack(|| (#value).to_string()).into())
                            });
                            chrome_binds.push(quote! {
                                __jadix_el = jadix::app::reactive::bind_attr(
                                    __jadix_el,
                                    format!("{}:{}:bind_attachment_state#{}", file!(), line!(), #idx),
                                    move || (#value).to_string(),
                                    |el, v| jadix::components::apply_bound_attachment_state(el, v),
                                );
                            });
                            continue;
                        }
                        if name_str == "Attachment" && attr_name == "size" {
                            prop_names.push(attr.clone());
                            prop_values.push(quote! {
                                Some(jadix::core::reactivity::untrack(|| (#value).to_string()).into())
                            });
                            chrome_binds.push(quote! {
                                __jadix_el = jadix::app::reactive::bind_attr(
                                    __jadix_el,
                                    format!("{}:{}:bind_attachment_size#{}", file!(), line!(), #idx),
                                    move || (#value).to_string(),
                                    |el, v| jadix::components::apply_bound_attachment_size(el, v),
                                );
                            });
                            continue;
                        }
                        if name_str == "Attachment" && attr_name == "orientation" {
                            prop_names.push(attr.clone());
                            prop_values.push(quote! {
                                Some(jadix::core::reactivity::untrack(|| (#value).to_string()).into())
                            });
                            chrome_binds.push(quote! {
                                __jadix_el = jadix::app::reactive::bind_attr(
                                    __jadix_el,
                                    format!("{}:{}:bind_attachment_orientation#{}", file!(), line!(), #idx),
                                    move || (#value).to_string(),
                                    |el, v| jadix::components::apply_bound_attachment_orientation(el, v),
                                );
                            });
                            continue;
                        }
                        if name_str == "AttachmentMedia" && attr_name == "variant" {
                            prop_names.push(attr.clone());
                            prop_values.push(quote! {
                                Some(jadix::core::reactivity::untrack(|| (#value).to_string()).into())
                            });
                            chrome_binds.push(quote! {
                                __jadix_el = jadix::app::reactive::bind_attr(
                                    __jadix_el,
                                    format!("{}:{}:bind_attachment_media_variant#{}", file!(), line!(), #idx),
                                    move || (#value).to_string(),
                                    |el, v| jadix::components::apply_bound_attachment_media_variant(el, v),
                                );
                            });
                            continue;
                        }
                        if name_str == "TableRow" && attr_name == "selected" {
                            prop_names.push(attr.clone());
                            prop_values.push(quote! {
                                Some(jadix::core::reactivity::untrack(|| #value))
                            });
                            chrome_binds.push(quote! {
                                __jadix_el = jadix::app::reactive::bind_attr(
                                    __jadix_el,
                                    format!("{}:{}:bind_table_row_selected#{}", file!(), line!(), #idx),
                                    move || #value,
                                    |el, v| jadix::components::apply_bound_table_row_selected(el, v),
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
                                    Some(jadix::core::reactivity::untrack(|| (#value).to_string()).into())
                                }
                            } else {
                                quote! { Some((#value).to_string()) }
                            });
                            if is_reactive {
                                chrome_binds.push(quote! {
                                    __jadix_el = jadix::app::reactive::bind_attr(
                                        __jadix_el,
                                        format!("{}:{}:bind_table_align#{}", file!(), line!(), #idx),
                                        move || (#value).to_string(),
                                        |el, v| jadix::components::apply_bound_table_align(el, v),
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
                                    __jadix_el = jadix::app::reactive::bind_attr(
                                        __jadix_el,
                                        format!("{}:{}:bind_badge_chrome#{}", file!(), line!(), #idx),
                                        move || #value,
                                        |el, enabled| {
                                            jadix::components::apply_bound_badge_chrome(el, enabled)
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
                                Some(jadix::core::reactivity::untrack(|| (#value).to_string()).into())
                            });
                            chrome_binds.push(quote! {
                                __jadix_el = jadix::app::reactive::bind_attr(
                                    __jadix_el,
                                    format!("{}:{}:bind_avatar_size#{}", file!(), line!(), #idx),
                                    move || (#value).to_string(),
                                    |el, v| jadix::components::apply_bound_avatar_size(el, v),
                                );
                            });
                            continue;
                        }
                        if name_str == "Avatar" && attr_name == "shape" {
                            prop_names.push(attr.clone());
                            prop_values.push(quote! {
                                Some(jadix::core::reactivity::untrack(|| (#value).to_string()).into())
                            });
                            chrome_binds.push(quote! {
                                __jadix_el = jadix::app::reactive::bind_attr(
                                    __jadix_el,
                                    format!("{}:{}:bind_avatar_shape#{}", file!(), line!(), #idx),
                                    move || (#value).to_string(),
                                    |el, v| jadix::components::apply_bound_avatar_shape(el, v),
                                );
                            });
                            continue;
                        }
                        if matches!(name_str.as_str(), "Badge" | "AvatarBadge")
                            && attr_name == "variant"
                        {
                            prop_names.push(attr.clone());
                            prop_values.push(quote! {
                                Some(jadix::core::reactivity::untrack(|| (#value).to_string()).into())
                            });
                            let bind_key = if name_str == "AvatarBadge" {
                                "avatarbadge"
                            } else {
                                "badge"
                            };
                            let apply_fn = if name_str == "AvatarBadge" {
                                quote! { jadix::components::apply_bound_avatar_badge_variant }
                            } else {
                                quote! { jadix::components::apply_bound_badge_variant }
                            };
                            chrome_binds.push(quote! {
                                __jadix_el = jadix::app::reactive::bind_attr(
                                    __jadix_el,
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
                                Some(jadix::core::reactivity::untrack(|| (#value).to_string()).into())
                            });
                            chrome_binds.push(quote! {
                                __jadix_el = jadix::app::reactive::bind_attr(
                                    __jadix_el,
                                    format!("{}:{}:bind_btn_variant#{}", file!(), line!(), #idx),
                                    move || (#value).to_string(),
                                    |el, v| jadix::components::apply_bound_button_variant(el, v),
                                );
                            });
                            continue;
                        }
                        if name_str == "Input" && attr_name == "invalid" {
                            prop_names.push(attr.clone());
                            prop_values.push(quote! {
                                Some(jadix::core::reactivity::untrack(|| #value))
                            });
                            chrome_binds.push(quote! {
                                __jadix_el = jadix::app::reactive::bind_attr(
                                    __jadix_el,
                                    format!("{}:{}:bind_input_invalid#{}", file!(), line!(), #idx),
                                    move || #value,
                                    |el, v| jadix::components::apply_bound_input_invalid(el, v),
                                );
                            });
                            continue;
                        }
                    }

                    // own_prop：领域字段碰巧像 CSS，但不能进 is_base chrome。
                    // 新的布局测量先改领域名（docs/COMPONENT_LIBRARY.md §1.1），再把新名列入此处。
                    // Chart `plot_h` 是绘图区高度（轴标签在外），不能当 base 钳死整棵 vstack。
                    let chart_plot_height =
                        matches!(
                            name_str.as_str(),
                            "BarChart" | "LineChart" | "AreaChart" | "Chart"
                        ) && matches!(attr_name, "plot_h" | "plotH" | "plot_height" | "plotHeight");

                    // Textarea: `wrap` = soft|off（单行溢出），不是 flex-wrap；
                    // `rows` = 可视行数，不是 grid-template-rows。
                    let textarea_own_prop =
                        name_str == "Textarea" && matches!(attr_name, "wrap" | "rows");

                    // TableHead/TableCell：`col_w` / `align` 是列 prop，不是根节点 base。
                    let table_cell_own_prop =
                        matches!(name_str.as_str(), "TableHead" | "TableCell")
                            && matches!(
                                attr_name,
                                "col_w" | "colW" | "column_width" | "columnWidth" | "align"
                            );

                    // Sidebar rail `rail_w` is the collapsible track width, not layout `w`.
                    let sidebar_rail_own_prop = name_str == "Sidebar"
                        && matches!(attr_name, "rail_w" | "railW" | "rail_width" | "railWidth");

                    // Toaster `position` is the Sonner anchor (`top-center`…), not CSS `position`.
                    let toaster_own_prop = name_str == "Toaster"
                        && matches!(attr_name, "position" | "variant" | "overflow" | "limit");

                    // `<App width/height/font_*>` are window / CSS-root props, not layout `w`/`h`.
                    let app_shell_own_prop = name_str == "App"
                        && matches!(
                            attr_name,
                            "width"
                                | "height"
                                | "font"
                                | "font_family"
                                | "fontFamily"
                                | "font_size"
                                | "fontSize"
                                | "font_weight"
                                | "fontWeight"
                                | "title"
                                | "frameless"
                                | "decorations"
                                | "transparent"
                                | "blur"
                                | "backdrop"
                                | "synthetic_bold"
                                | "syntheticBold"
                                | "devtools"
                        );

                    let is_base = !chart_plot_height
                        && !textarea_own_prop
                        && !table_cell_own_prop
                        && !sidebar_rail_own_prop
                        && !toaster_own_prop
                        && !app_shell_own_prop
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
                                | "opacity"
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
                                | "ring_width"
                                | "ringWidth"
                                | "ring_color"
                                | "ringColor"
                                | "caret_color"
                                | "caretColor"
                                | "selection_background"
                                | "selectionBackground"
                                | "selection_foreground"
                                | "selectionForeground"
                                | "class"
                                | "className"
                                | "tw"
                                | "uno"
                        );

                    if is_base {
                        // a11y attrs apply on the finished Element (not BaseComponentBuilder).
                        if matches!(attr_name, "aria_label" | "aria-label") {
                            chrome_binds.push(quote! {
                                __jadix_el = __jadix_el.aria_label(#value);
                            });
                            continue;
                        }
                        if matches!(attr_name, "aria_labelledby" | "aria-labelledby") {
                            chrome_binds.push(quote! {
                                __jadix_el = __jadix_el.aria_labelledby(#value);
                            });
                            continue;
                        }
                        if attr_name == "accesskit_name" {
                            chrome_binds.push(quote! {
                                __jadix_el = __jadix_el.accesskit_name(#value);
                            });
                            continue;
                        }
                        if attr_name == "accesskit_role" {
                            chrome_binds.push(quote! {
                                __jadix_el = __jadix_el.accesskit_role(#value);
                            });
                            continue;
                        }
                        if attr_name == "accesskit_id" {
                            chrome_binds.push(quote! {
                                __jadix_el = __jadix_el.accesskit_id(#value);
                            });
                            continue;
                        }
                        // Dynamic class / color base attrs → bind on resulting Element.
                        if expr_reads_get(value) {
                            let idx = *node_counter;
                            *node_counter += 1;
                            if matches!(attr_name, "class" | "className" | "tw" | "uno") {
                                chrome_binds.push(quote! {
                                    __jadix_el = jadix::app::reactive::bind_class(
                                        __jadix_el,
                                        format!("{}:{}:bind_class#{}", file!(), line!(), #idx),
                                        move || (#value).to_string(),
                                    );
                                });
                                continue;
                            }
                            if attr_name == "bg" {
                                chrome_binds.push(quote! {
                                    __jadix_el = jadix::app::reactive::bind_prop(
                                        __jadix_el,
                                        format!("{}:{}:bind_bg#{}", file!(), line!(), #idx),
                                        move || {
                                            let __v = (#value).to_string();
                                            jadix::core::tree::parse_color(__v.as_str())
                                        },
                                        |el, color| jadix::style::apply_user_bg(el, color),
                                    );
                                });
                                continue;
                            }
                            if attr_name == "opacity" {
                                chrome_binds.push(quote! {
                                    __jadix_el = jadix::app::reactive::bind_prop(
                                        __jadix_el,
                                        format!("{}:{}:bind_opacity#{}", file!(), line!(), #idx),
                                        move || {
                                            jadix::core::tree::IntoOpacity::into_opacity(#value)
                                        },
                                        |el, o| jadix::app::reactive::apply_bound_opacity(el, o),
                                    );
                                });
                                continue;
                            }
                            if matches!(attr_name, "color" | "text_color") {
                                chrome_binds.push(quote! {
                                    __jadix_el = jadix::app::reactive::bind_prop(
                                        __jadix_el,
                                        format!("{}:{}:bind_color#{}", file!(), line!(), #idx),
                                        move || {
                                            let __v = (#value).to_string();
                                            jadix::core::tree::parse_text_color(__v.as_str())
                                        },
                                        |el, color| jadix::style::apply_user_fg(el, color),
                                    );
                                });
                                continue;
                            }
                            if matches!(attr_name, "border_color" | "borderColor") {
                                chrome_binds.push(quote! {
                                    __jadix_el = jadix::app::reactive::bind_prop(
                                        __jadix_el,
                                        format!("{}:{}:bind_border_color#{}", file!(), line!(), #idx),
                                        move || {
                                            let __v = (#value).to_string();
                                            jadix::core::tree::parse_color(__v.as_str())
                                        },
                                        |el, color| jadix::style::apply_user_border_color(el, color),
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
                                    __jadix_el = jadix::app::reactive::bind_prop(
                                        __jadix_el,
                                        format!("{}:{}:bind_size_{}#{}", file!(), line!(), #axis, #idx),
                                        move || {
                                            jadix::core::tree::IntoCssDim::into_css_dim(#value)
                                        },
                                        |el, dim| {
                                            jadix::app::reactive::apply_bound_size(el, #axis, dim)
                                        },
                                    );
                                });
                                continue;
                            }
                            if matches!(attr_name, "aspect_ratio" | "aspect-ratio" | "aspectRatio")
                            {
                                chrome_binds.push(quote! {
                                    __jadix_el = jadix::app::reactive::bind_prop(
                                        __jadix_el,
                                        format!("{}:{}:bind_aspect_ratio#{}", file!(), line!(), #idx),
                                        move || (#value).to_string(),
                                        |el, s: String| {
                                            let next =
                                                jadix::core::tree::parse_css_aspect_ratio(s.as_str());
                                            if el.style.aspect_ratio == next {
                                                jadix::core::tree::LayoutStatus::Clean
                                            } else {
                                                el.style.aspect_ratio = next;
                                                jadix::core::tree::LayoutStatus::LayoutChanged
                                            }
                                        },
                                    );
                                });
                                continue;
                            }
                            if matches!(attr_name, "animation_play_state" | "animationPlayState") {
                                chrome_binds.push(quote! {
                                    __jadix_el = jadix::app::reactive::bind_prop(
                                        __jadix_el,
                                        format!(
                                            "{}:{}:bind_animation_play_state#{}",
                                            file!(),
                                            line!(),
                                            #idx
                                        ),
                                        move || (#value).to_string(),
                                        |el, v| {
                                            jadix::app::reactive::apply_bound_animation_play_state(
                                                el, v,
                                            )
                                        },
                                    );
                                });
                                continue;
                            }
                            if matches!(attr_name, "z_index" | "zIndex") {
                                chrome_binds.push(quote! {
                                    __jadix_el = jadix::app::reactive::bind_prop(
                                        __jadix_el,
                                        format!("{}:{}:bind_z_index#{}", file!(), line!(), #idx),
                                        move || (#value) as i32,
                                        |el, z| jadix::app::reactive::apply_bound_z_index(el, z),
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
                            if matches!(name_str.as_str(), "StepperItem" | "TimelineItem") {
                                // Collect-then-paint: the marker is discarded by *_scope.
                                collect_item_class = Some(value);
                            } else {
                                chrome_binds.push(quote_apply_class_list(value));
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
                                Some("snug") => quote! { b = b.leading_snug(); },
                                Some("normal") => quote! { b = b.leading_normal(); },
                                Some("relaxed") => quote! { b = b.leading_relaxed(); },
                                Some("loose") => quote! { b = b.leading_loose(); },
                                _ => {
                                    quote! { b = b.leading(jadix::core::tree::IntoF32Raw::into_f32_raw(#value)); }
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
                                "ringWidth" => "ring_width",
                                "ringColor" => "ring_color",
                                "caretColor" => "caret_color",
                                "selectionBackground" => "selection_background",
                                "selectionForeground" => "selection_foreground",
                                _ => attr_name,
                            };
                            let method_ident = syn::Ident::new(method, attr.span());
                            let final_value = match method {
                                "bg" | "border" | "shadow" => {
                                    quote! { jadix::core::tree::parse_color(#value) }
                                }
                                "border_color"
                                | "border_top_color"
                                | "border_right_color"
                                | "border_bottom_color"
                                | "border_left_color"
                                | "outline_color"
                                | "box_shadow_color"
                                | "ring_color" => {
                                    quote! { jadix::core::tree::parse_color(#value) }
                                }
                                "text_color"
                                | "text_decoration_color"
                                | "text_shadow_color"
                                | "caret_color"
                                | "selection_background"
                                | "selection_foreground" => {
                                    // 字面量走 parse；`{ ... }` 表达式假定已是 `[u8; 4]`（避免双重 parse）
                                    if let syn::Expr::Lit(syn::ExprLit {
                                        lit: syn::Lit::Str(lit),
                                        ..
                                    }) = value
                                    {
                                        quote! { jadix::core::tree::parse_text_color(#lit) }
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
                                        quote! { jadix::core::tree::parse_bool_like(#value) }
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

                        if name_str == "App" {
                            let prop = match attr_str.as_str() {
                                "font" | "fontFamily" => "font_family",
                                "fontSize" => "font_size",
                                "fontWeight" => "font_weight",
                                "syntheticBold" => "synthetic_bold",
                                other => other,
                            };
                            let ident = syn::Ident::new(prop, attr.span());
                            let final_value = match prop {
                                "font_size" | "width" | "height" => {
                                    if expr_reads_get(value) {
                                        quote! { Some(jadix::core::reactivity::untrack(|| (#value) as f64)) }
                                    } else {
                                        quote! { Some((#value) as f64) }
                                    }
                                }
                                "font_weight" => {
                                    if expr_reads_get(value) {
                                        quote! { Some(jadix::core::reactivity::untrack(|| (#value) as u16)) }
                                    } else {
                                        quote! { Some((#value) as u16) }
                                    }
                                }
                                _ => {
                                    if expr_reads_get(value) {
                                        quote! { Some(jadix::core::reactivity::untrack(|| (#value).into())) }
                                    } else {
                                        quote! { Some((#value).into()) }
                                    }
                                }
                            };
                            prop_names.push(ident);
                            prop_values.push(final_value);
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
                            && matches!(
                                attr_str.as_str(),
                                "col_w" | "colW" | "column_width" | "columnWidth"
                            )
                        {
                            syn::Ident::new("col_w", attr.span())
                        } else if matches!(
                            name_str.as_str(),
                            "BarChart" | "LineChart" | "AreaChart" | "Chart"
                        ) && matches!(
                            attr_str.as_str(),
                            "plot_h" | "plotH" | "plot_height" | "plotHeight"
                        ) {
                            syn::Ident::new("plot_h", attr.span())
                        } else if name_str == "Sidebar"
                            && matches!(
                                attr_str.as_str(),
                                "rail_w" | "railW" | "rail_width" | "railWidth"
                            )
                        {
                            syn::Ident::new("rail_w", attr.span())
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
                                "scrollIntoView" => {
                                    syn::Ident::new("scroll_into_view", attr.span())
                                }
                                "scrollAlign" => syn::Ident::new("scroll_align", attr.span()),
                                "scrollIntoViewWhen" => {
                                    syn::Ident::new("scroll_into_view_when", attr.span())
                                }
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
                            quote! { Some(jadix::core::tree::Handler::from(jadix::core::tree::ZeroArgFn(#value))) }
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
                                quote! { Some(jadix::core::tree::parse_bool_like(#value)) }
                            } else if expr_reads_get(value) {
                                quote! { Some(jadix::core::reactivity::untrack(|| (#value).into())) }
                            } else {
                                quote! { Some((#value).into()) }
                            }
                        } else if matches!(final_attr_name.as_str(), "open_delay" | "close_delay") {
                            // Unsuffixed `{10}` → u64（不必写 `10_u64`）；与 Carousel autoplay 一致。
                            if expr_reads_get(value) {
                                quote! { Some(jadix::core::reactivity::untrack(|| (#value) as u64)) }
                            } else {
                                quote! { Some((#value) as u64) }
                            }
                        } else if matches!(final_attr_name.as_str(), "side_offset" | "align_offset")
                        {
                            // Length：`{4}` ≡ 4px；无单位字符串 `"4"` = rem。
                            let v = quote_length_f32(value);
                            if expr_reads_get(value) {
                                quote! {
                                    Some(jadix::core::reactivity::untrack(|| { #v }))
                                }
                            } else {
                                quote! { Some(#v) }
                            }
                        } else if expr_reads_get(value) {
                            // Avoid subscribing the parent effect during props construction.
                            quote! { Some(jadix::core::reactivity::untrack(|| (#value).into())) }
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
                        format!("__jadix_input_auto_{}", #seq)
                    });
                }

                if name_str == "Checkbox" && !prop_names.iter().any(|p| *p == "id") {
                    let seq = CHECKBOX_RSX_ANON_SEQ.fetch_add(1, Ordering::Relaxed);
                    prop_names.push(syn::Ident::new("id", name_ident.span()));
                    prop_values.push(quote! {
                        format!("__jadix_checkbox_auto_{}", #seq)
                    });
                }

                if name_str == "Switch" && !prop_names.iter().any(|p| *p == "id") {
                    let seq = SWITCH_RSX_ANON_SEQ.fetch_add(1, Ordering::Relaxed);
                    prop_names.push(syn::Ident::new("id", name_ident.span()));
                    prop_values.push(quote! {
                        format!("__jadix_switch_auto_{}", #seq)
                    });
                }

                if name_str == "Button" && !prop_names.iter().any(|p| *p == "id") {
                    let seq = BUTTON_RSX_ANON_SEQ.fetch_add(1, Ordering::Relaxed);
                    prop_names.push(syn::Ident::new("id", name_ident.span()));
                    prop_values.push(quote! {
                        format!("__jadix_button_auto_{}", #seq)
                    });
                }

                let base_expr = if base_calls.is_empty() {
                    quote! { None }
                } else {
                    quote! { Some(Box::new(|mut b: jadix::components::base::BaseComponentBuilder| { #(#base_calls)* b })) }
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
                    // Multi-child → `jadix_fragment` (single-root children slot). Layout
                    // primitives HStack/VStack/GStack use the primitive path below.
                    // Runtime unpacks via `jadix::utils::take_packed` / `take_fragment`.
                    if expanded_children.len() == 1 {
                        let first = &expanded_children[0];
                        children_expr = quote! {
                            Some({
                                let mut __list = jadix::core::tree::build_ui_child(|| (#first));
                                if __list.len() == 1 {
                                    __list.remove(0)
                                } else {
                                    let mut __frag = jadix::core::tree::Element::new();
                                    __frag.set_attr("jadix_fragment", "1");
                                    __frag.children(__list)
                                }
                            })
                        };
                    } else {
                        children_expr = quote! {
                            Some({
                                let mut __list: Vec<jadix::core::tree::Element> = Vec::new();
                                #( __list.extend(jadix::core::tree::build_ui_child(|| (#expanded_children))); )*
                                if __list.len() == 1 {
                                    __list.remove(0)
                                } else {
                                    let mut __frag = jadix::core::tree::Element::new();
                                    __frag.set_attr("jadix_fragment", "1");
                                    __frag.children(__list)
                                }
                            })
                        };
                    }
                }

                if matches!(name_str.as_str(), "MediaPlayer" | "AudioPlayer" | "VideoPlayer") && !el.children.is_empty() {
                    prop_names.push(syn::Ident::new("body", name_ident.span()));
                    prop_values.push(quote! { Some(Box::new(move || (#children_expr).unwrap_or_default())) });
                    children_expr = quote! { None };
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
                        if __jadix_el.template_id.is_none() {
                            __jadix_el = __jadix_el.template_id(
                                format!("{}:{}:ref#{}", file!(), line!(), #idx),
                            );
                        }
                        jadix::core::tree::bind_element_ref(&(#ref_target), &__jadix_el);
                    }
                } else {
                    quote! {}
                };

                let collect_chrome_ts = if let Some(c) = collect_item_class {
                    quote! {
                        {
                            let __jadix_sc_class: Option<String> = Some((#c).to_string());
                            jadix::components::base::push_pending_item_chrome(
                                jadix::components::base::SlotChrome::from_opts(__jadix_sc_class, None)
                            );
                        }
                    }
                } else {
                    quote! {}
                };

                if chrome_binds.is_empty() && ref_expr.is_none() {
                    return quote! {
                        {
                            #collect_chrome_ts
                            #[allow(clippy::needless_update)]
                            let __jadix_comp = #name_ident(#props_ident {
                                base: #base_expr,
                                children: jadix::utils::children_none_if_vacuous(#children_expr),
                                #(#prop_names: #prop_values,)*
                                #rest_ts
                            });
                            jadix::style::apply_stylesheet_rules_tree(__jadix_comp)
                        }
                    };
                }
                return quote! {
                    {
                        #collect_chrome_ts
                        #[allow(clippy::needless_update)]
                        let mut __jadix_el: jadix::core::tree::Element = #name_ident(#props_ident {
                            base: #base_expr,
                            children: jadix::utils::children_none_if_vacuous(#children_expr),
                            #(#prop_names: #prop_values,)*
                            #rest_ts
                        }).into();
                        __jadix_el = jadix::style::apply_stylesheet_rules_tree(__jadix_el);
                        #(#chrome_binds)*
                        #ref_bind_ts
                        __jadix_el
                    }
                };
            } else {
                match name_str.as_str() {
                    "VStack" => quote! { jadix::primitives::vstack() },
                    "HStack" => quote! { jadix::primitives::hstack() },
                    // Grid 为 GStack 的 HTML/Solid 风格别名。
                    "GStack" | "Grid" => quote! { jadix::primitives::gstack() },
                    // Layout container. `Rect` is SVG — use `<Box>`.
                    // HTML/Solid `<div>` / `<Div>` are remapped to `Box` above.
                    "Box" => quote! { jadix::primitives::r#box() },
                    "Interactive" => {
                        quote! { jadix::primitives::interactive(jadix::core::tree::Element::new()) }
                    }
                    "Text" => quote! { jadix::primitives::text("") },
                    // Public SVG tags exist only with jadix `svg`. Do not emit
                    // `#[cfg(feature = "svg")]` — that would check the consuming crate.
                    "svg" | "Svg" => quote! { jadix::primitives::svg() },
                    "circle" | "Circle" => quote! { jadix::primitives::svg::circle() },
                    "ellipse" | "Ellipse" => quote! { jadix::primitives::svg::ellipse() },
                    "path" | "Path" => quote! { jadix::primitives::svg::path() },
                    "rect" | "Rect" => quote! { jadix::primitives::svg::rect() },
                    "line" | "Line" => quote! { jadix::primitives::svg::line() },
                    "polyline" | "Polyline" => quote! { jadix::primitives::svg::polyline() },
                    "polygon" | "Polygon" => quote! { jadix::primitives::svg::polygon() },
                    "g" => quote! { jadix::primitives::svg::g() },
                    "defs" | "Defs" => quote! { jadix::primitives::svg::defs() },
                    "use" | "Use" => quote! { jadix::primitives::svg::svg_use() },
                    "clipPath" | "ClipPath" => quote! { jadix::primitives::svg::clip_path() },
                    "linearGradient" | "LinearGradient" => {
                        quote! { jadix::primitives::svg::linear_gradient() }
                    }
                    "radialGradient" | "RadialGradient" => {
                        quote! { jadix::primitives::svg::radial_gradient() }
                    }
                    "symbol" | "Symbol" => quote! { jadix::primitives::svg::symbol() },
                    "stop" | "Stop" => quote! { jadix::primitives::svg::stop() },
                    "animate" | "Animate" => quote! { jadix::primitives::svg::animate() },
                    "animateTransform" | "AnimateTransform" => {
                        quote! { jadix::primitives::svg::animate_transform() }
                    }
                    "animateMotion" | "AnimateMotion" => {
                        quote! { jadix::primitives::svg::animate_motion() }
                    }
                    "mpath" | "Mpath" | "MPath" => quote! { jadix::primitives::svg::mpath() },
                    "set" | "Set" => quote! { jadix::primitives::svg::set_el() },
                    "text" => quote! { jadix::primitives::svg::svg_text() },
                    "tspan" | "Tspan" | "TSpan" => quote! { jadix::primitives::svg::tspan() },
                    "textPath" | "TextPath" => quote! { jadix::primitives::svg::text_path() },
                    "image" => quote! { jadix::primitives::svg::image() },
                    "mask" | "Mask" => quote! { jadix::primitives::svg::mask() },
                    "pattern" | "Pattern" => quote! { jadix::primitives::svg::pattern() },
                    "marker" => quote! { jadix::primitives::svg::marker() },
                    "filter" | "Filter" => quote! { jadix::primitives::svg::filter() },
                    "title" | "Title" => quote! { jadix::primitives::svg::title() },
                    "desc" | "Desc" => quote! { jadix::primitives::svg::desc() },
                    "feGaussianBlur" | "FeGaussianBlur" => {
                        quote! { jadix::primitives::svg::fe_gaussian_blur() }
                    }
                    "feDropShadow" | "FeDropShadow" => {
                        quote! { jadix::primitives::svg::fe_drop_shadow() }
                    }
                    "feOffset" | "FeOffset" => quote! { jadix::primitives::svg::fe_offset() },
                    "feBlend" | "FeBlend" => quote! { jadix::primitives::svg::fe_blend() },
                    "feColorMatrix" | "FeColorMatrix" => {
                        quote! { jadix::primitives::svg::fe_color_matrix() }
                    }
                    "feFlood" | "FeFlood" => quote! { jadix::primitives::svg::fe_flood() },
                    "feMerge" | "FeMerge" => quote! { jadix::primitives::svg::fe_merge() },
                    "feMergeNode" | "FeMergeNode" => {
                        quote! { jadix::primitives::svg::fe_merge_node() }
                    }
                    "canvas" | "Canvas" => quote! { jadix::primitives::canvas() },
                    "scene3d" | "Scene3d" => quote! { jadix::primitives::scene3d() },
                    "shader" | "Shader" => quote! { jadix::primitives::shader() },
                    other => {
                        if let Some(tag) = svg_tag_canonical(other) {
                            quote! { jadix::primitives::svg::svg_el(#tag) }
                        } else {
                            quote! { jadix::core::tree::Element::new() }
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
                            let __src = jadix::core::reactivity::untrack(|| (#src_expr).to_string());
                            jadix::utils::build_icon_from_src(__src)
                        }
                    };
                    pending_icon_bind = Some(quote! {
                        __jadix_el = jadix::app::reactive::bind_attr(
                            __jadix_el,
                            format!("{}:{}:bind_icon_src#{}", file!(), line!(), #idx),
                            move || (#src_expr).to_string(),
                            |el, src| jadix::utils::apply_bound_icon_src(el, src),
                        );
                    });
                } else if let Some(src) = static_src {
                    element = quote! { jadix::utils::build_icon_from_src(#src) };
                } else if let Some(name_expr) = dynamic_name {
                    let idx = *node_counter;
                    *node_counter += 1;
                    element = quote! {
                        {
                            let __n = jadix::core::reactivity::untrack(|| (#name_expr).to_string());
                            jadix::utils::build_icon_named(__n)
                        }
                    };
                    pending_icon_bind = Some(quote! {
                        __jadix_el = jadix::app::reactive::bind_attr(
                            __jadix_el,
                            format!("{}:{}:bind_icon_name#{}", file!(), line!(), #idx),
                            move || (#name_expr).to_string(),
                            |el, name| jadix::utils::apply_bound_icon_name(el, name),
                        );
                    });
                } else if let Some(iname) = static_icon_name {
                    element = quote! { jadix::utils::build_icon_named(#iname) };
                } else {
                    element = quote! {
                        jadix::utils::text_node::build_icon_text_node("\u{ea0e}")
                    };
                }
                let mut hover = quote! { "none" };
                let mut change = quote! { "none" };
                let mut spring = quote! { "snappy" };
                let site_idx = *node_counter;
                *node_counter += 1;
                let mut motion_id =
                    quote! { format!("{}:{}:Icon#{}", file!(), line!(), #site_idx) };
                let mut motion = false;
                for (attr, value) in &el.attributes {
                    match attr.to_string().as_str() {
                        "hover_animation" | "hoverAnimation" | "hover-animation" => {
                            hover = quote! { #value };
                            motion = true;
                        }
                        "change_animation" | "changeAnimation" | "change-animation" => {
                            change = quote! { #value };
                            motion = true;
                        }
                        "spring" => spring = quote! { #value },
                        "id" => motion_id = quote! { #value },
                        _ => {}
                    }
                }
                if motion {
                    element = quote! {
                        jadix::utils::with_icon_animations(
                            jadix::core::tree::Element::from(#element),
                            #hover, #change, #motion_id, #spring,
                        )
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
                            size_ts = quote! { jadix::core::tree::IntoF32::into_f32(#value) };
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
                        let mut __b = jadix::utils::animated_icon(#name_ts)
                            .size_px(#size_ts);
                        if #controlled_ts {
                            __b = __b.controlled(true);
                        }
                        #pe_chain
                        jadix::core::tree::Element::from(__b)
                    }
                };
            }

            // Expands to `jadix::utils::morph_icon` (compiled only with jadix `svg`).
            // Do not emit `#[cfg(feature = "svg")]` here — that would check the
            // consuming crate's features, not jadix's.
            if name_str == "MorphIcon" {
                let mut icon_ts = quote! { "" };
                let mut icon_bind: Option<&syn::Expr> = None;
                let mut from_chain = quote! {};
                let mut from_bind: Option<&syn::Expr> = None;
                let mut to_chain = quote! {};
                let mut to_bind: Option<&syn::Expr> = None;
                let mut progress_chain = quote! {};
                let mut progress_bind: Option<&syn::Expr> = None;
                let mut size_ts = quote! { 24.0_f32 };
                let mut spring_chain = quote! {};
                let mut spring_bind: Option<&syn::Expr> = None;
                let mut stroke_chain = quote! {};
                let mut stroke_bind: Option<&syn::Expr> = None;
                let mut abs_chain = quote! {};
                let mut abs_bind: Option<&syn::Expr> = None;
                let mut interp_chain = quote! {};
                let mut interp_bind: Option<&syn::Expr> = None;
                let mut reduced_chain = quote! {};
                let mut reduced_bind: Option<&syn::Expr> = None;
                let mut view_box_chain = quote! {};
                let mut view_box_bind: Option<&syn::Expr> = None;
                let mut label_chain = quote! {};
                let mut label_bind: Option<&syn::Expr> = None;
                let mut id_chain = quote! {};
                let mut hover_chain = quote! {};
                for (attr, value) in &el.attributes {
                    match attr.to_string().as_str() {
                        "icon" => {
                            if expr_reads_get(value) {
                                icon_bind = Some(value);
                                icon_ts = quote! {
                                    jadix::core::reactivity::untrack(|| (#value).to_string())
                                };
                            } else if let syn::Expr::Lit(expr_lit) = value
                                && let syn::Lit::Str(lit_str) = &expr_lit.lit
                            {
                                let n = lit_str.value();
                                icon_ts = quote! { #n };
                            } else {
                                icon_ts = quote! { (#value).to_string() };
                            }
                        }
                        "from" => {
                            if expr_reads_get(value) {
                                from_bind = Some(value);
                                from_chain = quote! {
                                    __b = __b.from_icon(
                                        jadix::core::reactivity::untrack(|| (#value).to_string())
                                    );
                                };
                            } else {
                                from_chain = quote! { __b = __b.from_icon((#value).to_string()); };
                            }
                        }
                        "to" => {
                            if expr_reads_get(value) {
                                to_bind = Some(value);
                                to_chain = quote! {
                                    __b = __b.to_icon(
                                        jadix::core::reactivity::untrack(|| (#value).to_string())
                                    );
                                };
                            } else {
                                to_chain = quote! { __b = __b.to_icon((#value).to_string()); };
                            }
                        }
                        "progress" => {
                            if expr_reads_get(value) {
                                progress_bind = Some(value);
                            } else {
                                progress_chain = quote! {
                                    __b = __b.progress(jadix::core::tree::IntoF32::into_f32(#value));
                                };
                            }
                        }
                        "size" => {
                            size_ts = quote! { jadix::core::tree::IntoF32::into_f32(#value) };
                        }
                        "spring" => {
                            if expr_reads_get(value) {
                                spring_bind = Some(value);
                                spring_chain = quote! {
                                    __b = __b.spring(
                                        jadix::core::reactivity::untrack(|| (#value).to_string())
                                    );
                                };
                            } else {
                                spring_chain = quote! { __b = __b.spring((#value).to_string()); };
                            }
                        }
                        "stroke_width" | "strokeWidth" | "stroke-width" => {
                            if expr_reads_get(value) {
                                stroke_bind = Some(value);
                            }
                            stroke_chain = quote! {
                                __b = __b.stroke_width(jadix::core::tree::IntoF32::into_f32(#value));
                            };
                        }
                        "absolute_stroke_width"
                        | "absoluteStrokeWidth"
                        | "absolute-stroke-width" => {
                            if expr_reads_get(value) {
                                abs_bind = Some(value);
                            }
                            abs_chain = quote! {
                                __b = __b.absolute_stroke_width(#value);
                            };
                        }
                        "view_box" | "viewBox" | "view-box" => {
                            if expr_reads_get(value) {
                                view_box_bind = Some(value);
                                view_box_chain = quote! {
                                    __b = __b.view_box(
                                        jadix::core::reactivity::untrack(|| (#value).to_string())
                                    );
                                };
                            } else {
                                view_box_chain =
                                    quote! { __b = __b.view_box((#value).to_string()); };
                            }
                        }
                        "label" => {
                            if expr_reads_get(value) {
                                label_bind = Some(value);
                                label_chain = quote! {
                                    __b = __b.label(
                                        jadix::core::reactivity::untrack(|| (#value).to_string())
                                    );
                                };
                            } else {
                                label_chain = quote! { __b = __b.label((#value).to_string()); };
                            }
                        }
                        "interp" => {
                            if expr_reads_get(value) {
                                interp_bind = Some(value);
                                interp_chain = quote! {
                                    __b = __b.interp(
                                        jadix::core::reactivity::untrack(|| (#value).to_string())
                                    );
                                };
                            } else {
                                interp_chain = quote! { __b = __b.interp((#value).to_string()); };
                            }
                        }
                        "reduced_motion" | "reducedMotion" | "reduced-motion" => {
                            if expr_reads_get(value) {
                                reduced_bind = Some(value);
                                reduced_chain = quote! {
                                    __b = __b.reduced_motion(
                                        jadix::core::reactivity::untrack(|| (#value).to_string())
                                    );
                                };
                            } else {
                                reduced_chain =
                                    quote! { __b = __b.reduced_motion((#value).to_string()); };
                            }
                        }
                        "id" => {
                            id_chain = quote! { __b = __b.id((#value).to_string()); };
                        }
                        "hover_animation" | "hoverAnimation" | "hover-animation" => {
                            hover_chain = quote! { __b = __b.hover_animation(#value); };
                        }
                        _ => {}
                    }
                }
                let site_idx = *node_counter;
                *node_counter += 1;
                if id_chain.is_empty() {
                    id_chain = quote! {
                        __b = __b.id(format!("{}:{}:MorphIcon#{}", file!(), line!(), #site_idx));
                    };
                }
                let mut binds = Vec::new();
                if let Some(expr) = from_bind {
                    let idx = *node_counter;
                    *node_counter += 1;
                    binds.push(quote! {
                        __el = jadix::app::reactive::bind_attr(
                            __el,
                            format!("{}:{}:bind_morph_from#{}", file!(), line!(), #idx),
                            move || (#expr).to_string(),
                            |el, name| jadix::utils::apply_bound_morph_from(el, name),
                        );
                    });
                }
                if let Some(expr) = to_bind {
                    let idx = *node_counter;
                    *node_counter += 1;
                    binds.push(quote! {
                        __el = jadix::app::reactive::bind_attr(
                            __el,
                            format!("{}:{}:bind_morph_to#{}", file!(), line!(), #idx),
                            move || (#expr).to_string(),
                            |el, name| jadix::utils::apply_bound_morph_to(el, name),
                        );
                    });
                }
                if let Some(expr) = icon_bind {
                    let idx = *node_counter;
                    *node_counter += 1;
                    binds.push(quote! {
                        __el = jadix::app::reactive::bind_attr(
                            __el,
                            format!("{}:{}:bind_morph_icon#{}", file!(), line!(), #idx),
                            move || (#expr).to_string(),
                            |el, name| jadix::utils::apply_bound_morph_icon(el, name),
                        );
                    });
                }
                if let Some(expr) = spring_bind {
                    let idx = *node_counter;
                    *node_counter += 1;
                    binds.push(quote! {
                        __el = jadix::app::reactive::bind_attr(
                            __el,
                            format!("{}:{}:bind_morph_spring#{}", file!(), line!(), #idx),
                            move || (#expr).to_string(),
                            |el, name| jadix::utils::apply_bound_morph_spring(el, name),
                        );
                    });
                }
                if let Some(expr) = stroke_bind {
                    let idx = *node_counter;
                    *node_counter += 1;
                    binds.push(quote! {
                        __el = jadix::app::reactive::bind_attr(
                            __el,
                            format!("{}:{}:bind_morph_stroke#{}", file!(), line!(), #idx),
                            move || jadix::core::tree::IntoF32::into_f32(#expr),
                            |el, w| jadix::utils::apply_bound_morph_stroke(el, w),
                        );
                    });
                }
                if let Some(expr) = progress_bind {
                    let idx = *node_counter;
                    *node_counter += 1;
                    binds.push(quote! {
                        __el = jadix::app::reactive::bind_attr(
                            __el,
                            format!("{}:{}:bind_morph_progress#{}", file!(), line!(), #idx),
                            move || jadix::core::tree::IntoF32::into_f32(#expr),
                            |el, p| jadix::utils::apply_bound_morph_progress(el, p),
                        );
                    });
                }
                if let Some(expr) = interp_bind {
                    let idx = *node_counter;
                    *node_counter += 1;
                    binds.push(quote! {
                        __el = jadix::app::reactive::bind_attr(
                            __el,
                            format!("{}:{}:bind_morph_interp#{}", file!(), line!(), #idx),
                            move || (#expr).to_string(),
                            |el, kind| jadix::utils::apply_bound_morph_interp(el, kind),
                        );
                    });
                }
                if let Some(expr) = reduced_bind {
                    let idx = *node_counter;
                    *node_counter += 1;
                    binds.push(quote! {
                        __el = jadix::app::reactive::bind_attr(
                            __el,
                            format!("{}:{}:bind_morph_reduced#{}", file!(), line!(), #idx),
                            move || (#expr).to_string(),
                            |el, mode| jadix::utils::apply_bound_morph_reduced(el, mode),
                        );
                    });
                }
                if let Some(expr) = abs_bind {
                    let idx = *node_counter;
                    *node_counter += 1;
                    binds.push(quote! {
                        __el = jadix::app::reactive::bind_attr(
                            __el,
                            format!("{}:{}:bind_morph_abs_stroke#{}", file!(), line!(), #idx),
                            move || #expr,
                            |el, on| jadix::utils::apply_bound_morph_abs_stroke(el, on),
                        );
                    });
                }
                if let Some(expr) = view_box_bind {
                    let idx = *node_counter;
                    *node_counter += 1;
                    binds.push(quote! {
                        __el = jadix::app::reactive::bind_attr(
                            __el,
                            format!("{}:{}:bind_morph_view_box#{}", file!(), line!(), #idx),
                            move || (#expr).to_string(),
                            |el, vb| jadix::utils::apply_bound_morph_view_box(el, vb),
                        );
                    });
                }
                if let Some(expr) = label_bind {
                    let idx = *node_counter;
                    *node_counter += 1;
                    binds.push(quote! {
                        __el = jadix::app::reactive::bind_attr(
                            __el,
                            format!("{}:{}:bind_morph_label#{}", file!(), line!(), #idx),
                            move || (#expr).to_string(),
                            |el, name| jadix::utils::apply_bound_morph_label(el, name),
                        );
                    });
                }
                element = quote! {
                    {
                        let mut __el = {
                            let mut __b = jadix::utils::morph_icon(#icon_ts)
                                .size_px(#size_ts);
                            #from_chain
                            #to_chain
                            #progress_chain
                            #spring_chain
                            #stroke_chain
                            #abs_chain
                            #interp_chain
                            #reduced_chain
                            #view_box_chain
                            #label_chain
                            #id_chain
                            #hover_chain
                            jadix::core::tree::Element::from(__b)
                        };
                        #(#binds)*
                        __el
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
                            __jadix_el = jadix::app::reactive::bind_class(
                                __jadix_el,
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
                        if __jadix_el.template_id.is_none() {
                            __jadix_el = __jadix_el.template_id(
                                format!("{}:{}:ref#{}", file!(), line!(), #idx),
                            );
                        }
                        jadix::core::tree::bind_element_ref(&(#value), &__jadix_el);
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
                if is_shader_tag(&name_str) && attr_str.as_str() == "wgsl" {
                    element = quote! { #element.wgsl(#value) };
                    continue;
                }
                if is_shader_tag(&name_str) && matches!(attr_str.as_str(), "texture" | "src") {
                    element = quote! { #element.texture("src", #value) };
                    continue;
                }
                if is_shader_tag(&name_str) && matches!(attr_str.as_str(), "texture1" | "src1") {
                    element = quote! { #element.texture("src1", #value) };
                    continue;
                }
                if is_shader_tag(&name_str) && matches!(attr_str.as_str(), "texture2" | "src2") {
                    element = quote! { #element.texture("src2", #value) };
                    continue;
                }
                if is_shader_tag(&name_str) && attr_str.as_str() == "preset" {
                    element = quote! { #element.preset(#value) };
                    continue;
                }
                if is_shader_tag(&name_str) && attr_str.as_str() == "buffer" {
                    element = quote! { #element.buffer(#value) };
                    continue;
                }
                match attr_str.as_str() {
                    "w" | "width" => {
                        if expr_reads_get(value) && parse_min_max_call(value).is_none() {
                            let idx = *node_counter;
                            *node_counter += 1;
                            let axis = "w";
                            dynamic_binds.push(quote! {
                                __jadix_el = jadix::app::reactive::bind_prop(
                                    __jadix_el,
                                    format!("{}:{}:bind_size_w#{}", file!(), line!(), #idx),
                                    move || {
                                        jadix::core::tree::IntoCssDim::into_css_dim(#value)
                                    },
                                    |el, dim| jadix::app::reactive::apply_bound_size(el, #axis, dim),
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
                                __jadix_el = jadix::app::reactive::bind_prop(
                                    __jadix_el,
                                    format!("{}:{}:bind_size_inline#{}", file!(), line!(), #idx),
                                    move || {
                                        jadix::core::tree::IntoCssDim::into_css_dim(#value)
                                    },
                                    |el, dim| jadix::app::reactive::apply_bound_size(el, #axis, dim),
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
                                __jadix_el = jadix::app::reactive::bind_prop(
                                    __jadix_el,
                                    format!("{}:{}:bind_size_max_w#{}", file!(), line!(), #idx),
                                    move || {
                                        jadix::core::tree::IntoCssDim::into_css_dim(#value)
                                    },
                                    |el, dim| jadix::app::reactive::apply_bound_size(el, #axis, dim),
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
                                __jadix_el = jadix::app::reactive::bind_prop(
                                    __jadix_el,
                                    format!("{}:{}:bind_size_min_w#{}", file!(), line!(), #idx),
                                    move || {
                                        jadix::core::tree::IntoCssDim::into_css_dim(#value)
                                    },
                                    |el, dim| jadix::app::reactive::apply_bound_size(el, #axis, dim),
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
                                __jadix_el = jadix::app::reactive::bind_prop(
                                    __jadix_el,
                                    format!("{}:{}:bind_size_h#{}", file!(), line!(), #idx),
                                    move || {
                                        jadix::core::tree::IntoCssDim::into_css_dim(#value)
                                    },
                                    |el, dim| jadix::app::reactive::apply_bound_size(el, #axis, dim),
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
                                __jadix_el = jadix::app::reactive::bind_prop(
                                    __jadix_el,
                                    format!("{}:{}:bind_size_block#{}", file!(), line!(), #idx),
                                    move || {
                                        jadix::core::tree::IntoCssDim::into_css_dim(#value)
                                    },
                                    |el, dim| jadix::app::reactive::apply_bound_size(el, #axis, dim),
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
                                __jadix_el = jadix::app::reactive::bind_prop(
                                    __jadix_el,
                                    format!("{}:{}:bind_size_max_h#{}", file!(), line!(), #idx),
                                    move || {
                                        jadix::core::tree::IntoCssDim::into_css_dim(#value)
                                    },
                                    |el, dim| jadix::app::reactive::apply_bound_size(el, #axis, dim),
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
                                __jadix_el = jadix::app::reactive::bind_prop(
                                    __jadix_el,
                                    format!("{}:{}:bind_size_min_h#{}", file!(), line!(), #idx),
                                    move || {
                                        jadix::core::tree::IntoCssDim::into_css_dim(#value)
                                    },
                                    |el, dim| jadix::app::reactive::apply_bound_size(el, #axis, dim),
                                );
                            });
                        } else {
                            element = quote! { #element.min_h(#value) };
                        }
                    }
                    "opacity" => {
                        if expr_reads_get(value) {
                            let idx = *node_counter;
                            *node_counter += 1;
                            dynamic_binds.push(quote! {
                                __jadix_el = jadix::app::reactive::bind_prop(
                                    __jadix_el,
                                    format!("{}:{}:bind_opacity#{}", file!(), line!(), #idx),
                                    move || {
                                        jadix::core::tree::IntoOpacity::into_opacity(#value)
                                    },
                                    |el, o| jadix::app::reactive::apply_bound_opacity(el, o),
                                );
                            });
                        } else {
                            element = quote! { #element.opacity(#value) };
                        }
                    }
                    "bg" => {
                        if expr_reads_get(value) {
                            let idx = *node_counter;
                            *node_counter += 1;
                            dynamic_binds.push(quote! {
                                __jadix_el = jadix::app::reactive::bind_prop(
                                    __jadix_el,
                                    format!("{}:{}:bind_bg#{}", file!(), line!(), #idx),
                                    move || {
                                        let __v = (#value).to_string();
                                        jadix::core::tree::parse_color(__v.as_str())
                                    },
                                    |el, color| {
                                        if el.style.bg_color == color {
                                            jadix::core::tree::LayoutStatus::Clean
                                        } else {
                                            el.style.bg_color = color;
                                            jadix::core::tree::LayoutStatus::StyleChanged
                                        }
                                    },
                                );
                            });
                        } else {
                            element =
                                quote! { #element.bg(jadix::core::tree::parse_color(#value)) };
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
                                __jadix_el = jadix::app::reactive::bind_prop(
                                    __jadix_el,
                                    format!("{}:{}:bind_border_color#{}", file!(), line!(), #idx),
                                    move || {
                                        let __v = (#value).to_string();
                                        jadix::core::tree::parse_color(__v.as_str())
                                    },
                                    |el, color| {
                                        if el.style.border_color == color {
                                            jadix::core::tree::LayoutStatus::Clean
                                        } else {
                                            el.style.border_color = color;
                                            jadix::core::tree::LayoutStatus::StyleChanged
                                        }
                                    },
                                );
                            });
                        } else {
                            element = quote! { #element.border_color(jadix::core::tree::parse_color(#value)) };
                        }
                    }
                    "border_width" | "borderWidth" => {
                        element = quote! { #element.border_width(#value) }
                    }
                    "border_top_width" | "borderTopWidth" | "border_top" | "borderTop" => {
                        element = quote! { #element.border_top_width(jadix::core::tree::IntoF32::into_f32(#value)) }
                    }
                    "border_right_width" | "borderRightWidth" | "border_right" | "borderRight" => {
                        element = quote! { #element.border_right_width(jadix::core::tree::IntoF32::into_f32(#value)) }
                    }
                    "border_bottom_width"
                    | "borderBottomWidth"
                    | "border_bottom"
                    | "borderBottom" => {
                        element = quote! { #element.border_bottom_width(jadix::core::tree::IntoF32::into_f32(#value)) }
                    }
                    "border_left_width" | "borderLeftWidth" | "border_left" | "borderLeft" => {
                        element = quote! { #element.border_left_width(jadix::core::tree::IntoF32::into_f32(#value)) }
                    }
                    "border_top_color" | "borderTopColor" => {
                        element = quote! { #element.border_top_color(jadix::core::tree::parse_color(#value)) }
                    }
                    "border_right_color" | "borderRightColor" => {
                        element = quote! { #element.border_right_color(jadix::core::tree::parse_color(#value)) }
                    }
                    "border_bottom_color" | "borderBottomColor" => {
                        element = quote! { #element.border_bottom_color(jadix::core::tree::parse_color(#value)) }
                    }
                    "border_left_color" | "borderLeftColor" => {
                        element = quote! { #element.border_left_color(jadix::core::tree::parse_color(#value)) }
                    }
                    "outline_style" | "outlineStyle" => {
                        element = quote! { #element.outline_style(#value) }
                    }
                    "outline_color" | "outlineColor" => {
                        element = quote! { #element.outline_color(jadix::core::tree::parse_color(#value)) }
                    }
                    "outline_width" | "outlineWidth" => {
                        element = quote! { #element.outline_width(jadix::core::tree::IntoF32::into_f32(#value)) }
                    }
                    "outline_offset" | "outlineOffset" => {
                        element = quote! { #element.outline_offset(jadix::core::tree::IntoF32::into_f32(#value)) }
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
                                __jadix_el = jadix::app::reactive::bind_prop(
                                    __jadix_el,
                                    format!(
                                        "{}:{}:bind_animation_play_state#{}",
                                        file!(),
                                        line!(),
                                        #idx
                                    ),
                                    move || (#value).to_string(),
                                    |el, v| {
                                        jadix::app::reactive::apply_bound_animation_play_state(el, v)
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
                                __jadix_el = jadix::app::reactive::bind_prop(
                                    __jadix_el,
                                    format!("{}:{}:bind_color#{}", file!(), line!(), #idx),
                                    move || {
                                        let __v = (#value).to_string();
                                        jadix::core::tree::parse_text_color(__v.as_str())
                                    },
                                    |el, color| {
                                        if el.text_style.color == color {
                                            jadix::core::tree::LayoutStatus::Clean
                                        } else {
                                            el.text_style.color = color;
                                            jadix::core::tree::LayoutStatus::StyleChanged
                                        }
                                    },
                                );
                            });
                        } else {
                            element = quote! { #element.text_color(jadix::core::tree::parse_text_color(#value)) };
                        }
                    }
                    "font_weight" | "text_weight" => {
                        // Unitless（Tailwind `font-[600]`）；走 IntoF32Raw，勿用长度 rem 路径。
                        element = quote! {
                            #element.font_weight(jadix::core::tree::IntoF32Raw::into_f32_raw(#value))
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
                        element = quote! { #element.text_decoration_color(jadix::core::tree::parse_text_color(#value)) }
                    }
                    "text_decoration_thickness" | "textDecorationThickness" => {
                        element = quote! { #element.text_decoration_thickness(jadix::core::tree::IntoF32::into_f32(#value)) }
                    }
                    "text_underline_offset" | "textUnderlineOffset" => {
                        element = quote! { #element.text_underline_offset(jadix::core::tree::IntoF32::into_f32(#value)) }
                    }
                    "line_height" | "lineHeight" | "lh" | "text_line_height" => {
                        if is_custom {
                            element = quote! { #element.line_height(jadix::core::tree::IntoF32::into_f32(#value)) }
                        } else {
                            element = quote! { #element.text_line_height(jadix::core::tree::IntoF32::into_f32(#value)) }
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
                        if name_str == "AnimatedIcon" || name_str == "MorphIcon" {
                            // Consumed by AnimatedIcon / MorphIcon pre-pass (builder size_px).
                        } else if name_str == "Icon" {
                            element = quote! {
                                jadix::utils::with_icon_size(
                                    #element,
                                    jadix::core::tree::IntoF32::into_f32(#value),
                                )
                            }
                        } else if is_custom || name_str == "Button" || name_str == "Input" {
                            element = quote! { #element.size(#value) }
                        } else {
                            element = quote! { #element.text_size(jadix::core::tree::IntoF32::into_f32(#value)) }
                        }
                    }
                    "hover_animation" | "hoverAnimation" | "hover-animation"
                    | "change_animation" | "changeAnimation" | "change-animation" | "spring"
                    | "id"
                        if name_str == "Icon" => {}
                    "controlled" if name_str == "AnimatedIcon" => {}
                    "pointer_events" | "pointerEvents" | "pointer-events"
                        if name_str == "AnimatedIcon" => {}
                    "icon"
                    | "from"
                    | "to"
                    | "progress"
                    | "spring"
                    | "stroke_width"
                    | "strokeWidth"
                    | "stroke-width"
                    | "id"
                    | "interp"
                    | "reduced_motion"
                    | "reducedMotion"
                    | "reduced-motion"
                    | "absolute_stroke_width"
                    | "absoluteStrokeWidth"
                    | "absolute-stroke-width"
                    | "view_box"
                    | "viewBox"
                    | "view-box"
                    | "label"
                    | "hover_animation"
                    | "hoverAnimation"
                    | "hover-animation"
                        if name_str == "MorphIcon" => {}
                    "text_size" | "font_size" => {
                        element = quote! { #element.text_size(jadix::core::tree::IntoF32::into_f32(#value)) }
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
                                __jadix_el = jadix::app::reactive::bind_prop(
                                    __jadix_el,
                                    format!("{}:{}:bind_aspect_ratio#{}", file!(), line!(), #idx),
                                    move || (#value).to_string(),
                                    |el, s: String| {
                                        let next =
                                            jadix::core::tree::parse_css_aspect_ratio(s.as_str());
                                        if el.style.aspect_ratio == next {
                                            jadix::core::tree::LayoutStatus::Clean
                                        } else {
                                            el.style.aspect_ratio = next;
                                            jadix::core::tree::LayoutStatus::LayoutChanged
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
                        element = quote! { #element.scrollbar_thickness(jadix::core::tree::IntoF32::into_f32(#value)) }
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
                        element = quote! { #element.rounded(jadix::core::tree::IntoF32::into_f32(#value)) }
                    }
                    "shadow" => {
                        element = quote! { #element.shadow([0.0, 0.0, 0.0, 0.5], jadix::core::tree::IntoF32::into_f32(#value)) }
                    }
                    "box_shadow" | "boxShadow" | "box-shadow" => {
                        element = quote! { #element.box_shadow(#value) }
                    }
                    "box_shadow_color" | "boxShadowColor" => {
                        element = quote! { #element.box_shadow_color(jadix::core::tree::parse_color(#value)) }
                    }
                    "box_shadow_blur" | "boxShadowBlur" => {
                        element = quote! { #element.box_shadow_blur(jadix::core::tree::IntoF32::into_f32(#value)) }
                    }
                    "backdrop_blur" | "backdropBlur" => {
                        element = quote! { #element.backdrop_blur(jadix::core::tree::IntoF32::into_f32(#value)) }
                    }
                    "filter" => element = quote! { #element.filter(#value) },
                    "box_shadow_offset_x" | "boxShadowOffsetX" => {
                        element = quote! { #element.box_shadow_offset_x(jadix::core::tree::IntoF32::into_f32(#value)) }
                    }
                    "box_shadow_offset_y" | "boxShadowOffsetY" => {
                        element = quote! { #element.box_shadow_offset_y(jadix::core::tree::IntoF32::into_f32(#value)) }
                    }
                    "box_shadow_spread" | "boxShadowSpread" => {
                        element = quote! { #element.box_shadow_spread(jadix::core::tree::IntoF32::into_f32(#value)) }
                    }
                    "box_shadow_inset" | "boxShadowInset" => {
                        element = quote! { #element.box_shadow_inset(#value) }
                    }
                    "text_shadow" => element = quote! { #element.text_shadow(#value) },
                    "textShadow" | "text-shadow" => {
                        element = quote! { #element.text_shadow(#value) }
                    }
                    "text_shadow_color" | "textShadowColor" => {
                        element = quote! { #element.text_shadow_color(jadix::core::tree::parse_text_color(#value)) }
                    }
                    "text_shadow_blur" | "textShadowBlur" => {
                        element = quote! { #element.text_shadow_blur(jadix::core::tree::IntoF32::into_f32(#value)) }
                    }
                    "text_shadow_offset_x" | "textShadowOffsetX" => {
                        element = quote! { #element.text_shadow_offset_x(jadix::core::tree::IntoF32::into_f32(#value)) }
                    }
                    "text_shadow_offset_y" | "textShadowOffsetY" => {
                        element = quote! { #element.text_shadow_offset_y(jadix::core::tree::IntoF32::into_f32(#value)) }
                    }
                    "text_align" => element = quote! { #element.text_align(#value) },
                    "on_click" | "onClick" => element = quote! { #element.on_click(#value) },
                    "on_hover" | "onHover" => element = quote! { #element.on_hover(#value) },
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
                        element = quote! { #element.thickness(jadix::core::tree::IntoF32::into_f32(#value)) }
                    }
                    "track_color" => {
                        element =
                            quote! { #element.track_color(jadix::core::tree::parse_color(#value)) }
                    }
                    "thumb_color" => {
                        element =
                            quote! { #element.thumb_color(jadix::core::tree::parse_color(#value)) }
                    }
                    "track_border_color" | "trackBorderColor" => {
                        element = quote! {
                            #element.track_border_color(jadix::core::tree::parse_color(#value))
                        }
                    }
                    "thumb_border_color" | "thumbBorderColor" => {
                        element = quote! {
                            #element.thumb_border_color(jadix::core::tree::parse_color(#value))
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
                        element = quote! { #element.flex_grow(jadix::core::tree::IntoF32Raw::into_f32_raw(#value)) }
                    }
                    "shrink" | "flex_shrink" | "flexShrink" => {
                        element = quote! { #element.flex_shrink(jadix::core::tree::IntoF32Raw::into_f32_raw(#value)) }
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
                                #element.absolute(jadix::core::tree::parse_bool_like(#value))
                            };
                        }
                    }
                    "relative" => {
                        if let Some(b) = expr_as_bool_compat(value) {
                            element = quote! { #element.relative(#b) };
                        } else {
                            element = quote! {
                                #element.relative(jadix::core::tree::parse_bool_like(#value))
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
                                __jadix_el = jadix::app::reactive::bind_prop(
                                    __jadix_el,
                                    format!("{}:{}:bind_z_index#{}", file!(), line!(), #idx),
                                    move || (#value) as i32,
                                    |el, z| jadix::app::reactive::apply_bound_z_index(el, z),
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
                                __jadix_el = jadix::app::reactive::bind_attr(
                                    __jadix_el,
                                    format!("{}:{}:bind_attr:{}#{}", file!(), line!(), #key, #idx),
                                    move || (#value).to_string(),
                                    |el, v| jadix::app::reactive::apply_bound_attr(el, #key, v),
                                );
                            });
                        } else {
                            let msg =
                                format!("unknown jadix attribute `{attr_str}` on `<{name_str}>`");
                            let span = attr.span();
                            unknown_attr_warnings.push(quote_spanned! { span =>
                                {
                                    #[deprecated(note = #msg)]
                                    #[allow(dead_code)]
                                    fn __jadix_unknown_attr() {}
                                    __jadix_unknown_attr();
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
                                __jadix_kids.extend(jadix::core::tree::build_ui_child(|| (#expanded_child)));
                            });
                        }
                    }
                    RsxNode::Expr(_) | RsxNode::Element(_) => {
                        let expanded_child = expand_node(child, node_counter, in_reactive_boundary);
                        kid_extends.push(quote! {
                            __jadix_kids.extend(jadix::core::tree::build_ui_child(|| (#expanded_child)));
                        });
                    }
                }
            }
            if !kid_extends.is_empty() {
                element = quote! {
                    {
                        let mut __jadix_kids: Vec<jadix::core::tree::Element> = Vec::new();
                        #(#kid_extends)*
                        #element.children(__jadix_kids)
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
                        let mut __jadix_el: jadix::core::tree::Element = (#element).into();
                        #(#utility_calls)*
                        #(#dynamic_binds)*
                        __jadix_el
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
            __jadix_el = jadix::style::with_class_list(__jadix_el, #lit);
        }
    } else {
        quote! {
            {
                let __jadix_class = #value;
                __jadix_el = jadix::style::with_class_list(
                    __jadix_el,
                    ::std::convert::AsRef::<str>::as_ref(&__jadix_class),
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
        let parsed = jadix_style_tokens::parse_class_token(token);
        if !jadix_style_tokens::is_known_utility(&parsed.utility) {
            let msg = format!("unknown jadix utility class `{token}`");
            out.push(quote_spanned! { span =>
                {
                    #[deprecated(note = #msg)]
                    #[allow(dead_code)]
                    fn __jadix_unknown_utility_class() {}
                    __jadix_unknown_utility_class();
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
            from_div.contains("r#box"),
            "div must lower to primitives::r#box(); got {from_div}"
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
        assert!(from_div.contains("r#box"), "{from_div}");
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
        let expanded = quote! { jadix::core::tree::Element::new() };
        let hash = syn::LitStr::new("h", proc_macro2::Span::call_site());
        let s = wrap_ui_expansion(expanded, hash).to_string();
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
        let s = expand(
            r#"<Box>{ if n.get() { ui!(<Box>"a"</Box>) } else { ui!(<Box>"b"</Box>) } }</Box>"#,
        );
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
}

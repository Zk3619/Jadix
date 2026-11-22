//! Inert HTML for OS attributed-string importers. Unlike the document schema
//! importer, this preserves presentational text and table formatting.
use html5ever::{buffer_queue::BufferQueue, tokenizer::{states::RawKind, StartTag, Token, TokenSink, TokenSinkResult, Tokenizer}};
use std::cell::RefCell;

/// A resource reference separated from importer HTML. Hosts authorize and load
/// it through the same pipeline as ordinary images, never through WebKit/HTML.
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeHtmlImage {
    pub source: String,
    pub alt: String,
    pub width: Option<f64>,
    pub height: Option<f64>,
    pub html: String,
}
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeHtmlPlan {
    pub html: String,
    /// Importer callbacks can attach these inert element identities to text
    /// ranges, retaining attributes the OS's HTML importer does not understand.
    pub tagged_html: String,
    pub elements: Vec<NativeHtmlElement>,
    pub images: Vec<NativeHtmlImage>,
    /// Maximum decoded image bytes retained by one attributed document.
    pub image_budget_bytes: usize,
}
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeHtmlElement {
    #[serde(skip)]
    pub name: String,
    #[serde(skip)]
    pub attributes: Vec<(String, String)>,
    pub open: String,
    pub close: String,
    pub block: bool,
    pub parent: Option<usize>,
}

/// A display run keeps the sanitized ancestor identities for native text buffers.
/// The original wrappers remain in `NativeHtmlPlan`, so edits need not discard
/// attributes which a particular host cannot paint.
#[derive(Clone, Debug, PartialEq)]
pub struct NativeHtmlRun {
    pub text: String,
    pub elements: Vec<usize>,
    pub image: Option<usize>,
    /// Layout separator supplied by a block/table wrapper, omitted on HTML export.
    pub structural: bool,
}

impl NativeHtmlPlan {
    /// Export edited display runs while retaining sanitized ancestor and image
    /// identities. Hosts supply text and identities; markup remains shared.
    pub fn export_runs(&self, runs: &[NativeHtmlRun]) -> Result<String, &'static str> {
        if runs.len() > 32_768 || runs.iter().try_fold(0usize, |n, run| n.checked_add(run.text.len())).is_none_or(|n| n > 1_048_576) {
            return Err("native HTML runs exceed budget");
        }
        let mut html = String::new();
        let mut stack: Vec<usize> = Vec::new();
        for run in runs {
            if run.elements.len() > 256 || run.elements.iter().enumerate().any(|(position, &index)| {
                self.elements.get(index).is_none_or(|element| element.parent != position.checked_sub(1).map(|p| run.elements[p]))
            }) || run.image.is_some_and(|index| index >= self.images.len()) {
                return Err("invalid native HTML element identity");
            }
            let common = stack.iter().zip(&run.elements).take_while(|(a, b)| a == b).count();
            for &index in stack[common..].iter().rev() { html.push_str(&self.elements[index].close); }
            for &index in &run.elements[common..] { html.push_str(&self.elements[index].open); }
            stack.clone_from(&run.elements);
            if html.len() > 1_048_576 { return Err("native HTML export exceeds budget"); }
            for ch in run.text.chars() {
                if ch == '\u{fffc}' {
                    if let Some(image) = run.image.and_then(|index| self.images.get(index)) { html.push_str(&image.html); }
                    else { escape(&mut html, "\u{fffc}"); }
                } else if run.structural && matches!(ch, '\n' | '\r' | '\t') { continue; }
                else if ch == '\n' || ch == '\r' { html.push_str("<br>"); }
                else { escape(&mut html, ch.encode_utf8(&mut [0; 4])); }
                if html.len() > 1_048_576 { return Err("native HTML export exceeds budget"); }
            }
        }
        for &index in stack.iter().rev() { html.push_str(&self.elements[index].close); }
        if html.len() > 1_048_576 { Err("native HTML export exceeds budget") } else { Ok(html) }
    }

    pub fn text_runs(&self) -> Result<Vec<NativeHtmlRun>, &'static str> {
        struct Runs {
            runs: Vec<NativeHtmlRun>, stack: Vec<usize>, image: Option<usize>,
            bytes: usize, invalid: bool,
        }
        impl Runs {
            fn push(&mut self, text: &str) { self.append(text, false); }
            fn append(&mut self, text: &str, structural: bool) {
                self.bytes = self.bytes.saturating_add(text.len());
                if self.bytes > 1_048_576 || self.runs.len() >= 32_768 { self.invalid = true; return; }
                if let Some(last) = self.runs.last_mut().filter(|last| last.elements == self.stack && last.image == self.image && last.structural == structural) {
                    last.text.push_str(text);
                } else { self.runs.push(NativeHtmlRun { text: text.into(), elements: self.stack.clone(), image: self.image, structural }); }
            }
            fn line(&mut self) {
                if self.runs.last().is_some_and(|run| !run.text.ends_with('\n')) { self.append("\n", true); }
            }
        }
        struct RunSink(RefCell<Runs>);
        impl TokenSink for RunSink {
            type Handle = ();
            fn process_token(&self, token: Token, _: u64) -> TokenSinkResult<()> {
                let mut runs = self.0.borrow_mut();
                if runs.invalid { return TokenSinkResult::Continue; }
                match token {
                    Token::CharacterTokens(text) => runs.push(&text),
                    Token::TagToken(tag) => {
                        let name = tag.name.as_ref();
                        if let Some(index) = name.strip_prefix("jadixformat").and_then(|s| s.parse::<usize>().ok()) {
                            if tag.kind == StartTag { runs.stack.push(index); } else { runs.stack.pop(); }
                        } else if name == "a" {
                            runs.image = if tag.kind == StartTag {
                                tag.attrs.iter().find(|a| a.name.local.as_ref() == "href").and_then(|a| a.value.strip_prefix("jadix-attachment:")).and_then(|s| s.parse().ok())
                            } else { None };
                        } else if name == "br" || name == "hr" { runs.push("\n"); }
                        else if matches!(name, "p"|"li"|"h1"|"h2"|"h3"|"h4"|"h5"|"h6"|"tr") {
                            if tag.kind == StartTag { runs.line(); } else { runs.append("\n", true); }
                        }
                        else if matches!(name, "div"|"pre"|"blockquote") { runs.line(); }
                        else if name == "td" || name == "th" {
                            if tag.kind != StartTag { runs.append("\t", true); }
                        }
                    }
                    _ => ()
                }
                TokenSinkResult::Continue
            }
        }
        let tokenizer = Tokenizer::new(RunSink(RefCell::new(Runs { runs: Vec::new(), stack: Vec::new(), image: None, bytes: 0, invalid: false })), Default::default());
        let input = BufferQueue::default(); input.push_back(self.tagged_html.clone().into());
        let _ = tokenizer.feed(&input); tokenizer.end();
        let result = tokenizer.sink.0.into_inner();
        if result.invalid { Err("native HTML text exceeds budget") } else { Ok(result.runs) }
    }
}

struct MarkupSink(RefCell<(String, Vec<NativeHtmlElement>, Vec<(String, usize)>)>);
impl TokenSink for MarkupSink {
    type Handle = ();
    fn process_token(&self, token: Token, _: u64) -> TokenSinkResult<()> {
        let mut output = self.0.borrow_mut();
        let (html, elements, stack) = &mut *output;
        match token {
            Token::CharacterTokens(text) => escape(html, &text),
            Token::TagToken(tag) => {
                let name = tag.name.to_string();
                if tag.kind == StartTag {
                    let mut open = format!("<{name}");
                    let attributes = tag.attrs.iter().map(|attr| (attr.name.local.to_string(), attr.value.to_string())).collect();
                    for attr in tag.attrs {
                        open.push(' '); open.push_str(attr.name.local.as_ref()); open.push_str("=\"");
                        escape(&mut open, &attr.value); open.push('"');
                    }
                    open.push('>');
                    // Line breaks are represented by the importer's newline;
                    // generated attachment links already have their own identity.
                    if matches!(name.as_str(), "br" | "hr") || elements.len() >= 16_384 {
                        html.push_str(&open);
                    } else {
                        let index = elements.len();
                        let attachment = name == "a" && open.contains("href=\"jadix-attachment:");
                        let close = if attachment { String::new() } else { format!("</{name}>") };
                        elements.push(NativeHtmlElement { name: name.clone(), attributes, open: if attachment { String::new() } else { open.clone() }, close, block: matches!(name.as_str(), "p"|"div"|"pre"|"blockquote"|"ul"|"ol"|"li"|"h1"|"h2"|"h3"|"h4"|"h5"|"h6"|"table"|"thead"|"tbody"|"tfoot"|"tr"|"td"|"th"), parent: stack.last().map(|(_, index)| *index) });
                        html.push_str(&format!("<jadixformat{index}>")); html.push_str(&open);
                        stack.push((name, index));
                    }
                } else if let Some((name, index)) = stack.pop() {
                    html.push_str(&format!("</{name}></jadixformat{index}>"));
                }
            }
            _ => {}
        }
        TokenSinkResult::Continue
    }
}
fn tagged_html(html: &str) -> Result<(String, Vec<NativeHtmlElement>), &'static str> {
    let tokenizer = Tokenizer::new(MarkupSink(RefCell::new((String::new(), Vec::new(), Vec::new()))), Default::default());
    let input = BufferQueue::default(); input.push_back(html.into()); let _ = tokenizer.feed(&input); tokenizer.end();
    let (html, elements, stack) = tokenizer.sink.0.into_inner();
    if !stack.is_empty() || elements.len() >= 16_384 || html.len() > 4 * 1_048_576 { return Err("native HTML markup exceeds budget"); }
    Ok((html, elements))
}

/// Unspecified axes use natural size; one specified axis preserves aspect ratio.
/// Before a successful load, unknown axes occupy zero logical points.
pub fn image_dimensions(width: Option<f64>, height: Option<f64>, natural: Option<[f64; 2]>) -> [f64; 2] {
    fn valid(n: f64) -> bool { n.is_finite() && (0.0..=1_000_000.0).contains(&n) }
    let width = width.filter(|n| valid(*n));
    let height = height.filter(|n| valid(*n));
    let natural = natural.filter(|n| n.iter().all(|v| valid(*v) && *v > 0.0));
    let result = match (width, height, natural) {
        (Some(w), Some(h), _) => [w, h],
        (Some(w), None, Some([nw, nh])) => [w, w * nh / nw],
        (None, Some(h), Some([nw, nh])) => [h * nw / nh, h],
        (None, None, Some(size)) => size,
        (w, h, None) => [w.unwrap_or(0.0), h.unwrap_or(0.0)],
    };
    result.map(|n| if valid(n) { n } else { 0.0 })
}

#[derive(Default)]
struct Output { text: String, images: Vec<NativeHtmlImage>, extract_images: bool, stack: Vec<(String, bool)>, suppressed: Option<String>, invalid: bool }
struct Sink(RefCell<Output>);
fn escape(out: &mut String, value: &str) {
    for c in value.chars() { match c { '&'=>out.push_str("&amp;"), '<'=>out.push_str("&lt;"), '>'=>out.push_str("&gt;"), '"'=>out.push_str("&quot;"), _=>out.push(c) } }
}
impl TokenSink for Sink {
    type Handle = ();
    fn process_token(&self, token: Token, _: u64) -> TokenSinkResult<()> {
        let mut o = self.0.borrow_mut();
        if o.invalid { return TokenSinkResult::Continue; }
        match token {
            Token::CharacterTokens(s) if o.suppressed.is_none() => escape(&mut o.text, &s),
            Token::TagToken(t) => {
                let name = t.name.to_string();
                if let Some(hidden) = &o.suppressed {
                    if t.kind != StartTag && *hidden == name { o.suppressed = None; }
                    return TokenSinkResult::Continue;
                }
                if t.kind != StartTag {
                    if let Some(i) = o.stack.iter().rposition(|(n,_)| *n == name) {
                        while o.stack.len() > i { let (n, allowed) = o.stack.pop().unwrap(); if allowed { o.text.push_str("</"); o.text.push_str(&n); o.text.push('>'); } }
                    }
                } else if matches!(name.as_str(), "script"|"style"|"iframe"|"object"|"head"|"svg"|"math"|"template") {
                    if t.self_closing && matches!(name.as_str(), "svg" | "math") { return TokenSinkResult::Continue; }
                    o.suppressed = Some(name.clone());
                    if name == "script" { return TokenSinkResult::RawData(RawKind::ScriptData); }
                    if matches!(name.as_str(), "style"|"iframe") { return TokenSinkResult::RawData(RawKind::Rawtext); }
                } else if name == "img" && o.extract_images {
                    let attr = |key: &str| t.attrs.iter().find(|a| a.name.local.as_ref() == key).map(|a| a.value.to_string());
                    let source = attr("src").unwrap_or_default();
                    let alt = attr("alt").unwrap_or_default();
                    let allowed = ["https://", "http://", "asset:/", "file:///", "data:image/"].iter().any(|prefix| source.starts_with(prefix))
                        && !source.chars().any(|c| c.is_control() || c.is_whitespace());
                    if allowed {
                        if o.images.len() >= 128 { o.invalid = true; return TokenSinkResult::Continue; }
                        let dimension = |key| attr(key).and_then(|s| s.parse::<f64>().ok()).filter(|n| n.is_finite() && (0.0..=1_000_000.0).contains(n));
                        let width = dimension("width"); let height = dimension("height");
                        let mut html = String::from("<img src=\""); escape(&mut html, &source); html.push_str("\" alt=\""); escape(&mut html, &alt); html.push('"');
                        for (name, value) in [("width", width), ("height", height)] { if let Some(value) = value { html.push_str(&format!(" {name}=\"{value}\"")); } }
                        html.push('>');
                        let index = o.images.len();
                        o.images.push(NativeHtmlImage { source, alt, width, height, html });
                        // Only generated links may use this private, inert scheme.
                        o.text.push_str(&format!("<a href=\"jadix-attachment:{index}\">&#xfffc;</a>"));
                    } else { escape(&mut o.text, &alt); }
                } else {
                    let allowed = matches!(name.as_str(), "p"|"div"|"span"|"b"|"strong"|"i"|"em"|"u"|"s"|"strike"|"del"|"sub"|"sup"|"code"|"pre"|"blockquote"|"ul"|"ol"|"li"|"h1"|"h2"|"h3"|"h4"|"h5"|"h6"|"a"|"table"|"thead"|"tbody"|"tfoot"|"tr"|"td"|"th"|"br"|"hr"|"font");
                    if allowed {
                        o.text.push('<'); o.text.push_str(&name);
                        for a in t.attrs {
                            let key = a.name.local.as_ref(); let value = a.value.as_ref();
                            let safe = match key {
                                "href" => !value.starts_with("//") && ["https:","http:","mailto:","tel:","#"].iter().any(|p|value.to_ascii_lowercase().starts_with(p)) && !value.chars().any(char::is_control),
                                "color"|"face"|"size"|"align"|"dir"|"colspan"|"rowspan"|"start" => value.len() <= 256,
                                "style" => false, _ => false,
                            };
                            if key == "style" {
                                let mut css = String::new();
                                for declaration in value.split(';') {
                                    let Some((property, v)) = declaration.split_once(':') else { continue; };
                                    let property = property.trim().to_ascii_lowercase(); let v = v.trim();
                                    if matches!(property.as_str(), "color"|"background-color"|"font-family"|"font-size"|"font-weight"|"font-style"|"text-decoration"|"text-align"|"vertical-align"|"white-space"|"line-height"|"letter-spacing"|"direction"|"margin-left"|"margin-right"|"text-indent")
                                        && v.len() <= 256 && v.chars().all(|c|c.is_alphanumeric() || " #.,%()-+'\"".contains(c))
                                        && !v.to_ascii_lowercase().contains("url") && !v.to_ascii_lowercase().contains("expression") {
                                        css.push_str(&property); css.push(':'); css.push_str(v); css.push(';');
                                    }
                                }
                                if !css.is_empty() { o.text.push_str(" style=\""); escape(&mut o.text, &css); o.text.push('"'); }
                            } else if safe { o.text.push(' '); o.text.push_str(key); o.text.push_str("=\""); escape(&mut o.text, value); o.text.push('"'); }
                        }
                        o.text.push('>');
                    }
                    if !matches!(name.as_str(), "br"|"hr"|"img"|"meta"|"link"|"input"|"wbr"|"source"|"embed"|"area"|"base"|"param"|"col") {
                        o.stack.push((name, allowed)); if o.stack.len()>256 { o.invalid=true; }
                    }
                }
            }
            _ => {}
        }
        if o.text.len() > 1_048_576 { o.invalid = true; }
        TokenSinkResult::Continue
    }
}
/// No resource-bearing tags, CSS URLs, active content or event handlers survive.
pub fn sanitize_native_html(html: &str) -> Result<String, &'static str> {
    parse_native_html(html, false).map(|plan| plan.html)
}
/// Produces inert importer HTML and at most 128 independently owned resources.
pub fn prepare_native_html(html: &str) -> Result<NativeHtmlPlan, &'static str> {
    parse_native_html(html, true)
}
fn parse_native_html(html: &str, extract_images: bool) -> Result<NativeHtmlPlan, &'static str> {
    if html.len()>1_048_576 || html.contains('\0') { return Err("invalid native HTML"); }
    let tokenizer=Tokenizer::new(Sink(RefCell::new(Output { extract_images, ..Output::default() })),Default::default());
    let input=BufferQueue::default(); input.push_back(html.into()); let _=tokenizer.feed(&input); tokenizer.end();
    let mut out=tokenizer.sink.0.into_inner();
    if out.invalid { return Err("native HTML exceeds budget"); }
    while let Some((name,allowed))=out.stack.pop() { if allowed { out.text.push_str("</"); out.text.push_str(&name); out.text.push('>'); } }
    if out.text.len()>1_048_576 { return Err("native HTML exceeds budget"); }
    let (tagged_html, elements) = tagged_html(&out.text)?;
    Ok(NativeHtmlPlan { html: out.text, tagged_html, elements, images: out.images, image_budget_bytes: 16 * 1024 * 1024 })
}

#[cfg(test)] mod tests {
    use super::*;
    #[test] fn edited_runs_retain_safe_wrappers_resources_and_unicode() {
        let plan = prepare_native_html("<p><b>A&amp;😀</b><img src='asset:/cat.png' alt='cat'></p><p>next</p>").unwrap();
        let mut runs = plan.text_runs().unwrap();
        let run = runs.iter_mut().find(|run| run.text.contains('A')).unwrap();
        run.text = "中文<&😀\nedit".into();
        let html = plan.export_runs(&runs).unwrap();
        assert!(html.contains("<b>中文&lt;&amp;😀<br>edit</b>"), "{html}");
        assert!(html.contains("<img src=\"asset:/cat.png\" alt=\"cat\""), "{html}");
        assert!(!html.contains("jadix-attachment:"));
        assert!(!html.contains("<br></p>"), "layout separators must not become extra breaks: {html}");
        runs[0].elements.push(usize::MAX);
        assert!(plan.export_runs(&runs).is_err());
    }
    #[test] fn preserves_formatting_without_importer_fetches() {
        let s=sanitize_native_html(r#"<head><link href="https://bad"><style>x{}</style></head><p style="color:red;font-weight:700;background-image:url(https://bad)" onclick="run()">A&amp;😀<img src="https://bad"><a href="javascript:run()">B</a></p>"#).unwrap();
        assert!(s.contains("color:red;font-weight:700;")); assert!(s.contains("A&amp;😀")); assert!(!s.contains("https://bad")); assert!(!s.contains("javascript")); assert!(!s.contains("onclick"));
    }
    #[test] fn rejects_excess_nesting_and_bounds_encoded_output() { assert!(sanitize_native_html(&"<span>".repeat(257)).is_err()); assert!(sanitize_native_html(&"&".repeat(300_000)).is_err()); }
    #[test] fn attributed_importers_retain_sanitized_structure_and_attributes() {
        let plan = prepare_native_html("<table><tr><td style='color:red' onclick='bad()'>A&amp;B</td><td><ol start='3'><li>X</li></ol></td></tr></table><img src='asset:/x'>").unwrap();
        assert_eq!(plan.elements[0].open, "<table>");
        assert_eq!(plan.elements[2].open, "<td style=\"color:red;\">");
        assert!(plan.elements.iter().any(|element| element.open == "<ol start=\"3\">"));
        assert!(!plan.tagged_html.contains("onclick"));
        assert!(plan.tagged_html.contains("<jadixformat2><td"));
        assert!(plan.elements.last().unwrap().open.is_empty(), "attachment URLs must never become exported hyperlink markup");
        assert!(plan.tagged_html.contains("jadix-attachment:0"));
        assert!(prepare_native_html(&"<b>x</b>".repeat(16_385)).is_err());
    }
}

#[cfg(test)] mod resource_tests {
    use super::*;
    #[test] fn separates_resources_preserves_dimensions_and_round_trips_safely() {
        let plan = prepare_native_html(r#"<p>A<img src="https://example.test/a?x=1&amp;y=2" alt="&lt;cat&gt;" width="48"><img src="javascript:bad" alt="unsafe"><a href="jadix-attachment:0">fake</a><img src="asset:/a.png" height="24"></p>"#).unwrap();
        assert_eq!(plan.images.len(), 2);
        assert!(!plan.html.contains("https://"));
        assert_eq!(plan.html.matches("jadix-attachment:").count(), 2);
        assert!(plan.html.contains("unsafe"));
        assert_eq!(plan.images[0].source, "https://example.test/a?x=1&y=2");
        assert!(plan.images[0].html.contains("alt=\"&lt;cat&gt;\" width=\"48\""));
        assert!(prepare_native_html(&"<img src='asset:/a'>".repeat(129)).is_err());
        assert_eq!(prepare_native_html(&plan.images[0].html).unwrap().images, vec![plan.images[0].clone()]);
    }
    #[test] fn natural_size_partial_size_and_failed_loads_are_deterministic() {
        assert_eq!(image_dimensions(None, None, None), [0.0, 0.0]);
        assert_eq!(image_dimensions(Some(40.0), None, None), [40.0, 0.0]);
        assert_eq!(image_dimensions(None, Some(20.0), Some([80.0, 40.0])), [40.0, 20.0]);
        assert_eq!(image_dimensions(Some(40.0), None, Some([80.0, 40.0])), [40.0, 20.0]);
        assert_eq!(image_dimensions(None, None, Some([80.0, 40.0])), [80.0, 40.0]);
        assert_eq!(image_dimensions(Some(0.0), None, Some([80.0, 40.0])), [0.0, 0.0]);
        assert_eq!(image_dimensions(Some(f64::NAN), None, Some([0.0, 40.0])), [0.0, 0.0]);
        assert_eq!(image_dimensions(Some(40.0), Some(70.0), Some([80.0, 40.0])), [40.0, 70.0]);
    }
    #[test] fn empty_elements_retain_parent_identity_for_native_round_trip() {
        let plan = prepare_native_html("<table><tr><td><p></p><p><strong></strong></p></td></tr></table>").unwrap();
        assert_eq!(plan.elements.len(), 6);
        assert_eq!(plan.elements.iter().map(|element| element.parent).collect::<Vec<_>>(), vec![None, Some(0), Some(1), Some(2), Some(2), Some(4)]);
        assert!(plan.tagged_html.contains("<jadixformat5><strong></strong></jadixformat5>"));
    }
}

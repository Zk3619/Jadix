//! 与 `rui_macros::ui!` 展开后的 **text节点 DFS 顺序** align的 literal 提取。
//! 线性扫描会漏掉 `Icon`（字形来自 `name="..."` property），导致与运行时 `count_text_nodes` 不一致。
//!
//! `Button` 的 `Into<Element>` 先把 `.child(icon)` 放进 `base.el`，再把 `label` 用 `build_text_node` **追加**到
//! child末尾（见 `button.rs`），因此 DFS 是「先图标subtree，后按钮copy」。普通 `Element`/`Label` 仍是
//! 父 `el.text` 先于subtree（见 `replace_text_at_index_inner`）。

use std::collections::HashMap;
use std::sync::OnceLock;

#[derive(Debug)]
enum Node {
    Text(String),
    Element(ElementNode),
}

#[derive(Debug)]
struct ElementNode {
    name: String,
    /// 仅keepused for `Icon` 的 `name` 等；值均为引号内字面值。
    attrs: Vec<(String, String)>,
    children: Vec<Node>,
}

fn lucide_map() -> &'static HashMap<String, String> {
    static MAP: OnceLock<HashMap<String, String>> = OnceLock::new();
    MAP.get_or_init(|| {
        let json = include_str!("../../rui/assets/lucide.json");
        serde_json::from_str(json).expect("lucide.json")
    })
}

fn icon_glyph_char(name: &str) -> Option<String> {
    let hex = lucide_map().get(name)?;
    let cp = u32::from_str_radix(hex, 16).ok()?;
    char::from_u32(cp).map(|c| c.to_string())
}

struct Parser<'a> {
    src: &'a [u8],
    i: usize,
}

impl<'a> Parser<'a> {
    fn new(s: &'a str) -> Self {
        Self {
            src: s.as_bytes(),
            i: 0,
        }
    }

    fn peek(&self) -> Option<u8> {
        self.src.get(self.i).copied()
    }

    fn skip_ws(&mut self) {
        while self
            .peek()
            .is_some_and(|b| (b as char).is_ascii_whitespace())
        {
            self.i += 1;
        }
    }

    fn consume_if(&mut self, pat: &[u8]) -> bool {
        if self.src.len().saturating_sub(self.i) >= pat.len()
            && &self.src[self.i..self.i + pat.len()] == pat
        {
            self.i += pat.len();
            return true;
        }
        false
    }

    fn parse_ident(&mut self) -> Option<String> {
        let start = self.i;
        while let Some(b) = self.peek() {
            if b.is_ascii_alphanumeric() || b == b'_' || b == b'-' {
                self.i += 1;
            } else {
                break;
            }
        }
        if self.i == start {
            return None;
        }
        std::str::from_utf8(&self.src[start..self.i])
            .ok()
            .map(|s| s.to_string())
    }

    fn parse_quoted_string(&mut self) -> Option<String> {
        if self.peek()? != b'"' {
            return None;
        }
        self.i += 1;
        let start = self.i;
        let mut escaped = false;
        while self.i < self.src.len() {
            let c = self.src[self.i];
            if escaped {
                escaped = false;
                self.i += 1;
                continue;
            }
            if c == b'\\' {
                escaped = true;
                self.i += 1;
                continue;
            }
            if c == b'"' {
                let s = std::str::from_utf8(&self.src[start..self.i])
                    .ok()?
                    .to_string();
                self.i += 1;
                return Some(s);
            }
            self.i += 1;
        }
        None
    }

    fn skip_brace_group(&mut self) {
        if self.peek() != Some(b'{') {
            return;
        }
        self.i += 1;
        let mut depth = 1usize;
        let mut in_str = false;
        let mut escaped = false;
        while self.i < self.src.len() && depth > 0 {
            let c = self.src[self.i];
            if in_str {
                if escaped {
                    escaped = false;
                } else if c == b'\\' {
                    escaped = true;
                } else if c == b'"' {
                    in_str = false;
                }
                self.i += 1;
                continue;
            }
            if c == b'"' {
                in_str = true;
                self.i += 1;
                continue;
            }
            if c == b'{' {
                depth += 1;
            } else if c == b'}' {
                depth -= 1;
            }
            self.i += 1;
        }
    }

    fn parse_attr_value_string(&mut self) -> Option<String> {
        self.skip_ws();
        match self.peek()? {
            b'"' => self.parse_quoted_string(),
            b'{' => {
                self.skip_brace_group();
                None
            }
            _ => None,
        }
    }

    fn parse_element(&mut self) -> Option<ElementNode> {
        if self.peek()? != b'<' {
            return None;
        }
        self.i += 1;
        if self.peek() == Some(b'/') {
            return None;
        }
        let name = self.parse_ident()?;
        let mut attrs = Vec::new();
        loop {
            self.skip_ws();
            if self.consume_if(b"/>") {
                return Some(ElementNode {
                    name,
                    attrs,
                    children: Vec::new(),
                });
            }
            if self.peek() == Some(b'>') {
                self.i += 1;
                break;
            }
            let an = self.parse_ident()?;
            self.skip_ws();
            if self.peek() != Some(b'=') {
                return None;
            }
            self.i += 1;
            if let Some(v) = self.parse_attr_value_string() {
                attrs.push((an, v));
            } else {
                self.skip_ws();
                if self.peek() == Some(b'{') {
                    self.skip_brace_group();
                }
            }
        }

        let mut children = Vec::new();
        loop {
            self.skip_ws();
            if self.consume_if(b"</") {
                self.skip_ws();
                let close = self.parse_ident()?;
                self.skip_ws();
                if self.peek() == Some(b'>') {
                    self.i += 1;
                }
                if close != name {
                    return None;
                }
                return Some(ElementNode {
                    name,
                    attrs,
                    children,
                });
            }

            match self.peek() {
                Some(b'<') => {
                    children.push(Node::Element(self.parse_element()?));
                }
                Some(b'"') => {
                    children.push(Node::Text(self.parse_quoted_string()?));
                }
                Some(b'{') => {
                    self.skip_brace_group();
                }
                Some(_) => {
                    self.i += 1;
                }
                None => return None,
            }
        }
    }

    fn parse_root_nodes(&mut self) -> Option<Vec<Node>> {
        let mut out = Vec::new();
        while self.i < self.src.len() {
            self.skip_ws();
            if self.i >= self.src.len() {
                break;
            }
            match self.peek() {
                None => break,
                Some(b'<') => {
                    let el = self.parse_element()?;
                    out.push(Node::Element(el));
                }
                Some(b'"') => {
                    out.push(Node::Text(self.parse_quoted_string()?));
                }
                Some(b'{') => {
                    self.skip_brace_group();
                }
                Some(_) => {
                    self.i += 1;
                }
            }
        }
        Some(out)
    }
}

/// `Button` 与 `button.rs` `content_slots`：宏展开顺序即child顺序（`Text` / `Icon` / 嵌套元素交错）。
fn dfs_collect_button(el: &ElementNode, out: &mut Vec<String>) {
    for c in &el.children {
        match c {
            Node::Text(s) => {
                let trimmed = s.trim();
                if !trimmed.is_empty() {
                    out.push(trimmed.to_string());
                }
            }
            Node::Element(child) => dfs_collect_element(child, out),
        }
    }
}

/// 与 `replace_text_at_index_inner` 一致：先父级 `el.text`（宏里连续 `.text()` 仅最后一个valid），再subtree DFS。
fn dfs_collect_element(el: &ElementNode, out: &mut Vec<String>) {
    if el.name == "Icon" {
        if let Some((_, nm)) = el.attrs.iter().find(|(k, _)| k == "name") {
            if let Some(g) = icon_glyph_char(nm) {
                out.push(g);
            }
        }
        return;
    }

    if el.name == "Button" {
        dfs_collect_button(el, out);
        return;
    }

    let mut last_direct_text: Option<String> = None;
    for c in &el.children {
        if let Node::Text(s) = c {
            last_direct_text = Some(s.clone());
        }
    }
    if let Some(t) = last_direct_text {
        out.push(t);
    }
    for c in &el.children {
        if let Node::Element(child) = c {
            dfs_collect_element(child, out);
        }
    }
}

fn dfs_collect_root(node: &Node, out: &mut Vec<String>) {
    match node {
        Node::Text(s) => out.push(s.clone()),
        Node::Element(el) => dfs_collect_element(el, out),
    }
}

/// 与运行时 `inject_text_literals_by_id` / `count_text_nodes` 的 DFS 顺序align。
pub fn extract_text_literals_tree_order(body: &str) -> Vec<String> {
    let mut p = Parser::new(body);
    let roots = match p.parse_root_nodes() {
        Some(r) => r,
        None => return Vec::new(),
    };
    let mut out = Vec::new();
    for n in &roots {
        dfs_collect_root(n, &mut out);
    }
    out
}

pub fn extract_style_attributes_tree_order(body: &str) -> Vec<super::StyleAttr> {
    let mut p = Parser::new(body);
    let roots = match p.parse_root_nodes() {
        Some(r) => r,
        None => return Vec::new(),
    };
    let mut out = Vec::new();
    let mut element_idx = 0usize;
    for n in &roots {
        dfs_collect_styles(n, &mut out, &mut element_idx);
    }
    out
}

fn dfs_collect_styles(node: &Node, out: &mut Vec<super::StyleAttr>, element_idx: &mut usize) {
    match node {
        Node::Text(_) => {}
        Node::Element(el) => {
            let cur_idx = *element_idx;
            *element_idx += 1;
            for (k, v) in &el.attrs {
                out.push(super::StyleAttr {
                    element_index: cur_idx,
                    name: k.clone(),
                    value: v.clone(),
                });
            }
            if el.name == "Icon" {
                return;
            }
            for child in &el.children {
                dfs_collect_styles(child, out, element_idx);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn icon_inserts_glyph_between_button_labels() {
        let body = r#"<HStack><Button variant="outline">"LG"</Button><Button><Icon name="star" color="primary-foreground" size="0.75rem" /></Button><Button>"Leading"</Button></HStack>"#;
        let v = extract_text_literals_tree_order(body);
        let star = icon_glyph_char("star").unwrap();
        assert_eq!(v, vec!["LG".to_string(), star, "Leading".to_string()]);
    }

    /// 源码顺序：先 `<Icon/>` 再 `"Suffix"` → 与 `Button::content_slots` 一致 [glyph, Suffix]。
    #[test]
    fn text_after_icon_matches_runtime_dfs_order() {
        let body = r#"<Button><Icon name="heart" />"Suffix"</Button>"#;
        let v = extract_text_literals_tree_order(body);
        let heart = icon_glyph_char("heart").unwrap();
        assert_eq!(v, vec![heart, "Suffix".to_string()]);
    }

    /// 源码顺序：先 `"Prefix"` 再 `<Icon/>` → [Prefix, glyph]。
    #[test]
    fn text_before_icon_matches_runtime_dfs_order() {
        let body = r#"<Button>"Prefix"<Icon name="star" /></Button>"#;
        let v = extract_text_literals_tree_order(body);
        let star = icon_glyph_char("star").unwrap();
        assert_eq!(v, vec!["Prefix".to_string(), star]);
    }

    #[test]
    fn scroll_area_label_text_size_is_element_index_two() {
        let body = r#"<ScrollArea id="scroll-navigation"><VStack><Label text_size="1.25rem" font_weight="700">"导航"</Label></VStack></ScrollArea>"#;
        let attrs = extract_style_attributes_tree_order(body);
        let size = attrs
            .iter()
            .find(|a| a.name == "text_size")
            .expect("text_size");
        assert_eq!(size.element_index, 2);
        assert_eq!(size.value, "1.25rem");
        let weight = attrs
            .iter()
            .find(|a| a.name == "font_weight")
            .expect("font_weight");
        assert_eq!(weight.element_index, 2);
    }
}

use crate::TemplateNode;
use proc_macro2::{TokenStream, TokenTree};

pub(super) fn token_fingerprint(stream: TokenStream) -> String {
    fn tokens(stream: TokenStream) -> Vec<serde_json::Value> {
        stream
            .into_iter()
            .map(|token| match token {
                TokenTree::Group(group) => serde_json::json!([
                    "group",
                    format!("{:?}", group.delimiter()),
                    tokens(group.stream())
                ]),
                TokenTree::Ident(name) => serde_json::json!(["ident", name.to_string()]),
                TokenTree::Punct(mark) => {
                    serde_json::json!(["punct", mark.as_char(), format!("{:?}", mark.spacing())])
                }
                TokenTree::Literal(value) => serde_json::json!(["literal", value.to_string()]),
            })
            .collect()
    }
    format!(
        "tokens-v1:{}",
        serde_json::to_string(&tokens(stream)).unwrap()
    )
}

pub fn parse(source: &str) -> Option<TemplateNode> {
    let stream: TokenStream = source.parse().ok()?;
    let mut parser = Parser {
        tokens: stream.into_iter().collect(),
        index: 0,
    };
    let node = parser.node(0)?;
    (parser.index == parser.tokens.len()).then_some(node)
}

struct Parser {
    tokens: Vec<TokenTree>,
    index: usize,
}

impl Parser {
    fn mark(&mut self, value: char) -> bool {
        if matches!(self.tokens.get(self.index), Some(TokenTree::Punct(mark)) if mark.as_char() == value)
        {
            self.index += 1;
            true
        } else {
            false
        }
    }

    fn name(&mut self) -> Option<String> {
        let TokenTree::Ident(name) = self.tokens.get(self.index)? else {
            return None;
        };
        let name = name.to_string();
        self.index += 1;
        Some(match name.as_str() {
            "div" | "Div" => "Box".into(),
            "Grid" => "GStack".into(),
            _ => name,
        })
    }

    fn node(&mut self, depth: usize) -> Option<TemplateNode> {
        if depth > 128 {
            return None;
        }
        let start = self.index;
        if let Some(TokenTree::Literal(literal)) = self.tokens.get(self.index) {
            let value = syn::parse_str::<syn::LitStr>(&literal.to_string())
                .ok()?
                .value();
            self.index += 1;
            return Some(TemplateNode::Text { value });
        }
        if let Some(TokenTree::Group(group)) = self.tokens.get(self.index) {
            if group.delimiter() != proc_macro2::Delimiter::Brace {
                return None;
            }
            let fingerprint = token_fingerprint(group.stream());
            self.index += 1;
            return Some(TemplateNode::Slot { fingerprint });
        }
        if !self.mark('<') {
            return None;
        }
        let name = self.name()?;
        let mut attributes = Vec::new();
        let mut opaque = false;
        let mut key = None;
        while !matches!(self.tokens.get(self.index), Some(TokenTree::Punct(mark)) if matches!(mark.as_char(), '/' | '>'))
        {
            let attribute = self.name()?;
            if !self.mark('=') {
                return None;
            }
            let negative = self.mark('-');
            let token = self.tokens.get(self.index)?;
            let value = match token {
                TokenTree::Literal(literal) if !negative => {
                    syn::parse_str::<syn::LitStr>(&literal.to_string())
                        .ok()
                        .map(|value| value.value())
                        .or_else(|| {
                            crate::is_live_attribute(&attribute)
                                .then(|| super::numeric_attribute_literal(&literal.to_string()))
                                .flatten()
                        })
                }
                TokenTree::Literal(literal) if crate::is_live_attribute(&attribute) => {
                    super::numeric_attribute_literal(&format!("-{}", literal))
                }
                _ => None,
            };
            self.index += 1;
            match value {
                Some(value) if attribute == "id" || attribute == "key" => key = Some(value),
                Some(value) if crate::is_live_attribute(&attribute) => {
                    attributes.push((attribute, value))
                }
                _ => opaque = true,
            }
        }
        let closed = self.mark('/');
        if !self.mark('>') {
            return None;
        }
        let mut children = Vec::new();
        if !closed {
            loop {
                if matches!(self.tokens.get(self.index), Some(TokenTree::Punct(mark)) if mark.as_char() == '<')
                    && matches!(self.tokens.get(self.index + 1), Some(TokenTree::Punct(mark)) if mark.as_char() == '/')
                {
                    break;
                }
                children.push(self.node(depth + 1)?);
            }
            if !self.mark('<') || !self.mark('/') || self.name()? != name || !self.mark('>') {
                return None;
            }
        }
        if opaque {
            let fingerprint: TokenStream = self.tokens[start..self.index].iter().cloned().collect();
            Some(TemplateNode::Slot {
                fingerprint: token_fingerprint(fingerprint),
            })
        } else {
            Some(TemplateNode::Element {
                name,
                key,
                attributes,
                children,
            })
        }
    }
}

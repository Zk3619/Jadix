use proc_macro2::{TokenStream, TokenTree};
#[path = "syntax_tree.rs"]
mod tree;
pub use tree::parse as template_tree;

pub fn numeric_attribute_literal(source: &str) -> Option<String> {
    let source = source.trim();
    let (sign, digits) = source
        .strip_prefix('-')
        .map_or(("", source), |digits| ("-", digits.trim_start()));
    let value = match syn::parse_str::<syn::Lit>(digits).ok()? {
        syn::Lit::Int(value) => {
            let suffix = value.suffix();
            if !matches!(
                suffix,
                "" | "u8"
                    | "u16"
                    | "u32"
                    | "u64"
                    | "u128"
                    | "usize"
                    | "i8"
                    | "i16"
                    | "i32"
                    | "i64"
                    | "i128"
                    | "isize"
            ) || (sign == "-" && suffix.starts_with('u'))
            {
                return None;
            }
            format!("{sign}{}", value.base10_digits())
        }
        syn::Lit::Float(value) if matches!(value.suffix(), "" | "f32" | "f64") => {
            format!("{sign}{}", value.base10_digits())
        }
        _ => return None,
    };
    value
        .parse::<f64>()
        .ok()
        .filter(|value| value.is_finite())?;
    Some(value)
}

pub fn source_signature(source: &str) -> Option<String> {
    source
        .parse()
        .ok()
        .map(|stream| signature(stream, false, true))
}

pub fn template_signature(source: &str) -> Option<String> {
    if let Some(signature) = template_tree(source).and_then(|tree| tree.executable_signature()) {
        return Some(format!("tree-v1:{signature}"));
    }
    source
        .parse()
        .ok()
        .map(|stream| signature(stream, true, true))
}

pub fn declarations_signature(source: &str) -> Option<String> {
    source
        .parse()
        .ok()
        .map(|stream| signature(stream, true, false))
}

fn signature(stream: TokenStream, template: bool, declaration_edits: bool) -> String {
    let tokens: Vec<_> = stream.into_iter().collect();
    let mut output = String::new();
    let mut index = 0;
    while index < tokens.len() {
        if !template
            && matches!(tokens.get(index), Some(TokenTree::Ident(name)) if name == "ui")
            && matches!(tokens.get(index + 1), Some(TokenTree::Punct(mark)) if mark.as_char() == '!')
        {
            if let Some(TokenTree::Group(body)) = tokens.get(index + 2) {
                output.push_str("ui!{");
                output
                    .push_str(&template_signature(&body.stream().to_string()).unwrap_or_default());
                output.push('}');
                index += 3;
                continue;
            }
        }
        if template && declaration_edits {
            if let (
                Some(TokenTree::Ident(name)),
                Some(TokenTree::Punct(mark)),
                Some(TokenTree::Literal(literal)),
            ) = (
                tokens.get(index),
                tokens.get(index + 1),
                tokens.get(index + 2),
            ) {
                let literal = literal.to_string();
                if mark.as_char() == '='
                    && crate::is_live_attribute(&name.to_string())
                    && (literal.starts_with('"')
                        || literal.starts_with("r#")
                        || literal.starts_with("r\""))
                {
                    index += 3;
                    continue;
                }
            }
        }
        let token = &tokens[index];
        if template {
            if let TokenTree::Literal(literal) = token {
                let literal = literal.to_string();
                let string = literal.starts_with('"')
                    || literal.starts_with("r#")
                    || literal.starts_with("r\"");
                let value = matches!(index.checked_sub(1).and_then(|previous| tokens.get(previous)), Some(TokenTree::Punct(mark)) if mark.as_char() == '=');
                let name = index
                    .checked_sub(2)
                    .and_then(|previous| tokens.get(previous))
                    .map(ToString::to_string)
                    .unwrap_or_default();
                if string && (!value || crate::is_live_attribute(&name)) {
                    output.push_str("__live_literal__ ");
                    index += 1;
                    continue;
                }
            }
        }
        match token {
            TokenTree::Group(group) if !template => {
                output.push_str(&format!(
                    "{:?}({})",
                    group.delimiter(),
                    signature(group.stream(), false, declaration_edits)
                ));
            }
            TokenTree::Group(group) => {
                output.push_str(&format!(
                    "{:?}({})",
                    group.delimiter(),
                    tree::token_fingerprint(group.stream())
                ));
            }
            _ => output.push_str(&token.to_string()),
        }
        output.push(' ');
        index += 1;
    }
    output
}

#[cfg(test)]
#[path = "tests/syntax.rs"]
mod tests;

//! ICU MessageFormat subset: `{name}`, `{n, plural, …}`, `{g, select, …}`, `{n, number}`.

use super::plural::{self, PluralCategory};

/// Named argument for [`format_message`].
#[derive(Clone, Copy, Debug)]
pub struct Arg<'a> {
    pub name: &'a str,
    pub value: &'a str,
}

impl<'a> Arg<'a> {
    pub fn new(name: &'a str, value: &'a str) -> Self {
        Self { name, value }
    }
}

fn arg_value<'a>(args: &[Arg<'a>], name: &str) -> Option<&'a str> {
    args.iter().find(|a| a.name == name).map(|a| a.value)
}

fn parse_int(s: &str) -> Option<i64> {
    let t = s.trim();
    if t.is_empty() {
        return None;
    }
    t.parse::<i64>()
        .ok()
        .or_else(|| t.parse::<f64>().ok().map(|f| f.trunc() as i64))
}

/// Format `template` with named args. Unknown placeholders are left intact.
pub fn format_message(template: &str, args: &[Arg<'_>], language: &str) -> String {
    format_inner(template, args, language, None)
}

fn format_inner(
    template: &str,
    args: &[Arg<'_>],
    language: &str,
    hash_value: Option<&str>,
) -> String {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while !rest.is_empty() {
        let ch = rest.chars().next().unwrap();
        let ch_len = ch.len_utf8();
        match ch {
            '\'' => {
                let after = &rest[ch_len..];
                if after.starts_with('\'') {
                    out.push('\'');
                    rest = &after[1..];
                } else {
                    rest = after;
                    loop {
                        if rest.is_empty() {
                            break;
                        }
                        let q = rest.chars().next().unwrap();
                        let q_len = q.len_utf8();
                        if q == '\'' {
                            let nxt = &rest[q_len..];
                            if nxt.starts_with('\'') {
                                out.push('\'');
                                rest = &nxt[1..];
                            } else {
                                rest = nxt;
                                break;
                            }
                        } else {
                            out.push(q);
                            rest = &rest[q_len..];
                        }
                    }
                }
            }
            '{' => {
                if let Some((end, body)) = split_braces(rest) {
                    out.push_str(&replace_placeholder(body, args, language, hash_value));
                    rest = &rest[end..];
                } else {
                    out.push('{');
                    rest = &rest[ch_len..];
                }
            }
            '#' if hash_value.is_some() => {
                out.push_str(hash_value.unwrap());
                rest = &rest[ch_len..];
            }
            c => {
                out.push(c);
                rest = &rest[ch_len..];
            }
        }
    }
    out
}

fn split_braces(s: &str) -> Option<(usize, &str)> {
    if !s.starts_with('{') {
        return None;
    }
    let mut depth = 0;
    let mut quote = false;
    for (idx, c) in s.char_indices() {
        match c {
            '\'' => quote = !quote,
            '{' if !quote => depth += 1,
            '}' if !quote => {
                depth -= 1;
                if depth == 0 {
                    return Some((idx + 1, &s[1..idx]));
                }
            }
            _ => {}
        }
    }
    None
}

fn replace_placeholder(
    body: &str,
    args: &[Arg<'_>],
    language: &str,
    hash_value: Option<&str>,
) -> String {
    let body = body.trim();
    if body.is_empty() {
        return String::new();
    }
    let mut parts = body.splitn(2, ',');
    let name = parts.next().unwrap_or("").trim();
    let rest = parts.next().map(str::trim).unwrap_or("");
    if rest.is_empty() {
        return arg_value(args, name)
            .map(|s| s.to_string())
            .unwrap_or_else(|| format!("{{{name}}}"));
    }
    let mut kind_and_body = rest.splitn(2, ',');
    let kind = kind_and_body.next().unwrap_or("").trim();
    let clauses = kind_and_body.next().map(str::trim).unwrap_or("");
    match kind {
        "plural" => format_plural(name, clauses, args, language),
        "select" => format_select(name, clauses, args, language, hash_value),
        "number" => arg_value(args, name)
            .map(|s| s.to_string())
            .unwrap_or_else(|| format!("{{{name}}}")),
        _ => arg_value(args, name)
            .map(|s| s.to_string())
            .unwrap_or_else(|| format!("{{{body}}}")),
    }
}

fn format_plural(name: &str, clauses: &str, args: &[Arg<'_>], language: &str) -> String {
    let raw = arg_value(args, name).unwrap_or("0");
    let n = parse_int(raw).unwrap_or(0);
    let cat = plural::category(language, n);
    let chosen = pick_plural_clause(clauses, cat, n).unwrap_or("");
    format_inner(chosen, args, language, Some(raw))
}

fn format_select(
    name: &str,
    clauses: &str,
    args: &[Arg<'_>],
    language: &str,
    hash_value: Option<&str>,
) -> String {
    let raw = arg_value(args, name).unwrap_or("other");
    let chosen = pick_select_clause(clauses, raw).unwrap_or("");
    format_inner(chosen, args, language, hash_value)
}

fn pick_plural_clause(clauses: &str, cat: PluralCategory, n: i64) -> Option<&str> {
    let parsed = parse_clauses(clauses);
    let exact = format!("={n}");
    if let Some(body) = parsed.iter().find(|(k, _)| *k == exact).map(|(_, b)| *b) {
        return Some(body);
    }
    parsed
        .iter()
        .find(|(k, _)| *k == cat.as_str())
        .or_else(|| parsed.iter().find(|(k, _)| *k == "other"))
        .map(|(_, b)| *b)
}

fn pick_select_clause<'a>(clauses: &'a str, value: &str) -> Option<&'a str> {
    let parsed = parse_clauses(clauses);
    parsed
        .iter()
        .find(|(k, _)| *k == value)
        .or_else(|| parsed.iter().find(|(k, _)| *k == "other"))
        .map(|(_, b)| *b)
}

fn parse_clauses(s: &str) -> Vec<(&str, &str)> {
    let mut out = Vec::new();
    let mut i = 0;
    let bytes = s.as_bytes();
    while i < bytes.len() {
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= bytes.len() {
            break;
        }
        let start = i;
        while i < bytes.len() && !bytes[i].is_ascii_whitespace() && bytes[i] != b'{' {
            i += 1;
        }
        let key = s[start..i].trim();
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= bytes.len() || bytes[i] != b'{' {
            break;
        }
        if let Some((len, body)) = split_braces(&s[i..]) {
            out.push((key, body));
            i += len;
        } else {
            break;
        }
    }
    out
}

#[cfg(test)]
#[path = "tests/message.rs"]
mod tests;

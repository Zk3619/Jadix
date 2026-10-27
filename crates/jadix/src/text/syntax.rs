use crate::style::TextSpan;

pub fn highlight_code(source: &str, language: &str, dark: bool) -> Vec<TextSpan> {
    jadix_document::syntax::highlight_code(source, language, dark)
        .into_iter()
        .map(|span| {
            let mut output = TextSpan::new(span.text);
            output.color = span.color;
            output
        })
        .collect()
}

pub fn highlight_block_line(
    source: &std::sync::Arc<str>,
    language: &str,
    dark: bool,
    line: usize,
) -> Vec<TextSpan> {
    use std::{
        cell::RefCell,
        collections::VecDeque,
        sync::{Arc, Weak},
    };
    type Entry = (Weak<str>, String, bool, Vec<Vec<TextSpan>>);
    thread_local! { static CACHE: RefCell<VecDeque<Entry>> = const { RefCell::new(VecDeque::new()) }; }
    if source.len() > 256 * 1024 {
        return vec![TextSpan::new(source.lines().nth(line).unwrap_or(""))];
    }
    CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        cache.retain(|entry| entry.0.strong_count() != 0);
        if let Some(entry) = cache.iter().find(|entry| {
            entry
                .0
                .upgrade()
                .is_some_and(|previous| Arc::ptr_eq(&previous, source))
                && entry.1 == language
                && entry.2 == dark
        }) {
            return entry.3.get(line).cloned().unwrap_or_default();
        }
        let mut lines = vec![Vec::new()];
        for span in highlight_code(source, language, dark) {
            for part in span.text.split_inclusive('\n') {
                let mut fragment = span.clone();
                fragment.text = part.trim_end_matches(['\r', '\n']).to_string();
                if !fragment.text.is_empty() {
                    lines.last_mut().unwrap().push(fragment);
                }
                if part.ends_with('\n') {
                    lines.push(Vec::new());
                }
            }
        }
        let result = lines.get(line).cloned().unwrap_or_default();
        if cache.len() >= 8 {
            cache.pop_front();
        }
        cache.push_back((Arc::downgrade(source), language.into(), dark, lines));
        result
    })
}

use crate::style::TextSpan;

pub fn highlight_code(source: &str, language: &str, dark: bool) -> Vec<TextSpan> {
    if source.len() > 2 * 1024 * 1024 || source.lines().any(|line| line.len() > 64 * 1024) {
        return vec![TextSpan::new(source)];
    }
    #[cfg(feature = "syntax-highlighting")]
    {
        use std::sync::OnceLock;
        use syntect::{easy::HighlightLines, highlighting::ThemeSet, parsing::SyntaxSet};
        static SYNTAXES: OnceLock<SyntaxSet> = OnceLock::new();
        static THEMES: OnceLock<ThemeSet> = OnceLock::new();
        let syntaxes = SYNTAXES.get_or_init(SyntaxSet::load_defaults_newlines);
        let themes = THEMES.get_or_init(ThemeSet::load_defaults);
        let language = match language.to_ascii_lowercase().as_str() {
            "js" | "jsx" => "JavaScript",
            "ts" | "tsx" => "TypeScript",
            "md" | "markdown" => "Markdown",
            "py" => "Python",
            _ => language,
        };
        let syntax = syntaxes
            .find_syntax_by_token(language)
            .or_else(|| syntaxes.find_syntax_by_extension(language))
            .or_else(|| {
                (language == "TypeScript")
                    .then(|| syntaxes.find_syntax_by_token("JavaScript"))
                    .flatten()
            })
            .unwrap_or_else(|| syntaxes.find_syntax_plain_text());
        let theme = &themes.themes[if dark {
            "base16-ocean.dark"
        } else {
            "InspiredGitHub"
        }];
        let mut highlighter = HighlightLines::new(syntax, theme);
        let mut spans: Vec<TextSpan> = Vec::new();
        for line in syntect::util::LinesWithEndings::from(source) {
            match highlighter.highlight_line(line, syntaxes) {
                Ok(parts) => {
                    for (style, text) in parts {
                        let color = [
                            style.foreground.r,
                            style.foreground.g,
                            style.foreground.b,
                            255,
                        ];
                        if let Some(previous) =
                            spans.last_mut().filter(|span| span.color == Some(color))
                        {
                            previous.text.push_str(text);
                        } else {
                            spans.push(TextSpan::new(text).color(color));
                        }
                    }
                }
                Err(_) => spans.push(TextSpan::new(line)),
            }
        }
        return spans;
    }
    #[cfg(not(feature = "syntax-highlighting"))]
    {
        let _ = (language, dark);
        vec![TextSpan::new(source)]
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn highlighted_spans_preserve_every_source_byte() {
        let source = "/* 中文\n comment */\r\nfn main() { let value = \"🚀\"; }\n";
        let spans = highlight_code(source, "rs", false);
        assert_eq!(
            spans
                .iter()
                .map(|span| span.text.as_str())
                .collect::<String>(),
            source
        );
        #[cfg(feature = "syntax-highlighting")]
        assert!(spans.windows(2).any(|pair| pair[0].color != pair[1].color));
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HighlightSpan {
    pub text: String,
    pub color: Option<[u8; 4]>,
}

impl HighlightSpan {
    fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            color: None,
        }
    }

    #[cfg(feature = "syntax")]
    fn color(mut self, color: [u8; 4]) -> Self {
        self.color = Some(color);
        self
    }
}

pub fn highlight_code(source: &str, language: &str, dark: bool) -> Vec<HighlightSpan> {
    if source.len() > 2 * 1024 * 1024 || source.lines().any(|line| line.len() > 64 * 1024) {
        return vec![HighlightSpan::new(source)];
    }
    #[cfg(feature = "syntax")]
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
        let mut spans: Vec<HighlightSpan> = Vec::new();
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
                            spans.push(HighlightSpan::new(text).color(color));
                        }
                    }
                }
                Err(_) => spans.push(HighlightSpan::new(line)),
            }
        }
        return spans;
    }
    #[cfg(not(feature = "syntax"))]
    {
        let _ = (language, dark);
        vec![HighlightSpan::new(source)]
    }
}

#[cfg(test)]
#[path = "tests/syntax.rs"]
mod tests;

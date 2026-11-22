//! Worker-local sparse checkpoints and exact unchanged-suffix convergence.
use crate::{AnnotatedText, TextAnnotation, TextDocument};
use syntect::{
    easy::HighlightLines, highlighting::HighlightState, parsing::ParseState, util::LinesWithEndings,
};

// Byte spacing bounds checkpoint count even for millions of empty lines. The
// final line has an additional checkpoint so streaming still reparses only it.
const CHECKPOINT_BYTES: usize = 4096;

#[derive(Clone)]
struct Checkpoint {
    offset: usize,
    highlight: HighlightState,
    parse: ParseState,
}

struct Entry {
    colors: AnnotatedText<[u8; 4]>,
    language: String,
    dark: bool,
    checkpoints: Vec<Checkpoint>,
}

#[derive(Default)]
pub(super) struct ResumeHighlighter {
    previous: Option<Entry>,
    #[cfg(test)]
    parsed_bytes: usize,
}

fn append_color(
    annotations: &mut Vec<TextAnnotation<[u8; 4]>>,
    start: usize,
    end: usize,
    color: [u8; 4],
) {
    if start >= end {
        return;
    }
    if let Some(last) = annotations
        .last_mut()
        .filter(|annotation| annotation.range.end == start && annotation.value == color)
    {
        last.range.end = end;
    } else {
        annotations.push(TextAnnotation {
            range: start..end,
            value: color,
        });
    }
}

fn keep_checkpoint(checkpoints: &mut Vec<Checkpoint>, checkpoint: Checkpoint, last_line: bool) {
    if checkpoints.last().is_none_or(|last| {
        checkpoint.offset > last.offset
            && (last_line || checkpoint.offset - last.offset >= CHECKPOINT_BYTES)
    }) {
        checkpoints.push(checkpoint);
    }
}

impl ResumeHighlighter {
    pub(super) fn highlight(
        &mut self,
        document: TextDocument,
        language: &str,
        dark: bool,
    ) -> AnnotatedText<[u8; 4]> {
        self.highlight_progress(document, language, dark, &mut |_, _| true)
            .unwrap()
    }

    pub(super) fn highlight_progress(
        &mut self,
        document: TextDocument,
        language: &str,
        dark: bool,
        progress: &mut dyn FnMut(usize, &[TextAnnotation<[u8; 4]>]) -> bool,
    ) -> Option<AnnotatedText<[u8; 4]>> {
        // Take first so a fallback or parser panic cannot leave stale states.
        let previous = self.previous.take();
        #[cfg(test)]
        {
            self.parsed_bytes = 0;
        }
        let source = document.text();
        if !super::within_limits(source) {
            return Some(AnnotatedText::new(document, Vec::new()));
        }
        let previous = previous.filter(|entry| entry.language == language && entry.dark == dark);
        if previous
            .as_ref()
            .is_some_and(|entry| entry.colors.document().text() == source)
        {
            let mut entry = previous.unwrap();
            // A new snapshot revision may contain the same bytes. Publish its
            // identity without asking the parser to repeat identical work.
            entry.colors = AnnotatedText::new(document, entry.colors.annotations().to_vec());
            let colors = entry.colors.clone();
            self.previous = Some(entry);
            return Some(colors);
        }
        let (syntaxes, syntax, theme) = super::resources(language, dark);
        let mut offset = 0;
        let mut annotations: Vec<TextAnnotation<[u8; 4]>> = Vec::new();
        let mut checkpoints = Vec::new();
        let mut suffix = None;
        let mut highlighter = HighlightLines::new(syntax, theme);
        if let Some(entry) = previous.as_ref() {
            let old = entry.colors.document().text();
            let prefix = source
                .bytes()
                .zip(old.bytes())
                .take_while(|(a, b)| a == b)
                .count();
            // A byte prefix can end inside UTF-8. Only LF boundaries enter the
            // parser; edits within CRLF restart the whole affected line too.
            let line_start = source.as_bytes()[..prefix]
                .iter()
                .rposition(|b| *b == b'\n')
                .map_or(0, |at| at + 1);
            if let Some(checkpoint) = entry.checkpoints.iter().rfind(|c| c.offset <= line_start) {
                offset = checkpoint.offset;
                for annotation in entry
                    .colors
                    .annotations()
                    .iter()
                    .take_while(|a| a.range.start < offset)
                {
                    append_color(
                        &mut annotations,
                        annotation.range.start,
                        annotation.range.end.min(offset),
                        annotation.value,
                    );
                }
                for checkpoint in entry.checkpoints.iter().take_while(|c| c.offset < offset) {
                    keep_checkpoint(&mut checkpoints, checkpoint.clone(), false);
                }
                highlighter = HighlightLines::from_state(
                    theme,
                    checkpoint.highlight.clone(),
                    checkpoint.parse.clone(),
                );
            }
            let equal_tail = source.as_bytes()[prefix..]
                .iter()
                .rev()
                .zip(old.as_bytes()[prefix..].iter().rev())
                .take_while(|(a, b)| a == b)
                .count();
            suffix = Some((old.len() - equal_tail, source.len() - equal_tail));
        }
        let mut next_suffix = previous.as_ref().map_or(0, |entry| {
            entry
                .checkpoints
                .partition_point(|c| c.offset < suffix.unwrap().0)
        });
        let mut lines = LinesWithEndings::from(&source[offset..]).peekable();
        while let Some(line) = lines.next() {
            if !progress(offset, &annotations) {
                return None;
            }
            let last_line = lines.peek().is_none();
            let candidate = previous.as_ref().and_then(|entry| {
                let (old_start, new_start) = suffix.unwrap();
                while entry
                    .checkpoints
                    .get(next_suffix)
                    .is_some_and(|c| new_start + (c.offset - old_start) < offset)
                {
                    next_suffix += 1;
                }
                entry
                    .checkpoints
                    .get(next_suffix)
                    .filter(|c| new_start + (c.offset - old_start) == offset)
            });
            let save = last_line
                || checkpoints
                    .last()
                    .is_none_or(|c: &Checkpoint| offset - c.offset >= CHECKPOINT_BYTES);
            if save || candidate.is_some() {
                let (highlight, parse) = highlighter.state();
                if let Some(candidate) =
                    candidate.filter(|c| c.highlight == highlight && c.parse == parse)
                {
                    let entry = previous.as_ref().unwrap();
                    // Equal bytes alone are insufficient: a multiline delimiter
                    // earlier in the edit can change all following token colors.
                    for annotation in entry
                        .colors
                        .annotations()
                        .iter()
                        .filter(|a| a.range.end > candidate.offset)
                    {
                        append_color(
                            &mut annotations,
                            offset + annotation.range.start.max(candidate.offset)
                                - candidate.offset,
                            offset + annotation.range.end - candidate.offset,
                            annotation.value,
                        );
                    }
                    for (index, checkpoint) in
                        entry.checkpoints.iter().enumerate().skip(next_suffix)
                    {
                        let mut checkpoint = checkpoint.clone();
                        checkpoint.offset = offset + checkpoint.offset - candidate.offset;
                        keep_checkpoint(
                            &mut checkpoints,
                            checkpoint,
                            index + 1 == entry.checkpoints.len(),
                        );
                    }
                    break;
                }
                if save {
                    keep_checkpoint(
                        &mut checkpoints,
                        Checkpoint {
                            offset,
                            highlight: highlight.clone(),
                            parse: parse.clone(),
                        },
                        last_line,
                    );
                }
                highlighter = HighlightLines::from_state(theme, highlight, parse);
            }
            #[cfg(test)]
            {
                self.parsed_bytes += line.len();
            }
            if let Ok(parts) = highlighter.highlight_line(line, syntaxes) {
                let mut at = offset;
                for (style, text) in parts {
                    let end = at + text.len();
                    append_color(
                        &mut annotations,
                        at,
                        end,
                        [
                            style.foreground.r,
                            style.foreground.g,
                            style.foreground.b,
                            255,
                        ],
                    );
                    at = end;
                }
            }
            offset += line.len();
        }
        let colors = AnnotatedText::new(document, annotations);
        self.previous = Some(Entry {
            colors: colors.clone(),
            language: language.to_owned(),
            dark,
            checkpoints,
        });
        Some(colors)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn streaming_resume_matches_fresh_parsing_and_invalidates_changed_context() {
        let mut parser = ResumeHighlighter::default();
        let prefix = format!("{}/* 中文🙂\r\n", "let value = 1;\r\n".repeat(512));
        let first = format!("{prefix}unterminated *");
        let cases = [
            (first, "rs", false, false),
            (
                format!("{prefix}unterminated */\r\nfn main() {{}}"),
                "rs",
                false,
                true,
            ),
            (
                format!("{prefix}unterminated */\r\nfn edited() {{}}"),
                "rs",
                false,
                true,
            ),
            (
                format!("{prefix}changed tail */\r\nlet value = 2;"),
                "rs",
                false,
                true,
            ),
            (
                format!("{prefix}changed tail */\r\nlet value = 2;"),
                "rs",
                true,
                false,
            ),
            (
                format!("{prefix}changed tail */\r\nlet value = 2;"),
                "py",
                true,
                false,
            ),
            ("let changed_prefix = 3;\n".into(), "rs", false, false),
            (String::new(), "rs", false, false),
        ];
        for (source, language, dark, reused) in cases {
            let document = TextDocument::new(source);
            let result = parser.highlight(document.clone(), language, dark);
            let fresh = super::super::highlight_document(document.clone(), language, dark);
            assert_eq!(result.document(), &document);
            assert_eq!(result.annotations(), fresh.annotations());
            if reused {
                assert!(
                    parser.parsed_bytes < CHECKPOINT_BYTES + 128,
                    "streaming edits must reuse the prefix"
                );
            } else {
                assert_eq!(parser.parsed_bytes, document.len());
            }
        }
        let mut streamed = prefix.clone();
        parser.highlight(TextDocument::new(prefix.clone()), "rs", false);
        for chunk in ["tail *", "/\r", "\nlet 中文 = \"", "🙂", "\";\n", "// end"] {
            streamed.push_str(chunk);
            let document = TextDocument::new(streamed.clone());
            let result = parser.highlight(document.clone(), "rs", false);
            let fresh = super::super::highlight_document(document, "rs", false);
            assert_eq!(result.annotations(), fresh.annotations(), "chunk {chunk:?}");
            assert!(parser.parsed_bytes < 128, "appends retain last-line reuse");
        }
        parser.highlight(TextDocument::new(prefix.clone()), "rs", false);
        let fallback = parser.highlight(TextDocument::new("x".repeat(64 * 1024 + 1)), "rs", false);
        assert!(fallback.annotations().is_empty());
        assert!(parser.previous.is_none());
        parser.highlight(TextDocument::new(prefix), "rs", false);
        assert_eq!(
            parser.parsed_bytes,
            parser.previous.as_ref().unwrap().colors.document().len()
        );
    }

    #[test]
    fn edits_reuse_sparse_prefix_and_only_converge_with_equal_parser_states() {
        let mut parser = ResumeHighlighter::default();
        let base: String = (0..2048)
            .map(|i| format!("let value_{i} = \"中文🙂 {i}\";\r\n"))
            .collect();
        let mut source = base.clone();
        let verify = |parser: &mut ResumeHighlighter, source: &str, language, dark| {
            let document = TextDocument::new(source);
            let result = parser.highlight(document.clone(), language, dark);
            let fresh = super::super::highlight_document(document.clone(), language, dark);
            assert_eq!(result.document(), &document);
            assert_eq!(result.annotations(), fresh.annotations());
            for annotation in result.annotations() {
                assert!(source.get(annotation.range.clone()).is_some());
            }
            let entry = parser.previous.as_ref().unwrap();
            assert!(entry.checkpoints.len() <= source.len() / CHECKPOINT_BYTES + 2);
            assert!(entry
                .checkpoints
                .iter()
                .all(|c| c.offset == 0 || source.as_bytes()[c.offset - 1] == b'\n'));
        };
        verify(&mut parser, &source, "rs", false);
        assert_eq!(parser.parsed_bytes, source.len());
        // Growing, shrinking and newline-shifting edits near both ends and the
        // middle; each result retains the complete fresh-parser intervals.
        for (from, to) in [
            ("value_1000", "value_中文🚀_1000"),
            ("value_10", "value_10x"),
            ("value_2000", "value_2000_extra"),
            ("let value_1001", "// inserted\r\nlet value_1001"),
            ("// inserted\r\n", ""),
            ("value_中文🚀_1000", "v"),
        ] {
            source = source.replacen(from, to, 1);
            verify(&mut parser, &source, "rs", false);
            assert!(
                parser.parsed_bytes < 3 * CHECKPOINT_BYTES,
                "edit {from:?} reparsed {} of {} bytes",
                parser.parsed_bytes,
                source.len()
            );
        }
        source.insert_str(0, "// new first line\n");
        verify(&mut parser, &source, "rs", false);
        assert!(parser.parsed_bytes < 3 * CHECKPOINT_BYTES);

        // An unchanged suffix inside a newly opened comment cannot converge
        // until the matching close restores both parser and highlight state.
        verify(&mut parser, &base, "rs", false);
        source = base
            .replacen("let value_700", "/*\r\nlet value_700", 1)
            .replacen("let value_1200", "*/\r\nlet value_1200", 1);
        verify(&mut parser, &source, "rs", false);
        assert!(parser.parsed_bytes > 12 * 1024);
        assert!(parser.parsed_bytes < source.len() / 2);
        source = source.replacen("*/\r\n", "", 1);
        verify(&mut parser, &source, "rs", false);
        assert!(
            parser.parsed_bytes > source.len() / 3,
            "an unterminated comment must propagate through the tail"
        );
        source = base.clone();
        verify(&mut parser, &source, "rs", false);
        verify(&mut parser, &source, "rs", true);
        assert_eq!(parser.parsed_bytes, source.len());
        verify(&mut parser, &source, "rs", true);
        assert_eq!(
            parser.parsed_bytes, 0,
            "equal bytes with a new revision reuse all colors"
        );
        verify(&mut parser, &source, "py", true);
        assert_eq!(parser.parsed_bytes, source.len());

        // Captured raw-string/heredoc delimiters and UTF-8 prefix splits can
        // carry state beyond the edited line, across different syntaxes.
        for (language, original, replacement) in [
            (
                "rs",
                "let text = r#\"hello\n中文🙂\n\"#;\n",
                "let text = r##\"hello\n中文🚀\n\"#;\n",
            ),
            (
                "py",
                "value = '''hello\n中文🙂\n'''\n",
                "value = \"\"\"hello\n中文🚀\n'''\n",
            ),
            (
                "sh",
                "cat <<'END'\n中文🙂\nEND\n",
                "cat <<'OTHER'\n中文🚀\nEND\n",
            ),
        ] {
            let initial = format!(
                "{}{original}{}",
                "\n".repeat(5000),
                "tail = 1\n".repeat(2048)
            );
            verify(&mut parser, &initial, language, false);
            let edited = initial.replacen(original, replacement, 1);
            verify(&mut parser, &edited, language, false);
            verify(&mut parser, &initial, language, false);
        }
    }

    #[test]
    fn checkpoint_storage_stays_sparse_across_repeated_edits_and_line_boundaries() {
        let mut parser = ResumeHighlighter::default();
        let mut source = format!(
            "{}// 中文🙂\r\n{}",
            "\n".repeat(20000),
            "let v = 1;\n".repeat(1024)
        );
        for index in 0..40 {
            let document = TextDocument::new(&source);
            let result = parser.highlight(document.clone(), "rs", false);
            assert_eq!(
                result.annotations(),
                super::super::highlight_document(document, "rs", false).annotations()
            );
            let checkpoints = &parser.previous.as_ref().unwrap().checkpoints;
            assert!(checkpoints.len() <= source.len() / CHECKPOINT_BYTES + 2);
            assert!(checkpoints.windows(2).all(|p| p[1].offset > p[0].offset));
            if index % 2 == 0 {
                source.insert_str(4096, "\n");
            } else {
                source.remove(8192);
            }
            source.push('\n');
        }
        for source in [
            "x\r",
            "x\r\n",
            "x\n",
            "🙂\n",
            "🚀\n",
            "",
            "\n",
            "\n\n",
            "/* open",
            "/* open\n*/\n",
        ] {
            let document = TextDocument::new(source);
            let result = parser.highlight(document.clone(), "rs", false);
            assert_eq!(result.document(), &document);
            assert_eq!(
                result.annotations(),
                super::super::highlight_document(document, "rs", false).annotations()
            );
        }
        let oversized = parser.highlight(
            TextDocument::new("\n".repeat(2 * 1024 * 1024 + 1)),
            "rs",
            false,
        );
        assert!(oversized.annotations().is_empty());
        assert!(parser.previous.is_none());
    }
}

use std::path::{Path, PathBuf};

pub fn image_path(destination: &str, root: Option<&Path>) -> Option<PathBuf> {
    if destination.starts_with("//") || destination.chars().any(char::is_control) {
        return None;
    }
    let destination = decode_path(destination)?;
    let path = PathBuf::from(&destination);
    if path.is_absolute() {
        return Some(path);
    }
    if destination.contains(':') {
        return None;
    }
    Some(root.map(|root| root.join(&path)).unwrap_or(path))
}

fn decode_path(source: &str) -> Option<String> {
    let mut output = Vec::new();
    let bytes = source.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            if let Ok(value) = u8::from_str_radix(source.get(index + 1..index + 3)?, 16) {
                if value == 0 {
                    return None;
                }
                output.push(value);
                index += 3;
                continue;
            }
        }
        output.push(bytes[index]);
        index += 1;
    }
    String::from_utf8(output).ok()
}

#[cfg(feature = "webview")]
pub fn html_images(
    source: &str,
    root: Option<&Path>,
) -> std::collections::BTreeMap<String, String> {
    use base64::Engine;
    use html5ever::{
        buffer_queue::BufferQueue,
        tokenizer::{states::RawKind, StartTag, Token, TokenSink, TokenSinkResult, Tokenizer},
    };
    use std::{cell::RefCell, io::Read};
    struct Images(RefCell<Vec<String>>);
    impl TokenSink for Images {
        type Handle = ();
        fn process_token(&self, token: Token, _: u64) -> TokenSinkResult<()> {
            if let Token::TagToken(tag) = token {
                if tag.kind == StartTag {
                    match tag.name.as_ref() {
                        "script" => return TokenSinkResult::RawData(RawKind::ScriptData),
                        "style" | "xmp" | "iframe" => {
                            return TokenSinkResult::RawData(RawKind::Rawtext)
                        }
                        "textarea" | "title" => return TokenSinkResult::RawData(RawKind::Rcdata),
                        "img" if self.0.borrow().len() < 64 => {
                            for attribute in tag.attrs {
                                if attribute.name.local.as_ref() == "src" {
                                    self.0.borrow_mut().push(attribute.value.to_string());
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
            TokenSinkResult::Continue
        }
    }
    let mut result = std::collections::BTreeMap::new();
    let Some(root) = root.and_then(|root| root.canonicalize().ok()) else {
        return result;
    };
    let tokenizer = Tokenizer::new(Images(RefCell::new(Vec::new())), Default::default());
    let input = BufferQueue::default();
    input.push_back(source.into());
    let _ = tokenizer.feed(&input);
    tokenizer.end();
    let mut total = 0usize;
    for destination in tokenizer.sink.0.into_inner() {
        if result.contains_key(&destination) {
            continue;
        }
        let Some(path) = image_path(&destination, Some(&root))
            .and_then(|path| path.canonicalize().ok())
            .filter(|path| path.starts_with(&root))
        else {
            continue;
        };
        let Ok(file) = std::fs::File::open(&path) else {
            continue;
        };
        if !file
            .metadata()
            .is_ok_and(|metadata| metadata.is_file() && metadata.len() <= 16 * 1024 * 1024)
        {
            continue;
        }
        let mut bytes = Vec::new();
        if file
            .take(16 * 1024 * 1024 + 1)
            .read_to_end(&mut bytes)
            .is_err()
            || bytes.len() > 16 * 1024 * 1024
        {
            continue;
        }
        total += bytes.len();
        if total > 32 * 1024 * 1024 {
            break;
        }
        let mime = match image::guess_format(&bytes) {
            Ok(image::ImageFormat::Png) => "image/png",
            Ok(image::ImageFormat::Jpeg) => "image/jpeg",
            Ok(image::ImageFormat::Gif) => "image/gif",
            Ok(image::ImageFormat::WebP) => "image/webp",
            _ if path
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("svg"))
                && std::str::from_utf8(&bytes).is_ok_and(|text| text.contains("<svg")) =>
            {
                "image/svg+xml"
            }
            _ => continue,
        };
        result.insert(
            destination,
            format!(
                "data:{mime};base64,{}",
                base64::engine::general_purpose::STANDARD.encode(bytes)
            ),
        );
    }
    result
}

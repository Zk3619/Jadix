pub mod annotations;
#[cfg(feature = "diff")]
pub mod diff;
pub mod document;
pub mod document_layout;
#[cfg(feature = "document-editing")]
pub mod html_fragment;
#[cfg(feature = "document-editing")]
pub mod native_html;

pub use annotations::{AnnotatedText, TextAnnotation};
#[cfg(feature = "diff")]
pub use diff::{DiffDocument, DiffLineKind, DiffMode, DiffRow};
pub use document::{DocumentEditError, TextDocument};
pub use document_layout::{DocumentHeightIndex, DocumentScrollAnchor};

/// Allow relative document targets and HTTP(S), mail and image-data schemes.
/// This is a presentation policy, not authorization to read a local asset.
pub fn is_safe_document_destination(destination: &str) -> bool {
    if destination.chars().any(char::is_control) {
        return false;
    }
    let destination = destination.trim();
    let Some((scheme, rest)) = destination.split_once(':') else {
        return true;
    };
    scheme.eq_ignore_ascii_case("https")
        || scheme.eq_ignore_ascii_case("http")
        || scheme.eq_ignore_ascii_case("mailto")
        || (scheme.eq_ignore_ascii_case("data")
            && rest
                .get(..6)
                .is_some_and(|prefix| prefix.eq_ignore_ascii_case("image/")))
}

pub mod syntax;

#[cfg(feature = "document-math")]
pub mod math;
#[cfg(feature = "science")]
pub mod molecule;
#[cfg(feature = "science")]
pub mod protein;
#[cfg(feature = "markdown")]
pub mod rich_text;
#[cfg(feature = "markdown")]
pub mod semantic_document;
#[cfg(feature = "markdown")]
pub use rich_text::{RichTextBlock, RichTextSpan};
#[cfg(feature = "markdown")]
pub use semantic_document::{
    Document, DocumentAlignment, DocumentBlock, DocumentBlockKind, DocumentFormat, DocumentInline,
    DocumentInlineKind, DocumentInlineStyle,
};

#[cfg(feature = "checkpoint")]
pub mod checkpoint;

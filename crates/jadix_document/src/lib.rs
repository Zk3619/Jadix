#[cfg(feature = "diff")]
pub mod diff;
pub mod document;
pub mod document_layout;

#[cfg(feature = "diff")]
pub use diff::{DiffDocument, DiffLineKind, DiffMode, DiffRow};
pub use document::{DocumentEditError, TextDocument};
pub use document_layout::{DocumentHeightIndex, DocumentScrollAnchor};

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

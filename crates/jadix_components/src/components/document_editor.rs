//! In-place source reveal over the same semantic viewport used for document previews.

use std::cell::RefCell;
use std::ops::Range;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_EDITOR_MOUNT: AtomicU64 = AtomicU64::new(1);

use crate::components::base::{paint_slot, SlotChrome};
use crate::components::document_view::{render_document_block, DocumentBlockRenderer};
use crate::components::document_viewport::{document_viewport, ActiveSource, DocumentSurface};
use crate::components::text_editor::text_editor;
use crate::components::text_view::redraw_on_snapshot;
use crate::core::reactivity::{use_signal, Signal};
use crate::core::tree::Element;
use crate::text::{
    DocumentBlock, DocumentBlockKind, DocumentEditMode, DocumentEditorSnapshot,
    DocumentEditorState, DocumentMark, DocumentSelection,
};
use crate::utils::text_field::{
    self, input_caret_signal_key, input_preedit_signal_key, input_selection_signal_key,
    input_value_signal_key,
};
use unicode_segmentation::UnicodeSegmentation;
use winit::event::{ElementState, KeyEvent};
use winit::keyboard::{Key, ModifiersState, NamedKey};

pub fn document_editor(id: impl Into<String>, state: DocumentEditorState) -> DocumentEditorBuilder {
    DocumentEditorBuilder {
        id: id.into(),
        state,
        viewport_h: 480.0,
        overscan: 3,
        renderer: None,
        asset_root: None,
        chrome: SlotChrome::default(),
    }
}

pub struct DocumentEditorBuilder {
    id: String,
    state: DocumentEditorState,
    viewport_h: f32,
    overscan: usize,
    renderer: Option<DocumentBlockRenderer>,
    asset_root: Option<std::path::PathBuf>,
    chrome: SlotChrome,
}

impl DocumentEditorBuilder {
    crate::impl_slot_chrome_methods!();
    pub fn asset_root(mut self, path: impl Into<std::path::PathBuf>) -> Self {
        self.asset_root = Some(path.into());
        self
    }
    pub fn viewport_h(mut self, height: f32) -> Self {
        self.viewport_h = height.max(1.0);
        self
    }
    pub fn overscan(mut self, blocks: usize) -> Self {
        self.overscan = blocks;
        self
    }
    pub fn block_renderer(
        mut self,
        renderer: impl Fn(&crate::text::Document, &DocumentBlock) -> Option<Element> + 'static,
    ) -> Self {
        self.renderer = Some(Rc::new(renderer));
        self
    }
}

struct NativeEdit {
    range: Range<usize>,
    text: String,
    epoch: u64,
    mount: u64,
}

struct EditorBridge {
    state: DocumentEditorState,
    field: String,
    value: Signal<String>,
    active: Option<NativeEdit>,
    next_mount: u64,
}

fn reveal_range(snapshot: &DocumentEditorSnapshot) -> Option<Range<usize>> {
    if !snapshot.editing || snapshot.mode == DocumentEditMode::Preview {
        return None;
    }
    let source = snapshot.document.source();
    if snapshot.mode == DocumentEditMode::Source {
        return Some(0..source.len());
    }
    let selection = snapshot.selection.range();
    let first = source.line_at_byte(selection.start)?;
    let last = source.line_at_byte(selection.end)?;
    let mut range = source.line_range(first)?.start..source.line_range(last)?.end;
    for byte in [selection.start, selection.end] {
        if let Some(position) = snapshot.document.block_at(byte) {
            let block = &snapshot.document.blocks()[position];
            let group = block.edit_range.clone().or_else(|| {
                matches!(
                    block.kind,
                    DocumentBlockKind::Math
                        | DocumentBlockKind::Chemistry
                        | DocumentBlockKind::Html
                        | DocumentBlockKind::Molecule { .. }
                )
                .then(|| block.source.clone())
            });
            if let Some(group) = group {
                range.start = range.start.min(group.start);
                let end = source.text()[group.clone()]
                    .trim_end_matches(['\r', '\n'])
                    .len()
                    + group.start;
                range.end = range.end.max(end);
            }
        }
    }
    Some(range)
}

fn replacement_delta<'a>(previous: &str, next: &'a str) -> (Range<usize>, &'a str) {
    let mut prefix = previous
        .bytes()
        .zip(next.bytes())
        .take_while(|(left, right)| left == right)
        .count();
    prefix = previous
        .floor_char_boundary(prefix)
        .min(next.floor_char_boundary(prefix));
    let mut suffix = previous[prefix..]
        .bytes()
        .rev()
        .zip(next[prefix..].bytes().rev())
        .take_while(|(left, right)| left == right)
        .count();
    while !previous.is_char_boundary(previous.len() - suffix)
        || !next.is_char_boundary(next.len() - suffix)
    {
        suffix -= 1;
    }
    (
        prefix..previous.len() - suffix,
        &next[prefix..next.len() - suffix],
    )
}

impl EditorBridge {
    fn sync(&mut self) {
        if !self.state.signal().is_alive() {
            return;
        }
        let mut snapshot = self.state.snapshot();
        let preedit = use_signal(&input_preedit_signal_key(&self.field), String::new);
        if !preedit.peek().is_empty() {
            return;
        }
        if let Some(active) = &mut self.active {
            if snapshot.epoch == active.epoch {
                let text = use_signal(&input_value_signal_key(&self.field), String::new).peek();
                let caret = use_signal(&input_caret_signal_key(&self.field), || None::<usize>)
                    .peek()
                    .unwrap_or(text.len())
                    .min(text.len());
                let native_selection = use_signal(&input_selection_signal_key(&self.field), || {
                    None::<(usize, usize)>
                })
                .peek();
                let anchor = native_selection
                    .map(|(start, end)| if caret == start { end } else { start })
                    .unwrap_or(caret)
                    .min(text.len());
                let selection = DocumentSelection {
                    anchor: active.range.start + text.floor_char_boundary(anchor),
                    caret: active.range.start + text.floor_char_boundary(caret),
                };
                if text != active.text {
                    let (local, insert) = replacement_delta(&active.text, &text);
                    let range = active.range.start + local.start..active.range.start + local.end;
                    if self.state.apply(range, insert, selection, true).is_ok() {
                        active.range.end = active.range.start + text.len();
                        active.text = text;
                        snapshot = self.state.snapshot();
                        active.epoch = snapshot.epoch;
                    }
                } else if selection != snapshot.selection {
                    let _ = self.state.select(selection, true);
                    snapshot = self.state.snapshot();
                    active.epoch = snapshot.epoch;
                }
            }
        }
        let desired = reveal_range(&snapshot);
        let Some(range) = desired else {
            let focused = use_signal("__jadix_button_focus_id", String::new);
            if focused.peek() == text_field::textarea_focus_id(&self.field) {
                let viewport = self.field.strip_suffix("__source").unwrap_or(&self.field);
                focused.set_silent(crate::core::scroll_viewport::focus_id_for_scroll_viewport(
                    viewport,
                ));
            }
            self.active = None;
            return;
        };
        let text = snapshot.document.source().text()[range.clone()].to_string();
        let rebind = self.active.as_ref().is_none_or(|active| {
            active.range != range || active.text != text || active.epoch != snapshot.epoch
        });
        if !rebind {
            return;
        }
        if self
            .active
            .as_ref()
            .is_none_or(|active| active.range.start != range.start)
        {
            let scroll = text_field::textarea_scroll_rest(&self.field);
            for key in [
                crate::core::scroll_viewport::scroll_offset_x_signal_key(&scroll),
                crate::core::scroll_viewport::scroll_offset_y_signal_key(&scroll),
                crate::core::scroll_viewport::scroll_target_x_signal_key(&scroll),
                crate::core::scroll_viewport::scroll_target_y_signal_key(&scroll),
            ] {
                use_signal(&key, || 0.0_f32).set_silent(0.0);
            }
        }
        let caret = snapshot
            .selection
            .caret
            .saturating_sub(range.start)
            .min(text.len());
        let anchor = snapshot
            .selection
            .anchor
            .saturating_sub(range.start)
            .min(text.len());
        self.value.set_silent(text.clone());
        use_signal(&input_value_signal_key(&self.field), String::new).set_silent(text.clone());
        use_signal(&input_caret_signal_key(&self.field), || None::<usize>).set_silent(Some(caret));
        use_signal(&input_selection_signal_key(&self.field), || {
            None::<(usize, usize)>
        })
        .set_silent((anchor != caret).then_some((anchor, caret)));
        use_signal("__jadix_button_focus_id", String::new)
            .set_silent(text_field::textarea_focus_id(&self.field));
        self.next_mount += 1;
        self.active = Some(NativeEdit {
            range,
            text,
            epoch: snapshot.epoch,
            mount: self.next_mount,
        });
    }

    fn surface(&self) -> DocumentSurface {
        DocumentSurface {
            document: self.state.document(),
            active: self.active.as_ref().map(|active| ActiveSource {
                range: active.range.clone(),
                mount: active.mount,
            }),
        }
    }

    fn key(&mut self, event: &KeyEvent, modifiers: ModifiersState) -> bool {
        if event.state != ElementState::Pressed {
            return false;
        }
        self.sync();
        let Some(active) = self.active.as_ref() else {
            return false;
        };
        let snapshot = self.state.snapshot();
        let shortcut = crate::utils::clipboard::shortcut_modifiers(&modifiers);
        if crate::utils::clipboard::is_undo_shortcut(event, &modifiers) {
            self.state.undo();
            return true;
        }
        if crate::utils::clipboard::is_redo_shortcut(event, &modifiers) {
            self.state.redo();
            return true;
        }
        if shortcut {
            match crate::utils::clipboard::edit_shortcut_letter(event) {
                Some('a') => {
                    let _ = self.state.set_selection(DocumentSelection {
                        anchor: 0,
                        caret: snapshot.document.source().len(),
                    });
                    return true;
                }
                Some('b') => {
                    let _ = self.state.toggle_mark(DocumentMark::Bold);
                    return true;
                }
                Some('i') => {
                    let _ = self.state.toggle_mark(DocumentMark::Italic);
                    return true;
                }
                Some('e') => {
                    let _ = self.state.toggle_mark(DocumentMark::Code);
                    return true;
                }
                _ => {}
            }
        }
        let source = snapshot.document.source();
        let caret = snapshot.selection.caret;
        let local = caret
            .saturating_sub(active.range.start)
            .min(active.text.len());
        let destination = match event.logical_key {
            Key::Named(NamedKey::Escape) => {
                self.state.finish_editing();
                return true;
            }
            Key::Named(NamedKey::Home) if shortcut => Some(0),
            Key::Named(NamedKey::End) if shortcut => Some(source.len()),
            Key::Named(NamedKey::ArrowLeft) if local == 0 && !shortcut => {
                Some(previous_boundary(source.text(), caret))
            }
            Key::Named(NamedKey::ArrowRight) if local == active.text.len() && !shortcut => {
                Some(next_boundary(source.text(), caret))
            }
            Key::Named(NamedKey::ArrowUp | NamedKey::ArrowDown) if !shortcut => {
                if active.text.len() >= crate::text::visual_lines::VIRT_CHAR_THRESHOLD {
                    return false;
                }
                let up = event.logical_key == Key::Named(NamedKey::ArrowUp);
                let width = use_signal(
                    &text_field::textarea_wrap_width_signal_key(&self.field),
                    || 600.0_f32,
                )
                .peek();
                let model = crate::text::build_visual_lines(
                    &active.text,
                    16.0,
                    400,
                    24.0,
                    width,
                    crate::text::LineBreakSource::SoftWord,
                );
                let visual = crate::text::caret_visual_line_index(&model.ranges, local);
                if (up && visual == 0) || (!up && visual + 1 == model.line_count()) {
                    let line = source.line_at_byte(caret).unwrap_or(0);
                    let target_line = if up {
                        line.saturating_sub(1)
                    } else {
                        (line + 1).min(source.line_count() - 1)
                    };
                    let range = source.line_range(target_line).unwrap();
                    let target = &source.text()[range.clone()];
                    let target_model = crate::text::build_visual_lines(
                        target,
                        16.0,
                        400,
                        24.0,
                        width,
                        crate::text::LineBreakSource::SoftWord,
                    );
                    let target_visual = if up { target_model.line_count() - 1 } else { 0 };
                    Some(
                        range.start
                            + target_model
                                .byte_at_preferred_x(target_visual, model.caret_x_on_line(local)),
                    )
                } else {
                    None
                }
            }
            Key::Named(NamedKey::Backspace)
                if local == 0 && snapshot.selection.is_empty() && !shortcut =>
            {
                let start = previous_boundary(source.text(), caret);
                if start != caret {
                    let _ = self.state.replace(start..caret, "");
                }
                return true;
            }
            Key::Named(NamedKey::Delete)
                if local == active.text.len() && snapshot.selection.is_empty() && !shortcut =>
            {
                let end = next_boundary(source.text(), caret);
                if end != caret {
                    let _ = self.state.replace(caret..end, "");
                }
                return true;
            }
            Key::Named(NamedKey::Enter) => {
                let newline = if source.text().contains("\r\n") {
                    "\r\n"
                } else {
                    "\n"
                };
                let _ = self.state.replace_selection(newline);
                return true;
            }
            _ => None,
        };
        if let Some(destination) = destination {
            let anchor = if modifiers.shift_key() {
                snapshot.selection.anchor
            } else {
                destination
            };
            let _ = self.state.set_selection(DocumentSelection {
                anchor,
                caret: destination,
            });
            return true;
        }
        false
    }
}

fn previous_boundary(text: &str, caret: usize) -> usize {
    text[..caret]
        .grapheme_indices(true)
        .next_back()
        .map(|(position, _)| position)
        .unwrap_or(0)
}
fn next_boundary(text: &str, caret: usize) -> usize {
    text[caret..]
        .graphemes(true)
        .next()
        .map(|grapheme| caret + grapheme.len())
        .unwrap_or(caret)
}

fn source_at_pointer(block: &DocumentBlock, horizontal: f32, vertical: f32, width: f32) -> usize {
    let text = block.text();
    let size = if let DocumentBlockKind::Heading(level) = block.kind {
        [32.0, 28.0, 24.0, 21.0, 18.0, 16.0][level.saturating_sub(1).min(5) as usize]
    } else {
        16.0
    };
    let model = crate::text::build_visual_lines(
        &text,
        size,
        400,
        size * 1.5,
        width,
        crate::text::LineBreakSource::SoftWord,
    );
    let line = ((vertical.max(0.0) / (size * 1.5)) as usize).min(model.line_count() - 1);
    let display = model.byte_at_preferred_x(line, horizontal.max(0.0));
    let mut consumed = 0;
    for (position, inline) in block.inlines.iter().enumerate() {
        if display <= consumed + inline.text.len() {
            return block
                .source_position(position, display - consumed)
                .unwrap_or(block.source.start);
        }
        consumed += inline.text.len();
    }
    block.source.start
}

impl From<DocumentEditorBuilder> for Element {
    fn from(builder: DocumentEditorBuilder) -> Self {
        let id = crate::utils::resolve_instance_id(builder.id, "document-editor");
        let field = format!("{id}__source");
        let value_key = format!("{id}__source_buffer");
        let mount_key = format!("{id}__mount_owner");
        let mount_owner = use_signal(&mount_key, || 0u64);
        let mount = NEXT_EDITOR_MOUNT.fetch_add(1, Ordering::Relaxed);
        mount_owner.set_silent(mount);
        let bridge = Rc::new(RefCell::new(EditorBridge {
            state: builder.state.clone(),
            field: field.clone(),
            value: use_signal(&value_key, String::new),
            active: None,
            next_mount: 0,
        }));
        bridge.borrow_mut().sync();
        let surface_bridge = bridge.clone();
        let source = Rc::new(move || surface_bridge.borrow().surface());
        let render_bridge = bridge.clone();
        let state = builder.state.clone();
        let renderer = builder.renderer;
        let asset_root = builder.asset_root;
        let render = Rc::new(
            move |surface: &DocumentSurface, index: usize, width: f32, active: bool| {
                if active {
                    let bridge = render_bridge.borrow();
                    let source_mode = bridge.state.snapshot().mode == DocumentEditMode::Source;
                    let weak = Rc::downgrade(&render_bridge);
                    let mut editor: Element = text_editor(&bridge.field)
                        .value_signal(bridge.value)
                        .rows(1)
                        .font_size(16.0)
                        .line_numbers(source_mode)
                        .language("markdown")
                        .wrap(if source_mode { "none" } else { "word" })
                        .auto_height(1024.0)
                        .undo_enabled(false)
                        .key_handler(move |event, modifiers| {
                            weak.upgrade()
                                .is_some_and(|bridge| bridge.borrow_mut().key(event, modifiers))
                        })
                        .into();
                    editor.set_attr("data-document-source", "true");
                    return editor;
                }
                let block = surface.document.blocks()[index].clone();
                #[cfg(feature = "webview")]
                let element = if (block.kind == DocumentBlockKind::Html
                    || matches!(block.kind, DocumentBlockKind::Heading(_))
                        && block.payload.is_some())
                    && renderer.is_none()
                {
                    let html_state = state.clone();
                    let block_id = block.id;
                    crate::components::html_document::html_document_source_view(
                        format!("document-html-{}", block.id),
                        block.payload.as_deref().unwrap_or(""),
                        asset_root.as_deref(),
                        move || {
                            let snapshot = html_state.snapshot();
                            if snapshot.mode != DocumentEditMode::Preview {
                                if let Some(block) = snapshot
                                    .document
                                    .blocks()
                                    .get(index)
                                    .filter(|block| block.id == block_id)
                                {
                                    let _ = html_state.set_selection(DocumentSelection::caret(
                                        block.source.start,
                                    ));
                                }
                            }
                        },
                    )
                } else {
                    render_document_block(
                        &surface.document,
                        &block,
                        &renderer,
                        asset_root.as_deref(),
                    )
                };
                #[cfg(not(feature = "webview"))]
                let element = render_document_block(
                    &surface.document,
                    &block,
                    &renderer,
                    asset_root.as_deref(),
                );
                let mut element = element;
                if let DocumentBlockKind::ListItem {
                    checked: Some(checked),
                    ..
                } = block.kind
                {
                    if !element.children.is_empty()
                        && state.snapshot().mode != DocumentEditMode::Preview
                    {
                        let toggle_state = state.clone();
                        let block_id = block.id;
                        element.children[0] = std::mem::take(&mut element.children[0])
                            .cursor("pointer")
                            .on_mousedown(move |event| {
                                if event.button_index() != 0 {
                                    return;
                                }
                                event.stop_propagation();
                                event.prevent_default();
                                let snapshot = toggle_state.snapshot();
                                if let Some(block) = snapshot
                                    .document
                                    .blocks()
                                    .get(index)
                                    .filter(|block| block.id == block_id)
                                {
                                    let raw =
                                        &snapshot.document.source().text()[block.source.clone()];
                                    if let Some(offset) = raw
                                        .find(if checked { "[x]" } else { "[ ]" })
                                        .or_else(|| checked.then(|| raw.find("[X]")).flatten())
                                    {
                                        let start = block.source.start + offset + 1;
                                        let _ = toggle_state.replace(
                                            start..start + 1,
                                            if checked { " " } else { "x" },
                                        );
                                        let _ = toggle_state.set_selection(snapshot.selection);
                                    }
                                }
                            });
                    }
                }
                let graphical = matches!(block.kind, DocumentBlockKind::Molecule { .. });
                let block_id = block.id;
                let state = state.clone();
                let activate = move |event: &crate::app::pointer_events::MouseEvent| {
                    if event.button_index() != 0 {
                        return;
                    }
                    let snapshot = state.snapshot();
                    if snapshot.mode == DocumentEditMode::Preview {
                        return;
                    }
                    let Some(block) = snapshot
                        .document
                        .blocks()
                        .get(index)
                        .filter(|block| block.id == block_id)
                    else {
                        return;
                    };
                    event.prevent_default();
                    let byte = source_at_pointer(block, event.offset_x(), event.offset_y(), width);
                    let byte = snapshot
                        .document
                        .source()
                        .text()
                        .floor_char_boundary(byte.min(snapshot.document.source().len()));
                    let anchor = if event.shift_key {
                        snapshot.selection.anchor
                    } else {
                        byte
                    };
                    let _ = state.set_selection(DocumentSelection {
                        anchor,
                        caret: byte,
                    });
                };
                if graphical {
                    element.on_double_click(activate)
                } else {
                    element.cursor_text().on_mousedown(activate)
                }
            },
        );
        let sync_bridge = bridge.clone();
        let root = document_viewport(
            &id,
            builder.viewport_h,
            builder.overscan,
            source,
            render,
            Some(Rc::new(move || sync_bridge.borrow_mut().sync())),
        );
        let signal = builder.state.signal();
        let mut root = redraw_on_snapshot(
            paint_slot(root, "document-editor", &builder.chrome),
            Some(signal),
        );
        let previous = root.on_unmount.take();
        root.on_unmount(move || {
            if mount_owner.try_peek() != Some(mount) {
                if let Some(previous) = previous {
                    previous.fire();
                }
                return;
            }
            bridge.borrow_mut().sync();
            let focus = use_signal("__jadix_button_focus_id", String::new);
            if focus.peek() == text_field::textarea_focus_id(&field) {
                focus.set_silent(String::new());
            }
            text_field::wipe_text_edit_named_slots(&field);
            crate::core::reactivity::remove_named_signal(&value_key);
            crate::core::reactivity::remove_named_signal(&mount_key);
            if let Some(previous) = previous {
                previous.fire();
            }
        })
    }
}

crate::__component_contract! {
DocumentEditorProps [];
#[allow(non_snake_case)]
pub fn DocumentEditor(
identity [] id: String,
default [] state: DocumentEditorState,
plain [] viewport_h: Option<f32>,
plain [] children: Option<crate::core::tree::Element>,
) -> Element {
    let mut builder = document_editor(id, state);
    if let Some(height) = viewport_h {
        builder = builder.viewport_h(height);
    }
    builder.into()
}
}

#[cfg(all(test, feature = "product-tests"))]
#[path = "tests/document_editor.rs"]
mod tests;

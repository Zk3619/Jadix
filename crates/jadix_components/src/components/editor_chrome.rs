use crate::{
    core::{
        reactivity::use_signal,
        tree::{Element, LayoutStatus},
    },
    utils::text_field,
};
use std::cell::RefCell;
#[cfg(feature = "syntax-highlighting")]
use std::{
    rc::Rc,
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc, Arc, Mutex,
    },
};

pub(crate) fn decorate_editor(
    mut editor: Element,
    field: &str,
    numbers: bool,
    language: Option<String>,
    first_line: usize,
) -> Element {
    if let Some(language) = language {
        #[cfg(feature = "syntax-highlighting")]
        {
            let (requests, jobs) = mpsc::sync_channel::<()>(1);
            let pending = Arc::new(Mutex::new(None::<(u64, String, bool)>));
            let worker_pending = pending.clone();
            let (completed, results) = mpsc::channel();
            let revision = Arc::new(AtomicU64::new(0));
            let latest = revision.clone();
            std::thread::spawn(move || {
                while jobs.recv().is_ok() {
                    let Some(job) = worker_pending
                        .lock()
                        .unwrap_or_else(|error| error.into_inner())
                        .take()
                    else {
                        continue;
                    };
                    let spans = crate::text::syntax::highlight_code(&job.1, &language, job.2);
                    if latest.load(Ordering::Acquire) == job.0 {
                        if completed.send((job.0, spans)).is_err() {
                            break;
                        }
                        crate::app::run_on_ui_thread(crate::app::runtime::request_redraw);
                    }
                }
            });
            let previous = Rc::new(RefCell::new((String::new(), false, 0u64)));
            let result = Rc::new(RefCell::new(results));
            let template = text_field::input_text_template_id(field);
            fn attach(
                element: &mut Element,
                template: &str,
                callback: &mut Option<
                    Box<dyn Fn(&mut Element, &mut crate::core::tree::LayoutTree) -> LayoutStatus>,
                >,
            ) {
                if element.template_id.as_deref() == Some(template) {
                    let callback = callback.take().unwrap();
                    *element = std::mem::take(element).reactive_sync(callback);
                } else {
                    for child in &mut element.children {
                        attach(child, template, callback);
                    }
                }
            }
            let callback = move |leaf: &mut Element, layout: &mut crate::core::tree::LayoutTree| {
                let text = leaf.text.as_deref().unwrap_or("");
                let dark = crate::style::is_dark_theme();
                let mut previous = previous.borrow_mut();
                let mut changed = false;
                if previous.0 != text || previous.1 != dark {
                    let version = revision.fetch_add(1, Ordering::AcqRel) + 1;
                    *previous = (text.to_string(), dark, version);
                    *pending.lock().unwrap_or_else(|error| error.into_inner()) =
                        Some((version, text.to_string(), dark));
                    let _ = requests.try_send(());
                    changed = leaf.text_spans.take().is_some();
                }
                while let Ok((version, spans)) = result.borrow_mut().try_recv() {
                    if version == previous.2 {
                        leaf.text_spans =
                            (!spans.iter().all(|span| span.color.is_none())).then_some(spans);
                        changed = true;
                    }
                }
                if changed {
                    layout.mark_element_layout_dirty(leaf);
                    LayoutStatus::LayoutChanged
                } else {
                    LayoutStatus::Clean
                }
            };
            attach(&mut editor, &template, &mut Some(Box::new(callback)));
        }
        #[cfg(not(feature = "syntax-highlighting"))]
        let _ = language;
    }
    if !numbers {
        return editor;
    }
    let Some(description) = editor.input_paint_sync.clone() else {
        return editor;
    };
    let scroll = text_field::textarea_scroll_rest(field);
    let line_height = text_field::text_edit_line_box_px(
        description.text_size_px,
        description.line_height_px,
        description.leading_scale,
    );
    let viewport = crate::core::scroll_viewport::focus_id_for_scroll_viewport(&scroll);
    for child in &mut editor.children {
        if child.focus_id.as_deref() == Some(&viewport) {
            *child = std::mem::take(child).pl(64.0);
        }
    }
    let field = field.to_string();
    let previous_text = RefCell::new(String::new());
    let document = RefCell::new(crate::text::TextDocument::new(""));
    let mut labels = Element::new()
        .w(48.0)
        .absolute()
        .top(0.0)
        .left(0.0)
        .text_size(description.text_size_px)
        .text_leading(line_height / description.text_size_px)
        .text_align("right")
        .text_color("muted-foreground")
        .white_space("pre")
        .reactive_sync(move |element, layout| {
            let text = use_signal(&text_field::input_value_signal_key(&field), String::new).peek();
            if *previous_text.borrow() != text {
                *document.borrow_mut() = crate::text::TextDocument::new(text.clone());
                *previous_text.borrow_mut() = text;
            }
            let offset = use_signal(
                &crate::core::scroll_viewport::scroll_offset_y_signal_key(&scroll),
                || 0.0_f32,
            )
            .peek();
            let height = use_signal(
                &crate::core::scroll_viewport::scroll_viewport_h_signal_key(&scroll),
                || 400.0_f32,
            )
            .peek();
            let first = ((offset - 8.0).max(0.0) / line_height).floor() as usize;
            let last = (first + (height / line_height).ceil() as usize + 2)
                .min(document.borrow().line_count());
            let text = (first..last)
                .map(|line| (first_line + line).to_string())
                .collect::<Vec<_>>()
                .join("\n");
            let position = 8.0 + first as f32 * line_height - offset;
            if element.text.as_deref() != Some(&text) || element.style.translate[1] != position {
                *element = std::mem::take(element).text(text).translate(0.0, position);
                layout.mark_element_layout_dirty(element);
                LayoutStatus::LayoutChanged
            } else {
                LayoutStatus::Clean
            }
        });
    labels.set_attr("data-editor-line-numbers", "true");
    labels.set_attr("jadix_text_edit_ignore", "true");
    editor.child(
        Element::new()
            .absolute()
            .left(0.0)
            .top(0.0)
            .bottom(0.0)
            .w(56.0)
            .overflow("hidden")
            .pointer_events_none()
            .bg("muted")
            .child(labels),
    )
}

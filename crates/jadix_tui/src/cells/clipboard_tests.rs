use super::input_cost_tests::editor_surface;
use super::*;
use crossterm::event::KeyEvent;
use std::{cell::RefCell, rc::Rc};
#[derive(Default)]
struct State {
    text: String,
    reads: usize,
    writes: usize,
    drops: usize,
    fail_read: bool,
    fail_write: bool,
}
struct Provider(Rc<RefCell<State>>);
impl crate::TextClipboard for Provider {
    fn read_text(&self) -> jadix_clipboard::Result<String> {
        let mut s = self.0.borrow_mut();
        s.reads += 1;
        if s.fail_read {
            return Err(io::Error::other("fixture unavailable").into());
        }
        Ok(s.text.clone())
    }
    fn write_text(&self, text: &str) -> jadix_clipboard::Result<()> {
        let mut s = self.0.borrow_mut();
        s.writes += 1;
        if s.fail_write {
            return Err(io::Error::other("fixture busy").into());
        }
        s.text = text.into();
        Ok(())
    }
}
impl Drop for Provider {
    fn drop(&mut self) {
        self.0.borrow_mut().drops += 1;
    }
}
fn provider(s: &mut CellSurface) -> Rc<RefCell<State>> {
    let shared = Rc::new(RefCell::new(State::default()));
    s.set_clipboard(Some(Box::new(Provider(shared.clone()))))
        .unwrap();
    shared
}
fn key(s: &mut CellSurface, letter: char) -> io::Result<Option<PlatformEvent>> {
    s.handle(Event::Key(KeyEvent::new(
        KeyCode::Char(letter),
        KeyModifiers::CONTROL,
    )))
}
fn text(s: &CellSurface, id: NodeId) -> &str {
    s.fields[&id].editor.document().text()
}
#[test]
fn host_clipboard_crosses_fields_and_normalizes_input_without_losing_undo() {
    let (mut s, nodes) = editor_surface(2);
    let shared = provider(&mut s);
    s.focus_node(nodes[0].id).unwrap();
    key(&mut s, 'a').unwrap();
    s.handle(Event::Paste("shared 中文 🦀".into())).unwrap();
    key(&mut s, 'a').unwrap();
    assert!(key(&mut s, 'c').unwrap().is_none());
    assert_eq!(shared.borrow().text, "shared 中文 🦀");
    s.focus_node(nodes[1].id).unwrap();
    key(&mut s, 'a').unwrap();
    key(&mut s, 'v').unwrap();
    assert_eq!(text(&s, nodes[1].id), "shared 中文 🦀");
    key(&mut s, 'z').unwrap();
    assert_eq!(text(&s, nodes[1].id), "seed");
    shared.borrow_mut().text = "one\r\ntwo\rthree\u{1b}[31m 🦀".into();
    key(&mut s, 'a').unwrap();
    key(&mut s, 'v').unwrap();
    assert_eq!(text(&s, nodes[1].id), "one two three[31m 🦀");
    assert_eq!(shared.borrow().reads, 2);
    s.fields
        .get_mut(&nodes[1].id)
        .unwrap()
        .editor
        .set_max_length(Some(4))
        .unwrap();
    shared.borrow_mut().text = "🦀🦀🦀".into();
    key(&mut s, 'a').unwrap();
    key(&mut s, 'v').unwrap();
    assert_eq!(text(&s, nodes[1].id), "🦀🦀");
    for value in ["", "\u{1b}\0"] {
        shared.borrow_mut().text = value.into();
        key(&mut s, 'a').unwrap();
        assert!(key(&mut s, 'v').unwrap().is_none());
        assert_eq!(text(&s, nodes[1].id), "🦀🦀");
        assert_eq!(s.fields[&nodes[1].id].editor.selection(), 0..8);
    }
    let root = jadix_core::presentation::RootId::new();
    let mut nodes = vec![super::input_cost_tests::field(NodeId::new(root), "", 0, 0.)];
    let mut multiline = CellSurface::new(RootLifetime::new(root), [1., 1.]).unwrap();
    nodes[0].descriptor.content = NodeContent::Textarea {
        value: String::new(),
        placeholder: String::new(),
        enabled: true,
        revision: 0,
        max_length: None,
    };
    multiline
        .present(multiline.revision(), nodes.clone())
        .unwrap();
    multiline.layout(Rect::new(0, 0, 40, 12));
    let host = provider(&mut multiline);
    host.borrow_mut().text = "line1\r\nline2\r🦀".into();
    multiline.focus_node(nodes[0].id).unwrap();
    key(&mut multiline, 'a').unwrap();
    key(&mut multiline, 'v').unwrap();
    assert_eq!(text(&multiline, nodes[0].id), "line1\nline2\n🦀");
}
#[test]
fn failed_cut_or_paste_preserves_document_selection_history_and_budget() {
    let (mut s, nodes) = editor_surface(2);
    let shared = provider(&mut s);
    s.focus_node(nodes[0].id).unwrap();
    key(&mut s, 'a').unwrap();
    let before = s.fields[&nodes[0].id].editor.clone();
    let bytes = s.editor_bytes;
    shared.borrow_mut().fail_write = true;
    assert!(key(&mut s, 'x').is_err());
    assert_eq!(s.fields[&nodes[0].id].editor.document(), before.document());
    assert_eq!(
        s.fields[&nodes[0].id].editor.selection(),
        before.selection()
    );
    assert_eq!(s.editor_bytes, bytes);
    shared.borrow_mut().fail_read = true;
    assert!(key(&mut s, 'v').is_err());
    assert_eq!(
        s.fields[&nodes[0].id].editor.retained_bytes(),
        before.retained_bytes()
    );
    assert_eq!(
        s.fields[&nodes[0].id].editor.selection(),
        before.selection()
    );
    assert_eq!(text(&s, nodes[0].id), "seed");
}
#[test]
fn password_release_focus_loss_and_disposed_surface_never_export_or_read() {
    let (mut s, mut nodes) = editor_surface(2);
    if let NodeContent::Input { password, .. } = &mut nodes[0].descriptor.content {
        *password = true;
    }
    s.present(s.revision(), nodes.clone()).unwrap();
    s.layout(Rect::new(0, 0, 40, 12));
    let shared = provider(&mut s);
    s.focus_node(nodes[0].id).unwrap();
    key(&mut s, 'a').unwrap();
    key(&mut s, 'c').unwrap();
    key(&mut s, 'x').unwrap();
    assert_eq!(text(&s, nodes[0].id), "seed");
    s.focus_node(nodes[1].id).unwrap();
    let mut release = KeyEvent::new(KeyCode::Char('v'), KeyModifiers::CONTROL);
    release.kind = KeyEventKind::Release;
    s.handle(Event::Key(release)).unwrap();
    s.handle(Event::FocusLost).unwrap();
    key(&mut s, 'v').unwrap();
    s.lifetime.dispose();
    assert!(key(&mut s, 'v').is_err());
    assert_eq!((shared.borrow().reads, shared.borrow().writes), (0, 0));
}
#[test]
fn clipboard_transfer_budget_and_empty_selection_are_checked_before_export() {
    let (mut s, nodes) = editor_surface(2);
    let shared = provider(&mut s);
    s.focus_node(nodes[0].id).unwrap();
    let too_large = "x".repeat(crate::TerminalClipboard::MAX_BYTES + 1);
    shared.borrow_mut().text = too_large.clone();
    assert!(key(&mut s, 'v').is_err());
    assert_eq!(text(&s, nodes[0].id), "seed");
    s.handle(Event::Paste(too_large)).unwrap();
    key(&mut s, 'a').unwrap();
    assert!(key(&mut s, 'c').is_err());
    assert_eq!(shared.borrow().writes, 0);
    s.fields
        .get_mut(&nodes[0].id)
        .unwrap()
        .editor
        .set_selection(0, 0)
        .unwrap();
    assert!(key(&mut s, 'c').unwrap().is_none());
    assert_eq!(shared.borrow().writes, 0);
}
#[test]
fn aggregate_preflight_rejects_cut_before_any_host_write() {
    let (mut s, nodes) = editor_surface(2);
    let shared = provider(&mut s);
    s.focus_node(nodes[0].id).unwrap();
    key(&mut s, 'a').unwrap();
    s.editor_bytes = CellSurface::EDITOR_BYTES;
    assert!(key(&mut s, 'x').is_err());
    assert_eq!(shared.borrow().writes, 0);
    assert_eq!(text(&s, nodes[0].id), "seed");
    assert_eq!(s.fields[&nodes[0].id].editor.selection(), 0..4);
}
#[test]
fn removing_provider_restores_local_clipboard_and_disposal_releases_host_owner() {
    let (mut s, nodes) = editor_surface(2);
    let shared = provider(&mut s);
    s.set_clipboard(None).unwrap();
    assert_eq!(shared.borrow().drops, 1);
    s.focus_node(nodes[0].id).unwrap();
    key(&mut s, 'a').unwrap();
    key(&mut s, 'x').unwrap();
    key(&mut s, 'v').unwrap();
    assert_eq!(text(&s, nodes[0].id), "seed");
    assert_eq!((shared.borrow().reads, shared.borrow().writes), (0, 0));
    s.set_clipboard(Some(Box::new(Provider(shared.clone()))))
        .unwrap();
    s.lifetime.dispose();
    let mut b = Buffer::empty(Rect::new(0, 0, 40, 12));
    s.render(b.area, &mut b);
    assert_eq!(shared.borrow().drops, 2);
    assert!(s
        .set_clipboard(Some(Box::new(Provider(shared.clone()))))
        .is_err());
    assert_eq!(shared.borrow().drops, 3);
    s.set_clipboard(None).unwrap();
    #[cfg(feature = "portable")]
    {
        let mut app = crate::CellApplication::new([1., 1.], jadix::portable::Element::new).unwrap();
        let owner = provider(app.surface_mut());
        app.surface().lifetime.dispose();
        app.dispose();
        assert_eq!(owner.borrow().drops, 1);
    }
}

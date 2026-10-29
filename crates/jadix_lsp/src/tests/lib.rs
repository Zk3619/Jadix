use super::*;
#[test]
fn unicode_framing_and_utf16_edits() {
    let message = json!({"text":"中文😀"});
    let mut bytes = Vec::new();
    write_message(&mut bytes, &message).unwrap();
    assert_eq!(
        read_message(&mut std::io::Cursor::new(bytes)).unwrap(),
        message
    );
    let text = "a😀中\r\nz";
    assert_eq!(position(text, 5), json!({"line":0,"character":3}));
    assert_eq!(byte_at(text, &json!({"line":0,"character":2})), None);
    let edit = json!({"range":{"start":{"line":0,"character":1},"end":{"line":0,"character":3}},"newText":"X"});
    assert_eq!(apply_edits(text, &[edit]).unwrap(), "aX中\r\nz");
}
#[test]
fn rejects_oversized_messages_and_overlapping_edits() {
    assert!(read_message(&mut std::io::Cursor::new(
        b"Content-Length: 20000000\r\n\r\n"
    ))
    .is_err());
    let edit = json!({"range":{"start":{"line":0,"character":0},"end":{"line":0,"character":2}},"newText":"X"});
    assert!(apply_edits("abcd", &[edit.clone(), edit]).is_err());
}

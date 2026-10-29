use super::*;
#[test]
fn html_keeps_full_document_inside_scriptless_frame() {
    let source = "<!doctype html><html><head><style>td{color:red}</style></head><body><table><tr><td>A &amp; B</td></tr></table><script>alert(1)</script></body></html>";
    let page = page(source, true, false);
    assert!(page.contains("sandbox=\"allow-same-origin\""));
    assert!(!page.contains("<script>alert"));
    assert!(page.contains("\\u003cstyle>td{color:red}"));
    assert!(page.contains("documentElement.cloneNode(true)"));
}

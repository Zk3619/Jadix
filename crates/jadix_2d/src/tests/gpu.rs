use super::*;

#[test]
fn vertex_stride_is_32() {
    assert_eq!(std::mem::size_of::<PathVert>(), 32);
    assert_eq!(std::mem::offset_of!(PathVert, color), 16);
}

#[test]
fn globals_stride_is_256() {
    assert_eq!(std::mem::size_of::<PathGlobals>(), 256);
    assert_eq!(std::mem::offset_of!(PathGlobals, stop_n), 48);
    assert_eq!(std::mem::offset_of!(PathGlobals, stop_t01), 64);
}

use super::*;

#[test]
fn shared_path_encoding_keeps_curves_and_arc_endpoints() {
    let commands = decode_path_d("M0 0 Q5 10 10 0 A5 5 0 0 1 20 0 Z");
    assert!(matches!(commands[1], PathCommand::QuadTo { .. }));
    assert!(matches!(commands[2], PathCommand::Arc { .. }));
    assert_eq!(decode_path_d(&encode_path_d(&commands)), commands);
    let flattened = flatten_path(&commands);
    assert!(flattened.len() > commands.len());
    assert!(flattened.iter().all(|command| matches!(
        command,
        PathCommand::MoveTo(_) | PathCommand::LineTo(_) | PathCommand::Close
    )));
    assert_eq!(
        flattened[flattened.len() - 2],
        PathCommand::LineTo((20.0, 0.0))
    );
}

use super::*;

#[test]
fn step_end_holds_then_jumps() {
    let y = eval_steps(0.49, 2, StepJumpTerm::JumpEnd);
    assert!((y - 0.0).abs() < 1e-5);
    let y = eval_steps(0.5, 2, StepJumpTerm::JumpEnd);
    assert!((y - 0.5).abs() < 1e-5);
    let y = eval_steps(1.0, 2, StepJumpTerm::JumpEnd);
    assert!((y - 1.0).abs() < 1e-5);
}

#[test]
fn step_start_jumps_immediately() {
    let y = eval_steps(0.0, 2, StepJumpTerm::JumpStart);
    assert!((y - 0.5).abs() < 1e-5, "got {y}");
    let y = eval_steps(0.49, 2, StepJumpTerm::JumpStart);
    assert!((y - 0.5).abs() < 1e-5);
    let y = eval_steps(0.5, 2, StepJumpTerm::JumpStart);
    assert!((y - 1.0).abs() < 1e-5);
}

#[test]
fn parse_step_keywords() {
    assert!(matches!(
        parse_steps_timing("step-start"),
        Some(TimingFunction::Steps {
            n: 1,
            jump: StepJumpTerm::JumpStart
        })
    ));
    assert!(matches!(
        parse_steps_timing("steps(4, jump-none)"),
        Some(TimingFunction::Steps {
            n: 4,
            jump: StepJumpTerm::JumpNone
        })
    ));
}

#[test]
fn split_ws_keeps_paren_tokens() {
    let t = split_ws_tokens("200ms steps(4, jump-end) both fade-in-0");
    assert_eq!(t, vec!["200ms", "steps(4, jump-end)", "both", "fade-in-0"]);
}

#[test]
fn linear_fn_holds_then_ramps() {
    let tf = parse_linear_timing("linear(0, 0 75%, 1)").unwrap();
    let TimingFunction::LinearStops(id) = tf else {
        panic!("{tf:?}");
    };
    assert!(eval_linear_stops(id, 0.5).abs() < 1e-4);
    let y = eval_linear_stops(id, 0.875);
    assert!((y - 0.5).abs() < 1e-3, "got {y}");
    assert!((eval_linear_stops(id, 1.0) - 1.0).abs() < 1e-4);
}

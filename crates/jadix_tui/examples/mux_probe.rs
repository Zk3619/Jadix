//! Small real KGP/multiplexer peer; uses CPU pixels, no GPU performance claim.
use jadix_tui::{
    crossterm::event::{Event, KeyCode},
    ratatui::layout::Rect,
    KittyTerminal, RgbaFrame, StdinEvents, TerminalModes, TerminalViewport, WireEvent,
};
use std::{
    num::NonZeroU32,
    path::PathBuf,
    time::{Duration, Instant},
};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut depth = 0;
    let mut receipt = None;
    let mut gate = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--depth" => depth = args.next().ok_or("depth required")?.parse()?,
            "--receipt" => receipt = Some(PathBuf::from(args.next().ok_or("receipt required")?)),
            "--gate" => gate = Some(PathBuf::from(args.next().ok_or("gate required")?)),
            _ => return Err("unsupported fixture argument".into()),
        }
    }
    let receipt = receipt.ok_or("receipt required")?;
    let mut modes = TerminalModes::enter_with_output_timeout(Duration::from_secs(2))?;
    let viewport = TerminalViewport::new(Rect::new(0, 0, 16, 8), [128, 128], 1.)?;
    let mut terminal = KittyTerminal::new(
        modes.output(),
        viewport,
        NonZeroU32::new(0x4a4410).unwrap(),
        NonZeroU32::new(0x70000000).unwrap(),
        Duration::from_secs(8),
    )?;
    if depth > 0 {
        terminal.request_tmux_passthrough_depth(depth)?;
    }
    terminal.request_compression()?;
    let mut input = StdinEvents::new()?;
    let mut started = false;
    let mut replies = 0;
    let mut receipt_written = false;
    let result = (|| -> Result<(), Box<dyn std::error::Error>> {
        loop {
            if modes.exit_signal().is_some() {
                break;
            }
            if !started && gate.as_ref().is_none_or(|p| p.exists()) {
                terminal.start(Instant::now())?;
                started = true;
            }
            if started {
                terminal.check(Instant::now())?;
                terminal.present(Instant::now(), |_| {
                    RgbaFrame::new(128, 128, [23, 170, 92, 255].repeat(128 * 128))
                })?;
                if !receipt_written
                    && terminal.ready()
                    && terminal.submitted_frames() > 0
                    && terminal.in_flight() == 0
                {
                    std::fs::write(&receipt,serde_json::json!({"status":"active","depth":depth,"frames":terminal.submitted_frames(),"replies":replies,"fixed_fixture_geometry":true,"automatic_tmux_geometry":automatic_tmux_geometry()}).to_string())?;
                    receipt_written = true;
                }
            }
            if let Some(event) = input.poll(Duration::from_millis(30))? {
                match event {
                    WireEvent::Graphics(reply) => {
                        terminal.acknowledge(&reply, Instant::now())?;
                        replies += 1;
                    }
                    WireEvent::Input(Event::Key(key)) if key.code == KeyCode::Char('q') => break,
                    _ => {}
                }
            }
        }
        Ok(())
    })();
    let _ = modes.begin_output_cleanup();
    let _ = terminal.close();
    let restored = modes.restore();
    if receipt.exists() {
        let mut value: serde_json::Value = serde_json::from_slice(&std::fs::read(&receipt)?)?;
        value["status"] = "closed".into();
        value["restored"] = restored.is_ok().into();
        value["error"] = result.as_ref().err().map(ToString::to_string).into();
        std::fs::write(&receipt, serde_json::to_vec(&value)?)?;
    }
    result?;
    restored?;
    Ok(())
}

fn automatic_tmux_geometry() -> bool {
    #[cfg(unix)]
    {
        TerminalViewport::current_tmux(1.).is_ok()
    }
    #[cfg(not(unix))]
    {
        false
    }
}

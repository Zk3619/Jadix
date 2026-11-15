//! cargo run -p jadix_tui --example workbench -- [--kitty | --export directory]
use jadix_2d::{Canvas2D, Font2D, GlyphRun, PositionedGlyph, RasterCanvas};
use jadix_tui::{
    crossterm::{
        event::{
            self, DisableBracketedPaste, DisableFocusChange, DisableMouseCapture,
            EnableBracketedPaste, EnableFocusChange, EnableMouseCapture, KeyCode, KeyEventKind,
            KeyModifiers, MouseButton, MouseEventKind,
        },
        execute,
    },
    ratatui::{
        self,
        backend::CrosstermBackend,
        layout::{Constraint, Layout, Rect, Size},
        widgets::{Block, Paragraph},
        Frame, Terminal, TerminalOptions, Viewport,
    },
    Capability, KittySurface, KittyView, Picker, RgbaFrame, TerminalInput, TerminalViewport,
};
use std::{error::Error, io, num::NonZeroU32, path::PathBuf};

fn picture(value: u32) -> io::Result<RgbaFrame> {
    let mut canvas = RasterCanvas::new_linear(480, 240);
    canvas.clear("#101a30");
    let font = Font2D::new(
        &include_bytes!("../../jadix_cosmic_text/fonts/Inter-Regular.ttf")[..],
        0,
    )
    .map_err(io::Error::other)?;
    let mut x = 20.0;
    let text = format!("JADIX SELF-DRAW / {value}");
    let glyphs: Vec<_> = text
        .chars()
        .map(|c| {
            let id = font.glyph_id(c).unwrap();
            let glyph = PositionedGlyph { id, x, y: 32.0 };
            x += font.advance(id, 18.0).unwrap();
            glyph
        })
        .collect();
    canvas.set_fill("#e2e8ff");
    canvas.fill_glyph_run(&GlyphRun::new(&font, 18.0, &glyphs).map_err(io::Error::other)?);
    for i in 0..9 {
        let height = 35.0 + ((i * 31 + value * 17) % 130) as f32;
        canvas.set_fill(if i % 2 == 0 { "#38bdf8" } else { "#a78bfa" });
        canvas.fill_rect(20.0 + i as f32 * 50.0, 220.0 - height, 34.0, height);
    }
    RgbaFrame::from_canvas(canvas)
}

fn regions(area: Rect) -> [Rect; 3] {
    Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(1),
        Constraint::Length(2),
    ])
    .areas(area)
}

fn draw(
    frame: &mut Frame,
    state: &Workbench,
    image: Option<&KittyView>,
    braille: Option<&RgbaFrame>,
) {
    let value = state.value;
    let [header, body, footer] = regions(frame.area());
    frame.render_widget(
        Paragraph::new(format!(
            "Shared state: {value}  |  Jadix terminal workbench"
        ))
        .block(Block::bordered()),
        header,
    );
    if let Some(image) = image {
        frame.render_widget(image, body);
    } else if let Some(rgba) = braille {
        frame.render_widget(
            jadix_tui::BrailleImage::new(rgba).background([16, 26, 48]),
            body,
        );
    } else {
        frame.render_widget(Paragraph::new(format!("CHARACTER TUI\nCounter: {value}\nRatatui layout / Unicode: 中文\nUse --kitty for Jadix Canvas pixels.")).block(Block::bordered()), body);
    }
    frame.render_widget(
        Paragraph::new(if state.focused {
            "+/- or wheel/click; paste a number; q/Esc quit"
        } else {
            "Terminal unfocused — retained frame"
        }),
        footer,
    );
}

fn export(directory: PathBuf) -> Result<(), Box<dyn Error>> {
    // Offline evidence only; this does not assert real-terminal support.
    std::fs::create_dir(&directory)?;
    let rgba = picture(3)?;
    let (w, h) = rgba.size();
    image::save_buffer(
        directory.join("jadix-canvas.png"),
        rgba.pixels(),
        w,
        h,
        image::ColorType::Rgba8,
    )?;
    let view = KittyView::new(NonZeroU32::new(0x4a4458).unwrap(), rgba, Size::new(80, 23))?;
    let fallback = picture(3)?;
    for (name, image, braille) in [
        ("characters.ansi", None, None),
        ("kitty.ansi", Some(&view), None),
        ("braille.ansi", None, Some(&fallback)),
    ] {
        let file = std::fs::File::create(directory.join(name))?;
        let mut terminal = Terminal::with_options(
            CrosstermBackend::new(file),
            TerminalOptions {
                viewport: Viewport::Fixed(Rect::new(0, 0, 80, 28)),
            },
        )?;
        terminal.draw(|f| {
            draw(
                f,
                &Workbench {
                    value: 3,
                    focused: true,
                },
                image,
                braille,
            )
        })?;
    }
    println!(
        "Exported offline character/Kitty streams and Jadix Canvas PNG to {}",
        directory.display()
    );
    Ok(())
}

fn main() -> Result<(), Box<dyn Error>> {
    let arguments: Vec<_> = std::env::args().skip(1).collect();
    if arguments.first().map(String::as_str) == Some("--export") {
        return export(
            arguments
                .get(1)
                .ok_or("--export needs a NEW directory")?
                .into(),
        );
    }
    if arguments.iter().any(|a| a != "--kitty" && a != "--braille") || arguments.len() > 1 {
        return Err("usage: workbench [--kitty | --braille | --export NEW_DIRECTORY]".into());
    }
    let graphics = arguments.iter().any(|a| a == "--kitty");
    let braille = arguments.iter().any(|a| a == "--braille");
    let mut fallback = None;
    if graphics && (std::env::var_os("TMUX").is_some() || std::env::var_os("STY").is_some()) {
        return Err("Kitty mode currently requires a direct terminal (no tmux/screen)".into());
    }
    let mut terminal = ratatui::try_init()?;
    let mut surface: Option<KittySurface> = None;
    let result = (|| -> Result<(), Box<dyn Error>> {
        // Probe before the event reader takes ownership of stdin.
        if graphics
            && !Picker::from_query_stdio()?
                .capabilities()
                .contains(&Capability::Kitty)
        {
            return Err(
                "Kitty graphics not confirmed; run without --kitty for character TUI".into(),
            );
        }
        execute!(
            io::stdout(),
            EnableMouseCapture,
            EnableBracketedPaste,
            EnableFocusChange
        )?;
        let mut state = Workbench {
            value: 0,
            focused: true,
        };
        let mut needs_frame = true;
        loop {
            let size = terminal.size()?;
            let body = regions(Rect::new(0, 0, size.width, size.height))[1];
            if body.is_empty() {
                // Drop old ownership, including worker tickets, when no region is visible.
                if let Some(mut old) = surface.take() {
                    old.close(&mut io::stdout())?;
                }
                needs_frame = true;
            } else if graphics {
                let viewport = TerminalViewport::new(body, [480, 240], 1.0)?;
                let current = surface.get_or_insert_with(|| {
                    KittySurface::new(NonZeroU32::new(0x4a4458).unwrap(), viewport)
                });
                if needs_frame || current.viewport() != viewport {
                    let ticket = current.request_frame(viewport)?;
                    if !current.submit(&ticket, picture(state.value)?)? {
                        return Err("fresh frame rejected".into());
                    }
                    current.prepare()?;
                    needs_frame = false;
                }
            }
            if braille && needs_frame {
                fallback = Some(picture(state.value)?);
                needs_frame = false;
            }
            terminal.draw(|f| {
                draw(
                    f,
                    &state,
                    surface.as_ref().and_then(KittySurface::view),
                    fallback.as_ref(),
                )
            })?;
            // Blocking event loop: no timer, idle render or continuous upload.
            loop {
                let event = event::read()?;
                // Character mode uses an explicit cell-space coordinate model.
                let viewport = surface.as_ref().map(KittySurface::viewport).or_else(|| {
                    TerminalViewport::new(
                        body,
                        [u32::from(body.width), u32::from(body.height)],
                        1.0,
                    )
                    .ok()
                });
                let Some(input) = TerminalInput::from_event(event, viewport) else {
                    continue;
                };
                match state.update(input) {
                    Update::None => {}
                    Update::Exit => return Ok(()),
                    Update::Repaint => break,
                    Update::Frame => {
                        needs_frame = true;
                        break;
                    }
                    Update::Resize => {
                        terminal.clear()?;
                        break;
                    }
                }
            }
        }
    })();
    // Attempt every cleanup even if an earlier write fails. Disposing precedes IO.
    let image_cleanup = surface
        .as_mut()
        .map_or(Ok(()), |s| s.close(&mut io::stdout()));
    let mode_cleanup = execute!(
        io::stdout(),
        DisableFocusChange,
        DisableBracketedPaste,
        DisableMouseCapture
    );
    ratatui::restore();
    result?;
    image_cleanup?;
    mode_cleanup?;
    Ok(())
}

struct Workbench {
    value: u32,
    focused: bool,
}
#[derive(Debug, PartialEq)]
enum Update {
    None,
    Repaint,
    Frame,
    Resize,
    Exit,
}
impl Workbench {
    fn update(&mut self, input: TerminalInput) -> Update {
        let old = self.value;
        match input {
            TerminalInput::Resize(_) => return Update::Resize,
            TerminalInput::Focus(focused) => {
                if self.focused == focused {
                    return Update::None;
                }
                self.focused = focused;
                return Update::Repaint;
            }
            TerminalInput::Key(key) if key.kind != KeyEventKind::Release => {
                if key.code == KeyCode::Esc
                    || key.code == KeyCode::Char('q')
                    || (key.code == KeyCode::Char('c')
                        && key.modifiers.contains(KeyModifiers::CONTROL))
                {
                    return Update::Exit;
                }
                match key.code {
                    KeyCode::Char('+') | KeyCode::Char('=') => {
                        self.value = self.value.saturating_add(1).min(1_000_000)
                    }
                    KeyCode::Char('-') => self.value = self.value.saturating_sub(1),
                    _ => {}
                }
            }
            TerminalInput::Pointer {
                kind: MouseEventKind::ScrollUp | MouseEventKind::Down(MouseButton::Left),
                ..
            } => {
                self.value = self.value.saturating_add(1).min(1_000_000);
            }
            TerminalInput::Pointer {
                kind: MouseEventKind::ScrollDown,
                ..
            } => self.value = self.value.saturating_sub(1),
            TerminalInput::Paste(text) => {
                if let Ok(value) = text.trim().parse::<u32>() {
                    if value <= 1_000_000 {
                        self.value = value;
                    }
                }
            }
            _ => {}
        }
        if self.value == old {
            Update::None
        } else {
            Update::Frame
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jadix_tui::crossterm::event::{Event, KeyEvent};
    #[test]
    fn reported_events_update_one_state_and_noops_do_not_request_frames() {
        let mut state = Workbench {
            value: 0,
            focused: true,
        };
        let viewport = TerminalViewport::new(Rect::new(0, 3, 20, 10), [480, 240], 1.0).unwrap();
        let release = KeyEvent::new_with_kind(
            KeyCode::Char('+'),
            KeyModifiers::NONE,
            KeyEventKind::Release,
        );
        for event in [
            Event::Key(release),
            Event::Paste("invalid".into()),
            Event::Paste("1000001".into()),
        ] {
            assert_eq!(state.update(viewport.input(event).unwrap()), Update::None);
        }
        assert_eq!(
            state.update(TerminalInput::Paste(" 42 ".into())),
            Update::Frame
        );
        assert_eq!(state.value, 42);
        assert_eq!(
            state.update(TerminalInput::Paste("42".into())),
            Update::None
        );
        assert_eq!(state.update(TerminalInput::Focus(false)), Update::Repaint);
        assert_eq!(state.update(TerminalInput::Focus(false)), Update::None);
        assert_eq!(
            state.update(TerminalInput::Key(KeyEvent::new(
                KeyCode::Char('c'),
                KeyModifiers::CONTROL
            ))),
            Update::Exit
        );
    }

    #[test]
    fn retained_surface_updates_real_terminal_stream_only_for_new_pixels() {
        #[derive(Clone, Default)]
        struct Output(std::rc::Rc<std::cell::RefCell<Vec<u8>>>);
        impl std::io::Write for Output {
            fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
                self.0.borrow_mut().extend_from_slice(bytes);
                Ok(bytes.len())
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }
        let mut output = Output::default();
        let area = Rect::new(0, 0, 40, 18);
        let body = regions(area)[1];
        let viewport = TerminalViewport::new(body, [480, 240], 1.0).unwrap();
        let mut state = Workbench {
            value: 0,
            focused: true,
        };
        let mut surface = KittySurface::new(NonZeroU32::new(42).unwrap(), viewport);
        let mut terminal = Terminal::with_options(
            CrosstermBackend::new(output.clone()),
            TerminalOptions {
                viewport: Viewport::Fixed(area),
            },
        )
        .unwrap();
        let ticket = surface.request_frame(viewport).unwrap();
        surface
            .submit(&ticket, picture(state.value).unwrap())
            .unwrap();
        surface.prepare().unwrap();
        let now = std::time::Instant::now();
        let mut flow = jadix_tui::KittyFrameFlow::new(
            NonZeroU32::new(1000).unwrap(),
            std::time::Duration::from_secs(1),
        )
        .unwrap();
        let fence = flow
            .submit(now, &mut output, |_| {
                terminal
                    .draw(|f| draw(f, &state, surface.view(), None))
                    .map(|_| ())
            })
            .unwrap();
        let first = output.0.borrow().len();
        assert!(output.0.borrow().windows(3).any(|w| w == b"\x1b_G"));
        assert_eq!(state.update(TerminalInput::Focus(false)), Update::Repaint);
        terminal
            .draw(|f| draw(f, &state, surface.view(), None))
            .unwrap();
        let focused = output.0.borrow().len();
        assert!(!output.0.borrow()[first..]
            .windows(3)
            .any(|w| w == b"\x1b_G"));
        terminal
            .draw(|f| draw(f, &state, surface.view(), None))
            .unwrap();
        assert!(!output.0.borrow()[focused..]
            .windows(3)
            .any(|w| w == b"\x1b_G"));
        assert_eq!(
            state.update(TerminalInput::Paste("3".into())),
            Update::Frame
        );
        let next = surface.request_frame(viewport).unwrap();
        assert!(!surface.submit(&ticket, picture(999).unwrap()).unwrap());
        surface
            .submit(&next, picture(state.value).unwrap())
            .unwrap();
        surface.prepare().unwrap();
        flow.submit(now, &mut output, |_| {
            terminal
                .draw(|f| draw(f, &state, surface.view(), None))
                .map(|_| ())
        })
        .unwrap();
        assert_eq!(flow.in_flight(), 2);
        assert_eq!(
            flow.submit(now, &mut output, |_| panic!("third GPU upload must wait"))
                .unwrap_err()
                .kind(),
            io::ErrorKind::WouldBlock
        );
        let mut replies = jadix_tui::KittyReplyDecoder::default();
        let reply = format!("\x1b_Gi={fence};OK\x1b\\");
        let packets: Vec<_> = reply
            .as_bytes()
            .chunks(2)
            .flat_map(|b| replies.feed(b))
            .collect();
        let [jadix_tui::TerminalPacket::Graphics(reply)] = &packets[..] else {
            panic!("fence reply");
        };
        assert!(flow.acknowledge(reply, now).unwrap());
        assert_eq!(flow.in_flight(), 1);
        assert!(output.0.borrow()[focused..]
            .windows(3)
            .any(|w| w == b"\x1b_G"));
        surface.close(&mut output).unwrap();
        assert!(!surface.submit(&next, picture(999).unwrap()).unwrap());
        assert!(output
            .0
            .borrow()
            .ends_with(b"\x1b_Ga=d,d=I,i=42,q=2;\x1b\\"));
    }
}

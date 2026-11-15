//! A real Jadix Element scene inside a Ratatui terminal; no native window.
use jadix::{
    core::tree::Element,
    primitives::{
        canvas::canvas,
        path::{attach_path, filled_circle, PathStyle},
    },
};
use jadix_tui::{
    crossterm::{
        event::{
            self, DisableFocusChange, DisableMouseCapture, EnableFocusChange, EnableMouseCapture,
            KeyCode, KeyEventKind, KeyModifiers,
        },
        execute,
    },
    ratatui::{
        self,
        backend::CrosstermBackend,
        layout::{Rect, Size},
        widgets::Paragraph,
        Terminal, TerminalOptions, Viewport,
    },
    Capability, JadixScene, KittySurface, Picker, SceneInput, TerminalInput, TerminalViewport,
};
use std::{cell::Cell, error::Error, io, num::NonZeroU32, path::Path, rc::Rc};

fn scene(value: Rc<Cell<u32>>) -> Element {
    let n = value.get();
    let pressed = value.clone();
    let plot: Element = canvas()
        .w(440.0)
        .h(120.0)
        .paint(move |ctx, _| {
            ctx.clear("#14243a");
            for i in 0..12 {
                let h = 12.0 + ((i * 17 + n * 7) % 86) as f32;
                ctx.set_fill(if i % 2 == 0 { "#38bdf8" } else { "#a78bfa" });
                ctx.fill_rect(10.0 + i as f32 * 35.0, 110.0 - h, 24.0, h);
            }
            jadix::core::tree::LayoutStatus::Clean
        })
        .into();
    let circle = attach_path(
        Element::new().w(44.0).h(44.0),
        &filled_circle(22.0, 22.0, 18.0, 32),
        &PathStyle {
            fill: Some([0.2, 0.9, 0.5, 1.0]),
            ..Default::default()
        },
    );
    Element::new().w(480.0).h(320.0).bg("#0b1220").flex_col().p(20.0).gap(10.0)
        .child(Element::new().text("Jadix / real Element scene").text_size(23.0).text_color("#ffffff"))
        .child(Element::new().text(format!("Value {n}   |   glyphs / images / paths / Canvas")).text_size(14.0).text_color("#a8bad4"))
        .child(Element::new().w(440.0).h(44.0).flex_row().gap(14.0)
            .child(Element::new().focus_id("increment").w(220.0).h(44.0).rounded(10.0).bg("#2563eb")
                .p(10.0).text("Click here to increment").text_size(16.0).text_color("#ffffff")
                .on_click(move || pressed.set(pressed.get().saturating_add(1))))
            .child(Element::new().w(44.0).h(44.0).image_texture("element-workbench-image"))
            .child(circle))
        .child(plot)
        .child(Element::new().w(440.0).h(22.0).overflow("hidden")
            .child(Element::new().text("Clipped text stays within the scene. +/- keys and real hit-tested mouse input.")
                .text_size(14.0).text_color("#d4e0f4").whitespace_nowrap()))
}

fn viewport(area: Rect) -> io::Result<TerminalViewport> {
    TerminalViewport::new(area, [480, 320], 1.0)
}

fn export(directory: &Path) -> Result<(), Box<dyn Error>> {
    std::fs::create_dir(directory)?;
    let mut gpu = JadixScene::new()?;
    let value = Rc::new(Cell::new(3));
    let mut root = scene(value.clone());
    let area = Rect::new(0, 0, 80, 25);
    let vp = viewport(area)?;
    let mut surface = KittySurface::new(NonZeroU32::new(0x4a4459).unwrap(), vp);
    let mut input = SceneInput::default();
    for step in 0..3 {
        let frame = gpu.render(&mut root, vp, [-1.0; 2])?;
        image::save_buffer(
            directory.join(format!("element-{step}.png")),
            frame.pixels(),
            480,
            320,
            image::ColorType::Rgba8,
        )?;
        let ticket = surface.request_frame(vp)?;
        assert!(surface.submit(&ticket, frame)?);
        surface.prepare()?;
        let file = std::fs::File::create(directory.join(format!("element-{step}.ansi")))?;
        let mut terminal = Terminal::with_options(
            CrosstermBackend::new(file),
            TerminalOptions {
                viewport: Viewport::Fixed(area),
            },
        )?;
        terminal.draw(|f| f.render_widget(surface.view().unwrap(), area))?;
        if step == 2 {
            break;
        }
        // Replay genuine terminal cell input through coordinate conversion and
        // Jadix hit paths, then rebuild the business view from the callback state.
        for kind in [
            event::MouseEventKind::Down(event::MouseButton::Left),
            event::MouseEventKind::Up(event::MouseButton::Left),
        ] {
            input.dispatch(
                &root,
                &vp.input(event::Event::Mouse(event::MouseEvent {
                    kind,
                    column: 15,
                    row: 8,
                    modifiers: KeyModifiers::NONE,
                }))
                .unwrap(),
            );
        }
        assert_eq!(value.get(), 4 + step, "real Element callback");
        gpu.replace_root(&mut root, scene(value.clone()));
    }
    let adapter = gpu.renderer().adapter_info();
    std::fs::write(
        directory.join("result.txt"),
        format!(
            "adapter={adapter:?}\nrendered_frames={}\nvalue={}\nreal_terminal=false\n",
            gpu.renderer().rendered_frames(),
            value.get()
        ),
    )?;
    surface.close(&mut std::fs::File::create(directory.join("cleanup.ansi"))?)?;
    println!(
        "Real GPU Element frames and replay exported to {}",
        directory.display()
    );
    Ok(())
}

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    jadix::queue_image_rgba8(
        "element-workbench-image",
        2,
        2,
        vec![
            255, 80, 90, 255, 255, 200, 70, 255, 50, 140, 240, 255, 160, 100, 250, 255,
        ],
    );
    if args.len() == 2 && args[0] == "--export" {
        return export(Path::new(&args[1]));
    }
    if !args.is_empty() {
        return Err("usage: element_workbench [--export NEW_DIRECTORY]".into());
    }
    if std::env::var_os("TMUX").is_some() || std::env::var_os("STY").is_some() {
        return Err("Element graphics currently require a direct Kitty-capable terminal".into());
    }
    let mut gpu = JadixScene::new()?;
    let mut terminal = ratatui::try_init()?;
    let mut surface: Option<KittySurface> = None;
    let result = (|| -> Result<(), Box<dyn Error>> {
        if !Picker::from_query_stdio()?
            .capabilities()
            .contains(&Capability::Kitty)
        {
            return Err("Kitty graphics not confirmed; use workbench for character TUI".into());
        }
        execute!(io::stdout(), EnableMouseCapture, EnableFocusChange)?;
        let value = Rc::new(Cell::new(0));
        let mut root = scene(value.clone());
        let mut input = SceneInput::default();
        let mut dirty = true;
        loop {
            let Size { width, height } = terminal.size()?;
            let area = Rect::new(0, 0, width, height.saturating_sub(1));
            if !area.is_empty() {
                let vp = viewport(area)?;
                let surface = surface.get_or_insert_with(|| {
                    KittySurface::new(NonZeroU32::new(0x4a4459).unwrap(), vp)
                });
                if dirty || surface.viewport() != vp {
                    let ticket = surface.request_frame(vp)?;
                    surface.submit(&ticket, gpu.render(&mut root, vp, [-1.0; 2])?)?;
                    surface.prepare()?;
                    dirty = false;
                }
            } else if let Some(mut old) = surface.take() {
                old.close(&mut io::stdout())?;
                dirty = true;
            }
            terminal.draw(|f| {
                if let Some(view) = surface.as_ref().and_then(KittySurface::view) {
                    f.render_widget(view, area);
                }
                f.render_widget(
                    Paragraph::new(
                        "+/- or click blue button; q quit | retained real Jadix GPU scene",
                    ),
                    Rect::new(0, height.saturating_sub(1), width, u16::from(height > 0)),
                );
            })?;
            loop {
                let Some(event) = TerminalInput::from_event(
                    event::read()?,
                    surface.as_ref().map(KittySurface::viewport),
                ) else {
                    continue;
                };
                let before = value.get();
                input.dispatch(&root, &event);
                match event {
                    TerminalInput::Key(k) if k.kind != KeyEventKind::Release => match k.code {
                        KeyCode::Esc | KeyCode::Char('q') => return Ok(()),
                        KeyCode::Char('c') if k.modifiers.contains(KeyModifiers::CONTROL) => {
                            return Ok(())
                        }
                        KeyCode::Char('+') | KeyCode::Char('=') => {
                            value.set(before.saturating_add(1))
                        }
                        KeyCode::Char('-') => value.set(before.saturating_sub(1)),
                        _ => {}
                    },
                    TerminalInput::Resize(_) => {
                        terminal.clear()?;
                        break;
                    }
                    _ => {}
                }
                if before != value.get() {
                    gpu.replace_root(&mut root, scene(value.clone()));
                    dirty = true;
                    break;
                }
            }
        }
    })();
    let cleanup = surface
        .as_mut()
        .map_or(Ok(()), |s| s.close(&mut io::stdout()));
    let modes = execute!(io::stdout(), DisableFocusChange, DisableMouseCapture);
    ratatui::restore();
    result?;
    cleanup?;
    modes?;
    Ok(())
}

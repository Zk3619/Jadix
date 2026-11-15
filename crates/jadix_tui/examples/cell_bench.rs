//! Reproducible retained/virtual character workloads; no terminal scanout claim.
//! The dimensions/cadence follow nsh's official test, but the application/layout
//! pipelines differ. These are engineering baselines, not an equivalence proof.
use jadix_core::{
    presentation::{
        style::NodeStyle, NodeContent, NodeDescriptor, NodeId, PresentationNode, Rect as HostRect,
        RootId, RootLifetime,
    },
    virtualization::{VirtualAxis, VirtualGrid},
};
use jadix_tui::{
    crossterm::event::{Event, KeyModifiers, MouseEvent, MouseEventKind},
    ratatui::{
        backend::{Backend, ClearType, CrosstermBackend, TestBackend, WindowSize},
        buffer::Cell,
        layout::{Position, Rect, Size},
        Terminal,
    },
    CellColors, CellSurface, FrameMeter, MeasuredBackend, MeasuredWriter,
};
use std::{
    cell::RefCell,
    io::{self, Write},
    rc::Rc,
    time::{Duration, Instant},
};

#[derive(Clone, Default)]
struct Capture(Rc<RefCell<Vec<u8>>>);
impl Write for Capture {
    fn write(&mut self, b: &[u8]) -> io::Result<usize> {
        self.0.borrow_mut().extend_from_slice(b);
        Ok(b.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
// Send the exact same diff to a memory screen and a real ANSI encoder, as in
// nsh-tui-host's memory benchmark. Neither side incurs a PTY or network write.
struct Memory {
    screen: TestBackend,
    ansi: CrosstermBackend<MeasuredWriter<jadix_tui::AnsiColorWriter<Capture>>>,
}
impl Backend for Memory {
    type Error = io::Error;
    fn draw<'a, I>(&mut self, content: I) -> io::Result<()>
    where
        I: Iterator<Item = (u16, u16, &'a Cell)>,
    {
        let cells: Vec<_> = content.collect();
        self.screen.draw(cells.iter().copied()).unwrap();
        self.ansi.draw(cells.into_iter())
    }
    fn hide_cursor(&mut self) -> io::Result<()> {
        self.ansi.hide_cursor()
    }
    fn show_cursor(&mut self) -> io::Result<()> {
        self.ansi.show_cursor()
    }
    fn get_cursor_position(&mut self) -> io::Result<Position> {
        Ok(self.screen.get_cursor_position().unwrap())
    }
    fn set_cursor_position<P: Into<Position>>(&mut self, p: P) -> io::Result<()> {
        self.ansi.set_cursor_position(p)
    }
    fn clear(&mut self) -> io::Result<()> {
        self.screen.clear().unwrap();
        self.ansi.clear()
    }
    fn clear_region(&mut self, r: ClearType) -> io::Result<()> {
        self.screen.clear_region(r).unwrap();
        self.ansi.clear_region(r)
    }
    fn size(&self) -> io::Result<Size> {
        Ok(self.screen.size().unwrap())
    }
    fn window_size(&mut self) -> io::Result<WindowSize> {
        Ok(self.screen.window_size().unwrap())
    }
    fn flush(&mut self) -> io::Result<()> {
        Backend::flush(&mut self.ansi)
    }
}
type Screen = Terminal<MeasuredBackend<Memory>>;
fn text(id: NodeId, value: String, frame: [f64; 4]) -> PresentationNode {
    node(id, NodeContent::Text(value), frame)
}
fn node(id: NodeId, content: NodeContent, frame: [f64; 4]) -> PresentationNode {
    PresentationNode {
        id,
        descriptor: NodeDescriptor {
            content,
            frame: HostRect {
                x: frame[0],
                y: frame[1],
                width: frame[2],
                height: frame[3],
            },
            style: NodeStyle {
                foreground: [230, 232, 238, 255],
                ..Default::default()
            },
            accessible_description: None,
            accessible_name: None,
            accessible_role: None,
            accessible_selected: None,
            accessible_expanded: None,
            accessibility_relations: Default::default(),
        },
        children: vec![],
    }
}
fn frame(
    view: &mut CellSurface,
    screen: &mut Screen,
    meter: &FrameMeter,
    capture: &Capture,
) -> io::Result<()> {
    capture.0.borrow_mut().clear();
    meter.measure(|| screen.draw(|f| f.render_widget(view, Rect::new(0, 0, 200, 60))))?;
    assert_eq!(
        meter.latest().unwrap().bytes,
        capture.0.borrow().len() as u64
    );
    Ok(())
}
fn run(table: bool, colors: CellColors, auto: bool) -> io::Result<serde_json::Value> {
    let root = RootId::new();
    let mut view = CellSurface::new(RootLifetime::new(root), [8., 16.])?;
    view.set_colors(colors);
    let ids: Vec<_> = (0..if table {
        240024
    } else if auto {
        2503
    } else {
        2002
    })
        .map(|_| NodeId::new(root))
        .collect();
    let meter = FrameMeter::default();
    let capture = Capture::default();
    let mut screen = Terminal::new(meter.backend(Memory {
        screen: TestBackend::new(200, 60),
        ansi: CrosstermBackend::new(
            meter.writer(jadix_tui::AnsiColorWriter::new(capture.clone(), colors)),
        ),
    }))?;
    let words = [
        "alpha", "bravo", "charlie", "delta", "echo", "foxtrot", "golf", "hotel", "india",
        "juliet", "kilo", "lima",
    ];
    let data: Vec<_> = (0..if table { 10000 } else { 500 })
        .map(|r| {
            if table {
                (0..24)
                    .map(|c| format!("R{} C{}", r + 1, c + 1))
                    .collect::<Vec<_>>()
            } else {
                vec![
                    format!("{:>6}", r + 1),
                    " ".into(),
                    format!("{} {}", words[r % 12], words[(r * 7) % 12]),
                    format!(
                        "{:>11} ",
                        format!("{:.1} KB", ((r * 7919) % 9000 + 120) as f64 / 100.)
                    ),
                ]
            }
        })
        .collect();
    let grid = VirtualGrid {
        rows: VirtualAxis::new(0..10000usize, 16.).unwrap(),
        columns: VirtualAxis::new(0..24usize, 128.).unwrap(),
    };
    let table_tree = |offset: f64| {
        let window = grid
            .window([0., offset], [1600., 944.], [0., 16.], &[], &[], 2000)
            .unwrap();
        let mut result = Vec::new();
        for row in &window[1].mounted {
            for column in &window[0].mounted {
                result.push(text(
                    ids[row * 24 + column],
                    data[*row][*column].clone(),
                    [
                        *column as f64 * 128.,
                        16. + *row as f64 * 16. - window[1].offset,
                        128.,
                        16.,
                    ],
                ));
            }
        }
        for column in &window[0].mounted {
            let mut header = text(
                ids[240000 + *column],
                format!("Column {}", column + 1),
                [*column as f64 * 128., 0., 128., 16.],
            );
            header.descriptor.style.background = [20, 20, 20, 255];
            result.push(header);
        }
        result
    };
    if table {
        view.present(0, table_tree(0.))?;
    } else {
        let mut scroll = node(ids[0], NodeContent::Scroll, [0., 0., 1600., 960.]);
        scroll.children.push(text(
            ids[1],
            "500 rows in one scroll view: wheel, ↑ ↓, PgUp / PgDn. Click a row to mark it · 0 marked".into(),
            [8., 16., 1584., 16.],
        ));
        for (row, columns) in data.iter().enumerate() {
            for (column, (x, width)) in [(8., 48.), (72., 8.), (96., 1384.), (1496., 96.)]
                .iter()
                .enumerate()
            {
                let mut cell = text(
                    ids[2 + row * 4 + column],
                    columns[column].clone(),
                    [*x, 48. + row as f64 * 16., *width, 16.],
                );
                if column != 2 {
                    cell.descriptor.style.foreground = [95, 102, 118, 255];
                }
                scroll.children.push(cell);
            }
        }
        if auto {
            use jadix_core::presentation::style::Axis;
            scroll.descriptor.style.width = Some(1600.);
            scroll.descriptor.style.height = Some(960.);
            scroll.descriptor.style.padding = [16., 8., 0., 8.];
            let leaves = std::mem::take(&mut scroll.children);
            let mut leaves = leaves.into_iter();
            let mut title = leaves.next().unwrap();
            title.descriptor.style.height = Some(16.);
            title.descriptor.style.shrink = 0.;
            scroll.children.push(title);
            let mut spacer = node(ids[2502], NodeContent::Container, [0.; 4]);
            spacer.descriptor.style.height = Some(16.);
            spacer.descriptor.style.shrink = 0.;
            scroll.children.push(spacer);
            for row in 0..500 {
                let mut line = node(ids[2002 + row], NodeContent::Container, [0.; 4]);
                line.descriptor.style.axis = Axis::Row;
                line.descriptor.style.height = Some(16.);
                line.descriptor.style.shrink = 0.;
                line.descriptor.style.gap = 16.;
                for width in [48., 8., 1384., 96.] {
                    let mut leaf = leaves.next().unwrap();
                    leaf.descriptor.style.width = Some(width);
                    leaf.descriptor.style.height = Some(16.);
                    leaf.descriptor.style.shrink = 0.;
                    line.children.push(leaf);
                }
                scroll.children.push(line);
            }
            view.present_auto(0, vec![scroll], Rect::new(0, 0, 200, 60), 0)?;
        } else {
            view.present(0, vec![scroll])?;
        }
    }
    frame(&mut view, &mut screen, &meter, &capture)?;
    let mut samples = Vec::new();
    let mut apply = Vec::new();
    let mut offset = 0f64;
    let mut max_nodes = 0;
    for index in 0..260 {
        let start = Instant::now();
        let down = (index / 100) % 2 == 0;
        if table {
            offset = (offset + if down { 48. } else { -48. }).clamp(0., 160000. - 944.);
            let nodes = table_tree(offset);
            max_nodes = max_nodes.max(nodes.len());
            view.present(view.revision(), nodes)?;
        } else {
            assert!(view
                .handle(Event::Mouse(MouseEvent {
                    kind: if down {
                        MouseEventKind::ScrollDown
                    } else {
                        MouseEventKind::ScrollUp
                    },
                    column: 100,
                    row: 30,
                    modifiers: KeyModifiers::NONE
                }))?
                .is_some());
        }
        let cost = start.elapsed();
        frame(&mut view, &mut screen, &meter, &capture)?;
        if index >= 20 {
            apply.push(cost.as_nanos() as u64);
            samples.push(meter.latest().unwrap());
        }
        std::thread::sleep(Duration::from_millis(16));
    }
    let contents: String = screen
        .backend()
        .inner()
        .screen
        .buffer()
        .content
        .iter()
        .map(|c| c.symbol())
        .collect();
    assert!(!contents.contains(if table {
        "R1 C1"
    } else {
        "500 rows in one scroll view"
    }));
    assert!(!view.needs_frame());
    let mut costs: Vec<_> = samples
        .iter()
        .zip(&apply)
        .map(|(s, a)| s.elapsed.as_nanos() as u64 + a)
        .collect();
    costs.sort_unstable();
    let result = serde_json::json!({"workload":if table{"DataTable"}else{"LongList"},"cells":[200,60],"warmup":20,"frames":240,"cadence_ms":16,"application":if auto && !table {"shared Taffy initial layout, retained scroll; no JS"}else{"shared PresentationNode with explicit frames; no JS/Taffy"}, "scope":"apply/input + paint + memory backend + ANSI diff/flush; initial layout excluded", "median_ns":costs[119],"p95_ns":costs[227],"apply_mean_ns":apply.iter().sum::<u64>()/240,"frame_mean_ns":samples.iter().map(|s|s.elapsed.as_nanos() as u64).sum::<u64>()/240,"bytes_per_frame":samples.iter().map(|s|s.bytes).sum::<u64>() as f64/240.,"diff_cells_per_frame":samples.iter().map(|s|s.diff_cells).sum::<u64>() as f64/240.,"max_mounted_nodes":if table{max_nodes}else if auto{2503}else{2002},"idle_needs_frame":view.needs_frame()});
    Ok(result)
}
fn main() -> io::Result<()> {
    let args: Vec<_> = std::env::args().collect();
    let colors = args
        .iter()
        .position(|s| s == "--colors")
        .map(|index| {
            args.get(index + 1)
                .ok_or_else(|| io::Error::other("missing colors"))?
                .parse::<CellColors>()
        })
        .transpose()?
        .unwrap_or_default();
    let auto = args.iter().any(|a| a == "--auto-layout");
    let results = [run(false, colors, auto)?, run(true, colors, auto)?];
    println!(
        "{}",
        serde_json::json!({"profile":if cfg!(debug_assertions){"dev"}else{"release"},"colors":format!("{colors:?}"),"results":results})
    );
    Ok(())
}

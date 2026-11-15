//! Bounded UI-thread telemetry for backend diffs and accepted writes.
use ratatui::{
    backend::{Backend, ClearType, WindowSize},
    buffer::Cell,
    layout::{Position, Size},
};
use std::{
    cell::RefCell,
    collections::VecDeque,
    io::{self, Write},
    rc::Rc,
    time::{Duration, Instant},
};

#[derive(Clone, Copy, Debug, Default)]
pub struct FrameSample {
    /// Caller scope; wrap draw/diff/flush together to measure a complete frame.
    pub elapsed: Duration,
    pub bytes: u64,
    pub diff_cells: u64,
    pub success: bool,
}
#[derive(Default)]
struct State {
    bytes: u64,
    cells: u64,
    samples: VecDeque<FrameSample>,
}
/// No timer or invalidation: recording cannot wake an idle application.
/// Wrap the final writer exactly once, then the backend, with the same meter.
#[derive(Clone, Default)]
pub struct FrameMeter(Rc<RefCell<State>>);
impl FrameMeter {
    pub fn writer<W: Write>(&self, inner: W) -> MeasuredWriter<W> {
        MeasuredWriter {
            inner,
            meter: self.clone(),
        }
    }
    pub fn backend<B: Backend>(&self, inner: B) -> MeasuredBackend<B> {
        MeasuredBackend {
            inner,
            meter: self.clone(),
        }
    }
    pub fn totals(&self) -> (u64, u64) {
        let s = self.0.borrow();
        (s.bytes, s.cells)
    }
    pub fn samples(&self) -> Vec<FrameSample> {
        self.0.borrow().samples.iter().copied().collect()
    }
    pub fn latest(&self) -> Option<FrameSample> {
        self.0.borrow().samples.back().copied()
    }
    /// Failed frames retain elapsed time and successfully written bytes.
    pub fn measure<T, E>(&self, operation: impl FnOnce() -> Result<T, E>) -> Result<T, E> {
        let before = self.totals();
        let start = Instant::now();
        let result = operation();
        let elapsed = start.elapsed();
        let after = self.totals();
        let sample = FrameSample {
            elapsed,
            bytes: after.0.saturating_sub(before.0),
            diff_cells: after.1.saturating_sub(before.1),
            success: result.is_ok(),
        };
        let mut state = self.0.borrow_mut();
        if state.samples.len() == 120 {
            state.samples.pop_front();
        }
        state.samples.push_back(sample);
        result
    }
}
pub struct MeasuredWriter<W> {
    inner: W,
    meter: FrameMeter,
}
impl<W> MeasuredWriter<W> {
    pub fn inner(&self) -> &W {
        &self.inner
    }
    pub fn into_inner(self) -> W {
        self.inner
    }
}
impl<W: Write> Write for MeasuredWriter<W> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let n = self.inner.write(bytes)?;
        let mut state = self.meter.0.borrow_mut();
        state.bytes = state.bytes.saturating_add(n as u64);
        Ok(n)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}
pub struct MeasuredBackend<B> {
    inner: B,
    meter: FrameMeter,
}
impl<B> MeasuredBackend<B> {
    pub fn inner(&self) -> &B {
        &self.inner
    }
    pub fn inner_mut(&mut self) -> &mut B {
        &mut self.inner
    }
    pub fn into_inner(self) -> B {
        self.inner
    }
}
impl<B: Backend> Backend for MeasuredBackend<B> {
    type Error = B::Error;
    fn draw<'a, I>(&mut self, content: I) -> Result<(), Self::Error>
    where
        I: Iterator<Item = (u16, u16, &'a Cell)>,
    {
        let mut count = 0;
        self.inner.draw(content.inspect(|_| count += 1))?;
        let mut state = self.meter.0.borrow_mut();
        state.cells = state.cells.saturating_add(count);
        Ok(())
    }
    fn append_lines(&mut self, n: u16) -> Result<(), Self::Error> {
        self.inner.append_lines(n)
    }
    fn hide_cursor(&mut self) -> Result<(), Self::Error> {
        self.inner.hide_cursor()
    }
    fn show_cursor(&mut self) -> Result<(), Self::Error> {
        self.inner.show_cursor()
    }
    fn get_cursor_position(&mut self) -> Result<Position, Self::Error> {
        self.inner.get_cursor_position()
    }
    fn set_cursor_position<P: Into<Position>>(&mut self, p: P) -> Result<(), Self::Error> {
        self.inner.set_cursor_position(p)
    }
    fn clear(&mut self) -> Result<(), Self::Error> {
        self.inner.clear()
    }
    fn clear_region(&mut self, region: ClearType) -> Result<(), Self::Error> {
        self.inner.clear_region(region)
    }
    fn size(&self) -> Result<Size, Self::Error> {
        self.inner.size()
    }
    fn window_size(&mut self) -> Result<WindowSize, Self::Error> {
        self.inner.window_size()
    }
    fn flush(&mut self) -> Result<(), Self::Error> {
        self.inner.flush()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn partial_writes_failed_frames_and_history_are_accounted_without_a_timer() {
        struct Partial(usize);
        impl Write for Partial {
            fn write(&mut self, b: &[u8]) -> io::Result<usize> {
                if self.0 == 0 {
                    return Err(io::ErrorKind::BrokenPipe.into());
                }
                let n = b.len().min(self.0);
                self.0 -= n;
                Ok(n)
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }
        let meter = FrameMeter::default();
        let mut writer = meter.writer(Partial(3));
        assert!(meter.measure(|| writer.write_all(b"abcdef")).is_err());
        assert_eq!(meter.samples()[0].bytes, 3);
        assert!(!meter.samples()[0].success);
        let mut backend = meter.backend(ratatui::backend::TestBackend::new(2, 2));
        meter
            .measure(|| backend.draw([(0, 0, &Cell::default())].into_iter()))
            .unwrap();
        assert_eq!(meter.samples()[1].diff_cells, 1);
        for _ in 0..130 {
            meter.measure(|| Ok::<_, io::Error>(())).unwrap();
        }
        assert_eq!(meter.samples().len(), 120);
        assert_eq!(meter.totals(), (3, 1));
    }
}

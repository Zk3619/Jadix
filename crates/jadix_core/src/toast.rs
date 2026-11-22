//! Bounded announcements with one shared timer and independent hover/focus holds.
use std::{cell::{Cell, RefCell}, rc::Rc};
use web_time::{Duration, Instant};
use crate::{clock::{self, Timer}, reactivity::{create_signal, Signal}};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Announcement { #[default] Polite, Assertive }
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Toast { pub id: u64, pub message: String, pub announcement: Announcement }
#[derive(Clone, Copy, Debug)]
pub struct ToastOptions { pub duration: Option<Duration>, pub announcement: Announcement }
impl Default for ToastOptions {
    fn default() -> Self { Self { duration: Some(Duration::from_secs(5)), announcement: Announcement::Polite } }
}
#[derive(Clone, Copy)]
pub enum ToastHold { Hover, Focus }
struct Entry { toast: Toast, remaining: Option<Duration>, deadline: Option<Instant>, holds: u8 }
struct Inner { entries: RefCell<Vec<Entry>>, view: Signal<Vec<Toast>>, timer: Timer, next_id: Cell<u64> }
#[derive(Clone)]
pub struct ToastQueue(Rc<Inner>);
impl ToastQueue {
    pub fn new() -> Result<Self, &'static str> {
        let timer = Timer::new()?;
        Ok(Self(Rc::new(Inner { entries: RefCell::new(Vec::new()), view: create_signal(Vec::new()), timer, next_id: Cell::new(0) })))
    }
    /// Reading subscribes the caller to insertions, expiration and dismissal.
    pub fn items(&self) -> Vec<Toast> { self.0.view.get() }
    pub fn push(&self, message: impl Into<String>, options: ToastOptions) -> Result<u64, &'static str> {
        if !self.0.view.is_alive() { return Err("toast owner disposed"); }
        let message = message.into();
        if message.trim().is_empty() || message.len() > 16384 || message.contains('\0') { return Err("invalid announcement"); }
        if options.duration.is_some_and(|duration| duration > Duration::from_secs(86400)) { return Err("toast duration exceeds one day"); }
        let deadline = options.duration.map(|duration| clock::now().checked_add(duration).ok_or("toast deadline overflow")).transpose()?;
        let id = self.0.next_id.get().checked_add(1).ok_or("toast identity exhausted")?;
        {
            let mut entries = self.0.entries.borrow_mut();
            if entries.len() >= 64 { return Err("toast budget exceeded"); }
            entries.push(Entry { toast: Toast { id, message, announcement: options.announcement }, remaining: options.duration, deadline, holds: 0 });
        }
        self.0.next_id.set(id); self.publish(); Ok(id)
    }
    pub fn dismiss(&self, id: u64) {
        if !self.0.view.is_alive() { return; }
        self.0.entries.borrow_mut().retain(|entry| entry.toast.id != id); self.publish();
    }
    pub fn clear(&self) {
        if !self.0.view.is_alive() { return; }
        self.0.entries.borrow_mut().clear(); self.publish();
    }
    /// A focus hold survives hover leaving, and vice versa. Time spent held
    /// never consumes the remaining display duration.
    pub fn hold(&self, id: u64, reason: ToastHold, active: bool) {
        if !self.0.view.is_alive() { return; }
        let bit = match reason { ToastHold::Hover => 1, ToastHold::Focus => 2 };
        {
            let mut entries = self.0.entries.borrow_mut();
            let Some(entry) = entries.iter_mut().find(|entry| entry.toast.id == id) else { return; };
            let previous = entry.holds;
            if active { entry.holds |= bit; } else { entry.holds &= !bit; }
            if entry.holds == previous { return; }
            if previous == 0 {
                entry.remaining = entry.deadline.take().map(|at| at.saturating_duration_since(clock::now()));
            } else if entry.holds == 0 {
                entry.deadline = entry.remaining.and_then(|delay| clock::now().checked_add(delay));
            }
        }
        self.publish();
    }
    fn publish(&self) {
        let sample = clock::now();
        let (view, deadline) = {
            let mut entries = self.0.entries.borrow_mut();
            entries.retain(|entry| entry.deadline.is_none_or(|at| at > sample));
            (entries.iter().map(|entry| entry.toast.clone()).collect::<Vec<_>>(), entries.iter().filter_map(|entry| entry.deadline).min())
        };
        self.0.timer.cancel();
        if let Some(at) = deadline {
            let weak = Rc::downgrade(&self.0);
            let _ = self.0.timer.arm(at.saturating_duration_since(sample), move || {
                if let Some(inner) = weak.upgrade() { ToastQueue(inner).publish(); }
            });
        }
        // Effects may synchronously insert/dismiss; all queue borrows are released
        // and the timer is armed before publishing so reentrant work wins.
        if self.0.view.peek() != view { self.0.view.set(view); }
    }
}

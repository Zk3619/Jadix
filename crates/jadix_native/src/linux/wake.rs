//! A coalesced GLib file-descriptor source: no idle polling, including worker
//! completions. A datagram already pending represents all outstanding work.
use super::*;
use std::{
    io,
    os::unix::net::UnixDatagram,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

#[derive(Clone)]
pub struct WakeHandle(Arc<Signal>);
struct Signal {
    socket: UnixDatagram,
    pending: AtomicBool,
}
impl WakeHandle {
    pub fn wake(&self) {
        if !self.0.pending.swap(true, Ordering::AcqRel) {
            if self.0.socket.send(&[1]).is_err() {
                self.0.pending.store(false, Ordering::Release);
            }
        }
    }
}
type Handler = Rc<RefCell<Option<Rc<dyn Fn()>>>>;
pub(super) struct Wake {
    pub handle: WakeHandle,
    handler: Handler,
    source: Option<glib::SourceId>,
    timer: Rc<RefCell<Option<glib::SourceId>>>,
}
impl Wake {
    pub fn new() -> Result<Self, String> {
        let (reader, writer) = UnixDatagram::pair().map_err(|e| e.to_string())?;
        reader.set_nonblocking(true).map_err(|e| e.to_string())?;
        writer.set_nonblocking(true).map_err(|e| e.to_string())?;
        let handle = WakeHandle(Arc::new(Signal {
            socket: writer,
            pending: AtomicBool::new(false),
        }));
        let signal = handle.clone();
        let handler: Handler = Default::default();
        let callback = handler.clone();
        let socket = gio::Socket::from_fd(reader.try_clone().map_err(|e| e.to_string())?.into())
            .map_err(|e| e.to_string())?;
        let source = gio::prelude::SocketExtManual::create_source(
            &socket,
            glib::IOCondition::IN,
            gio::Cancellable::NONE,
            Some("jadix-native-wake"),
            glib::Priority::DEFAULT,
            move |_, _| {
                let mut byte = [0];
                loop {
                    match reader.recv(&mut byte) {
                        Ok(_) => (),
                        Err(error) if error.kind() == io::ErrorKind::WouldBlock => break,
                        Err(_) => return glib::ControlFlow::Break,
                    }
                }
                signal.0.pending.store(false, Ordering::Release);
                let callback = callback.borrow().clone();
                if let Some(callback) = callback {
                    callback();
                }
                glib::ControlFlow::Continue
            },
        );
        let source = source.attach(None);
        Ok(Self {
            handle,
            handler,
            source: Some(source),
            timer: Default::default(),
        })
    }
    pub fn bind(&self, callback: impl Fn() + 'static) {
        *self.handler.borrow_mut() = Some(Rc::new(callback));
        self.handle.wake();
    }
    pub fn schedule(&self, delay: Option<std::time::Duration>) {
        if let Some(timer) = self.timer.borrow_mut().take() {
            timer.remove();
        }
        if let Some(delay) = delay {
            if delay.is_zero() {
                self.handle.wake();
                return;
            }
            let signal = self.handle.clone();
            let slot = self.timer.clone();
            let timer = glib::timeout_add_local_once(delay, move || {
                slot.borrow_mut().take();
                signal.wake();
            });
            *self.timer.borrow_mut() = Some(timer);
        }
    }
    pub fn dispose(&mut self) {
        self.handler.borrow_mut().take();
        if let Some(timer) = self.timer.borrow_mut().take() {
            timer.remove();
        }
        if let Some(source) = self.source.take() {
            source.remove();
        }
    }
}
impl Drop for Wake {
    fn drop(&mut self) {
        self.dispose();
    }
}

mod allocation {
    use super::*;
    use gtk::subclass::prelude::*;
    #[derive(Default)]
    pub struct Content {
        pub wake: RefCell<Option<WakeHandle>>,
        size: Cell<[i32; 2]>,
    }
    #[glib::object_subclass]
    impl ObjectSubclass for Content {
        const NAME: &'static str = "JadixNativeContent";
        type Type = super::Content;
        type ParentType = gtk::Fixed;
    }
    impl ObjectImpl for Content {}
    impl WidgetImpl for Content {
        fn size_allocate(&self, width: i32, height: i32, baseline: i32) {
            self.parent_size_allocate(width, height, baseline);
            if self.size.replace([width, height]) != [width, height] {
                if let Some(wake) = self.wake.borrow().as_ref() {
                    wake.wake();
                }
            }
        }
    }
    impl FixedImpl for Content {}
}
glib::wrapper! {
    pub struct Content(ObjectSubclass<allocation::Content>) @extends gtk::Fixed, gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}
impl Content {
    pub fn new(wake: WakeHandle) -> gtk::Fixed {
        use gtk::subclass::prelude::ObjectSubclassIsExt;
        let content: Self = glib::Object::new();
        *content.imp().wake.borrow_mut() = Some(wake);
        content.upcast()
    }
}

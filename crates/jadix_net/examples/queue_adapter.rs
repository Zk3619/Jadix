use jadix_net::{HttpBackend, HttpCommand, HttpError, HttpErrorKind, HttpEvent};
use std::sync::mpsc::{self, Receiver, Sender};

struct QueueAdapter {
    commands: Sender<HttpCommand>,
    events: Receiver<HttpEvent>,
    closed: bool,
}

impl HttpBackend for QueueAdapter {
    fn dispatch(&mut self, command: HttpCommand) -> Result<(), HttpError> {
        command.validate()?;
        if self.closed {
            return if matches!(command, HttpCommand::Send { .. }) {
                Err(HttpError::new(
                    HttpErrorKind::Disconnected,
                    "adapter is shut down",
                ))
            } else {
                Ok(())
            };
        }
        let shutdown = matches!(command, HttpCommand::Shutdown);
        self.commands.send(command).map_err(|_| {
            HttpError::new(HttpErrorKind::Disconnected, "adapter worker disconnected")
        })?;
        self.closed = shutdown;
        Ok(())
    }

    fn poll_event(&mut self) -> Option<HttpEvent> {
        self.events.try_recv().ok()
    }
}

fn main() -> Result<(), HttpError> {
    let (commands, worker_commands) = mpsc::channel();
    let (_worker_events, events) = mpsc::channel();
    let mut adapter = QueueAdapter {
        commands,
        events,
        closed: false,
    };
    adapter.dispatch(HttpCommand::Shutdown)?;
    assert_eq!(worker_commands.try_recv().unwrap(), HttpCommand::Shutdown);
    assert!(adapter.poll_event().is_none());
    adapter.dispatch(HttpCommand::Shutdown)?;
    assert!(adapter
        .dispatch(HttpCommand::Send {
            request_id: 1,
            request: jadix_net::HttpRequest::get("https://example.test")
        })
        .is_err());
    Ok(())
}

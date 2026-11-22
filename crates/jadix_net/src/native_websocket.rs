//! Native ws/wss transport with system TLS trust, bounded queues and owned cancellation.
use crate::websocket::{
    Command, SocketEvent, SocketMessage, SocketRequest, SocketService, MAX_BYTES, MAX_CONNECTIONS,
};
use futures_util::{SinkExt, StreamExt};
use std::{
    collections::BTreeMap,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread::JoinHandle,
    time::Duration,
};
use tokio::sync::{mpsc, watch, Notify};
use tokio_tungstenite::tungstenite::{
    client::IntoClientRequest,
    protocol::{CloseFrame, WebSocketConfig},
    Message,
};
pub struct NativeWebSocket {
    service: SocketService,
    worker: Option<JoinHandle<()>>,
}
struct Connection {
    sender: mpsc::Sender<Command>,
    cancel: watch::Sender<bool>,
    cancelled: Arc<AtomicBool>,
}
impl NativeWebSocket {
    pub fn new(wake: impl Fn() + Send + Sync + 'static) -> Result<Self, String> {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .max_blocking_threads(MAX_CONNECTIONS)
            .enable_all()
            .build()
            .map_err(|e| e.to_string())?;
        let service = SocketService::new();
        let worker_service = service.clone();
        let notify = Arc::new(Notify::new());
        let command_notify = notify.clone();
        service.set_command_waker(Some(Arc::new(move || command_notify.notify_one())));
        let wake = Arc::new(wake);
        let worker=std::thread::Builder::new().name("jadix-websocket".into()).spawn(move||{
            runtime.block_on(async move{
                let mut active:BTreeMap<u32,Connection>=BTreeMap::new();let mut jobs=tokio::task::JoinSet::new();
                loop{
                    for connection in active.values(){if connection.cancelled.load(Ordering::Acquire){let _=connection.cancel.send(true);}}
                    if worker_service.disposed(){for c in active.values(){let _=c.cancel.send(true);}break;}
                    for command in worker_service.take_commands(){match command{
                        Command::Connect{id,request,cancelled}=>{
                            if cancelled.load(Ordering::Acquire){continue;}
                            if active.len()>=MAX_CONNECTIONS{worker_service.event(id,SocketEvent::Failed{message:"WebSocket worker capacity reached".into()});wake();continue;}
                            let (sender,receiver)=mpsc::channel(9);let (cancel,stop)=watch::channel(false);
                            active.insert(id,Connection{sender,cancel,cancelled});let service=worker_service.clone();let wake=wake.clone();
                            jobs.spawn(async move{let result=run(id,request,receiver,stop,&service,&wake).await;if let Err(message)=result{service.event(id,SocketEvent::Failed{message});wake();}id});
                        }
                        command=>{let id=match &command{Command::Send{id,..}|Command::Close{id,..}=>*id,_=>unreachable!()};if let Some(c)=active.get(&id){if c.sender.try_send(command).is_err(){let _=c.cancel.send(true);worker_service.event(id,SocketEvent::Failed{message:"WebSocket worker queue full".into()});wake();}}}
                    }}
                    tokio::select!{_ = notify.notified()=>{}, result=jobs.join_next(),if !jobs.is_empty()=>{if let Some(Ok(id))=result{active.remove(&id);}}}
                }
                jobs.abort_all();while jobs.join_next().await.is_some(){}
            });
            // OS DNS lookups may be in Tokio's blocking resolver. Shutdown never waits on DNS.
            runtime.shutdown_timeout(Duration::from_millis(100));
        }).map_err(|e|e.to_string())?;
        Ok(Self {
            service,
            worker: Some(worker),
        })
    }
    pub fn service(&self) -> SocketService {
        self.service.clone()
    }
    pub fn dispose(&mut self) {
        self.service.dispose();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
        self.service.set_command_waker(None);
    }
}
impl Drop for NativeWebSocket {
    fn drop(&mut self) {
        self.dispose();
    }
}
async fn publish(
    id: u32,
    event: SocketEvent,
    service: &SocketService,
    wake: &Arc<impl Fn() + Send + Sync>,
    stop: &mut watch::Receiver<bool>,
) -> bool {
    loop {
        if *stop.borrow() || service.disposed() {
            return false;
        }
        if service.event(id, event.clone()) {
            wake();
            return true;
        }
        tokio::select! {_=stop.changed()=>return false,_=tokio::time::sleep(Duration::from_millis(10))=>{}}
    }
}
async fn run(
    id: u32,
    request: SocketRequest,
    commands: mpsc::Receiver<Command>,
    mut stop: watch::Receiver<bool>,
    service: &SocketService,
    wake: &Arc<impl Fn() + Send + Sync>,
) -> Result<(), String> {
    if *stop.borrow() {
        return Ok(());
    }
    let connection_stop = stop.clone();
    // Covers blocked sends, TLS, reads and close flushes as well as handshakes.
    tokio::select! {
        _ = stop.changed() => Ok(()),
        result = run_connection(id, request, commands, connection_stop, service, wake) => result,
    }
}

async fn run_connection(
    id: u32,
    request: SocketRequest,
    mut commands: mpsc::Receiver<Command>,
    mut stop: watch::Receiver<bool>,
    service: &SocketService,
    wake: &Arc<impl Fn() + Send + Sync>,
) -> Result<(), String> {
    let mut handshake = request
        .url
        .as_str()
        .into_client_request()
        .map_err(|e| e.to_string())?;
    for (name, value) in &request.headers {
        handshake.headers_mut().append(
            tokio_tungstenite::tungstenite::http::HeaderName::from_bytes(name.as_bytes())
                .map_err(|e| e.to_string())?,
            value.parse().map_err(
                |e: tokio_tungstenite::tungstenite::http::header::InvalidHeaderValue| e.to_string(),
            )?,
        );
    }
    if !request.protocols.is_empty() {
        handshake.headers_mut().insert(
            "Sec-WebSocket-Protocol",
            request.protocols.join(", ").parse().map_err(
                |e: tokio_tungstenite::tungstenite::http::header::InvalidHeaderValue| e.to_string(),
            )?,
        );
    }
    let config = WebSocketConfig::default()
        .max_message_size(Some(MAX_BYTES))
        .max_frame_size(Some(MAX_BYTES))
        .max_write_buffer_size(MAX_BYTES + 1024)
        .write_buffer_size(0);
    let connect = tokio::time::timeout(
        Duration::from_millis(request.timeout_ms),
        tokio_tungstenite::connect_async_with_config(handshake, Some(config), true),
    );
    let (socket, response) = tokio::select! {_ = stop.changed()=>return Ok(()),result=connect=>result.map_err(|_|"WebSocket handshake timeout".to_string())?.map_err(|e|e.to_string())?};
    let protocol = response
        .headers()
        .get("Sec-WebSocket-Protocol")
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned);
    if protocol
        .as_ref()
        .is_some_and(|p| !request.protocols.contains(p))
    {
        return Err("server selected an unrequested WebSocket protocol".into());
    }
    if !publish(
        id,
        SocketEvent::Connected { protocol },
        service,
        wake,
        &mut stop,
    )
    .await
    {
        return Ok(());
    }
    let (mut sender, mut receiver) = socket.split();
    let mut close_deadline = None;
    loop {
        tokio::select! {
            _=stop.changed()=>return Ok(()),
            _=async{if let Some(deadline)=close_deadline{tokio::time::sleep_until(deadline).await}else{std::future::pending::<()>().await}}=>return Err("WebSocket close timeout".into()),
            command=commands.recv(),if close_deadline.is_none()=>{match command{
                Some(Command::Send{message,..})=>{let bytes=message.len();let message=match message{SocketMessage::Text(v)=>Message::Text(v.into()),SocketMessage::Binary(v)=>Message::Binary(v.into())};let result=tokio::time::timeout(Duration::from_secs(30),sender.send(message)).await;service.acknowledge(id,bytes);result.map_err(|_|"WebSocket send timeout".to_string())?.map_err(|e|e.to_string())?;}
                Some(Command::Close{code,reason,..})=>{tokio::time::timeout(Duration::from_secs(10),sender.send(Message::Close(Some(CloseFrame{code:code.into(),reason:reason.into()})))).await.map_err(|_|"WebSocket close timeout".to_string())?.map_err(|e|e.to_string())?;close_deadline=Some(tokio::time::Instant::now()+Duration::from_secs(10));}
                None=>return Ok(()),_=>{}
            }},
            message=receiver.next()=>{match message{
                Some(Ok(Message::Text(v)))=>{if !publish(id,SocketEvent::Message{message:SocketMessage::Text(v.to_string())},service,wake,&mut stop).await{return Ok(());}}
                Some(Ok(Message::Binary(v)))=>{if !publish(id,SocketEvent::Message{message:SocketMessage::Binary(v.to_vec())},service,wake,&mut stop).await{return Ok(());}}
                Some(Ok(Message::Close(frame)))=>{let(code,reason)=frame.map(|f|(Some(f.code.into()),f.reason.to_string())).unwrap_or_default();let _=tokio::time::timeout(Duration::from_secs(1),sender.flush()).await;service.event(id,SocketEvent::Closed{code,reason});wake();return Ok(());}
                Some(Ok(Message::Ping(_)))=>{tokio::time::timeout(Duration::from_secs(10),sender.flush()).await.map_err(|_|"WebSocket pong timeout".to_string())?.map_err(|e|e.to_string())?;}
                Some(Ok(_))=>{},Some(Err(e))=>return Err(e.to_string()),None=>return Err("WebSocket transport ended without a close frame".into())
            }}
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;
    fn wait(h: &crate::websocket::SocketHandle) -> Vec<SocketEvent> {
        let end = std::time::Instant::now() + Duration::from_secs(5);
        loop {
            let events = h.events();
            if !events.is_empty() {
                return events;
            }
            assert!(std::time::Instant::now() < end, "WebSocket event timeout");
            std::thread::sleep(Duration::from_millis(5));
        }
    }
    #[test]
    fn loopback_text_binary_ping_close_and_release() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut socket = tokio_tungstenite::tungstenite::accept_hdr(
                stream,
                |request: &tokio_tungstenite::tungstenite::handshake::server::Request, response| {
                    assert_eq!(request.headers()["Authorization"], "Bearer local-test");
                    Ok(response)
                },
            )
            .unwrap();
            assert_eq!(socket.read().unwrap(), Message::Text("hello".into()));
            socket.send(Message::Binary(vec![1, 2, 3].into())).unwrap();
            socket.send(Message::Ping(vec![42].into())).unwrap();
            assert_eq!(socket.read().unwrap(), Message::Pong(vec![42].into()));
            socket
                .close(Some(CloseFrame {
                    code: 1000.into(),
                    reason: "done".into(),
                }))
                .unwrap();
            let _ = socket.read();
        });
        let mut native = NativeWebSocket::new(|| {}).unwrap();
        let service = native.service();
        let handle = service
            .connect(SocketRequest {
                url: format!("ws://{address}/test"),
                headers: vec![("Authorization".into(), "Bearer local-test".into())],
                protocols: vec![],
                timeout_ms: 1000,
            })
            .unwrap();
        assert!(matches!(wait(&handle)[0], SocketEvent::Connected { .. }));
        handle.send(SocketMessage::Text("hello".into())).unwrap();
        let mut binary = false;
        let mut closed = false;
        while !closed {
            for event in wait(&handle) {
                match event {
                    SocketEvent::Message { message } => {
                        assert_eq!(message, SocketMessage::Binary(vec![1, 2, 3]));
                        binary = true;
                    }
                    SocketEvent::Closed { code, reason } => {
                        assert_eq!(code, Some(1000));
                        assert_eq!(reason, "done");
                        closed = true;
                    }
                    SocketEvent::Failed { message } => panic!("{message}"),
                    _ => {}
                }
            }
        }
        assert!(binary);
        drop(handle);
        server.join().unwrap();
        native.dispose();
        assert!(service
            .connect(SocketRequest {
                url: "ws://localhost/".into(),
                headers: vec![],
                protocols: vec![],
                timeout_ms: 1000
            })
            .is_err());
    }
    #[test]
    fn oversized_incoming_frame_fails_without_delivering_payload() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            stream
                .set_write_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut socket = tokio_tungstenite::tungstenite::accept(stream).unwrap();
            let _ = socket.send(Message::Binary(vec![1; MAX_BYTES + 1].into()));
        });
        let native = NativeWebSocket::new(|| {}).unwrap();
        let handle = native
            .service()
            .connect(SocketRequest {
                url: format!("ws://{address}/"),
                headers: vec![],
                protocols: vec![],
                timeout_ms: 1000,
            })
            .unwrap();
        let mut failed = false;
        while !failed {
            for event in wait(&handle) {
                match event {
                    SocketEvent::Failed { .. } => failed = true,
                    SocketEvent::Message { .. } => panic!("oversized frame delivered"),
                    _ => {}
                }
            }
        }
        server.join().unwrap();
    }

    #[test]
    fn cancellation_interrupts_incomplete_handshake() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let mut native = NativeWebSocket::new(|| {}).unwrap();
        let handle = native
            .service()
            .connect(SocketRequest {
                url: format!("ws://{address}/"),
                headers: vec![],
                protocols: vec![],
                timeout_ms: 120000,
            })
            .unwrap();
        let (stream, _) = listener.accept().unwrap();
        let start = std::time::Instant::now();
        drop(handle);
        native.dispose();
        assert!(start.elapsed() < Duration::from_secs(2));
        drop(stream);
    }
}

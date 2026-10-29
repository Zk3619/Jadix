use jadix_hot_reload::{
    CommitAck, CommitStatus, HotReloadCommand, HotReloadReceiver, SessionGate, SessionInfo,
    Transaction,
};
use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;
use std::sync::mpsc;
use std::time::{Duration, Instant};

#[test]
fn transport_ack_waits_for_commit_and_receiver_drop_closes_listener() {
    let (ready_sender, ready_receiver) = mpsc::channel();
    let receiver = HotReloadReceiver::spawn(
        "127.0.0.1:0".into(),
        || {},
        move |address| {
            ready_sender.send(address.to_string()).unwrap();
        },
        |reason| panic!("receiver: {reason}"),
    );
    let address = ready_receiver.recv_timeout(Duration::from_secs(2)).unwrap();
    let client_address = address.clone();
    let (waiting_sender, waiting_receiver) = mpsc::channel();
    let (queued_sender, queued_receiver) = mpsc::channel();
    let client = std::thread::spawn(move || {
        let stream = TcpStream::connect(client_address).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let mut connection = BufReader::new(stream);
        connection
            .get_mut()
            .write_all(b"{\"kind\":\"hello\"}\n")
            .unwrap();
        let mut line = String::new();
        connection.read_line(&mut line).unwrap();
        let session: SessionInfo = serde_json::from_str(&line).unwrap();
        let request = Transaction::new(&session).unwrap();
        let mut wire =
            serde_json::to_vec(&serde_json::json!({"kind": "transaction", "transaction": request}))
                .unwrap();
        wire.push(b'\n');
        assert!(
            matches!(
                jadix_hot_reload::parse_command(std::str::from_utf8(&wire).unwrap()),
                Some(HotReloadCommand::Transaction(_, _))
            ),
            "wire: {}",
            String::from_utf8_lossy(&wire)
        );
        connection.get_mut().write_all(&wire).unwrap();
        queued_receiver
            .recv_timeout(Duration::from_secs(2))
            .unwrap();
        connection.get_ref().set_nonblocking(true).unwrap();
        let mut pending = [0u8; 1];
        let reason = connection.get_ref().peek(&mut pending).unwrap_err();
        assert_eq!(reason.kind(), std::io::ErrorKind::WouldBlock);
        connection.get_ref().set_nonblocking(false).unwrap();
        line.clear();
        waiting_sender.send(()).unwrap();
        connection.read_line(&mut line).unwrap();
        let acknowledgement: CommitAck = serde_json::from_str(&line).unwrap();
        assert_eq!(acknowledgement.status, CommitStatus::Applied);
        assert_eq!(acknowledgement.revision, 1);
    });
    let deadline = Instant::now() + Duration::from_secs(2);
    let (request, reply) = loop {
        match receiver.try_recv() {
            Ok(HotReloadCommand::Transaction(request, Some(reply))) => break (request, reply),
            Err(mpsc::TryRecvError::Empty) if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(1))
            }
            result => panic!("missing transaction: {result:?}"),
        }
    };
    assert!(!receiver.has_pending());
    queued_sender.send(()).unwrap();
    waiting_receiver
        .recv_timeout(Duration::from_secs(2))
        .unwrap();
    let mut gate = SessionGate::new(receiver.session());
    let acknowledgement = gate.apply(&request, |_| Ok(CommitStatus::Applied));
    receiver.set_revision(acknowledgement.revision);
    reply.send(acknowledgement).unwrap();
    client.join().unwrap();
    assert_eq!(receiver.session().revision, 1);
    drop(receiver);
    assert!(TcpStream::connect(address).is_err());
}

#[test]
fn unversioned_commands_are_rejected_without_queueing_or_waking_ui() {
    let (ready_sender, ready_receiver) = mpsc::channel();
    let wakes = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let count = wakes.clone();
    let receiver = HotReloadReceiver::spawn(
        "127.0.0.1:0".into(),
        move || {
            count.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        },
        move |address| {
            ready_sender.send(address.to_string()).unwrap();
        },
        |_| {},
    );
    let stream =
        TcpStream::connect(ready_receiver.recv_timeout(Duration::from_secs(2)).unwrap()).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let mut connection = BufReader::new(stream);
    connection
        .get_mut()
        .write_all(b"{\"kind\":\"reload\"}\n")
        .unwrap();
    let mut line = String::new();
    connection.read_line(&mut line).unwrap();
    let acknowledgement: CommitAck = serde_json::from_str(&line).unwrap();
    assert_eq!(acknowledgement.status, CommitStatus::Rejected);
    assert_eq!(acknowledgement.revision, 0);
    assert_eq!(wakes.load(std::sync::atomic::Ordering::Relaxed), 0);
    assert!(!receiver.has_pending());
}

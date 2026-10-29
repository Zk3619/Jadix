use super::*;

#[test]
fn ownership_forwarding_release_and_profile_isolation() {
    let id = format!("jadix-test-{}-{:?}", std::process::id(), Instant::now());
    let launch = InstanceLaunch {
        args: vec!["文件.txt".into(), "--new-window".into()],
        working_directory: std::env::current_dir().unwrap(),
    };
    let (sender, receiver) = std::sync::mpsc::channel();
    let guard = claim(&id, launch.clone(), move |launch| {
        sender.send(launch).is_ok()
    })
    .unwrap()
    .unwrap();
    assert!(claim(&id, launch.clone(), |_| true).unwrap().is_none());
    assert_eq!(
        receiver.recv_timeout(Duration::from_secs(1)).unwrap(),
        launch
    );
    let other = claim(&format!("{id}-profile"), launch.clone(), |_| true)
        .unwrap()
        .unwrap();
    drop(guard);
    let recovered = claim(&id, launch, |_| true).unwrap().unwrap();
    drop((other, recovered));
    assert!(identity("").is_err());
    assert!(identity(&"x".repeat(513)).is_err());
    assert_ne!(identity("a/b").unwrap(), identity("a_b").unwrap());
}

#[test]
fn protocol_rejects_untrusted_and_oversized_messages() {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    for bytes in [vec![0xff; 4], {
        let message = Message {
            token: "wrong".into(),
            launch: InstanceLaunch {
                args: vec![],
                working_directory: PathBuf::new(),
            },
        };
        let payload = serde_json::to_vec(&message).unwrap();
        let mut bytes = (payload.len() as u32).to_be_bytes().to_vec();
        bytes.extend(payload);
        bytes
    }] {
        let mut client = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        client.write_all(&bytes).unwrap();
        let (mut server, _) = listener.accept().unwrap();
        assert!(receive(&mut server, "expected").is_err());
    }
}

#[test]
#[ignore = "helper process for the cross-process instance test"]
fn instance_child() {
    let id = std::env::var("JADIX_TEST_INSTANCE_ID").unwrap();
    let output = PathBuf::from(std::env::var_os("JADIX_TEST_INSTANCE_OUTPUT").unwrap());
    let launch = InstanceLaunch {
        args: vec![],
        working_directory: PathBuf::new(),
    };
    let _guard = claim(&id, launch, move |launch| {
        std::fs::write(&output, serde_json::to_vec(&launch).unwrap()).is_ok()
    })
    .unwrap()
    .unwrap();
    println!("INSTANCE_READY");
    std::io::stdout().flush().unwrap();
    std::thread::sleep(Duration::from_secs(30));
}

#[test]
fn cross_process_forward_and_crash_recovery() {
    use std::io::BufRead;
    use std::process::{Command, Stdio};
    struct ChildGuard(std::process::Child);
    impl Drop for ChildGuard {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    let id = format!(
        "jadix-process-test-{}-{:?}",
        std::process::id(),
        Instant::now()
    );
    let output = std::env::temp_dir().join(format!("{}.json", identity(&id).unwrap()));
    let mut child = ChildGuard(
        Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "app::instance::desktop::tests::instance_child",
                "--ignored",
                "--nocapture",
                "--test-threads=1",
            ])
            .env("JADIX_TEST_INSTANCE_ID", &id)
            .env("JADIX_TEST_INSTANCE_OUTPUT", &output)
            .stdout(Stdio::piped())
            .spawn()
            .unwrap(),
    );
    let reader = std::io::BufReader::new(child.0.stdout.take().unwrap());
    let mut ready = false;
    for line in reader.lines() {
        if line.unwrap().contains("INSTANCE_READY") {
            ready = true;
            break;
        }
    }
    assert!(ready, "child must acquire the OS lock");
    let launch = InstanceLaunch {
        args: vec!["独立文档.txt".into()],
        working_directory: std::env::current_dir().unwrap(),
    };
    assert!(claim(&id, launch.clone(), |_| true).unwrap().is_none());
    let received: InstanceLaunch =
        serde_json::from_slice(&std::fs::read(&output).unwrap()).unwrap();
    assert_eq!(received, launch);
    child.0.kill().unwrap();
    child.0.wait().unwrap();
    let recovered = claim(&id, launch, |_| true).unwrap().unwrap();
    drop(recovered);
    std::fs::remove_file(output).unwrap();
}

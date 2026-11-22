use super::*;
#[cfg(windows)]
const PROGRAM: &str = "program.exe";
#[cfg(unix)]
const PROGRAM: &str = "program";
#[cfg(windows)]
const OLD: &[u8] = b"MZold";
#[cfg(unix)]
const OLD: &[u8] = b"old";
#[cfg(windows)]
const NEW: &[u8] = b"MZnew";
#[cfg(unix)]
const NEW: &[u8] = b"new";
struct Fixture {
    parent: PathBuf,
    root: PathBuf,
    installation: Installation,
}
impl Fixture {
    fn new() -> Self {
        let parent = scratch(&std::env::temp_dir()).unwrap();
        let root = parent.join("app");
        fs::create_dir(&root).unwrap();
        fs::write(root.join(PROGRAM), OLD).unwrap();
        fs::write(
            root.join("app.json"),
            br#"{"id":"test.app","version":"1.0.0"}"#,
        )
        .unwrap();
        let config = super::super::Config {
            id: "test.app".into(),
            version: "1.0.0".into(),
            github: Some("test/app".into()),
            url: None,
            channels: Default::default(),
            public_key: None,
        };
        let installation =
            Installation::new(&root, Path::new(PROGRAM), Path::new("app.json"), &config).unwrap();
        Self {
            parent,
            root,
            installation,
        }
    }
    fn archive(&self, id: &str, mode: u32) -> PathBuf {
        let path = self.parent.join("update.tar.gz");
        let gzip = flate2::write::GzEncoder::new(
            fs::File::create(&path).unwrap(),
            flate2::Compression::default(),
        );
        let mut tar = tar::Builder::new(gzip);
        for (name, bytes, mode) in [
            (format!("app/{PROGRAM}"), if cfg!(windows) && mode & 0o111 == 0 { b"invalid".to_vec() } else { NEW.to_vec() }, mode),
            (
                "app/app.json".to_owned(),
                format!(r#"{{"id":"{id}","version":"2.0.0"}}"#).into_bytes(),
                0o644,
            ),
        ] {
            let mut header = tar::Header::new_gnu();
            header.set_size(bytes.len() as u64);
            header.set_mode(mode);
            header.set_cksum();
            tar.append_data(&mut header, name, bytes.as_slice())
                .unwrap();
        }
        tar.into_inner().unwrap().finish().unwrap();
        path
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.parent);
    }
}
#[test]
fn validated_update_commit_and_rollback() {
    let f = Fixture::new();
    let archive = f.archive("test.app", 0o755);
    assert!(f.installation.can_install());
    let staged = f.installation.stage(&archive, "2.0.0").unwrap();
    assert_eq!(fs::read(f.root.join(PROGRAM)).unwrap(), OLD);
    assert!(staged.commit(|_| Err("spawn rejected".into())).is_err());
    assert_eq!(fs::read(f.root.join(PROGRAM)).unwrap(), OLD);
    let staged = f.installation.stage(&archive, "2.0.0").unwrap();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        staged.commit(|_| panic!("post-swap failure"))
    }));
    assert!(result.is_err());
    assert_eq!(fs::read(f.root.join(PROGRAM)).unwrap(), OLD);
    f.installation
        .stage(&archive, "2.0.0")
        .unwrap()
        .commit(|path| {
            assert_eq!(fs::read(path).unwrap(), NEW);
            Ok(())
        })
        .unwrap();
    assert_eq!(fs::read(f.root.join(PROGRAM)).unwrap(), NEW);
    assert_eq!(fs::read_dir(&f.parent).unwrap().count(), 2);
}
#[test]
fn rejects_wrong_identity_non_executable_and_replaced_installation() {
    let f = Fixture::new();
    assert!(f
        .installation
        .stage(&f.archive("other.app", 0o755), "2.0.0")
        .is_err());
    assert!(f
        .installation
        .stage(&f.archive("test.app", 0o644), "2.0.0")
        .is_err());
    let staged = f
        .installation
        .stage(&f.archive("test.app", 0o755), "2.0.0")
        .unwrap();
    fs::rename(&f.root, f.parent.join("external-backup")).unwrap();
    fs::create_dir(&f.root).unwrap();
    fs::write(f.root.join(PROGRAM), b"external").unwrap();
    assert!(staged.commit(|_| Ok(())).is_err());
    assert_eq!(fs::read(f.root.join(PROGRAM)).unwrap(), b"external");
    assert_eq!(
        fs::read(f.parent.join("external-backup").join(PROGRAM)).unwrap(),
        OLD
    );
}

#[test]
fn handoff_transfers_cleanup_and_revalidates_before_commit() {
    let f = Fixture::new();
    let archive = f.archive("test.app", 0o755);
    let mut plan = PathBuf::new();
    f.installation.stage(&archive, "2.0.0").unwrap().handoff(|path| {
        plan = path.to_owned();
        Ok(())
    }).unwrap();
    assert!(plan.exists());
    assert_eq!(fs::read(f.root.join(PROGRAM)).unwrap(), OLD);
    let staged = Staged::resume_handoff(&plan).unwrap();
    staged.commit(|_| Err("restart failed".into())).unwrap_err();
    assert_eq!(fs::read(f.root.join(PROGRAM)).unwrap(), OLD);
    assert!(!plan.exists());

    f.installation.stage(&archive, "2.0.0").unwrap().handoff(|path| {
        plan = path.to_owned();
        Ok(())
    }).unwrap();
    let mut value: serde_json::Value = serde_json::from_slice(&fs::read(&plan).unwrap()).unwrap();
    value["new"] = serde_json::to_value(&f.root).unwrap();
    fs::write(&plan, serde_json::to_vec(&value).unwrap()).unwrap();
    assert!(Staged::resume_handoff(&plan).is_err());
    assert!(plan.exists(), "invalid transaction must not acquire cleanup ownership");
    assert_eq!(fs::read(f.root.join(PROGRAM)).unwrap(), OLD);
}

#[test]
fn failed_handoff_launch_cleans_only_its_owned_transaction() {
    let f = Fixture::new();
    let archive = f.archive("test.app", 0o755);
    let mut owned = PathBuf::new();
    assert!(f.installation.stage(&archive, "2.0.0").unwrap().handoff(|plan| {
        owned = plan.parent().unwrap().to_owned();
        Err("launch failed".into())
    }).is_err());
    assert!(!owned.exists());
    assert!(archive.exists());
    assert_eq!(fs::read(f.root.join(PROGRAM)).unwrap(), OLD);
}

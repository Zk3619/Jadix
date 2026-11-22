#![cfg(feature = "updater-install")]
use std::{fs, io::{Cursor, Write}, path::PathBuf, sync::atomic::{AtomicU64, Ordering}};
use jadix_core::updater::archive::unpack;
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Scratch(PathBuf);
impl Scratch { fn new() -> Self { let path = std::env::temp_dir().join(format!("jadix-archive-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed))); fs::create_dir(&path).unwrap(); Self(path) } }
impl Drop for Scratch { fn drop(&mut self) { let _ = fs::remove_dir_all(&self.0); } }
fn zip(files: &[(&str, &[u8])]) -> Vec<u8> {
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for (name, bytes) in files { writer.start_file(*name, zip::write::SimpleFileOptions::default().unix_permissions(0o755)).unwrap(); writer.write_all(bytes).unwrap(); }
    writer.finish().unwrap().into_inner()
}
#[test]
fn extracts_regular_zip_and_refuses_existing_destination() {
    let temp = Scratch::new(); let archive = temp.0.join("update.zip"); let out = temp.0.join("stage");
    fs::write(&archive, zip(&[("Example.app/Contents/MacOS/Example", b"executable")])).unwrap();
    unpack(&archive, &out).unwrap(); assert_eq!(fs::read(out.join("Example.app/Contents/MacOS/Example")).unwrap(), b"executable");
    assert!(unpack(&archive, &out).is_err()); assert!(out.exists());
}
#[test]
fn cancellation_interrupts_a_file_and_preserves_existing_paths() {
    use std::cell::Cell;
    use jadix_core::updater::archive::unpack_with_cancel;
    let temp = Scratch::new(); let archive = temp.0.join("update.zip"); let out = temp.0.join("stage");
    fs::write(&archive, zip(&[("payload", &vec![1; 2 * 1024 * 1024])])).unwrap();
    let reads = Cell::new(0);
    let result = unpack_with_cancel(&archive, &out, || {
        // Cancel after a file has actually been opened, rather than only before extraction.
        if out.join("payload").exists() { reads.set(reads.get() + 1); }
        reads.get() >= 3
    });
    assert!(result.unwrap_err().contains("cancelled"));
    assert!(reads.get() >= 3); assert!(!out.exists()); assert!(archive.exists());
    fs::create_dir(&out).unwrap(); fs::write(out.join("keep"), b"existing").unwrap();
    assert!(unpack_with_cancel(&archive, &out, || true).is_err());
    assert_eq!(fs::read(out.join("keep")).unwrap(), b"existing");
}
#[test]
fn traversal_duplicates_and_corruption_remove_only_owned_staging() {
    let temp = Scratch::new(); let archive = temp.0.join("update.zip"); let out = temp.0.join("stage");
    for name in ["../escaped", "/absolute", "C:/drive", "a\\..\\escaped"] {
        fs::write(&archive, zip(&[(name, b"bad")])).unwrap();
        assert!(unpack(&archive, &out).is_err(), "{name}"); assert!(!out.exists());
    }
    let mut bytes = zip(&[("Example.app/a", b"payload")]); bytes.truncate(bytes.len() - 12);
    fs::write(&archive, bytes).unwrap(); assert!(unpack(&archive, &out).is_err()); assert!(!out.exists());
    assert!(!temp.0.join("escaped").exists());
}
fn tar_link(target: &str) -> Vec<u8> {
    let encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default()); let mut builder = tar::Builder::new(encoder);
    let mut header = tar::Header::new_gnu(); header.set_size(3); header.set_mode(0o755); header.set_cksum();
    builder.append_data(&mut header, "Example.app/Contents/MacOS/Example", &b"exe"[..]).unwrap();
    let mut header = tar::Header::new_gnu(); header.set_entry_type(tar::EntryType::Symlink); header.set_size(0); header.set_mode(0o777); header.set_link_name(target).unwrap(); header.set_cksum();
    builder.append_data(&mut header, "Example.app/link", &b""[..]).unwrap();
    builder.into_inner().unwrap().finish().unwrap()
}
#[test]
fn tar_links_cannot_escape_or_loop() {
    let temp = Scratch::new(); let archive = temp.0.join("update.tar.gz"); let out = temp.0.join("stage");
    for target in ["../../escape", "link", "/outside"] {
        fs::write(&archive, tar_link(target)).unwrap(); assert!(unpack(&archive, &out).is_err()); assert!(!out.exists());
    }
}
#[test]
fn accepts_tar_root_directory_and_relative_members() {
    let temp = Scratch::new(); let archive = temp.0.join("update.tar.gz"); let out = temp.0.join("stage");
    let encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    let mut builder = tar::Builder::new(encoder);
    let mut header = tar::Header::new_gnu(); header.set_entry_type(tar::EntryType::Directory); header.set_size(0); header.set_mode(0o755); header.set_cksum();
    builder.append_data(&mut header, "./", &b""[..]).unwrap();
    let mut header = tar::Header::new_gnu(); header.set_size(3); header.set_mode(0o755); header.set_cksum();
    builder.append_data(&mut header, "./Example.app/Contents/MacOS/Example", &b"exe"[..]).unwrap();
    fs::write(&archive, builder.into_inner().unwrap().finish().unwrap()).unwrap();
    unpack(&archive, &out).unwrap(); assert_eq!(fs::read(out.join("Example.app/Contents/MacOS/Example")).unwrap(), b"exe");
}
#[cfg(unix)]
#[test]
fn preserves_framework_links_and_executable_permissions() {
    use std::os::unix::fs::PermissionsExt;
    let temp = Scratch::new(); let archive = temp.0.join("update.tar.gz"); let out = temp.0.join("stage");
    fs::write(&archive, tar_link("Contents/MacOS/Example")).unwrap(); unpack(&archive, &out).unwrap();
    assert_eq!(fs::read(out.join("Example.app/link")).unwrap(), b"exe");
    assert_eq!(fs::metadata(out.join("Example.app/link")).unwrap().permissions().mode() & 0o777, 0o755);
}

#[cfg(windows)]
#[test]
fn windows_aliases_and_device_names_are_rejected_before_extraction() {
    let temp = Scratch::new(); let archive = temp.0.join("update.zip"); let out = temp.0.join("stage");
    for name in ["NUL.txt", "app/Con", "COM1.exe", "LPT9", "COM¹", "a.", "a ", "a/<b", "a/b?", "a/b:stream"] {
        fs::write(&archive, zip(&[(name, b"bad")])).unwrap();
        assert!(unpack(&archive, &out).is_err(), "{name}");
        assert!(!out.exists(), "failed extraction left owned staging: {name}");
    }
}

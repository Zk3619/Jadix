//! Bounded ZIP / tar.gz extraction into an exclusively owned staging directory.
//! Links are validated as a complete graph and created after regular entries.
use std::{collections::{BTreeMap, VecDeque}, fs::{self, File, OpenOptions}, io::{self, Read}, path::{Path, PathBuf}};

const BYTE_LIMIT: u64 = 8 * 1024 * 1024 * 1024;
const ENTRY_LIMIT: usize = 100_000;

/// On failure only the staging directory created by this call is removed.
/// The caller must own the destination's parent and keep it private during extraction.
pub fn unpack(archive: &Path, destination: &Path) -> Result<(), String> {
    unpack_with_cancel(archive, destination, || false)
}
/// Cancellation is checked between entries and reads, including compressed input.
/// A cancelled extraction removes only the staging directory created by this call.
pub fn unpack_with_cancel(archive: &Path, destination: &Path, cancelled: impl Fn() -> bool) -> Result<(), String> {
    if !destination.is_absolute() { return Err("update staging path must be absolute".into()); }
    check_cancel(&cancelled).map_err(|e| e.to_string())?;
    fs::create_dir(destination).map_err(|e| format!("create update staging directory: {e}"))?;
    let result = extract(archive, destination, &cancelled);
    if result.is_err() { let _ = fs::remove_dir_all(destination); }
    result.map_err(|e| format!("unpack application update: {e}"))
}
fn check_cancel(cancelled: &dyn Fn() -> bool) -> io::Result<()> {
    if cancelled() { Err(io::Error::other("application update cancelled")) } else { Ok(()) }
}
struct Cancellable<'a, R> { reader: R, cancelled: &'a dyn Fn() -> bool }
impl<R: Read> Read for Cancellable<'_, R> {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        check_cancel(self.cancelled)?;
        self.reader.read(bytes)
    }
}
impl<R: io::Seek> io::Seek for Cancellable<'_, R> {
    fn seek(&mut self, position: io::SeekFrom) -> io::Result<u64> {
        check_cancel(self.cancelled)?;
        self.reader.seek(position)
    }
}
fn invalid(message: &str) -> io::Error { io::Error::new(io::ErrorKind::InvalidData, message) }
fn relative(raw: &str) -> io::Result<PathBuf> {
    if raw.is_empty() || raw.len() > 4096 || raw.starts_with('/') || raw.contains(['\\', ':', '\0']) {
        return Err(invalid("invalid archive path"));
    }
    let mut path = PathBuf::new();
    for part in raw.split('/') {
        #[cfg(windows)]
        if part != "." && !part.is_empty() {
            let stem = part.split('.').next().unwrap_or("").to_ascii_uppercase();
            if part.ends_with(['.', ' ']) || part.chars().any(|c| c.is_control() || "<>\"|?*".contains(c))
                || matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$")
                || ["COM", "LPT"].iter().any(|prefix| stem.strip_prefix(prefix).is_some_and(|n| matches!(n, "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "¹" | "²" | "³")))
            { return Err(invalid("invalid Windows archive component")); }
        }
        match part { "" | "." => {}, ".." => return Err(invalid("archive path escapes staging")), _ => path.push(part) }
    }
    if path.as_os_str().is_empty() { return Err(invalid("empty archive path")); }
    Ok(path)
}
struct Link { path: PathBuf, target: String, hard: bool }
struct Extraction<'a> {
    root: &'a Path, entries: usize, bytes: u64, links: Vec<Link>, directories: BTreeMap<PathBuf, u32>,
    cancelled: &'a dyn Fn() -> bool,
}
impl Extraction<'_> {
    fn entry(&mut self, raw: &str) -> io::Result<PathBuf> {
        check_cancel(self.cancelled)?;
        self.entries += 1;
        if self.entries > ENTRY_LIMIT { return Err(invalid("archive entry limit exceeded")); }
        relative(raw)
    }
    fn directory(&mut self, path: &Path, mode: u32) -> io::Result<()> {
        fs::create_dir_all(self.root.join(path))?;
        self.directories.insert(path.to_owned(), mode); Ok(())
    }
    fn file(&mut self, path: &Path, size: u64, mode: u32, source: &mut impl Read) -> io::Result<()> {
        self.bytes = self.bytes.checked_add(size).filter(|n| *n <= BYTE_LIMIT).ok_or_else(|| invalid("uncompressed update exceeds limit"))?;
        let destination = self.root.join(path);
        fs::create_dir_all(destination.parent().unwrap())?;
        let mut file = OpenOptions::new().write(true).create_new(true).open(&destination)?;
        let mut reader = Cancellable { reader: source.take(size.saturating_add(1)), cancelled: self.cancelled };
        if io::copy(&mut reader, &mut file)? != size { return Err(invalid("archive file size mismatch")); }
        file.sync_all()?;
        permissions(&destination, mode)
    }
    fn link(&mut self, path: PathBuf, target: String, hard: bool) -> io::Result<()> {
        if self.links.len() >= 4096 || target.is_empty() || target.len() > 4096 || target.starts_with('/') || target.contains(['\\', ':', '\0']) {
            return Err(invalid("invalid archive link"));
        }
        self.links.push(Link { path, target, hard }); Ok(())
    }
    fn finish(self) -> io::Result<()> {
        let mut symbolic = BTreeMap::new();
        for link in &self.links {
            check_cancel(self.cancelled)?;
            if symbolic.insert(link.path.clone(), link).is_some() { return Err(invalid("duplicate archive link")); }
            if fs::symlink_metadata(self.root.join(&link.path)).is_ok() { return Err(invalid("archive link overwrites an entry")); }
        }
        // Resolve through other links before interpreting '..'. Lexical normalization
        // alone misses aliases such as a -> . followed by b -> a/../outside.
        for link in &self.links {
            check_cancel(self.cancelled)?;
            let mut parts: VecDeque<String> = if link.hard { VecDeque::new() } else {
                link.path.parent().unwrap_or(Path::new("")).iter().map(|s| s.to_string_lossy().into_owned()).collect()
            };
            parts.extend(link.target.split('/').map(str::to_owned));
            let mut resolved = PathBuf::new(); let mut traversed = 0;
            while let Some(part) = parts.pop_front() {
                match part.as_str() {
                    "" | "." => continue,
                    ".." => { if !resolved.pop() { return Err(invalid("archive link escapes staging")); } },
                    _ => {
                        resolved.push(part);
                        if let Some(next) = symbolic.get(&resolved) {
                            traversed += 1;
                            if traversed > 256 { return Err(invalid("cyclic archive links")); }
                            if next.hard { resolved.clear(); } else { resolved.pop(); }
                            for component in next.target.split('/').rev() { parts.push_front(component.into()); }
                        }
                    }
                }
            }
            if link.hard && !fs::symlink_metadata(self.root.join(&resolved)).is_ok_and(|m| m.is_file()) {
                return Err(invalid("archive hard link requires a regular target"));
            }
        }
        for link in self.links.iter().filter(|link| link.hard) {
            check_cancel(self.cancelled)?;
            let target = relative(&link.target)?;
            let source = self.root.join(target);
            // Hard links cannot resolve through a symbolic ancestor.
            let mut ancestor = self.root.to_owned();
            for component in source.strip_prefix(self.root).map_err(|_| invalid("invalid hard link"))?.iter() {
                ancestor.push(component);
                if fs::symlink_metadata(&ancestor)?.file_type().is_symlink() { return Err(invalid("indirect archive hard link")); }
            }
            self.link_parent(&link.path)?;
            fs::hard_link(source, self.root.join(&link.path))?;
        }
        for link in self.links.iter().filter(|link| !link.hard) {
            check_cancel(self.cancelled)?;
            self.link_parent(&link.path)?;
            symlink(Path::new(&link.target), &self.root.join(&link.path))?;
        }
        let mut directories: Vec<_> = self.directories.into_iter().collect();
        directories.sort_by_key(|(path, _)| std::cmp::Reverse(path.components().count()));
        for (path, mode) in directories { check_cancel(self.cancelled)?; permissions(&self.root.join(path), mode)?; }
        check_cancel(self.cancelled)?;
        Ok(())
    }
    fn link_parent(&self, path: &Path) -> io::Result<()> {
        let mut parent = self.root.to_owned();
        for part in path.parent().unwrap_or(Path::new("")).iter() {
            parent.push(part);
            if fs::symlink_metadata(&parent).is_ok_and(|m| m.file_type().is_symlink()) { return Err(invalid("archive link nested under a link")); }
        }
        fs::create_dir_all(parent)
    }
}
fn extract(archive: &Path, root: &Path, cancelled: &dyn Fn() -> bool) -> io::Result<()> {
    let mut file = File::open(archive)?;
    let mut magic = [0; 4]; file.read_exact(&mut magic)?;
    let mut output = Extraction { root, entries: 0, bytes: 0, links: Vec::new(), directories: BTreeMap::new(), cancelled };
    if &magic[..2] == b"PK" {
        let mut zip = zip::ZipArchive::new(Cancellable { reader: File::open(archive)?, cancelled })?;
        if zip.len() > ENTRY_LIMIT { return Err(invalid("archive entry limit exceeded")); }
        for index in 0..zip.len() {
            let mut entry = zip.by_index(index)?;
            let path = output.entry(entry.name())?;
            let mode = entry.unix_mode().unwrap_or(if entry.is_dir() { 0o755 } else { 0o644 });
            if entry.is_dir() { output.directory(&path, mode)?; }
            else if mode & 0o170000 == 0o120000 {
                if entry.size() > 4096 { return Err(invalid("archive link target exceeds limit")); }
                let mut target = String::new(); entry.by_ref().take(4097).read_to_string(&mut target)?;
                output.link(path, target, false)?;
            } else if matches!(mode & 0o170000, 0 | 0o100000) {
                output.file(&path, entry.size(), mode, &mut entry)?;
            } else { return Err(invalid("unsupported archive entry")); }
        }
    } else if magic[..2] == [0x1f, 0x8b] {
        let mut tar = tar::Archive::new(flate2::read::GzDecoder::new(Cancellable { reader: File::open(archive)?, cancelled }));
        for entry in tar.entries()? {
            let mut entry = entry?;
            let entry_path = entry.path()?.into_owned();
            let raw = entry_path.to_str().ok_or_else(|| invalid("non-UTF-8 archive path"))?;
            let kind = entry.header().entry_type(); let mode = entry.header().mode()?;
            // `tar -C directory .` emits a harmless root directory record.
            if kind.is_dir() && !raw.is_empty() && raw.split('/').all(|p| p.is_empty() || p == ".") {
                output.entries += 1;
                if output.entries > ENTRY_LIMIT { return Err(invalid("archive entry limit exceeded")); }
                continue;
            }
            let path = output.entry(raw)?;
            if kind.is_dir() { output.directory(&path, mode)?; }
            else if kind.is_file() { output.file(&path, entry.size(), mode, &mut entry)?; }
            else if kind.is_symlink() || kind.is_hard_link() {
                let target = entry.link_name()?.ok_or_else(|| invalid("missing archive link target"))?.to_str().ok_or_else(|| invalid("non-UTF-8 archive link"))?.to_owned();
                output.link(path, target, kind.is_hard_link())?;
            } else { return Err(invalid("unsupported archive entry")); }
        }
    } else { return Err(invalid("application update must be a ZIP or tar.gz archive")); }
    if output.entries == 0 { return Err(invalid("empty application archive")); }
    output.finish()
}
#[cfg(unix)]
fn permissions(path: &Path, mode: u32) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(mode & 0o777))
}
#[cfg(not(unix))]
fn permissions(_: &Path, _: u32) -> io::Result<()> { Ok(()) }
#[cfg(unix)]
fn symlink(target: &Path, link: &Path) -> io::Result<()> { std::os::unix::fs::symlink(target, link) }
#[cfg(not(unix))]
fn symlink(_: &Path, _: &Path) -> io::Result<()> { Err(invalid("this update host does not support archive symlinks")) }

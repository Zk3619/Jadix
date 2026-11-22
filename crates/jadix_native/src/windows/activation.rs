//! Per-application activation, independent of the executable's other windows.
use super::*;
use jadix_core::deep_link::{ApplicationActivation, ApplicationLaunch};
use sha2::{Digest, Sha256};
use std::ptr::null;
use windows::{core::{HSTRING, PCWSTR}, Win32::{System::Registry::{RegGetValueW, RegSetKeyValueW, RegDeleteTreeW, RegDeleteKeyValueW, HKEY_CURRENT_USER, REG_SZ, RRF_RT_REG_SZ}, UI::Shell::{SHChangeNotify, SHCNE_ASSOCCHANGED, SHCNF_IDLIST}}};
use windows_sys::Win32::{Foundation::{GetLastError, ERROR_ALREADY_EXISTS, WAIT_ABANDONED, WAIT_OBJECT_0}, System::Threading::{CreateMutexW, ReleaseMutex, WaitForSingleObject}};

pub(super) const MESSAGE: usize = 0x4a445849;
const MAX_MESSAGE: usize = 128 * 1024;
pub(super) struct Registration { mutex: HANDLE, owned: bool, window: HWND, property: Vec<u16>, pub config: ApplicationActivation }
impl Drop for Registration {
    fn drop(&mut self) {
        unsafe {
            RemovePropW(self.window, self.property.as_ptr());
            if !self.mutex.is_null() { if self.owned { ReleaseMutex(self.mutex); } CloseHandle(self.mutex); }
        }
    }
}
impl Registration {
    pub fn start(window: HWND, config: ApplicationActivation) -> Result<Option<Self>, String> {
        config.validate().map_err(String::from)?;
        let launch = ApplicationLaunch::current().map_err(String::from)?;
        let property = wide(&format!("Jadix.Application.{}", key(&config)));
        let mut registration = Self { mutex: null_mut(), owned: false, window, property, config };
        if registration.config.deep_links.single_instance {
            let name = wide(&format!("Local\\Jadix.Application.{}", key(&registration.config)));
            registration.mutex = unsafe { CreateMutexW(null(), 1, name.as_ptr()) };
            let error = unsafe { GetLastError() };
            if registration.mutex.is_null() { return Err(format!("cannot create application activation mutex: {error}")); }
            registration.owned = error != ERROR_ALREADY_EXISTS;
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
            while !registration.owned {
                if forward(&registration.property, window, &launch)? { return Ok(None); }
                // A crashed or normally closed primary releases the mutex. The
                // next process takes ownership instead of leaving a stale lock.
                registration.owned = matches!(unsafe { WaitForSingleObject(registration.mutex, 0) }, WAIT_OBJECT_0 | WAIT_ABANDONED);
                if !registration.owned {
                    if std::time::Instant::now() >= deadline { return Err("the running application did not accept activation".into()); }
                    std::thread::sleep(std::time::Duration::from_millis(50));
                }
            }
        }
        Ok(Some(registration))
    }
    pub fn attach(&self) -> Result<(), String> {
        if unsafe { SetPropW(self.window, self.property.as_ptr(), 1usize as HANDLE) } == 0 { return Err("cannot attach application activation identity".into()); }
        Ok(())
    }
}
fn key(config: &ApplicationActivation) -> String {
    let bytes = serde_json::to_vec(&(&config.identifier, &config.arguments)).expect("activation identity serialization");
    format!("{:x}", Sha256::digest(bytes))
}

/// Quote one CommandLineToArgvW argument, including embedded quotes and trailing
/// backslashes. Registry commands never pass through cmd.exe or PowerShell.
pub(super) fn quote(argument: &str) -> String {
    let mut out = String::from("\""); let mut slashes = 0;
    for c in argument.chars() {
        if c == '\\' { slashes += 1; continue; }
        out.extend(std::iter::repeat_n('\\', if c == '"' { slashes * 2 + 1 } else { slashes }));
        slashes = 0; out.push(c);
    }
    out.extend(std::iter::repeat_n('\\', slashes * 2)); out.push('"'); out
}
pub(super) fn command(arguments: &[String], activation: &str) -> Result<String, String> {
    let executable = std::env::current_exe().map_err(|e| e.to_string())?;
    let executable = executable.to_str().ok_or("application executable is not Unicode")?;
    let mut parts = vec![quote(executable)];
    parts.extend(arguments.iter().map(|s| quote(s)));
    parts.push(quote(activation));
    let command = parts.join(" ");
    if command.encode_utf16().count() >= 32_767 { return Err("activation command exceeds Windows command line limit".into()); }
    Ok(command)
}
fn read(key: &str, name: Option<&str>) -> Result<Option<String>, String> {
    let key = HSTRING::from(key); let name = name.map(HSTRING::from);
    let name = name.as_ref().map_or(PCWSTR::null(), |s| PCWSTR(s.as_ptr()));
    let mut bytes = 0;
    let status = unsafe { RegGetValueW(HKEY_CURRENT_USER, &key, name, RRF_RT_REG_SZ, None, None, Some(&mut bytes)) };
    if status.0 == 2 { return Ok(None); }
    status.ok().map_err(|e| e.to_string())?;
    if bytes > 131_072 { return Err("activation registry value exceeds limit".into()); }
    let mut data = vec![0u16; bytes as usize / 2 + 1];
    unsafe { RegGetValueW(HKEY_CURRENT_USER, &key, name, RRF_RT_REG_SZ, None, Some(data.as_mut_ptr().cast()), Some(&mut bytes)) }.ok().map_err(|e| e.to_string())?;
    let length = data.iter().position(|c| *c == 0).unwrap_or(data.len());
    String::from_utf16(&data[..length]).map(Some).map_err(|e| e.to_string())
}
fn write(key: &str, name: Option<&str>, value: &str) -> Result<(), String> {
    let data = wide(value); let name = name.map(HSTRING::from);
    unsafe { RegSetKeyValueW(HKEY_CURRENT_USER, &HSTRING::from(key), name.as_ref().map_or(PCWSTR::null(), |s| PCWSTR(s.as_ptr())), REG_SZ.0, Some(data.as_ptr().cast()), (data.len() * 2) as u32) }.ok().map_err(|e| e.to_string())
}
/// Explicit installation operation. Configuring an in-process router alone does
/// not claim ownership of OS protocols. Uninstall only removes our exact command.
pub(super) fn protocols(config: &ApplicationActivation, install: bool) -> Result<(), String> {
    config.validate().map_err(String::from)?;
    let command = command(&config.arguments, "%1")?;
    for scheme in &config.deep_links.schemes {
        if matches!(scheme.as_str(), "http" | "https" | "file" | "data" | "javascript" | "shell" | "ms-settings") { return Err("system URL schemes cannot be registered by an application".into()); }
        let path = format!("Software\\Classes\\{scheme}");
        let owner = read(&path, Some("JadixApplication"))?;
        let existing = read(&format!("{path}\\shell\\open\\command"), None)?;
        if install {
            if owner.as_deref() != Some(config.identifier.as_str()) && (existing.is_some() || owner.is_some()
                || read(&path, None)?.is_some() || read(&path, Some("URL Protocol"))?.is_some()) {
                return Err(format!("protocol {scheme} belongs to another application"));
            }
        } else if owner.as_deref() != Some(config.identifier.as_str()) || existing.as_deref() != Some(command.as_str()) { continue; }
    }
    for scheme in &config.deep_links.schemes {
        let path = format!("Software\\Classes\\{scheme}");
        if install {
            write(&path, Some("JadixApplication"), &config.identifier)?;
            write(&path, None, &format!("URL:{}", config.identifier))?;
            write(&path, Some("URL Protocol"), "")?;
            write(&format!("{path}\\shell\\open\\command"), None, &command)?;
        } else if read(&path, Some("JadixApplication"))?.as_deref() == Some(config.identifier.as_str())
            && read(&format!("{path}\\shell\\open\\command"), None)?.as_deref() == Some(command.as_str()) {
            unsafe { RegDeleteTreeW(HKEY_CURRENT_USER, &HSTRING::from(path)) }.ok().map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}
pub(super) fn files(config: &ApplicationActivation, install: bool) -> Result<(), String> {
    config.validate().map_err(String::from)?;
    if config.file_extensions.is_empty() { return Ok(()); }
    let command = command(&config.arguments, "%1")?;
    let progid = format!("Jadix.{}", &key(config)[..32]);
    let path = format!("Software\\Classes\\{progid}");
    let owner = read(&path, Some("JadixApplication"))?;
    let existing = read(&format!("{path}\\shell\\open\\command"), None)?;
    if install {
        if owner.as_deref() != Some(&config.identifier) && (owner.is_some() || existing.is_some() || read(&path, None)?.is_some()) {
            return Err("file association belongs to another application".into());
        }
        write(&path, Some("JadixApplication"), &config.identifier)?;
        write(&path, None, &format!("{} document", config.identifier))?;
        write(&format!("{path}\\shell\\open\\command"), None, &command)?;
        for extension in &config.file_extensions {
            write(&format!("Software\\Classes\\.{extension}\\OpenWithProgids"), Some(&progid), "")?;
        }
    } else if owner.as_deref() == Some(&config.identifier) && existing.as_deref() == Some(&command) {
        for extension in &config.file_extensions {
            let extension_path = HSTRING::from(format!("Software\\Classes\\.{extension}\\OpenWithProgids"));
            let status = unsafe { RegDeleteKeyValueW(HKEY_CURRENT_USER, &extension_path, &HSTRING::from(&progid)) };
            if status.0 != 2 { status.ok().map_err(|e| e.to_string())?; }
        }
        unsafe { RegDeleteTreeW(HKEY_CURRENT_USER, &HSTRING::from(path)) }.ok().map_err(|e| e.to_string())?;
    }
    unsafe { SHChangeNotify(SHCNE_ASSOCCHANGED, SHCNF_IDLIST, None, None); }
    Ok(())
}
struct Handoff<'a> { property: &'a [u16], exclude: HWND, bytes: &'a [u8], delivered: bool }
unsafe extern "system" fn visit(window: HWND, parameter: LPARAM) -> i32 {
    let handoff = unsafe { &mut *(parameter as *mut Handoff<'_>) };
    if window == handoff.exclude || unsafe { GetPropW(window, handoff.property.as_ptr()) }.is_null() { return 1; }
    let data = windows_sys::Win32::System::DataExchange::COPYDATASTRUCT { dwData: MESSAGE, cbData: handoff.bytes.len() as u32, lpData: handoff.bytes.as_ptr().cast_mut().cast() };
    let mut result = 0;
    if unsafe { SendMessageTimeoutW(window, WM_COPYDATA, 0, &data as *const _ as LPARAM, SMTO_ABORTIFHUNG | SMTO_BLOCK, 250, &mut result) } != 0 && result == 1 {
        handoff.delivered = true; return 0;
    }
    1
}
fn forward(property: &[u16], exclude: HWND, launch: &ApplicationLaunch) -> Result<bool, String> {
    launch.validate().map_err(String::from)?;
    let bytes = serde_json::to_vec(launch).map_err(|e| e.to_string())?;
    if bytes.len() > MAX_MESSAGE { return Err("encoded launch exceeds transfer limit".into()); }
    let mut handoff = Handoff { property, exclude, bytes: &bytes, delivered: false };
    unsafe { EnumWindows(Some(visit), &mut handoff as *mut _ as LPARAM); }
    Ok(handoff.delivered)
}
pub(super) unsafe fn receive(context: &WindowContext, data: &windows_sys::Win32::System::DataExchange::COPYDATASTRUCT) -> LRESULT {
    if data.cbData == 0 || data.cbData as usize > MAX_MESSAGE || data.lpData.is_null() { return 0; }
    let Ok(launch) = serde_json::from_slice::<ApplicationLaunch>(unsafe { std::slice::from_raw_parts(data.lpData.cast::<u8>(), data.cbData as usize) }) else { return 0; };
    if launch.validate().is_err() { return 0; }
    let Ok(mut links) = context.deep_links.try_borrow_mut() else { return 0; };
    if links.activation.as_ref().is_none_or(|r| !r.config.deep_links.single_instance) || links.launches.len() >= 64 { return 0; }
    let config = links.activation.as_ref().unwrap().config.clone();
    links.deliver_launch(&config, &launch, jadix_core::deep_link::DeepLinkSource::Handoff, &context.desktop);
    links.launches.push_back(launch);
    unsafe { PostMessageW(context.root.get(), WM_NULL, 0, 0); }
    1
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn file_open_with_registration_keeps_defaults_and_other_handlers() {
        let extension = format!("jadixassoc{}", std::process::id());
        let config = ApplicationActivation { identifier: format!("dev.jadix.file-test.{}", std::process::id()), arguments: vec!["entry with space.tsx".into()], deep_links: Default::default(), file_extensions: BTreeSet::from([extension.clone()]) };
        let path = format!("Software\\Classes\\.{}", extension);
        assert!(read(&path, None).unwrap().is_none());
        write(&path, None, "Other.Default").unwrap();
        write(&format!("{path}\\OpenWithProgids"), Some("Other.Handler"), "").unwrap();
        let progid = format!("Jadix.{}", &key(&config)[..32]);
        let progpath = format!("Software\\Classes\\{progid}");
        files(&config, true).unwrap();
        assert_eq!(read(&format!("{progpath}\\shell\\open\\command"), None).unwrap(), Some(command(&config.arguments, "%1").unwrap()));
        assert_eq!(read(&format!("{path}\\OpenWithProgids"), Some(&progid)).unwrap(), Some(String::new()));
        write(&progpath, Some("JadixApplication"), "other-owner").unwrap();
        assert!(files(&config, true).is_err()); files(&config, false).unwrap();
        assert!(read(&progpath, None).unwrap().is_some());
        write(&progpath, Some("JadixApplication"), &config.identifier).unwrap(); files(&config, false).unwrap();
        assert!(read(&progpath, None).unwrap().is_none());
        assert_eq!(read(&path, None).unwrap().as_deref(), Some("Other.Default"));
        assert!(read(&format!("{path}\\OpenWithProgids"), Some(&progid)).unwrap().is_none());
        assert!(read(&format!("{path}\\OpenWithProgids"), Some("Other.Handler")).unwrap().is_some());
        unsafe { RegDeleteTreeW(HKEY_CURRENT_USER, &HSTRING::from(path)) }.ok().unwrap();
    }
    #[test]
    fn single_instance_handoff_uses_real_processes_and_releases_on_dispose() {
        const NAME: &str = "windows::activation::tests::single_instance_handoff_uses_real_processes_and_releases_on_dispose";
        let child = std::env::var("JADIX_ACTIVATION_CHILD").ok();
        let identifier = child.clone().unwrap_or_else(|| format!("dev.jadix.activation.{}", std::process::id()));
        let config = ApplicationActivation { identifier: identifier.clone(), arguments: vec![], deep_links: Default::default(), file_extensions: Default::default() };
        let mut host = WindowsHost::new("Jadix process activation", [320, 240]).unwrap();
        let primary = host.configure_activation(config.clone()).unwrap();
        if child.is_some() {
            assert_eq!(primary, std::env::var_os("JADIX_ACTIVATION_PRIMARY").is_some());
            host.dispose(); return;
        }
        assert!(primary);
        assert_eq!(host.take_application_launches().len(), 1);
        let launch = |primary: bool, host: &WindowsHost| {
            let mut command = std::process::Command::new(std::env::current_exe().unwrap());
            command.args(["--exact", NAME, "--nocapture"]).env("JADIX_ACTIVATION_CHILD", &identifier)
                .stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null());
            if primary { command.env("JADIX_ACTIVATION_PRIMARY", "1"); }
            let mut child = command.spawn().unwrap();
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(12);
            loop {
                host.pump();
                if let Some(status) = child.try_wait().unwrap() { assert!(status.success(), "activation child failed: {status}"); break; }
                if std::time::Instant::now() >= deadline { let _ = child.kill(); let _ = child.wait(); panic!("activation child timed out"); }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
        };
        launch(false, &host);
        let launches = host.take_application_launches();
        assert_eq!(launches.len(), 1);
        assert!(launches[0].arguments.iter().any(|a| a == NAME));
        assert!(launches[0].working_directory.is_some());
        host.dispose();
        launch(true, &host);
        // These identities belong only to this test, never to an installed app.
        let digest = Sha256::digest(format!("jadix.notifications:{identifier}"));
        let mut bytes: [u8; 16] = digest[..16].try_into().unwrap();
        bytes[6] = (bytes[6] & 15) | 0x50; bytes[8] = (bytes[8] & 63) | 0x80;
        let guid = windows::core::GUID::from_u128(u128::from_be_bytes(bytes));
        for path in [format!("Software\\Classes\\AppUserModelId\\{identifier}"), format!("Software\\Classes\\CLSID\\{{{guid:?}}}")] {
            unsafe { RegDeleteTreeW(HKEY_CURRENT_USER, &HSTRING::from(path)) }.ok().unwrap();
        }
    }
    #[test]
    fn protocol_registration_preserves_other_owners_and_quotes_windows_arguments() {
        let scheme = format!("jadix-activation-{}", std::process::id());
        let mut config = ApplicationActivation { identifier: "dev.jadix.test".into(), arguments: vec!["app entry.tsx".into(), "quote\"slash\\".into()], deep_links: jadix_core::deep_link::DeepLinkConfig { schemes: BTreeSet::from([scheme.clone()]), ..Default::default() }, file_extensions: Default::default() };
        protocols(&config, true).unwrap();
        let path = format!("Software\\Classes\\{scheme}");
        assert_eq!(read(&format!("{path}\\shell\\open\\command"), None).unwrap(), Some(command(&config.arguments, "%1").unwrap()));
        config.identifier = "dev.other.owner".into();
        assert!(protocols(&config, true).is_err());
        protocols(&config, false).unwrap();
        assert!(read(&path, Some("URL Protocol")).unwrap().is_some());
        config.identifier = "dev.jadix.test".into(); protocols(&config, false).unwrap();
        assert!(read(&path, Some("URL Protocol")).unwrap().is_none());
        assert_eq!(quote("a\\\"b\\"), "\"a\\\\\\\"b\\\\\"");
        config.deep_links.schemes = BTreeSet::from(["https".into()]);
        assert!(protocols(&config, true).is_err());
    }
}

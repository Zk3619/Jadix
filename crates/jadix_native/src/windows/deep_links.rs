use super::*;
use jadix_core::deep_link::{DeepLink, DeepLinkConfig, DeepLinkRouter, DeepLinkSource, MAX_DEEP_LINK_BYTES};
use windows_sys::Win32::System::{DataExchange::COPYDATASTRUCT, Threading::{OpenProcess, QueryFullProcessImageNameW, PROCESS_QUERY_LIMITED_INFORMATION}};

const MESSAGE: usize = 0x4a44584c;

pub(super) struct Links {
    pub router: DeepLinkRouter, configured: bool,
    pub activation: Option<super::activation::Registration>,
    pub launches: VecDeque<jadix_core::deep_link::ApplicationLaunch>,
}
impl Default for Links {
    fn default() -> Self { Self { router: DeepLinkRouter::new(DeepLinkConfig::default()).expect("valid default deep links"), configured: false, activation: None, launches: Default::default() } }
}
impl Links {
    pub fn configure_activation(&mut self, config: &jadix_core::deep_link::ApplicationActivation, launch: &jadix_core::deep_link::ApplicationLaunch, desktop: &jadix_core::desktop_services::DesktopServices) -> Result<(), String> {
        self.router.configure(config.deep_links.clone()).map_err(String::from)?;
        self.configured = true;
        self.deliver_launch(config, launch, DeepLinkSource::ColdStart, desktop);
        Ok(())
    }
    pub fn deliver_launch(&mut self, config: &jadix_core::deep_link::ApplicationActivation, launch: &jadix_core::deep_link::ApplicationLaunch, source: DeepLinkSource, desktop: &jadix_core::desktop_services::DesktopServices) {
        for link in config.launch_links(launch, source).unwrap_or_default() {
            if !desktop.complete_auth_callback(&link.url) { let _ = self.router.accept_application_link(config, link); }
        }
    }
    pub fn configure(&mut self, config: DeepLinkConfig, desktop: &jadix_core::desktop_services::DesktopServices) -> Result<(), String> {
        if self.activation.as_ref().is_some_and(|activation| activation.config.deep_links != config) {
            return Err("application activation owns the deep-link configuration".into());
        }
        self.router.configure(config).map_err(String::from)?;
        if !self.configured {
            self.configured = true;
            for argument in std::env::args_os().skip(1).take(64) {
                if let Some(url) = argument.to_str().filter(|s| s.len() <= MAX_DEEP_LINK_BYTES) {
                    let _ = self.deliver(DeepLink { url: url.into(), source: DeepLinkSource::ColdStart }, desktop);
                }
            }
        }
        Ok(())
    }
    pub fn deliver(&mut self, link: DeepLink, desktop: &jadix_core::desktop_services::DesktopServices) -> Result<(), String> {
        link.validate(&self.router.config().schemes).map_err(String::from)?;
        if !desktop.complete_auth_callback(&link.url) { self.router.accept(link).map_err(String::from)?; }
        Ok(())
    }
}

/// WM_COPYDATA owns the cross-process copy for the duration of this call. Never
/// retain the sender's address, and reject unbounded/non-UTF-8 payloads first.
pub(super) unsafe fn receive(context: &WindowContext, data: LPARAM) -> LRESULT {
    if data == 0 || context.closed.get() { return 0; }
    let data = unsafe { &*(data as *const COPYDATASTRUCT) };
    if data.dwData == super::activation::MESSAGE { return unsafe { super::activation::receive(context, data) }; }
    if data.dwData != MESSAGE || data.cbData == 0 || data.cbData as usize > MAX_DEEP_LINK_BYTES || data.lpData.is_null() { return 0; }
    let bytes = unsafe { std::slice::from_raw_parts(data.lpData.cast::<u8>(), data.cbData as usize) };
    let Ok(url) = std::str::from_utf8(bytes) else { return 0; };
    let Ok(mut links) = context.deep_links.try_borrow_mut() else { return 0; };
    if !links.configured || !links.router.config().single_instance { return 0; }
    if links.deliver(DeepLink { url: url.into(), source: DeepLinkSource::Handoff }, &context.desktop).is_err() { return 0; }
    unsafe { PostMessageW(context.root.get(), WM_NULL, 0, 0); }
    1
}

fn process_image(window: HWND) -> Option<Vec<u16>> {
    let mut pid = 0;
    unsafe { GetWindowThreadProcessId(window, &mut pid); }
    let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if process.is_null() { return None; }
    let mut path = vec![0u16; 32768];
    let mut size = path.len() as u32;
    let success = unsafe { QueryFullProcessImageNameW(process, 0, path.as_mut_ptr(), &mut size) };
    unsafe { CloseHandle(process); }
    if success == 0 { return None; }
    path.truncate(size as usize); Some(path)
}

struct Handoff<'a> { url: &'a str, image: Vec<u16>, exclude: HWND, delivered: bool, examined: usize }
unsafe extern "system" fn visit(window: HWND, parameter: LPARAM) -> i32 {
    let state = unsafe { &mut *(parameter as *mut Handoff<'_>) };
    if window == state.exclude { return 1; }
    let mut class = [0u16; 64];
    let size = unsafe { GetClassNameW(window, class.as_mut_ptr(), class.len() as i32) };
    if class[..size.max(0) as usize] != wide("JadixNativeHost")[..15] { return 1; }
    state.examined += 1;
    if state.examined > 256 { return 0; }
    if process_image(window).as_ref() != Some(&state.image) { return 1; }
    let data = COPYDATASTRUCT { dwData: MESSAGE, cbData: state.url.len() as u32, lpData: state.url.as_ptr().cast_mut().cast() };
    let mut result = 0;
    if unsafe { SendMessageTimeoutW(window, WM_COPYDATA, 0, &data as *const _ as LPARAM,
        SMTO_ABORTIFHUNG | SMTO_BLOCK, 500, &mut result) } != 0 && result == 1 {
        state.delivered = true;
        unsafe { ShowWindow(window, SW_RESTORE); SetForegroundWindow(window); }
        return 0;
    }
    1
}

pub(super) fn forward(config: &DeepLinkConfig, url: &str, exclude: HWND) -> Result<bool, String> {
    config.validate().map_err(String::from)?;
    DeepLink { url: url.into(), source: DeepLinkSource::Handoff }.validate(&config.schemes).map_err(String::from)?;
    if !config.single_instance { return Ok(false); }
    // Compare OS-reported executable paths on both sides, including embedded hosts.
    let image = (|| {
        let process = unsafe { windows_sys::Win32::System::Threading::GetCurrentProcess() };
        let mut path = vec![0u16; 32768]; let mut size = path.len() as u32;
        if unsafe { QueryFullProcessImageNameW(process, 0, path.as_mut_ptr(), &mut size) } == 0 { None }
        else { path.truncate(size as usize); Some(path) }
    })().ok_or("cannot identify the running executable")?;
    let mut state = Handoff { url, image, exclude, delivered: false, examined: 0 };
    unsafe { EnumWindows(Some(visit), &mut state as *mut _ as LPARAM); }
    Ok(state.delivered)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn window_handoff_validates_routes_bounds_queue_and_closes_auth_once() {
        use jadix_core::desktop_services::{AuthSessionOutcome, AuthSessionRequest, ServiceReply};
        let mut host = WindowsHost::new("Jadix protocol activation", [320, 240]).unwrap();
        // Unique per test process: other parallel test roots cannot consume it.
        let scheme = format!("jadix-test-{}", std::process::id());
        let config = DeepLinkConfig { schemes: BTreeSet::from([scheme.clone()]), queue_limit: 2, ..Default::default() };
        host.configure_deep_links(config.clone()).unwrap();
        assert!(forward(&config, "other://rejected", null_mut()).is_err());
        for item in 0..3 { assert!(forward(&config, &format!("{scheme}://open/消息/{item}"), null_mut()).unwrap()); }
        let links = host.take_deep_links();
        assert_eq!(links.len(), 2);
        assert!(links[0].url.ends_with("/1"));
        assert_eq!(links[1].source, DeepLinkSource::Handoff);
        let desktop = host.context.desktop.clone();
        let session = desktop.open_auth_session(AuthSessionRequest { url: "https://example.com/login".into(), callback_scheme: scheme.clone(), ephemeral: false }).unwrap();
        desktop.take_commands(); // Exercise callback receipt without launching an external browser.
        let callback = format!("{scheme}://callback?state=accepted");
        assert!(forward(&config, &callback, null_mut()).unwrap());
        assert!(host.take_deep_links().is_empty());
        assert_eq!(session.reply(), Some(ServiceReply::AuthSession { outcome: Some(AuthSessionOutcome::Completed { url: callback }), error: None }));
        let pending = desktop.open_auth_session(AuthSessionRequest { url: "https://example.com/login".into(), callback_scheme: scheme.clone(), ephemeral: false }).unwrap();
        desktop.take_commands(); desktop.close_auth_session();
        assert!(forward(&config, &format!("{scheme}://open/after-cancel"), null_mut()).unwrap());
        assert_eq!(host.take_deep_links().len(), 1);
        assert!(matches!(pending.reply(), Some(ServiceReply::AuthSession { outcome: Some(AuthSessionOutcome::Cancelled), .. })));
        let mut multiple = config.clone(); multiple.single_instance = false;
        assert!(!forward(&multiple, &format!("{scheme}://open"), null_mut()).unwrap());
        host.dispose();
        assert!(!forward(&config, &format!("{scheme}://open"), null_mut()).unwrap());
        assert!(host.deliver_deep_link(DeepLink { url: format!("{scheme}://open"), source: DeepLinkSource::HotStart }).is_err());
    }
}

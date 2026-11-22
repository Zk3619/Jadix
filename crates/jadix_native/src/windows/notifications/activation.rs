//! Unpackaged toast activation. The OS retains the bounded launch payload so a
//! new process can deliver a click without relying on an old root's handle IDs.
use super::{Pending, NotificationActivation};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use sha2::{Digest, Sha256};
use std::{ffi::c_void, sync::{Arc, atomic::{AtomicU64, Ordering}}};
use windows::{
    core::{implement, GUID, HSTRING, IUnknown, Interface, BOOL, PCWSTR, Ref},
    Win32::{
        Foundation::{CLASS_E_NOAGGREGATION, E_INVALIDARG, RPC_E_CHANGED_MODE},
        System::{Com::{CoInitializeEx, CoUninitialize, CoRegisterClassObject, CoRevokeClassObject, IClassFactory, IClassFactory_Impl, CLSCTX_LOCAL_SERVER, COINIT_APARTMENTTHREADED, REGCLS_MULTIPLEUSE}, Registry::{RegSetKeyValueW, HKEY_CURRENT_USER, REG_SZ}},
        UI::Notifications::{INotificationActivationCallback, INotificationActivationCallback_Impl, NOTIFICATION_USER_INPUT_DATA},
    },
};

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Payload { pub token: String, pub id: u32, pub data: String }

pub(super) fn token(id: u32) -> String {
    static SERIAL: AtomicU64 = AtomicU64::new(0);
    let time = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos();
    format!("{:x}-{time:x}-{:x}-{id:x}", std::process::id(), SERIAL.fetch_add(1, Ordering::Relaxed))
}
pub(super) fn encode(token: &str, id: u32, data: &str) -> String {
    format!("jadix-toast:1:{}", URL_SAFE_NO_PAD.encode(serde_json::to_vec(&Payload { token: token.into(), id, data: data.into() }).expect("string payload serialization")))
}
fn decode(arguments: &str) -> Option<Payload> {
    if arguments.len() > 16_384 { return None; }
    let bytes = URL_SAFE_NO_PAD.decode(arguments.strip_prefix("jadix-toast:1:")?).ok()?;
    let payload: Payload = serde_json::from_slice(&bytes).ok()?;
    let activation = NotificationActivation { identifier: payload.id.to_string(), data: payload.data.clone() };
    (payload.token.len() <= 128 && !payload.token.is_empty() && !payload.token.chars().any(char::is_control)
        && activation.data.len() <= 4096 && !activation.data.contains('\0')).then_some(payload)
}

struct Apartment(bool);
impl Drop for Apartment { fn drop(&mut self) { if self.0 { unsafe { CoUninitialize() }; } } }
pub(super) struct Registration { cookie: u32, _apartment: Apartment }
impl Registration {
    pub fn new(identity: &str, pending: Pending, wake: Arc<dyn Fn() + Send + Sync>) -> Result<Self, String> {
        Self::with_arguments(identity, &[], pending, wake)
    }
    pub fn with_arguments(identity: &str, arguments: &[String], pending: Pending, wake: Arc<dyn Fn() + Send + Sync>) -> Result<Self, String> {
        jadix_core::deep_link::ApplicationLaunch { arguments: arguments.to_vec(), working_directory: None }.validate().map_err(String::from)?;
        let initialized = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
        if initialized.is_err() && initialized != RPC_E_CHANGED_MODE { return Err(initialized.to_string()); }
        let apartment = Apartment(initialized.is_ok());
        let guid = class_id(identity);
        let factory: IClassFactory = Factory { app_id: identity.into(), pending, wake }.into();
        let cookie = unsafe { CoRegisterClassObject(&guid, &factory, CLSCTX_LOCAL_SERVER, REGCLS_MULTIPLEUSE) }.map_err(|e| e.to_string())?;
        let registration = Self { cookie, _apartment: apartment };
        let class = format!("{{{guid:?}}}");
        let app_key = format!("Software\\Classes\\AppUserModelId\\{identity}");
        let command = super::super::activation::command(arguments, "-JadixToastActivated")?;
        set_value(&app_key, Some("DisplayName"), identity).map_err(|e| e.to_string())?;
        set_value(&app_key, Some("CustomActivator"), &class).map_err(|e| e.to_string())?;
        set_value(&format!("Software\\Classes\\CLSID\\{class}\\LocalServer32"), None, &command).map_err(|e| e.to_string())?;
        Ok(registration)
    }
}
impl Drop for Registration {
    fn drop(&mut self) { let _ = unsafe { CoRevokeClassObject(self.cookie) }; }
}
fn class_id(identity: &str) -> GUID {
    let digest = Sha256::digest(format!("jadix.notifications:{identity}"));
    let mut bytes: [u8; 16] = digest[..16].try_into().unwrap();
    bytes[6] = (bytes[6] & 0x0f) | 0x50;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    GUID::from_u128(u128::from_be_bytes(bytes))
}
fn set_value(key: &str, name: Option<&str>, value: &str) -> windows::core::Result<()> {
    let data: Vec<u16> = value.encode_utf16().chain(Some(0)).collect();
    let name = name.map(HSTRING::from);
    unsafe { RegSetKeyValueW(HKEY_CURRENT_USER, &HSTRING::from(key), name.as_ref().map_or(PCWSTR::null(), |name| PCWSTR(name.as_ptr())), REG_SZ.0, Some(data.as_ptr().cast()), (data.len() * 2) as u32) }.ok()
}

#[implement(IClassFactory)]
struct Factory { app_id: String, pending: Pending, wake: Arc<dyn Fn() + Send + Sync> }
impl IClassFactory_Impl for Factory_Impl {
    fn CreateInstance(&self, outer: Ref<IUnknown>, interface: *const GUID, object: *mut *mut c_void) -> windows::core::Result<()> {
        if outer.is_some() { return Err(CLASS_E_NOAGGREGATION.into()); }
        if interface.is_null() || object.is_null() { return Err(E_INVALIDARG.into()); }
        let activator: INotificationActivationCallback = Activator { app_id: self.app_id.clone(), pending: self.pending.clone(), wake: self.wake.clone() }.into();
        unsafe { activator.query(interface, object) }.ok()
    }
    fn LockServer(&self, _lock: BOOL) -> windows::core::Result<()> { Ok(()) }
}
#[implement(INotificationActivationCallback)]
struct Activator { app_id: String, pending: Pending, wake: Arc<dyn Fn() + Send + Sync> }
impl INotificationActivationCallback_Impl for Activator_Impl {
    fn Activate(&self, app: &PCWSTR, arguments: &PCWSTR, _data: *const NOTIFICATION_USER_INPUT_DATA, _count: u32) -> windows::core::Result<()> {
        // Both strings belong to COM for this call. Bound the copy before parsing.
        let Some(app) = (unsafe { bounded_string(*app, 128) }) else { return Ok(()); };
        if app != self.app_id { return Ok(()); }
        let Some(arguments) = (unsafe { bounded_string(*arguments, 16_384) }) else { return Ok(()); };
        let Some(payload) = decode(&arguments) else { return Ok(()); };
        let mut queue = self.pending.lock().unwrap_or_else(|e| e.into_inner());
        if queue.closed || queue.app_id.as_deref() != Some(self.app_id.as_str()) || queue.activations.len() >= 64 { return Ok(()); }
        queue.activations.push_back(payload);
        (self.wake)();
        Ok(())
    }
}
unsafe fn bounded_string(value: PCWSTR, limit: usize) -> Option<String> {
    if value.is_null() { return None; }
    for length in 0..=limit {
        if unsafe { *value.0.add(length) } == 0 {
            return String::from_utf16(unsafe { std::slice::from_raw_parts(value.0, length) }).ok();
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn registered_local_server_creates_a_real_com_activation_callback() {
        use windows::Win32::System::{Com::CoCreateInstance, Registry::RegDeleteTreeW};
        let identity = format!("dev.jadix.activation.test.{}", token(0));
        let pending = Pending::default();
        pending.lock().unwrap().app_id = Some(identity.clone());
        let registration = Registration::new(&identity, pending.clone(), Arc::new(|| {})).unwrap();
        let callback: INotificationActivationCallback = unsafe { CoCreateInstance(&class_id(&identity), None, CLSCTX_LOCAL_SERVER) }.unwrap();
        unsafe { callback.Activate(&HSTRING::from(&identity), &HSTRING::from(encode(&token(9), 9, "registered activation 中文")), &[]).unwrap(); }
        assert_eq!(pending.lock().unwrap().activations.pop_front().unwrap().data, "registered activation 中文");
        drop(callback);
        drop(registration);
        // These two keys have a unique test-only identity; leave application registrations alone.
        unsafe {
            RegDeleteTreeW(HKEY_CURRENT_USER, &HSTRING::from(format!("Software\\Classes\\AppUserModelId\\{identity}"))).ok().unwrap();
            RegDeleteTreeW(HKEY_CURRENT_USER, &HSTRING::from(format!("Software\\Classes\\CLSID\\{{{:?}}}", class_id(&identity)))).ok().unwrap();
        }
    }
    #[test]
    fn com_callback_filters_identity_and_stops_after_disposal() {
        let pending = Pending::default();
        pending.lock().unwrap().app_id = Some("dev.jadix.test".into());
        let wakes = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let count = wakes.clone();
        let callback: INotificationActivationCallback = Activator { app_id: "dev.jadix.test".into(), pending: pending.clone(), wake: Arc::new(move || { count.fetch_add(1, Ordering::SeqCst); }) }.into();
        let app = HSTRING::from("dev.jadix.test");
        let wrong = HSTRING::from("dev.jadix.other");
        let launch = HSTRING::from(encode(&token(7), 7, "cold start"));
        unsafe {
            callback.Activate(&wrong, &launch, &[]).unwrap();
            callback.Activate(&app, &launch, &[]).unwrap();
        }
        assert_eq!(wakes.load(Ordering::SeqCst), 1);
        assert_eq!(pending.lock().unwrap().activations.pop_front().unwrap().data, "cold start");
        pending.lock().unwrap().closed = true;
        unsafe { callback.Activate(&app, &launch, &[]).unwrap(); }
        assert_eq!(wakes.load(Ordering::SeqCst), 1);
        assert!(pending.lock().unwrap().activations.is_empty());
    }
    #[test]
    fn activation_round_trip_survives_process_state_and_rejects_invalid_payloads() {
        let token = token(42);
        let encoded = encode(&token, 42, "中文 <&>\n🙂");
        let payload = decode(&encoded).unwrap();
        assert_eq!((payload.token.as_str(), payload.id, payload.data.as_str()), (token.as_str(), 42, "中文 <&>\n🙂"));
        assert_ne!(class_id("dev.jadix.a"), class_id("dev.jadix.b"));
        assert_eq!(class_id("dev.jadix.a"), class_id("dev.jadix.a"));
        assert!(decode("jadix-toast:2:e30").is_none());
        assert!(decode(&encode("x", 42, &"x".repeat(4097))).is_none());
        assert!(decode(&encode("x", 42, "a\0b")).is_none());
        assert!(decode(&encode("", 42, "")).is_none());
    }
}

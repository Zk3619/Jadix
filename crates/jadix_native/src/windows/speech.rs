//! SAPI dictation for unpackaged native applications. COM and microphone work
//! stay on one lazy worker; the shared service owns session identity/lifetime.
use jadix_core::speech::{SpeechCommand, SpeechError, SpeechEvent, SpeechRequest, SpeechService};
use std::{collections::VecDeque, rc::Rc, sync::{Arc, Mutex, mpsc}, time::{Duration, Instant}};
use windows::{core::{Interface, IUnknown, PCWSTR, PWSTR}, Win32::{Globalization::LocaleNameToLCID,
    Media::Speech::*, System::Com::{CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED}}};

type Wake = Arc<dyn Fn() + Send + Sync>;
#[derive(Default)]
struct Callbacks { active: u32, closed: bool, events: VecDeque<(u32, SpeechEvent)> }
type Queue = Arc<Mutex<Callbacks>>;
fn active(queue: &Queue, id: u32) -> bool { let q = queue.lock().unwrap(); !q.closed && q.active == id }
fn emit(queue: &Queue, wake: &Wake, id: u32, event: SpeechEvent) {
    let mut q = queue.lock().unwrap();
    if q.closed || q.active != id { return; }
    if matches!(event, SpeechEvent::Result { .. }) && matches!(q.events.back(), Some((_, SpeechEvent::Result { is_final: false, .. }))) { q.events.pop_back(); }
    if q.events.len() >= 64 {
        // Final transcripts must never disappear silently under backpressure.
        q.events.clear(); q.active = 0;
        q.events.push_back((id, SpeechEvent::End { error: Some(SpeechError::Failed) }));
    } else { q.events.push_back((id, event)); }
    drop(q); wake();
}

pub(super) struct Speech {
    pub service: SpeechService,
    queue: Queue,
    wake: Wake,
    send: Option<mpsc::SyncSender<SpeechCommand>>,
}
impl Speech {
    pub fn new(wake: impl Fn() + Send + Sync + 'static) -> Self {
        let service = SpeechService::new(); let wake: Wake = Arc::new(wake);
        let command_wake = wake.clone(); service.set_command_waker(Some(Rc::new(move || command_wake())));
        Self { service, queue: Default::default(), wake, send: None }
    }
    pub fn pump(&mut self) {
        for command in self.service.take_commands() {
            let id = match &command { SpeechCommand::Start { id, .. } | SpeechCommand::Stop { id } | SpeechCommand::Abort { id } => *id };
            {
                let mut q = self.queue.lock().unwrap();
                if q.closed { return; }
                match &command {
                    SpeechCommand::Start { .. } => { q.active = id; q.events.clear(); }
                    SpeechCommand::Abort { .. } if q.active == id => { q.active = 0; q.events.clear(); }
                    _ => {}
                }
            }
            if self.send.is_none() && matches!(command, SpeechCommand::Start { .. }) {
                let (send, receive) = mpsc::sync_channel(64);
                let (queue, wake) = (self.queue.clone(), self.wake.clone());
                if std::thread::Builder::new().name("jadix-win-speech".into()).spawn(move || worker(receive, queue, wake)).is_ok() { self.send = Some(send); }
            }
            if self.send.as_ref().is_none_or(|s| s.try_send(command).is_err()) {
                self.queue.lock().unwrap().active = 0;
                self.service.event(id, SpeechEvent::End { error: Some(SpeechError::Unavailable) });
            }
        }
        let events: Vec<_> = self.queue.lock().unwrap().events.drain(..).collect();
        for (id, event) in events { self.service.event(id, event); }
    }
    pub fn dispose(&mut self) {
        self.service.dispose(); let mut q = self.queue.lock().unwrap();
        q.closed = true; q.active = 0; q.events.clear(); self.send.take();
    }
}
impl Drop for Speech { fn drop(&mut self) { self.dispose(); } }

struct ComRuntime;
impl ComRuntime {
    fn new() -> windows::core::Result<Self> { unsafe { CoInitializeEx(None, COINIT_MULTITHREADED).ok()?; } Ok(Self) }
}
impl Drop for ComRuntime { fn drop(&mut self) { unsafe { CoUninitialize(); } } }
struct Text(PWSTR);
impl Drop for Text { fn drop(&mut self) { unsafe { CoTaskMemFree(Some(self.0.0.cast())); } } }
struct Event(SPEVENT);
impl Event {
    fn kind(&self) -> SPEVENTENUM { SPEVENTENUM(self.0._bitfield & 0xffff) }
    fn parameter_kind(&self) -> SPEVENTLPARAMTYPE { SPEVENTLPARAMTYPE((self.0._bitfield as u32 >> 16) as i32) }
    fn text(&self) -> windows::core::Result<String> { unsafe {
        if self.parameter_kind() != SPET_LPARAM_IS_OBJECT || self.0.lParam.0 == 0 { return Err(windows::core::Error::from_hresult(windows::core::HRESULT(0x80004005u32 as i32))); }
        let object = std::mem::ManuallyDrop::new(IUnknown::from_raw(self.0.lParam.0 as *mut _));
        let result: ISpRecoResult = object.cast()?;
        let mut text = Text(PWSTR::null()); result.GetText(0, u32::MAX, true, &mut text.0, None)?;
        // Bound the read and conversion even for a misbehaving installed engine.
        let pointer = text.0.0;
        if pointer.is_null() { return Ok(String::new()); }
        let length = (0..=32768).find(|i| *pointer.add(*i) == 0).ok_or_else(|| windows::core::Error::from_hresult(windows::core::HRESULT(0x80070057u32 as i32)))?;
        String::from_utf16(std::slice::from_raw_parts(pointer, length)).map_err(|_| windows::core::Error::from_hresult(windows::core::HRESULT(0x80070057u32 as i32)))
    } }
}
impl Drop for Event {
    fn drop(&mut self) { unsafe {
        let raw = self.0.lParam.0 as *mut std::ffi::c_void;
        if raw.is_null() { return; }
        match self.parameter_kind() {
            SPET_LPARAM_IS_OBJECT | SPET_LPARAM_IS_TOKEN => { drop(IUnknown::from_raw(raw)); }
            SPET_LPARAM_IS_POINTER | SPET_LPARAM_IS_STRING => CoTaskMemFree(Some(raw)),
            _ => {}
        }
    } }
}
fn classify(error: &windows::core::Error, fallback: SpeechError) -> SpeechError {
    match error.code().0 as u32 { 0x80070005 => SpeechError::NotAllowed, 0x80040154 => SpeechError::Unavailable, _ => fallback }
}
fn language_filter(language: &str) -> Result<String, SpeechError> {
    let wide: Vec<_> = language.encode_utf16().chain(Some(0)).collect();
    let locale = unsafe { LocaleNameToLCID(PCWSTR(wide.as_ptr()), 0) };
    if locale == 0 { Err(SpeechError::LanguageNotSupported) } else { Ok(format!("Language={:X}", locale & 0xffff)) }
}
fn recognizer_token(language: Option<&str>) -> Result<ISpObjectToken, SpeechError> { unsafe {
    let category: ISpObjectTokenCategory = CoCreateInstance(&SpObjectTokenCategory, None, CLSCTX_INPROC_SERVER).map_err(|e| classify(&e, SpeechError::Unavailable))?;
    category.SetId(SPCAT_RECOGNIZERS, false).map_err(|e| classify(&e, SpeechError::Unavailable))?;
    if let Some(language) = language {
        let filter: Vec<_> = language_filter(language)?.encode_utf16().chain(Some(0)).collect();
        category.EnumTokens(PCWSTR(filter.as_ptr()), PCWSTR::null()).and_then(|tokens| tokens.Item(0)).map_err(|e| classify(&e, SpeechError::LanguageNotSupported))
    } else {
        if let Ok(default) = category.GetDefaultTokenId() {
            let default = Text(default);
            let token: ISpObjectToken = CoCreateInstance(&SpObjectToken, None, CLSCTX_INPROC_SERVER).map_err(|e| classify(&e, SpeechError::Unavailable))?;
            token.SetId(PCWSTR::null(), PCWSTR(default.0.0), false).map_err(|e| classify(&e, SpeechError::Unavailable))?; return Ok(token);
        }
        category.EnumTokens(PCWSTR::null(), PCWSTR::null()).and_then(|tokens| tokens.Item(0)).map_err(|e| classify(&e, SpeechError::Unavailable))
    }
} }

struct Recognition { recognizer: ISpRecognizer, context: ISpRecoContext, _grammar: ISpRecoGrammar, interim: bool, started: Instant, stopping: Option<Instant>, heard: bool }
impl Recognition {
    fn new(request: &SpeechRequest, input: Option<&IUnknown>) -> Result<Self, SpeechError> { unsafe {
        let token = recognizer_token(request.lang.as_deref())?;
        let recognizer: ISpRecognizer = CoCreateInstance(&SpInprocRecognizer, None, CLSCTX_INPROC_SERVER).map_err(|e| classify(&e, SpeechError::Unavailable))?;
        recognizer.SetRecognizer(&token).map_err(|e| classify(&e, SpeechError::Unavailable))?;
        let microphone: IUnknown;
        let input = if let Some(input) = input { input } else {
            microphone = CoCreateInstance(&SpMMAudioIn, None, CLSCTX_INPROC_SERVER).map_err(|e| classify(&e, SpeechError::AudioCapture))?; &microphone
        };
        recognizer.SetInput(input, true).map_err(|e| classify(&e, SpeechError::AudioCapture))?;
        let context = recognizer.CreateRecoContext().map_err(|e| classify(&e, SpeechError::Failed))?;
        context.SetNotifyWin32Event().map_err(|e| classify(&e, SpeechError::Failed))?;
        let interest = [SPEI_RESERVED1, SPEI_RESERVED2, SPEI_RECOGNITION, SPEI_HYPOTHESIS, SPEI_END_SR_STREAM].iter().fold(0u64, |mask, event| mask | (1u64 << event.0));
        context.SetInterest(interest, interest).map_err(|e| classify(&e, SpeechError::Failed))?;
        let grammar = context.CreateGrammar(1).map_err(|e| classify(&e, SpeechError::Failed))?;
        grammar.LoadDictation(PCWSTR::null(), SPLO_STATIC).map_err(|e| classify(&e, SpeechError::LanguageNotSupported))?;
        let result = Self { recognizer, context, _grammar: grammar, interim: request.interim, started: Instant::now(), stopping: None, heard: false };
        result._grammar.SetDictationState(SPRS_ACTIVE).map_err(|e| classify(&e, SpeechError::AudioCapture))?;
        result.recognizer.SetRecoState(SPRST_ACTIVE).map_err(|e| classify(&e, SpeechError::AudioCapture))?;
        Ok(result)
    } }
    fn stop(&mut self) -> Result<(), SpeechError> {
        if self.stopping.is_none() {
            unsafe { self.recognizer.SetRecoState(SPRST_INACTIVE).map_err(|e| classify(&e, SpeechError::Failed))?; }
            self.stopping = Some(Instant::now());
        } Ok(())
    }
    fn poll(&mut self, mut deliver: impl FnMut(SpeechEvent)) -> Result<bool, SpeechError> {
        for _ in 0..64 {
            let mut event = Event(SPEVENT::default()); let mut fetched = 0;
            unsafe { self.context.GetEvents(1, &mut event.0, &mut fetched).map_err(|e| classify(&e, SpeechError::Failed))?; }
            if fetched == 0 { break; }
            match event.kind() {
                SPEI_RECOGNITION | SPEI_HYPOTHESIS => {
                    let is_final = event.kind() == SPEI_RECOGNITION;
                    if is_final || self.interim {
                        let text = event.text().map_err(|e| classify(&e, SpeechError::Failed))?;
                        if text.len() > 64 * 1024 { return Err(SpeechError::Failed); }
                        if !text.is_empty() { self.heard |= is_final; deliver(SpeechEvent::Result { text, is_final }); }
                    }
                }
                SPEI_END_SR_STREAM => {
                    let status = windows::core::HRESULT(event.0.lParam.0 as i32);
                    if status.is_err() { return Err(classify(&windows::core::Error::from_hresult(status), SpeechError::AudioCapture)); }
                    if !self.heard && self.stopping.is_none() { return Err(SpeechError::NoSpeech); }
                    return Ok(true);
                }
                _ => {}
            }
        }
        if self.stopping.is_some_and(|time| time.elapsed() >= Duration::from_secs(5)) { return Ok(true); }
        if self.started.elapsed() >= Duration::from_secs(60) { self.stop()?; }
        Ok(false)
    }
}
impl Drop for Recognition { fn drop(&mut self) { unsafe { let _ = self.recognizer.SetRecoState(SPRST_INACTIVE_WITH_PURGE); } } }

fn worker(receive: mpsc::Receiver<SpeechCommand>, queue: Queue, wake: Wake) {
    let runtime = ComRuntime::new(); let mut current: Option<(u32, Recognition)> = None;
    loop {
        if current.as_ref().is_some_and(|(id, _)| !active(&queue, *id)) { current = None; }
        let command = if current.is_some() { receive.recv_timeout(Duration::from_millis(20)) } else { receive.recv().map_err(|_| mpsc::RecvTimeoutError::Disconnected) };
        match command {
            Ok(SpeechCommand::Start { id, request }) if active(&queue, id) => {
                current = None;
                let result = match &runtime { Ok(_) => Recognition::new(&request, None), Err(e) => Err(classify(e, SpeechError::Unavailable)) };
                match result {
                    Ok(recognition) if active(&queue, id) => { emit(&queue, &wake, id, SpeechEvent::Start); current = Some((id, recognition)); }
                    Err(error) => emit(&queue, &wake, id, SpeechEvent::End { error: Some(error) }),
                    _ => {}
                }
            }
            Ok(SpeechCommand::Stop { id }) => {
                if let Some((owner, recognition)) = &mut current { if *owner == id { if let Err(error) = recognition.stop() { emit(&queue, &wake, id, SpeechEvent::End { error: Some(error) }); current = None; } } }
            }
            Ok(SpeechCommand::Abort { id }) => { if current.as_ref().is_some_and(|(owner, _)| *owner == id) { current = None; } }
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
            _ => {}
        }
        if let Some((id, recognition)) = &mut current {
            let result = recognition.poll(|event| emit(&queue, &wake, *id, event));
            if result != Ok(false) { emit(&queue, &wake, *id, SpeechEvent::End { error: result.err() }); current = None; }
        }
    }
    // Drop COM interfaces before the apartment, including disconnect/dispose.
    drop(current); drop(runtime);
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bounded_callbacks_preserve_final_or_report_overflow_and_reject_cancelled_ids() {
        let queue: Queue = Default::default(); let wake: Wake = Arc::new(|| {}); queue.lock().unwrap().active = 1;
        for n in 0..1000 { emit(&queue, &wake, 1, SpeechEvent::Result { text: n.to_string(), is_final: false }); }
        assert_eq!(queue.lock().unwrap().events.len(), 1);
        for _ in 0..100 { emit(&queue, &wake, 1, SpeechEvent::Result { text: "final".into(), is_final: true }); }
        assert_eq!(queue.lock().unwrap().events.back(), Some(&(1, SpeechEvent::End { error: Some(SpeechError::Failed) })));
        emit(&queue, &wake, 1, SpeechEvent::Start); assert_eq!(queue.lock().unwrap().events.len(), 1);
    }
    #[test]
    fn unsupported_language_reaches_shared_service_and_abort_discards_pending_work() {
        let mut speech = Speech::new(|| {});
        let first = speech.service.start(SpeechRequest { lang: Some("zz-ZZ".into()), interim: true }).unwrap(); speech.pump();
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            speech.pump(); let events = first.events();
            if !events.is_empty() { assert_eq!(events, vec![SpeechEvent::End { error: Some(SpeechError::LanguageNotSupported) }]); break; }
            assert!(Instant::now() < deadline); std::thread::sleep(Duration::from_millis(10));
        }
        let second = speech.service.start(Default::default()).unwrap(); second.abort(); speech.pump();
        assert_eq!(second.events(), vec![SpeechEvent::End { error: None }]);
        speech.dispose(); assert!(speech.service.start(Default::default()).is_err());
    }
    #[test]
    #[ignore = "requires installed English SAPI voice and dictation engine"]
    fn installed_engine_recognizes_native_synthesized_wave_without_a_microphone() {
        let _runtime = ComRuntime::new().unwrap();
        let path = std::env::temp_dir().join(format!("jadix-speech-{}.wav", std::process::id()));
        struct Remove(std::path::PathBuf);
        impl Drop for Remove { fn drop(&mut self) { let _ = std::fs::remove_file(&self.0); } }
        let _remove = Remove(path.clone());
        let path: Vec<_> = path.to_string_lossy().encode_utf16().chain(Some(0)).collect();
        let input: IUnknown = unsafe {
            let voice: ISpVoice = CoCreateInstance(&SpVoice, None, CLSCTX_INPROC_SERVER).unwrap();
            let category: ISpObjectTokenCategory = CoCreateInstance(&SpObjectTokenCategory, None, CLSCTX_INPROC_SERVER).unwrap();
            category.SetId(SPCAT_VOICES, false).unwrap();
            let tokens = category.EnumTokens(windows::core::w!("Language=409"), PCWSTR::null()).unwrap();
            voice.SetVoice(&tokens.Item(0).unwrap()).unwrap();
            let output: ISpStream = CoCreateInstance(&SpStream, None, CLSCTX_INPROC_SERVER).unwrap();
            let format = windows::Win32::Media::Audio::WAVEFORMATEX { wFormatTag: 1, nChannels: 1, nSamplesPerSec: 22050, nAvgBytesPerSec: 44100, nBlockAlign: 2, wBitsPerSample: 16, cbSize: 0 };
            let wave = windows::core::GUID::from_u128(0xc31adbae_527f_4ff5_a230_f62bb61ff70c);
            output.BindToFile(PCWSTR(path.as_ptr()), SPFM_CREATE_ALWAYS, Some(&wave), Some(&format), 0).unwrap();
            voice.SetOutput(&output, false).unwrap();
            voice.Speak(windows::core::w!("Hello world. This is a speech recognition test. The weather is nice today."), 0, None).unwrap();
            voice.SetOutput(None, true).unwrap(); output.Close().unwrap();
            let input: ISpStream = CoCreateInstance(&SpStream, None, CLSCTX_INPROC_SERVER).unwrap();
            input.BindToFile(PCWSTR(path.as_ptr()), SPFM_OPEN_READONLY, None, None, 0).unwrap(); input.cast().unwrap()
        };
        let mut recognition = Recognition::new(&SpeechRequest { lang: Some("en-US".into()), interim: true }, Some(&input)).unwrap();
        let mut finals = Vec::new(); let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            let done = recognition.poll(|event| if let SpeechEvent::Result { text, is_final: true } = event { finals.push(text); }).unwrap();
            if done { break; } assert!(Instant::now() < deadline, "SAPI stream did not finish: {finals:?}");
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(!finals.is_empty(), "installed SAPI engine returned no final transcript");
        println!("SAPI final transcripts: {finals:?}");
        drop(recognition); drop(input);
    }
}

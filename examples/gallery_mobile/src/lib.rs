#[cfg(target_os = "android")]
#[no_mangle]
pub fn android_main(app: winit::platform::android::activity::AndroidApp) {
    let directory = app
        .internal_data_path()
        .expect("Android application data directory");
    gallery::prepare_resources(&directory).expect("install Gallery resources");
    jadix::platform::install_android_app(app);
    gallery::run_gallery();
}

#[cfg(target_env = "ohos")]
#[openharmony_ability_derive::ability]
pub fn openharmony_app(app: openharmony_ability::OpenHarmonyApp) {
    let directory = app.pref_path().expect("OHOS application data directory");
    let log_path = std::path::Path::new(&directory).join("gallery.log");
    std::env::set_var("JADIX_LOG_FILE", log_path);
    let panic_path = std::path::Path::new(&directory).join("gallery-panic.log");
    let previous_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |information| {
        let _ = std::fs::write(&panic_path, information.to_string());
        previous_hook(information);
    }));
    gallery::prepare_resources(std::path::Path::new(&directory))
        .expect("install Gallery resources");
    #[cfg(feature = "ohos-desktop")]
    std::env::set_current_dir(&directory).expect("use writable Gallery data directory");
    jadix::platform::install_ohos_app(app);
    gallery::run_gallery();
}

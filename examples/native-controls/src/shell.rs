//! The SDK application is shared by native hosts; adapters only carry events.
use jadix_native::host::PlatformHost;
use jadix_shell::{application::ScriptApplication, native::NativeApplication};

pub struct ShellApp<Host: PlatformHost> {
    pub app: NativeApplication<Host>,
    viewport: [f32; 2],
    environment: u64,
    checkpoint: String,
}

impl<Host: PlatformHost> ShellApp<Host> {
    pub fn new(host: Host, viewport: [f32; 2], count: usize, value: String) -> Result<Self, String> {
        let script = ScriptApplication::development_standard(
            "examples.native.shell",
            include_str!("../shell/mobile.bundle.js"),
            Default::default(),
        ).map_err(|e| e.to_string())?;
        script.restore_checkpoint(&serde_json::json!({"version":1,"count":count,"value":value}).to_string())
            .map_err(|e| e.to_string())?;
        let mut result = Self { app: NativeApplication::new(script, host, Default::default()),
            viewport, environment: 0, checkpoint: String::new() };
        result.render()?;
        Ok(result)
    }

    pub fn new_with_content(host: Host, viewport: [f32; 2], content: jadix_core::window::WindowContent) -> Result<Self, String> {
        if content.factory != "shell" { return Err("unknown shell window content factory".into()); }
        Self::new(host, viewport, 0, content.argument.unwrap_or_default())
    }

    fn render(&mut self) -> Result<(), String> {
        self.app.poll().map_err(|e| e.to_string())?;
        self.app.publish(self.viewport, self.environment).map_err(|e| e.to_string())?;
        self.checkpoint = self.app.application().checkpoint().map_err(|e| e.to_string())?.to_string();
        Ok(())
    }

    pub fn event(&mut self, json: &str) -> Result<(), String> {
        self.app.event_json(json).map_err(|e| e.to_string())?;
        self.render()
    }

    pub fn resize(&mut self, viewport: [f32; 2], environment: u64) -> Result<(), String> {
        self.viewport = viewport;
        self.environment = environment;
        self.render()
    }

    pub fn state(&self) -> String { self.checkpoint.clone() }

    pub fn restore(&mut self, json: &str) -> Result<(), String> {
        self.app.application().restore_checkpoint(json).map_err(|e| e.to_string())?;
        self.render()
    }
}

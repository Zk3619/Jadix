use std::collections::HashMap;
use std::io;
use std::path::PathBuf;
use std::process::{Child, Command};
use std::sync::atomic::{AtomicBool, Ordering};

use super::options::Options;

static STOP: AtomicBool = AtomicBool::new(false);

pub(super) fn stop_requested() -> bool {
    STOP.load(Ordering::Relaxed)
}

#[cfg(windows)]
fn install_stop_handler() {
    unsafe extern "system" fn handler(event: u32) -> i32 {
        if event <= 1 {
            STOP.store(true, Ordering::Relaxed);
            1
        } else {
            0
        }
    }
    #[link(name = "Kernel32")]
    unsafe extern "system" {
        fn SetConsoleCtrlHandler(
            handler: Option<unsafe extern "system" fn(u32) -> i32>,
            add: i32,
        ) -> i32;
    }
    unsafe {
        SetConsoleCtrlHandler(Some(handler), 1);
    }
}

#[cfg(unix)]
fn install_stop_handler() {
    unsafe extern "C" fn handler(_: i32) {
        STOP.store(true, Ordering::Relaxed);
    }
    unsafe extern "C" {
        fn signal(number: i32, handler: usize) -> usize;
    }
    unsafe {
        signal(2, handler as *const () as usize);
        signal(15, handler as *const () as usize);
    }
}

#[cfg(not(any(windows, unix)))]
fn install_stop_handler() {}

pub(super) struct Supervisor {
    command: Vec<String>,
    root: PathBuf,
    child: Option<Child>,
    sources: HashMap<PathBuf, Option<String>>,
}

impl Supervisor {
    pub fn new(options: &Options) -> io::Result<Self> {
        install_stop_handler();
        let mut sources = HashMap::new();
        if !options.run.is_empty() {
            for root in &options.watch {
                for file in super::collect_recent_tracked_files(root, u64::MAX)
                    .into_iter()
                    .filter(|file| options.tracks(file))
                {
                    sources.insert(file.clone(), file_signature(&file));
                }
            }
        }
        let mut supervisor = Self {
            command: options.run.clone(),
            root: options.root.clone(),
            child: None,
            sources,
        };
        if !supervisor.command.is_empty() {
            supervisor.restart()?;
        }
        Ok(supervisor)
    }

    pub fn needs_restart(&mut self, files: &[PathBuf]) -> bool {
        if self.command.is_empty() {
            return false;
        }
        let exited = self
            .child
            .as_mut()
            .is_none_or(|child| child.try_wait().ok().flatten().is_some());
        let mut restart = exited;
        for file in files {
            let signature = file_signature(file);
            let previous = self.sources.insert(file.clone(), signature.clone());
            if super::is_rust_source(file)
                || file
                    .file_name()
                    .is_some_and(|name| name == "Cargo.toml" || name == "Cargo.lock")
            {
                restart |= signature.is_none() || previous != Some(signature);
            }
        }
        restart
    }

    pub fn restart(&mut self) -> io::Result<()> {
        self.stop();
        super::hmr_log::update(&format!("rebuild/run: {}", self.command.join(" ")));
        let mut command = Command::new(&self.command[0]);
        command.args(&self.command[1..]).current_dir(&self.root);
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            command.process_group(0);
        }
        self.child = Some(command.spawn()?);
        Ok(())
    }

    fn stop(&mut self) {
        if let Some(mut child) = self.child.take() {
            if child.try_wait().ok().flatten().is_none() {
                #[cfg(windows)]
                {
                    use std::os::windows::process::CommandExt;
                    let _ = Command::new("taskkill")
                        .args(["/PID", &child.id().to_string(), "/T", "/F"])
                        .creation_flags(0x08000000)
                        .output();
                }
                #[cfg(unix)]
                {
                    let _ = Command::new("kill")
                        .args(["-TERM", "--", &format!("-{}", child.id())])
                        .status();
                }
                let _ = child.kill();
            }
            let _ = child.wait();
        }
    }
}

impl Drop for Supervisor {
    fn drop(&mut self) {
        self.stop();
    }
}

fn file_signature(path: &std::path::Path) -> Option<String> {
    let source = std::fs::read_to_string(path).ok()?;
    if super::is_rust_source(path) {
        rust_signature(&source)
    } else {
        Some(source)
    }
}

fn rust_signature(source: &str) -> Option<String> {
    fn walk(stream: proc_macro2::TokenStream, ui: bool) -> String {
        use proc_macro2::TokenTree;
        let tokens: Vec<_> = stream.into_iter().collect();
        let mut signature = String::new();
        let mut index = 0;
        while index < tokens.len() {
            if !ui
                && matches!(tokens.get(index), Some(TokenTree::Ident(name)) if name == "ui")
                && matches!(tokens.get(index + 1), Some(TokenTree::Punct(mark)) if mark.as_char() == '!')
            {
                if let Some(TokenTree::Group(body)) = tokens.get(index + 2) {
                    signature.push_str("ui!{");
                    signature.push_str(&walk(body.stream(), true));
                    signature.push('}');
                    index += 3;
                    continue;
                }
            }
            let token = &tokens[index];
            if ui {
                if let TokenTree::Literal(literal) = token {
                    if literal.to_string().starts_with('"') {
                        let attribute = index
                            .checked_sub(1)
                            .and_then(|previous| tokens.get(previous));
                        let value = matches!(attribute, Some(TokenTree::Punct(mark)) if mark.as_char() == '=');
                        let name = index
                            .checked_sub(2)
                            .and_then(|previous| tokens.get(previous))
                            .map(ToString::to_string)
                            .unwrap_or_default();
                        if !value || is_live_attribute(&name) {
                            signature.push_str("__live_literal__ ");
                            index += 1;
                            continue;
                        }
                    }
                }
            }
            match token {
                TokenTree::Group(group) if !ui => {
                    signature.push_str(&format!(
                        "{:?}({})",
                        group.delimiter(),
                        walk(group.stream(), false)
                    ));
                }
                _ => signature.push_str(&token.to_string()),
            }
            signature.push(' ');
            index += 1;
        }
        signature
    }
    source.parse().ok().map(|stream| walk(stream, false))
}

fn is_live_attribute(name: &str) -> bool {
    matches!(
        name,
        "bg" | "color"
            | "text_size"
            | "font_size"
            | "font_weight"
            | "text_weight"
            | "text_decoration"
            | "text_decoration_line"
            | "text_decoration_color"
            | "text_decoration_style"
            | "rounded"
            | "radius"
            | "p"
            | "padding"
            | "m"
            | "margin"
            | "w"
            | "width"
            | "h"
            | "height"
            | "gap"
            | "animation"
            | "animation_name"
            | "animation_duration"
            | "animation_delay"
            | "animation_timing_function"
            | "animation_iteration_count"
            | "animation_direction"
            | "animation_fill_mode"
            | "animation_play_state"
            | "transition"
            | "transition_property"
            | "transition_duration"
            | "transition_delay"
            | "transition_timing_function"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn static_text_style_and_animation_use_live_patch_but_logic_rebuilds() {
        let old = r#"fn view() { let count = 1; ui! { <Box p="4" animation="spin 1s linear">"old"</Box> } }"#;
        let live = old
            .replace("old", "new")
            .replace(r#"p="4""#, r#"p="8""#)
            .replace("1s linear", "2s linear");
        assert_eq!(rust_signature(old), rust_signature(&live));
        assert_ne!(
            rust_signature(old),
            rust_signature(&old.replace("count = 1", "count = 2"))
        );
        assert_ne!(
            rust_signature(old),
            rust_signature(&old.replace("Box", "Button"))
        );
    }

    #[test]
    fn callback_dynamic_props_and_non_style_literals_require_rebuild() {
        for old in [
            r#"fn view() { ui! { <Box class="bg-red-500 p-4">"hello"</Box> } }"#,
            r#"fn view() { ui! { <Button variant="ghost">"hello"</Button> } }"#,
            r#"fn view() { ui! { <Box w={format!("100px")}>"hello"</Box> } }"#,
        ] {
            let next = old.replace("bg-red-500 ", "").replace("ghost", "outline").replace("100px", "200px");
            assert_ne!(rust_signature(old), rust_signature(&next));
        }
    }
}

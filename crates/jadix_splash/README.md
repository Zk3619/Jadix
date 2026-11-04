# jadix_splash

Startup policy, cancellation and retry epochs, native System presentation,
same-host Component presentation and independent native Window presentation.

Depends on `jadix`; `jadix` does not depend on this crate. The runtime keeps
window ownership, GPU rendering and presentation notifications.

Features: `system` enables desktop CPU-buffer presentation; `components` enables
`Splash` and its builder; `macros` adds RSX authoring. All features are opt-in.

Independent splash windows preserve the supplied `WindowOptions` decorations.
Use `StartupConfig::window(options, view).borderless(true)` to hide their system
frame, or `.borderless(false)` to restore it. This does not change the business
window, and does not require transparency. The three examples support
`--window --borderless`; shared-host System/Component modes keep the primary
window's own decoration settings.

On Windows, frameless windows also disable native shadows, DWM borders and
forced corner rounding, including transparent/backdrop windows. Component-drawn
borders and shadows remain controlled by the component's styles.


See [startup documentation](../../docs/startup.md) for host support and examples.

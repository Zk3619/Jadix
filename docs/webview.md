# Native WebView

Enable `jadix/webview` to use `webview(id, url)`, `webview_html(id, html)`, or the `WebView` component. `webview_status(id)` reports Loading, Ready, Failed, or Unsupported. `webview_supported()` reports compiled backend availability; creation can still fail if the host or runtime is unavailable.

All backends use the same logical document bounds and a separate visible clip. Scrolling/clipping must not resize the document viewport. Removing the component releases the platform view. Covered pages and active Dock/Resizable drags hide native views while their overlays are displayed.

| Platform | Backend | Requirements / current validation |
| --- | --- | --- |
| Windows | WebView2 composition controller | WebView2 Runtime; native desktop verified for click, context-menu origin, status popup origin, Dock drag and resize |
| macOS / iOS | WKWebView through Wry, native clipping parent | Apple SDK and native host; implementation requires on-device build/run validation |
| Linux X11 | WebKitGTK through Wry, native child shape/input clipping | GTK3 and WebKitGTK 4.1 development/runtime libraries; implementation requires Linux build/run validation |
| Android | android.webkit.WebView in a clipped Activity child | Java source integration below; aarch64 Rust check and Java compile verified, device run remains required |
| Linux Wayland / OpenHarmony | Host bridge required | Report an explicit failure/unsupported status; embedding is not implemented |

## Android application integration

Copy `crates/jadix_components/src/components/webview/android/JadixWebViewHost.java` into the application Java source set (`org/jadix/webview/JadixWebViewHost.java`). The helper works with the existing NativeActivity/GameActivity and dispatches view changes on the Activity UI thread. It does not require replacing the Activity. Keep this class and its static methods when enabling minification:

```proguard
-keep class org.jadix.webview.JadixWebViewHost { *; }
```

Add `android.permission.INTERNET` for remote sites. Use HTTPS; local HTTP fixtures require the application's normal debug cleartext-network configuration. Jadix must receive `install_android_app(app)` before running, as for other Android windows. Missing Java integration is reported through `WebViewStatus::Failed`.

## Gallery and native verification

Run `cargo run -p gallery --features webview`, open Navigation, then the Dock website workspace entry. The page stack covers Gallery with a back button and a Dock workspace displaying Bilibili. `JADIX_GALLERY_WEBVIEW_URL` can select a local calibration site.

Windows controller bounds carry the actual physical client origin in addition to the composition visual offset. Mouse input is local to the document. This keeps browser-owned menus and link-status UI anchored to the embedded view after docking, resizing, and moving the parent window.



Dock preview placement listens to drag move events. The drag overlay paint-follow signal uses silent writes, so it cannot be the sole reactive trigger for moving between center and edge zones inside one panel. The original author bounds remain available for drag constraints; collision and preview use the visible clipped intersection.

New-window requests such as links with `target="_blank"` and `window.open(url)` navigate the existing embedded browser on Windows and Wry hosts. They do not create a separate browser window.

On Windows, opening a stacked overlay captures the visible browser into a GPU texture before hiding its native visual. The texture participates in the same backdrop blur and scrim rendering as the rest of the page. Closing the overlay restores the live browser and its browsing state. Dock and divider drags use the same frozen backdrop while active; video and other page animations resume visually when the overlay or drag ends. This snapshot path is currently Windows-specific.

Dock uses the central 75% of each panel dimension for merging. Configure the fraction with `DockState::new(id, layout).center_drop_ratio(0.75)`; the remaining outer 12.5% on each side splits. The same fraction drives the hover preview and the committed drop. Panel bodies are plain drop targets, so hovering them never reserves an extra sortable row; only tab strips participate in sortable packing.

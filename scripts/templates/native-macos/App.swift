import AppKit

@main @MainActor final class Application: NSObject, NSApplicationDelegate, NSWindowDelegate {
    private var window: NSWindow?
    private var view: JadixNativeView?
    private var pendingURLs: [URL] = []
    private var allowedSchemes: Set<String> {
        let entries = Bundle.main.infoDictionary?["CFBundleURLTypes"] as? [[String: Any]] ?? []
        return Set(entries.flatMap { $0["CFBundleURLSchemes"] as? [String] ?? [] }.map { $0.lowercased() })
    }
    static func main() {
        let application = NSApplication.shared
        JadixPrepareDesktopServices()
        let delegate = Application(); application.delegate = delegate
        application.setActivationPolicy(.regular)
        withExtendedLifetime(delegate) { application.run() }
    }
    func applicationDidFinishLaunching(_ notification: Notification) {
        let window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 720, height: 480),
                              styleMask: [.titled, .closable, .miniaturizable, .resizable], backing: .buffered, defer: false)
        window.title = "@NAME@"; window.delegate = self; window.isReleasedWhenClosed = false
        let view = JadixNativeView(frame: window.contentView!.bounds)
        view.autoresizingMask = [.width, .height]; window.contentView = view
        view.onError = { NSLog("Jadix: %@", String(describing: $0)) }
        do { try view.setNotificationScope("main"); try view.mount() }
        catch { NSLog("Jadix mount failed: %@", String(describing: error)); NSApp.terminate(nil); return }
        self.window = window; self.view = view
        let menu = NSMenu()
        let applicationItem = NSMenuItem(); menu.addItem(applicationItem)
        let applicationMenu = NSMenu(); applicationItem.submenu = applicationMenu
        applicationMenu.addItem(withTitle: "Quit @NAME@", action: #selector(NSApplication.terminate(_:)), keyEquivalent: "q")
        let editItem = NSMenuItem(title: "Edit", action: nil, keyEquivalent: ""); menu.addItem(editItem)
        let edit = NSMenu(title: "Edit"); editItem.submenu = edit
        for (title, action, key) in [("Undo", "undo:", "z"), ("Cut", "cut:", "x"), ("Copy", "copy:", "c"), ("Paste", "paste:", "v"), ("Select All", "selectAll:", "a")] {
            edit.addItem(withTitle: title, action: Selector(action), keyEquivalent: key)
        }
        NSApp.mainMenu = menu
        let queued = pendingURLs; pendingURLs.removeAll()
        for url in queued { _ = view.openURL(url, source: "cold_start") }
        window.center(); window.makeKeyAndOrderFront(nil); NSApp.activate(ignoringOtherApps: true)
    }
    func application(_ application: NSApplication, open urls: [URL]) {
        for url in urls where allowedSchemes.contains(url.scheme?.lowercased() ?? "") && url.absoluteString.utf8.count <= 32768 {
            if let view { _ = view.openURL(url) }
            else if pendingURLs.count < 64 { pendingURLs.append(url) }
        }
    }
    func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool { true }
    func applicationWillTerminate(_ notification: Notification) { view?.close(); view = nil; window = nil }
}

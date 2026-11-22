import AppKit

@MainActor
private func shouldCloseForEscape(_ event: NSEvent, enabled: Bool, window: NSWindow) -> Bool {
    guard enabled, event.type == .keyDown, event.keyCode == 53, window.attachedSheet == nil,
          (window.contentView as? JadixNativeView)?.hasVisibleOverlay != true else { return false }
    return (window.firstResponder as? NSTextInputClient)?.hasMarkedText() != true
}

private final class JadixNativeWindow: NSWindow {
    var closesOnEscape = false
    var closeForEscape: (() -> Void)?
    var interceptEvent: ((NSEvent) -> Bool)?
    var pendingMode: JadixWindowRequest.Mode?
    var transitioningFullScreen = false
    var hideAfterFullScreen = false

    override func sendEvent(_ event: NSEvent) {
        if interceptEvent?(event) == true { return }
        if shouldCloseForEscape(event, enabled: closesOnEscape, window: self) {
            closeForEscape?()
            return
        }
        super.sendEvent(event)
    }
}

private final class JadixNativePanel: NSPanel {
    var acceptsKeyWindow = true
    var closesOnEscape = false
    var closeForEscape: (() -> Void)?
    var interceptEvent: ((NSEvent) -> Bool)?

    override var canBecomeKey: Bool { acceptsKeyWindow }
    override var canBecomeMain: Bool { false }

    override func sendEvent(_ event: NSEvent) {
        if interceptEvent?(event) == true { return }
        if shouldCloseForEscape(event, enabled: closesOnEscape, window: self) {
            closeForEscape?()
            return
        }
        super.sendEvent(event)
    }
}

@main
@MainActor
private final class NativeControlsApp: NSObject, NSApplicationDelegate, NSWindowDelegate {
    private enum WindowRequestError: LocalizedError {
        case invalidOptions
        case dialogRequiresParent
        case invalidModalWindow
        case missingWindow(String)
        case duplicateWindow(String)
        case declarationOwnerMismatch(String)
        case unsupportedMode
        case unsupportedContent(String)
        case contentAlreadyMounted(String)

        var errorDescription: String? {
            switch self {
            case .invalidOptions: return "Window ID and positive content dimensions are required"
            case .dialogRequiresParent: return "A dialog requires an open parent window"
            case .invalidModalWindow: return "A modal window requires a dialog and an open parent"
            case let .missingWindow(id): return "No open window with ID \(id)"
            case let .duplicateWindow(id): return "Window ID \(id) is already in use"
            case let .declarationOwnerMismatch(id): return "Window \(id) belongs to a different declaration"
            case .unsupportedMode: return "Window mode is unavailable for this window"
            case let .unsupportedContent(factory): return "Unknown window content factory \(factory)"
            case let .contentAlreadyMounted(id): return "Window \(id) already has different content"
            }
        }
    }

    private var windows: [String: NSWindow] = [:]
    private var windowContents: [String: JadixWindowRequest.Content] = [:]
    private let contentFactories = JadixWindowContentFactories()
    private var windowOwners: [String: String] = [:]
    private var declarationOwners: [String: String] = [:]
    private var closeOnBlurWindowIDs = Set<String>()
    private var keepsOpenOnPress = Set<String>()
    private var dismissOnCloseWindowIDs = Set<String>()
    private var dismissedWindowIDs = Set<String>()
    private var windowCloseCauses: [String: JadixWindowEvent.CloseCause] = [:]
    private var modalWindowIDs = Set<String>()
    private var nextWindowTopLeft = NSPoint.zero
    private weak var primaryWindow: NSWindow?
    private var isPreparingToTerminate = false
    private let confirmsExit = CommandLine.arguments.contains("--confirm-exit")
    private lazy var exitConfirmation = JadixExitConfirmation(prepareToTerminate: { [unowned self] complete in
        self.prepareToTerminate(complete)
    }) { [weak self] reason, resolve in
        guard let self else {
            resolve(false)
            return nil
        }
        return self.presentExitConfirmation(reason, resolve: resolve)
    }

    static func main() {
        let application = NSApplication.shared
        JadixPrepareDesktopServices()
        let delegate = NativeControlsApp()
        do { try delegate.registerWindowContents() }
        catch { fatalError("Could not register window contents: \(error)") }
        application.delegate = delegate
        application.setActivationPolicy(.regular)
        application.mainMenu = delegate.makeMainMenu(for: application)
        withExtendedLifetime(delegate) { application.run() }
    }

    private func registerWindowContents() throws {
        for name in ["settings", "document"] {
            try contentFactories.register(name) { content in
                let payload = try JSONEncoder().encode(content)
                let json = String(decoding: payload, as: UTF8.self)
                return { context, measure, publish, clear, file, window, alert, share, width, height in
                    json.withCString {
                        jadix_native_create_example_with_content(
                            context, measure, publish, clear, file, window, alert, share, width, height, $0
                        )
                    }
                }
            }
        }
    }

    private func makeMainMenu(for application: NSApplication) -> NSMenu {
        let menu = NSMenu()
        let applicationItem = NSMenuItem()
        applicationItem.submenu = NSMenu()
        applicationItem.submenu?.addItem(
            withTitle: "Quit Jadix Native Controls",
            action: #selector(NSApplication.terminate(_:)),
            keyEquivalent: "q"
        )
        menu.addItem(applicationItem)

        let file = NSMenu(title: "File")
        let newWindow = file.addItem(withTitle: "New Window", action: #selector(openWindow(_:)), keyEquivalent: "n")
        newWindow.target = self
        file.addItem(withTitle: "Close Window", action: #selector(NSWindow.performClose(_:)), keyEquivalent: "w")
        file.addItem(.separator())
        let contextMenu = file.addItem(withTitle: "Show Context Menu", action: #selector(openContextMenu(_:)), keyEquivalent: "")
        contextMenu.target = self
        let fileItem = NSMenuItem()
        fileItem.submenu = file
        menu.addItem(fileItem)

        let edit = NSMenu(title: "Edit")
        edit.addItem(withTitle: "Undo", action: NSSelectorFromString("undo:"), keyEquivalent: "z")
        let redo = edit.addItem(withTitle: "Redo", action: NSSelectorFromString("redo:"), keyEquivalent: "z")
        redo.keyEquivalentModifierMask = [.command, .shift]
        edit.addItem(.separator())
        edit.addItem(withTitle: "Cut", action: #selector(NSText.cut(_:)), keyEquivalent: "x")
        edit.addItem(withTitle: "Copy", action: #selector(NSText.copy(_:)), keyEquivalent: "c")
        edit.addItem(withTitle: "Paste", action: #selector(NSText.paste(_:)), keyEquivalent: "v")
        edit.addItem(withTitle: "Select All", action: #selector(NSText.selectAll(_:)), keyEquivalent: "a")
        let editItem = NSMenuItem()
        editItem.submenu = edit
        menu.addItem(editItem)

        let window = NSMenu(title: "Window")
        window.addItem(withTitle: "Minimize", action: #selector(NSWindow.performMiniaturize(_:)), keyEquivalent: "m")
        let windowItem = NSMenuItem()
        windowItem.submenu = window
        menu.addItem(windowItem)
        application.windowsMenu = window
        return menu
    }

    func applicationDidFinishLaunching(_ notification: Notification) {
        if windows.isEmpty { openWindow(nil) }
        NSApplication.shared.activate(ignoringOtherApps: true)
    }

    func application(_ application: NSApplication, open urls: [URL]) {
        for url in urls where JadixNativeView.acceptsURL(url) { _ = routeURL(url) }
    }

    func application(
        _ application: NSApplication, continue userActivity: NSUserActivity,
        restorationHandler: @escaping ([any NSUserActivityRestoring]) -> Void
    ) -> Bool {
        guard JadixNativeView.declaresHandoff, let url = JadixNativeView.continuedURL(userActivity),
              JadixNativeView.acceptsURL(url) else { return false }
        return routeURL(url)
    }

    private func routeURL(_ url: URL) -> Bool {
        if windows.isEmpty { openWindow(nil) }
        let keyWindow = NSApp.keyWindow.flatMap { window in
            windows.values.contains(where: { $0 === window }) ? window : nil
        }
        let window = keyWindow ?? primaryWindow ?? windows.values.first
        return (window?.contentView as? JadixNativeView)?.openURL(url) == true
    }

    @objc private func openWindow(_ sender: Any?) {
        do {
            let id = UUID().uuidString
            try openWindow(
                id: id, kind: .normal, parentID: nil,
                title: "Jadix Native Controls", width: 900, height: 660,
                resizable: true, closeOnBlur: false
            )
            if primaryWindow == nil { primaryWindow = windows[id] }
        } catch {
            NSAlert(error: error).runModal()
            if windows.isEmpty { NSApplication.shared.terminate(nil) }
        }
    }

    @objc private func openContextMenu(_ sender: Any?) {
        guard let view = NSApp.keyWindow?.contentView as? JadixNativeView, view.root != 0 else { return }
        _ = Locale.current.identifier.withCString { jadix_native_show_example_context_menu(view.root, $0) }
    }

    private func handleWindowRequest(_ request: JadixWindowRequest, sourceWindowID: String) throws {
        switch request.operation {
        case .open, .openDeclared:
            guard let width = request.width, let height = request.height else {
                throw WindowRequestError.invalidOptions
            }
            let kind = request.kind ?? .normal
            let declared = request.operation == .openDeclared
            if declared, windows[request.id] != nil { throw WindowRequestError.duplicateWindow(request.id) }
            let defaultParent = kind == .dialog || kind == .panel ? sourceWindowID : nil
            try openWindow(
                id: request.id, kind: kind,
                parentID: request.parent ?? defaultParent,
                title: request.title ?? "", width: CGFloat(width), height: CGFloat(height),
                resizable: request.resizable ?? true,
                closeOnBlur: request.closeOnBlur ?? (request.kind == .popup),
                modal: request.modal ?? false,
                initialMode: request.attributes?.mode ?? request.initialMode ?? .normal,
                initiallyVisible: request.attributes?.visible ?? request.initiallyVisible ?? true,
                attributes: request.attributes,
                content: request.content,
                declarationOwner: declared ? sourceWindowID : nil
            )
        case .update, .updateDeclared:
            if request.operation == .updateDeclared, declarationOwners[request.id] != sourceWindowID {
                throw WindowRequestError.declarationOwnerMismatch(request.id)
            }
            guard let window = windows[request.id] else { throw WindowRequestError.missingWindow(request.id) }
            guard let attributes = request.attributes else { throw WindowRequestError.invalidOptions }
            try applyWindowAttributes(attributes, to: window, id: request.id)
        case .setMode:
            guard let window = windows[request.id] else { throw WindowRequestError.missingWindow(request.id) }
            guard let nativeWindow = window as? JadixNativeWindow, let mode = request.mode,
                  supportsWindowMode(mode, in: window) else {
                throw WindowRequestError.unsupportedMode
            }
            nativeWindow.pendingMode = mode
            if window.isVisible || window.isMiniaturized { applyWindowMode(nativeWindow) }
        case .restore:
            guard let window = windows[request.id] else { throw WindowRequestError.missingWindow(request.id) }
            if window.isMiniaturized { window.deminiaturize(nil) }
        case .focus:
            guard let window = windows[request.id] else { throw WindowRequestError.missingWindow(request.id) }
            focusWindow(window, activateApplication: true)
        case .setVisibility:
            guard let window = windows[request.id] else { throw WindowRequestError.missingWindow(request.id) }
            guard let visible = request.visible else { throw WindowRequestError.invalidOptions }
            if visible {
                showWindow(window, key: false)
            } else {
                hideWindow(window)
            }
        case .close:
            guard let window = windows[request.id] else { throw WindowRequestError.missingWindow(request.id) }
            window.close()
        case .closeDeclared:
            guard let window = windows[request.id] else { return }
            guard declarationOwners[request.id] == sourceWindowID else {
                throw WindowRequestError.declarationOwnerMismatch(request.id)
            }
            window.close()
        }
    }

    private func supportsWindowMode(_ mode: JadixWindowRequest.Mode, in window: NSWindow) -> Bool {
        supportsWindowMode(mode, style: window.styleMask, behavior: window.collectionBehavior)
    }

    private func supportsWindowMode(
        _ mode: JadixWindowRequest.Mode, style: NSWindow.StyleMask, behavior: NSWindow.CollectionBehavior
    ) -> Bool {
        if mode == .fullScreen {
            if behavior.contains(.fullScreenAuxiliary) || behavior.contains(.fullScreenNone)
                || behavior.contains(.stationary) {
                return false
            }
        }
        return mode == .normal || (style.contains(.miniaturizable)
            && (mode == .minimized || style.contains(.resizable)))
    }

    private func applyWindowAttributes(
        _ attributes: JadixWindowRequest.Attributes, to window: NSWindow, id: String
    ) throws {
        let nativeWindow = window as? JadixNativeWindow
        try attributes.apply(to: window, preparePresentation: { mode, style, behavior in
            if let mode, mode != .normal,
               (nativeWindow == nil || !self.supportsWindowMode(mode, style: style, behavior: behavior)) {
                throw WindowRequestError.unsupportedMode
            }
        }) { mode, visible in
            if let mode, let nativeWindow {
                nativeWindow.pendingMode = mode
                // Hiding a full-screen window must first leave its Space. Keep
                // the requested mode pending until the retained root is shown.
                if visible == nil && (window.isVisible || window.isMiniaturized) {
                    self.applyWindowMode(nativeWindow)
                }
            }
            if let visible {
                if visible { self.showWindow(window, key: false) }
                else { self.hideWindow(window) }
            }
        }
        applyClosePolicy(attributes, to: window, id: id)
    }

    private func applyWindowMode(_ window: JadixNativeWindow) {
        guard !window.transitioningFullScreen, !window.hideAfterFullScreen,
              let mode = window.pendingMode else { return }
        let isFullScreen = window.styleMask.contains(.fullScreen)
        if isFullScreen && mode != .fullScreen {
            window.transitioningFullScreen = true
            window.toggleFullScreen(nil)
            return
        }
        window.pendingMode = nil
        if mode == .minimized {
            if !window.isMiniaturized { window.miniaturize(nil) }
            return
        }
        if window.isMiniaturized { window.deminiaturize(nil) }
        switch mode {
        case .normal where window.isZoomed, .maximized where !window.isZoomed:
            window.zoom(nil)
        case .fullScreen where !isFullScreen && supportsWindowMode(.fullScreen, in: window):
            window.transitioningFullScreen = true
            window.toggleFullScreen(nil)
        default:
            break
        }
    }

    func windowWillEnterFullScreen(_ notification: Notification) {
        (notification.object as? JadixNativeWindow)?.transitioningFullScreen = true
    }

    func windowWillExitFullScreen(_ notification: Notification) {
        (notification.object as? JadixNativeWindow)?.transitioningFullScreen = true
    }

    func windowDidEnterFullScreen(_ notification: Notification) { finishFullScreenTransition(notification) }
    func windowDidExitFullScreen(_ notification: Notification) { finishFullScreenTransition(notification) }

    private func finishFullScreenTransition(_ notification: Notification) {
        guard let window = notification.object as? JadixNativeWindow else { return }
        DispatchQueue.main.async { [weak self, weak window] in
            guard let self, let window, let id = window.identifier?.rawValue, self.windows[id] === window else {
                return
            }
            window.transitioningFullScreen = false
            if window.hideAfterFullScreen {
                self.hideWindow(window)
            } else {
                self.applyWindowMode(window)
            }
        }
    }

    func windowDidFailToEnterFullScreen(_ window: NSWindow) { clearFailedFullScreenTransition(window) }
    func windowDidFailToExitFullScreen(_ window: NSWindow) { clearFailedFullScreenTransition(window) }

    private func clearFailedFullScreenTransition(_ window: NSWindow) {
        guard let window = window as? JadixNativeWindow else { return }
        window.transitioningFullScreen = false
        window.pendingMode = nil
        if window.hideAfterFullScreen {
            window.hideAfterFullScreen = false
            if !window.styleMask.contains(.fullScreen) { hideWindow(window) }
        }
    }

    private func focusWindow(_ window: NSWindow, activateApplication: Bool = false) {
        var target = window
        while let modal = activeModal(in: target) { target = modal }
        if activateApplication, target.canBecomeKey, !(target is NSPanel), !NSApp.isActive {
            NSApp.activate(ignoringOtherApps: true)
        }
        showWindow(target, key: true)
    }

    private func showWindow(_ window: NSWindow, key: Bool) {
        if let id = window.identifier?.rawValue { dismissedWindowIDs.remove(id) }
        let nativeWindow = window as? JadixNativeWindow
        nativeWindow?.hideAfterFullScreen = false
        if !window.isVisible, nativeWindow?.pendingMode == .maximized, let nativeWindow {
            applyWindowMode(nativeWindow)
        }
        if let id = window.identifier?.rawValue, let ownerID = windowOwners[id],
           let owner = windows[ownerID], window.parent !== owner {
            owner.addChildWindow(window, ordered: .above)
        }
        if window.isMiniaturized { window.deminiaturize(nil) }
        if key, window.canBecomeKey {
            window.makeKeyAndOrderFront(nil)
        } else if key {
            window.orderFrontRegardless()
        } else {
            window.orderFront(nil)
        }
        if let nativeWindow { applyWindowMode(nativeWindow) }
        (window.contentView as? JadixNativeView)?.scheduleWindowState()
    }

    private func hideWindow(_ window: NSWindow) {
        if let nativeWindow = window as? JadixNativeWindow {
            nativeWindow.hideAfterFullScreen = true
            if nativeWindow.transitioningFullScreen { return }
            if nativeWindow.styleMask.contains(.fullScreen) {
                // Leave the full-screen Space before ordering the window out.
                nativeWindow.transitioningFullScreen = true
                nativeWindow.toggleFullScreen(nil)
                return
            }
            nativeWindow.hideAfterFullScreen = false
        }
        window.orderOut(nil)
        (window.contentView as? JadixNativeView)?.scheduleWindowState()
    }

    private func activeModal(in window: NSWindow) -> NSWindow? {
        window.childWindows?.reversed().first { child in
            child.isVisible && child.identifier.map { modalWindowIDs.contains($0.rawValue) } == true
        }
    }

    private func interceptModalInput(_ event: NSEvent, in window: NSWindow) -> Bool {
        guard let modal = activeModal(in: window) else { return false }
        switch event.type {
        case .leftMouseDown, .rightMouseDown, .otherMouseDown:
            NSBeep()
            focusWindow(modal)
            return true
        case .keyDown, .keyUp, .scrollWheel, .leftMouseUp, .rightMouseUp, .otherMouseUp,
             .leftMouseDragged, .rightMouseDragged, .otherMouseDragged:
            return true
        default:
            return false
        }
    }

    private func applyClosePolicy(_ attributes: JadixWindowRequest.Attributes?, to window: NSWindow, id: String) {
        if let closeOnPress = attributes?.closeOnPress {
            if closeOnPress { keepsOpenOnPress.remove(id) }
            else { keepsOpenOnPress.insert(id) }
        }
        if let closeOnBlur = attributes?.closeOnBlur {
            if closeOnBlur { closeOnBlurWindowIDs.insert(id) }
            else { closeOnBlurWindowIDs.remove(id) }
        }
        if let closeOnEscape = attributes?.closeOnEscape {
            (window as? JadixNativeWindow)?.closesOnEscape = closeOnEscape
            (window as? JadixNativePanel)?.closesOnEscape = closeOnEscape
        }
        if let dismissOnClose = attributes?.dismissOnClose {
            if dismissOnClose { dismissOnCloseWindowIDs.insert(id) }
            else { dismissOnCloseWindowIDs.remove(id) }
        }
    }

    private func shouldDismissOnClose(_ window: NSWindow) -> Bool {
        guard window !== primaryWindow, let id = window.identifier?.rawValue else { return false }
        return dismissOnCloseWindowIDs.contains(id)
    }

    private func closeFromWindowPolicy(_ window: NSWindow, cause: JadixWindowEvent.CloseCause) {
        if shouldDismissOnClose(window) {
            dismissWindow(window, cause: cause)
        } else {
            closeWindow(window, cause: cause)
        }
    }

    private func dismissWindow(_ window: NSWindow, cause: JadixWindowEvent.CloseCause) {
        guard let id = window.identifier?.rawValue, windows[id] === window,
              dismissedWindowIDs.insert(id).inserted else { return }
        let children = windowOwners.compactMap { $0.value == id ? windows[$0.key] : nil }
        for child in children { dismissWindow(child, cause: .parentClose) }
        guard windows[id] === window, dismissedWindowIDs.contains(id) else { return }
        hideWindow(window)
        (window.contentView as? JadixNativeView)?.reportWindowEvent(.closed(cause))
    }

    private func closeWindow(_ window: NSWindow, cause: JadixWindowEvent.CloseCause) {
        if let id = window.identifier?.rawValue { windowCloseCauses[id] = cause }
        window.close()
    }

    private func openWindow(
        id: String,
        kind: JadixWindowRequest.Kind,
        parentID: String?,
        title: String,
        width: CGFloat,
        height: CGFloat,
        resizable: Bool,
        closeOnBlur: Bool,
        modal: Bool = false,
        initialMode: JadixWindowRequest.Mode = .normal,
        initiallyVisible: Bool = true,
        attributes: JadixWindowRequest.Attributes? = nil,
        content: JadixWindowRequest.Content? = nil,
        declarationOwner: String? = nil
    ) throws {
        guard !id.isEmpty, width > 0, height > 0 else { throw WindowRequestError.invalidOptions }
        if let window = windows[id] {
            if let content, windowContents[id] != content {
                throw WindowRequestError.contentAlreadyMounted(id)
            }
            if let attributes { try applyWindowAttributes(attributes, to: window, id: id) }
            if attributes?.visible == nil && attributes?.mode != .minimized { focusWindow(window) }
            return
        }
        let contentFactory = try content.map { try contentFactories.resolve($0) }
        let parent = try parentID.map { parentID -> NSWindow in
            guard let parent = windows[parentID] else { throw WindowRequestError.missingWindow(parentID) }
            return parent
        }
        if kind == .dialog && parent == nil { throw WindowRequestError.dialogRequiresParent }
        if modal && (kind != .dialog || parent == nil) { throw WindowRequestError.invalidModalWindow }
        let frame = NSRect(x: 0, y: 0, width: width, height: height)
        var style: NSWindow.StyleMask
        switch kind {
        case .normal:
            style = [.titled, .closable, .miniaturizable]
        case .dialog:
            style = [.titled, .closable]
        case .panel:
            style = [.titled, .closable, .utilityWindow]
        case .popup, .overlay:
            style = .borderless
        }
        if resizable && style.contains(.titled) { style.insert(.resizable) }
        let window: NSWindow
        if kind == .panel || kind == .popup || kind == .overlay {
            let panel = JadixNativePanel(contentRect: frame, styleMask: style, backing: .buffered, defer: false)
            panel.acceptsKeyWindow = kind != .overlay
            panel.hidesOnDeactivate = false
            panel.closesOnEscape = kind == .popup
            window = panel
        } else {
            let nativeWindow = JadixNativeWindow(contentRect: frame, styleMask: style, backing: .buffered, defer: false)
            nativeWindow.closesOnEscape = kind == .dialog
            window = nativeWindow
        }
        window.identifier = NSUserInterfaceItemIdentifier(id)
        let interceptEvent: (NSEvent) -> Bool = { [weak self, weak window] event in
            guard let self, let window else { return false }
            return self.interceptModalInput(event, in: window)
        }
        (window as? JadixNativeWindow)?.interceptEvent = interceptEvent
        (window as? JadixNativePanel)?.interceptEvent = interceptEvent
        let closeForEscape: () -> Void = { [weak self, weak window] in
            guard let window else { return }
            self?.closeFromWindowPolicy(window, cause: .escape)
        }
        (window as? JadixNativeWindow)?.closeForEscape = closeForEscape
        (window as? JadixNativePanel)?.closeForEscape = closeForEscape
        window.title = title
        window.isReleasedWhenClosed = false
        window.delegate = self
        if kind == .panel {
            window.level = .floating
        } else if kind == .popup {
            window.level = .floating
            window.collectionBehavior = [.canJoinAllSpaces]
            window.hasShadow = true
        } else if kind == .overlay {
            window.level = .statusBar
            window.collectionBehavior = [.canJoinAllSpaces, .ignoresCycle]
            window.backgroundColor = .clear
            window.isOpaque = false
            window.hasShadow = false
            window.ignoresMouseEvents = true
        }
        // Creation consumes presentation attributes below, after the independent
        // Rust root and window registry entry exist.
        try attributes?.apply(to: window, presentation: { _, _ in })
        if initialMode != .normal {
            guard let nativeWindow = window as? JadixNativeWindow,
                  supportsWindowMode(initialMode, in: window) else {
                throw WindowRequestError.unsupportedMode
            }
            nativeWindow.pendingMode = initialMode
        }
        let contentFrame = window.contentView?.bounds ?? window.contentRect(forFrameRect: window.frame)
        let nativeView = JadixNativeView(frame: NSRect(origin: .zero, size: contentFrame.size))
        nativeView.onError = { error in NSLog("Jadix native root: %@", String(describing: error)) }
        nativeView.handoffEnabled = JadixNativeView.declaresHandoff
        nativeView.onOpenURL = { [weak nativeView] url in
            guard let nativeView else { return false }
            do {
                try nativeView.setAppAddress(url.absoluteString)
                return true
            } catch {
                nativeView.onError?(error)
                return false
            }
        }
        nativeView.onWindowRequest = { [weak self] request in
            guard let self else { throw WindowRequestError.missingWindow(request.id) }
            try self.handleWindowRequest(request, sourceWindowID: id)
        }
        if let declarationOwner {
            nativeView.onWindowStateChange = { [weak self, weak nativeView] state in
                guard let self, let nativeView, self.windows[id]?.contentView === nativeView,
                      self.declarationOwners[id] == declarationOwner,
                      let owner = self.windows[declarationOwner]?.contentView as? JadixNativeView else { return }
                owner.reportWindowEvent(.childState(id, state))
            }
        }
        nativeView.preferredSheetWindow = { [weak self] in
            guard let self, let key = NSApp.keyWindow, key.styleMask.contains(.titled),
                  let keyID = key.identifier?.rawValue, self.windows[keyID] === key else { return nil }
            return key
        }
        window.contentView = nativeView
        do {
            try nativeView.setNotificationScope(id)
            try nativeView.mount(factory: contentFactory)
            if CommandLine.arguments.contains("--rust-menu") {
                let installed = Locale.current.identifier.withCString {
                    jadix_native_set_example_window_menu(nativeView.root, $0)
                }
                if !installed { NSLog("Could not install the Jadix Rust window menu") }
            }
        } catch {
            nativeView.close()
            throw error
        }
        if kind == .popup && closeOnBlur { closeOnBlurWindowIDs.insert(id) }
        applyClosePolicy(attributes, to: window, id: id)
        if modal { modalWindowIDs.insert(id) }
        if attributes?.position == nil {
            if let parent, kind == .dialog {
                let parentFrame = parent.frame
                window.setFrameOrigin(NSPoint(
                    x: parentFrame.midX - window.frame.width / 2,
                    y: parentFrame.midY - window.frame.height / 2
                ))
            } else if windows.isEmpty { window.center() }
            else { nextWindowTopLeft = window.cascadeTopLeft(from: nextWindowTopLeft) }
        }
        windows[id] = window
        windowContents[id] = content
        if let parentID { windowOwners[id] = parentID }
        if let declarationOwner { declarationOwners[id] = declarationOwner }
        if initiallyVisible { focusWindow(window) }
    }

    func windowWillClose(_ notification: Notification) {
        guard let window = notification.object as? NSWindow else { return }
        if confirmsExit { exitConfirmation.windowWillClose(window) }
        var cause = JadixWindowEvent.CloseCause.close
        var alreadyDismissed = false
        var declaringRoot: JadixNativeView?
        var destroyedChildID: String?
        if let id = window.identifier?.rawValue, windows[id] === window {
            windows.removeValue(forKey: id)
            windowContents.removeValue(forKey: id)
            if let owner = declarationOwners.removeValue(forKey: id) {
                declaringRoot = windows[owner]?.contentView as? JadixNativeView
                destroyedChildID = id
            }
            closeOnBlurWindowIDs.remove(id)
            keepsOpenOnPress.remove(id)
            dismissOnCloseWindowIDs.remove(id)
            alreadyDismissed = dismissedWindowIDs.remove(id) != nil
            cause = windowCloseCauses.removeValue(forKey: id) ?? .close
            modalWindowIDs.remove(id)
            windowOwners.removeValue(forKey: id)
            let children = windows.compactMap { childID, child in
                windowOwners[childID] == id || declarationOwners[childID] == id ? child : nil
            }
            for child in children {
                if child.parent === window { window.removeChildWindow(child) }
                closeWindow(child, cause: .parentClose)
            }
        }
        if let root = window.contentView as? JadixNativeView {
            if !alreadyDismissed { root.reportWindowEvent(.closed(cause)) }
            if windows.isEmpty { jadix_native_collect_app_quit() }
            root.close()
        }
        if let destroyedChildID { declaringRoot?.reportWindowEvent(.childDestroyed(destroyedChildID)) }
        if windows.isEmpty { nextWindowTopLeft = .zero }
    }

    func windowShouldClose(_ sender: NSWindow) -> Bool {
        if let modal = activeModal(in: sender) {
            NSBeep()
            focusWindow(modal)
            return false
        }
        guard let id = sender.identifier?.rawValue else { return true }
        let reason: JadixWindowEvent.CloseReason
        switch NSApp.currentEvent?.type {
        case .keyDown: reason = .shortcut
        case .leftMouseDown, .leftMouseUp:
            reason = NSApp.currentEvent?.window === sender ? .closeButton : .menu
        default: reason = .menu
        }
        (sender.contentView as? JadixNativeView)?.reportWindowEvent(.closePress(reason))
        guard windows[id] === sender, !keepsOpenOnPress.contains(id) else { return false }
        if confirmsExit, sender === primaryWindow {
            return exitConfirmation.windowShouldClose(sender)
        }
        if shouldDismissOnClose(sender) {
            dismissWindow(sender, cause: .closePress)
            return false
        }
        windowCloseCauses[id] = .closePress
        return true
    }

    func windowShouldZoom(_ window: NSWindow, toFrame newFrame: NSRect) -> Bool {
        if newFrame != window.frame {
            (window.contentView as? JadixNativeView)?.prepareForWindowZoom()
        }
        return true
    }

    func windowDidBecomeKey(_ notification: Notification) {
        guard let window = notification.object as? NSWindow, activeModal(in: window) != nil else { return }
        DispatchQueue.main.async { [weak self, weak window] in
            guard let self, let window, window.isKeyWindow, let modal = self.activeModal(in: window) else { return }
            self.focusWindow(modal)
        }
    }

    func windowDidResignKey(_ notification: Notification) {
        guard let window = notification.object as? NSWindow,
              let id = window.identifier?.rawValue,
              closeOnBlurWindowIDs.contains(id) else { return }
        DispatchQueue.main.async { [weak self, weak window] in
            guard let self, let window, self.windows[id] === window,
                  window.isVisible, !window.isKeyWindow, window.attachedSheet == nil else { return }
            let focused = NSApp.keyWindow
            let candidateID = focused?.identifier?.rawValue
            var focusedID: String? = candidateID.flatMap {
                self.windows[$0] === focused ? $0 : nil
            }
            while let child = focusedID, let owner = self.windowOwners[child] {
                if owner == id { return }
                focusedID = owner
            }
            self.closeFromWindowPolicy(window, cause: .blur)
        }
    }

    func applicationWillTerminate(_ notification: Notification) {
        for window in windows.values {
            (window.contentView as? JadixNativeView)?.close()
        }
    }

    func applicationShouldTerminate(_ sender: NSApplication) -> NSApplication.TerminateReply {
        if isPreparingToTerminate { return .terminateLater }
        if confirmsExit, !windows.isEmpty { return exitConfirmation.applicationShouldTerminate(sender) }
        prepareToTerminate { sender.reply(toApplicationShouldTerminate: true) }
        return .terminateLater
    }

    private func prepareToTerminate(_ complete: @escaping @MainActor () -> Void) {
        guard !isPreparingToTerminate else { return }
        isPreparingToTerminate = true
        jadix_native_collect_app_quit()
        let timer = Timer(timeInterval: 0.01, repeats: true) { timer in
            MainActor.assumeIsolated {
                guard jadix_native_poll_app_quit() else { return }
                timer.invalidate()
                complete()
            }
        }
        RunLoop.main.add(timer, forMode: .common)
        RunLoop.main.add(timer, forMode: .modalPanel)
    }

    private func presentExitConfirmation(
        _ reason: JadixExitConfirmation.Reason, resolve: @escaping @MainActor (Bool) -> Void
    ) -> (@MainActor () -> Void)? {
        (primaryWindow?.contentView as? JadixNativeView)?.reportWindowEvent(.quitRequest)
        let parent: NSWindow?
        switch reason {
        case .quit: parent = NSApp.keyWindow
        case let .primaryWindowClose(window): parent = window
        }
        return JadixExitConfirmation.presentSystemAlert(
            title: "Quit Jadix Native Controls?",
            message: "All open Jadix Native Controls windows will close.",
            quitTitle: "Quit", cancelTitle: "Cancel", in: parent, resolve: resolve
        )
    }

    func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool { true }
}
